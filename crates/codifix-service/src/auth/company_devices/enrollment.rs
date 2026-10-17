// SPDX-License-Identifier: AGPL-3.0-or-later
use super::*;
use std::collections::BTreeSet;

fn validate(request: &CompanyDeviceApproveRequest) -> Result<(), ApiError> {
    device::code(&request.user_code)?;
    let mut seen = BTreeSet::new();
    if !request.confirmed || request.projects.len() > 32 {
        return Err(ApiError::status_message(
            StatusCode::BAD_REQUEST,
            "explicit bounded company device confirmation required",
        ));
    }
    for s in &request.projects {
        if !s.offline_confirmed
            || s.area == OnboardingArea::Trust
            || s.owner_instance_id.is_nil()
            || s.project_id.is_nil()
            || s.manifest_revision <= 0
            || s.manifest_sha256.len() != 64
            || !s
                .manifest_sha256
                .bytes()
                .all(|c| c.is_ascii_digit() || (b'a'..=b'f').contains(&c))
            || !seen.insert((s.area.as_str(), s.owner_instance_id, s.project_id))
        {
            return Err(ApiError::status_message(
                StatusCode::BAD_REQUEST,
                "invalid or duplicate exact company offline selection",
            ));
        }
    }
    Ok(())
}
async fn audit(
    tx: &mut Transaction<'_, Postgres>,
    human: Uuid,
    event: &str,
    request: Uuid,
    device: Uuid,
) -> Result<(), ApiError> {
    sqlx::query("INSERT INTO auth_audit_events(id,actor_principal_id,event_type,outcome,metadata_json) VALUES($1,$2,$3,'success',$4)").bind(Uuid::new_v4()).bind(human).bind(event).bind(json!({"request_id":request,"device_id":device})).execute(&mut **tx).await.map_err(ApiError::sql)?;
    Ok(())
}
async fn begin(state: &AppState) -> Result<Transaction<'_, Postgres>, ApiError> {
    let mut tx = state.pool()?.begin().await.map_err(ApiError::sql)?;
    sqlx::query("SET LOCAL lock_timeout='5s'")
        .execute(&mut *tx)
        .await
        .map_err(ApiError::sql)?;
    Ok(tx)
}
async fn fresh(
    state: &AppState,
    tx: &mut Transaction<'_, Postgres>,
    actor: &AuthenticatedPrincipal,
    intent: &Intent,
    row: &PgRow,
    request: &CompanyDeviceApproveRequest,
) -> Result<(), ApiError> {
    let identity = human(state, tx, actor).await?;
    match_intent(state, intent, request, &source(row)?, &identity)?;
    let live: bool = sqlx::query_scalar(
        "SELECT $1::timestamptz>clock_timestamp() AND $2::timestamptz>clock_timestamp()",
    )
    .bind(intent.grant_until)
    .bind(
        row.try_get::<DateTime<Utc>, _>("expires_at")
            .map_err(ApiError::sql)?,
    )
    .fetch_one(&mut **tx)
    .await
    .map_err(ApiError::sql)?;
    if !live {
        return Err(expired());
    }
    Ok(())
}
async fn replay(
    state: &AppState,
    tx: &mut Transaction<'_, Postgres>,
    actor: &AuthenticatedPrincipal,
    intent: &Intent,
    row: &PgRow,
) -> Result<OnboardingDevicePollResponse, ApiError> {
    let result = device::status(tx, row).await?;
    if result.status != "approved" || result.device_id != Some(intent.device_id) {
        return Err(denied());
    }
    for b in &intent.bindings {
        let token = sqlx::query_scalar::<_, Uuid>(
            "SELECT service_token_id FROM onboarding_devices WHERE id=$1",
        )
        .bind(intent.device_id)
        .fetch_one(&mut **tx)
        .await
        .map_err(ApiError::sql)?;
        let mut machine = actor.clone();
        machine.id = result.principal_id.ok_or_else(denied)?;
        machine.kind = AuthPrincipalKind::ServiceToken;
        machine.credential_source = CredentialSource::ServiceToken;
        machine.token_id = Some(token);
        machine.session_id = None;
        company_binding(
            state,
            tx,
            &machine,
            b.request.area.as_str(),
            b.request.owner_instance_id,
            b.request.project_id,
        )
        .await?
        .ok_or_else(denied)?;
    }
    Ok(result)
}
pub(crate) async fn approve(
    State(state): State<AppState>,
    Extension(actor): Extension<AuthenticatedPrincipal>,
    Json(request): Json<CompanyDeviceApproveRequest>,
) -> Result<Json<OnboardingDevicePollResponse>, ApiError> {
    validate(&request)?;
    let mut tx = begin(&state).await?;
    let identity = human(&state, &mut tx, &actor).await?;
    let row = request_row(&mut tx, &request.user_code).await?;
    let saved = source(&row)?;
    let existing = load_intent(&mut tx, saved.request_id).await?;
    if let Some(intent) = &existing {
        match_intent(&state, intent, &request, &saved, &identity)?;
    }
    if row.try_get::<String, _>("status").map_err(ApiError::sql)? != "pending" {
        let intent = existing.as_ref().ok_or_else(changed)?;
        if row.try_get::<String, _>("status").map_err(ApiError::sql)? == "denied"
            && request.projects.is_empty()
        {
            human(&state, &mut tx, &actor).await?;
            let result = device::status(&mut tx, &row).await?;
            tx.commit().await.map_err(ApiError::sql)?;
            return Ok(Json(result));
        }
        let result = replay(&state, &mut tx, &actor, intent, &row).await?;
        human(&state, &mut tx, &actor).await?;
        tx.commit().await.map_err(ApiError::sql)?;
        return Ok(Json(result));
    }
    if !row.try_get::<bool, _>("active").map_err(ApiError::sql)? {
        return Err(expired());
    }
    let registry = company_catalog::registry_sha256(&state)?;
    human(&state, &mut tx, &actor).await?;
    tx.commit().await.map_err(ApiError::sql)?;
    let intent = if let Some(intent) = existing {
        intent
    } else {
        // Metadata is fetched without holding any authorization/database lock.
        let response =
            company_catalog::catalog(State(state.clone()), Extension(actor.clone())).await?;
        let bytes = axum::body::to_bytes(response.into_body(), 3 * 512 * 1024)
            .await
            .map_err(|_| uncertain(saved.request_id))?;
        let catalog: CompanyDeviceCatalogResponse =
            serde_json::from_slice(&bytes).map_err(|_| uncertain(saved.request_id))?;
        let mut tx = begin(&state).await?;
        let current = human(&state, &mut tx, &actor).await?;
        let row = request_row(&mut tx, &request.user_code).await?;
        if current != identity
            || source(&row)? != saved
            || company_catalog::registry_sha256(&state)? != registry
            || row.try_get::<String, _>("status").map_err(ApiError::sql)? != "pending"
            || !row.try_get::<bool, _>("active").map_err(ApiError::sql)?
        {
            return Err(changed());
        }
        if let Some(intent) = load_intent(&mut tx, saved.request_id).await? {
            match_intent(&state, &intent, &request, &saved, &current)?;
            tx.commit().await.map_err(ApiError::sql)?;
            intent
        } else {
            let grant_until: DateTime<Utc> =
                sqlx::query_scalar("SELECT clock_timestamp()+interval '30 days'")
                    .fetch_one(&mut *tx)
                    .await
                    .map_err(ApiError::sql)?;
            let mut bindings = Vec::new();
            for selection in &request.projects {
                let item = catalog
                    .projects
                    .iter()
                    .find(|p| {
                        p.project.manifest.entry.area == selection.area
                            && p.project.manifest.entry.owner_instance_id
                                == selection.owner_instance_id
                            && p.project.manifest.project_id == selection.project_id
                    })
                    .ok_or_else(denied)?;
                if !item.project.offline_available
                    || (selection.contribution_confirmed && !item.curated_contributions_available)
                    || !item.project.manifest.entry.offline_allowed
                    || item.project.allowed_roles != vec![AuthRole::Reader]
                    || item.project.manifest.revision != selection.manifest_revision
                    || item.manifest_sha256 != selection.manifest_sha256
                    || company_catalog::digest(&item.project.manifest)? != selection.manifest_sha256
                {
                    return Err(changed());
                }
                let owner = company_catalog::delivery_owner(
                    &state,
                    selection.area.as_str(),
                    selection.owner_instance_id,
                )?;
                let remote = OwnerDeviceApprovalRequest {
                    operation_id: Uuid::new_v4(),
                    company_id: catalog.company_id,
                    device_request_id: saved.request_id,
                    device_id: saved.device_id,
                    credential_sha256: saved.credential_sha256.clone(),
                    owner_instance_id: selection.owner_instance_id,
                    area: selection.area,
                    project_id: selection.project_id,
                    issuer: identity.issuer.clone(),
                    subject: identity.subject.clone(),
                    manifest_revision: selection.manifest_revision,
                    manifest_sha256: selection.manifest_sha256.clone(),
                    delivery_policy_revision: owner.delivery_policy_revision.ok_or_else(denied)?,
                    expires_at: grant_until,
                    curated_contributions_allowed: selection.contribution_confirmed,
                };
                bindings.push(Binding {
                    manifest: item.project.manifest.clone(),
                    request: remote,
                });
            }
            let intent = Intent {
                request: request.clone(),
                source_sha256: company_catalog::digest(&saved)?,
                identity: current,
                company_id: catalog.company_id,
                device_id: saved.device_id,
                registry_sha256: registry.clone(),
                grant_until,
                bindings,
            };
            sqlx::query("INSERT INTO onboarding_company_device_intents(request_id,company_id,device_id,human_id,issuer,subject,approve_sha256,approve_json,source_sha256,registry_sha256,grant_until,bindings_json) VALUES($1,$2,$3,$4,$5,$6,$7,$8,$9,$10,$11,$12)").bind(saved.request_id).bind(intent.company_id).bind(intent.device_id).bind(intent.identity.id).bind(&intent.identity.issuer).bind(&intent.identity.subject).bind(company_catalog::digest(&request)?).bind(SqlJson(&request)).bind(&intent.source_sha256).bind(&intent.registry_sha256).bind(intent.grant_until).bind(SqlJson(&intent.bindings)).execute(&mut *tx).await.map_err(ApiError::sql)?;
            for binding in &intent.bindings {
                sqlx::query("INSERT INTO onboarding_company_device_outcomes(operation_id,request_id) VALUES($1,$2)").bind(binding.request.operation_id).bind(saved.request_id).execute(&mut *tx).await.map_err(ApiError::sql)?;
            }
            audit(
                &mut tx,
                actor.id,
                "onboarding.company_device.intent",
                saved.request_id,
                saved.device_id,
            )
            .await?;
            fresh(&state, &mut tx, &actor, &intent, &row, &request).await?;
            tx.commit().await.map_err(ApiError::sql)?;
            intent
        }
    };
    // The full immutable intent is durable before the first external effect.
    for binding in &intent.bindings {
        transport::confirm(&state, binding).await?;
    }
    // A durable historical confirmation is not current owner access. Recheck
    // every selected owner, still without local locks, before activation.
    for binding in &intent.bindings {
        let row=sqlx::query("SELECT receipt_json,bootstrap_json FROM onboarding_company_device_outcomes WHERE operation_id=$1 AND state='confirmed'").bind(binding.request.operation_id).fetch_one(&state.pool()?).await.map_err(|_|uncertain(saved.request_id))?;
        let receipt = row
            .try_get::<SqlJson<OwnerDeviceApprovalReceipt>, _>("receipt_json")
            .map_err(|_| uncertain(saved.request_id))?
            .0;
        let bootstrap = row
            .try_get::<SqlJson<DeviceProjectSubscription>, _>("bootstrap_json")
            .map_err(|_| uncertain(saved.request_id))?
            .0;
        let owner = company_catalog::delivery_owner(
            &state,
            binding.request.area.as_str(),
            binding.request.owner_instance_id,
        )?;
        transport::current_bootstrap(&owner, &receipt, &bootstrap)
            .await
            .map_err(|_| uncertain(saved.request_id))?;
    }
    let activation=async {
    let mut tx = begin(&state).await?;
    human(&state, &mut tx, &actor).await?;
    let row = request_row(&mut tx, &request.user_code).await?;
    let durable = load_intent(&mut tx, saved.request_id)
        .await?
        .ok_or_else(changed)?;
    fresh(&state, &mut tx, &actor, &durable, &row, &request).await?;
    let decision: String = row.try_get("status").map_err(ApiError::sql)?;
    if decision == "approved" {
        let result = replay(&state, &mut tx, &actor, &durable, &row).await?;
        human(&state, &mut tx, &actor).await?;
        tx.commit().await.map_err(ApiError::sql)?;
        return Ok(Json(result));
    }
    if decision != "pending" {
        return Err(expired());
    }
    let mut confirmed = Vec::new();
    for binding in &durable.bindings {
        let outcome=sqlx::query("SELECT receipt_json,bootstrap_json FROM onboarding_company_device_outcomes WHERE operation_id=$1 AND request_id=$2 AND state='confirmed' FOR SHARE").bind(binding.request.operation_id).bind(saved.request_id).fetch_optional(&mut *tx).await.map_err(ApiError::sql)?.ok_or_else(||uncertain(saved.request_id))?;
        let receipt = outcome
            .try_get::<SqlJson<OwnerDeviceApprovalReceipt>, _>("receipt_json")
            .map_err(ApiError::sql)?
            .0;
        let bootstrap = outcome
            .try_get::<SqlJson<DeviceProjectSubscription>, _>("bootstrap_json")
            .map_err(ApiError::sql)?
            .0;
        transport::validate_receipt(&binding.request, &receipt)?;
        transport::validate_bootstrap(&binding.request, &bootstrap)?;
        confirmed.push((binding, receipt, bootstrap));
    }
    let approved = !confirmed.is_empty();
    let machine = Uuid::new_v4();
    let token = Uuid::new_v4();
    if approved {
        sqlx::query("INSERT INTO auth_principals(id,kind,display_name,global_permissions_json) VALUES($1,'service_token',$2,'[]'::jsonb)").bind(machine).bind(format!("Company device {}",saved.name)).execute(&mut *tx).await.map_err(ApiError::sql)?;
        sqlx::query("INSERT INTO auth_service_tokens(id,principal_id,name,token_hash,token_prefix,created_by,expires_at) VALUES($1,$2,$3,$4,$5,$6,$7)").bind(token).bind(machine).bind(&saved.name).bind(row.try_get::<Vec<u8>,_>("credential_hash").map_err(ApiError::sql)?).bind(format!("device_{}",&saved.device_id.simple().to_string()[..8])).bind(actor.id).bind(durable.grant_until).execute(&mut *tx).await.map_err(ApiError::sql)?;
        sqlx::query("INSERT INTO onboarding_devices(id,principal_id,machine_principal_id,name,platform,service_token_id) VALUES($1,$2,$3,$4,$5,$6)").bind(saved.device_id).bind(actor.id).bind(machine).bind(&saved.name).bind(&saved.platform).bind(token).execute(&mut *tx).await.map_err(ApiError::sql)?;
        for (binding, receipt, bootstrap) in &confirmed {
            sqlx::query("INSERT INTO onboarding_company_device_selections(device_id,request_id,area,owner_instance_id,project_id,manifest_json,approval_json,bootstrap_json,registry_sha256,grant_until) VALUES($1,$2,$3,$4,$5,$6,$7,$8,$9,$10)").bind(saved.device_id).bind(saved.request_id).bind(binding.request.area.as_str()).bind(binding.request.owner_instance_id).bind(binding.request.project_id).bind(SqlJson(&binding.manifest)).bind(SqlJson(receipt)).bind(SqlJson(bootstrap)).bind(&durable.registry_sha256).bind(durable.grant_until).execute(&mut *tx).await.map_err(ApiError::sql)?;
        }
    }
    let grants = durable
        .bindings
        .iter()
        .map(|b| OnboardingDeviceGrant {
            manifest: b.manifest.clone(),
            role: AuthRole::Reader,
        })
        .collect::<Vec<_>>();
    let row=sqlx::query("UPDATE onboarding_device_requests SET status=$2,approved_by=$3,machine_principal_id=$4,selection_json='[]'::jsonb,grants_json=$5,decided_at=clock_timestamp() WHERE request_id=$1 RETURNING *").bind(saved.request_id).bind(if approved {"approved"} else {"denied"}).bind(actor.id).bind(approved.then_some(machine)).bind(SqlJson(&grants)).fetch_one(&mut *tx).await.map_err(ApiError::sql)?;
    audit(
        &mut tx,
        actor.id,
        "onboarding.company_device.decide",
        saved.request_id,
        saved.device_id,
    )
    .await?;
    fresh(&state, &mut tx, &actor, &durable, &row, &request).await?;
    let response = device::status(&mut tx, &row).await?;
    if serde_json::to_vec(&response).map_err(|_|denied())?.len()>3*512*1024 {return Err(denied());}
    tx.commit().await.map_err(ApiError::sql)?;
    Ok(Json(response))
    }.await;
    activation.map_err(|error: ApiError| {
        if error.status.is_server_error() {
            uncertain(saved.request_id)
        } else {
            error
        }
    })
}
