// SPDX-License-Identifier: AGPL-3.0-or-later
//! Remote effects cannot be atomic with the company's local transaction.
use super::*;
use reqwest::{Client, Url};
use std::time::Duration;

pub(crate) async fn company_configuration(
    State(state): State<AppState>,
) -> Result<Json<CompanyOnboardingView>, ApiError> {
    let Some(registry) = state.config.auth.company_onboarding.as_ref() else {
        return Ok(Json(CompanyOnboardingView {
            company_id: None,
            writes_enabled: false,
            owners: Vec::new(),
        }));
    };
    if registry.company_id.is_nil() || registry.owners.len() > 16 {
        return Err(ApiError::forbidden("invalid company owner registry"));
    }
    let mut owners = Vec::new();
    let mut identities = std::collections::BTreeSet::new();
    for cfg in &registry.owners {
        if cfg.owner_instance_id.is_nil()
            || !identities.insert(cfg.owner_instance_id)
            || cfg.issuer
                != mem_config::oidc_principal_namespace(&state.config.auth.oidc.issuer_url)
            || cfg.issuer.ends_with('/')
            || !super::super::safe_https(&cfg.issuer)
            || cfg.delegation_policy_revision < 1
        {
            return Err(ApiError::forbidden("invalid company owner registry"));
        }
        endpoint(&cfg.base_url)?;
        owners.push(CompanyOwnerView {
            owner_instance_id: cfg.owner_instance_id,
            area: cfg.area,
            issuer: cfg.issuer.clone(),
            delegation_policy_revision: cfg.delegation_policy_revision,
            grant_management: cfg.area != OnboardingArea::Trust,
        });
    }
    Ok(Json(CompanyOnboardingView {
        company_id: Some(registry.company_id),
        writes_enabled: writable(&state).is_ok(),
        owners,
    }))
}

fn owner<'a>(
    state: &'a AppState,
    request: &CompanyGrantRequest,
    writing: bool,
) -> Result<&'a mem_config::CompanyOwnerConfig, ApiError> {
    writable(state)?;
    let registry = state
        .config
        .auth
        .company_onboarding
        .as_ref()
        .ok_or_else(|| ApiError::forbidden("company owner registry is not configured"))?;
    if registry.company_id != request.company_id
        || registry.company_id.is_nil()
        || registry.owners.len() > 16
        || request.issuer
            != mem_config::oidc_principal_namespace(&state.config.auth.oidc.issuer_url)
    {
        return Err(ApiError::forbidden(
            "exact company and shared issuer required",
        ));
    }
    let owners: Vec<_> = registry
        .owners
        .iter()
        .filter(|o| o.owner_instance_id == request.owner_instance_id && o.area == request.area)
        .collect();
    if owners.len() != 1 {
        return Err(ApiError::forbidden("owner is not uniquely configured"));
    }
    let cfg = owners[0];
    if cfg.area == OnboardingArea::Trust
        || cfg.issuer != request.issuer
        || cfg.issuer.ends_with('/')
        || !super::super::safe_https(&cfg.issuer)
        || cfg.delegation_policy_revision < 1
        || (writing && cfg.delegation_policy_revision != request.delegation_policy_revision)
    {
        return Err(ApiError::forbidden(
            "owner issuer, delegation revision or non-Trust authority required",
        ));
    }
    endpoint(&cfg.base_url)?;
    Ok(cfg)
}
fn endpoint(value: &str) -> Result<Url, ApiError> {
    let url =
        Url::parse(value).map_err(|_| ApiError::forbidden("invalid configured owner origin"))?;
    if !(url.scheme() == "https"
        || (cfg!(test)
            && url.scheme() == "http"
            && matches!(url.host_str(), Some("127.0.0.1" | "localhost"))))
        || url.host_str().is_none()
        || !url.username().is_empty()
        || url.password().is_some()
        || !matches!(url.path(), "" | "/")
        || url.query().is_some()
        || url.fragment().is_some()
    {
        return Err(ApiError::forbidden(
            "configured owner must be a credential-free HTTPS origin",
        ));
    }
    Ok(url)
}
fn credential(cfg: &mem_config::CompanyOwnerConfig) -> Result<String, ApiError> {
    if cfg.token_env.is_empty()
        || cfg.token_env.len() > 128
        || !cfg
            .token_env
            .bytes()
            .all(|c| c.is_ascii_alphanumeric() || c == b'_')
    {
        return Err(ApiError::service_unavailable(
            "configured owner credential unavailable",
        ));
    }
    let secret = std::env::var(&cfg.token_env)
        .map_err(|_| ApiError::service_unavailable("configured owner credential unavailable"))?;
    if secret.len() < 32 || secret.len() > 4096 || secret.chars().any(char::is_control) {
        return Err(ApiError::service_unavailable(
            "configured owner credential unavailable",
        ));
    }
    Ok(secret)
}
enum RemoteResult {
    Verified(Box<CompanyGrantResult>),
    Rejected,
    Absent,
    Uncertain,
}
async fn remote(
    cfg: &mem_config::CompanyOwnerConfig,
    request: &CompanyGrantRequest,
    status: bool,
) -> Result<RemoteResult, ApiError> {
    let token = credential(cfg)?;
    let client = Client::builder()
        .redirect(reqwest::redirect::Policy::none())
        .connect_timeout(Duration::from_secs(3))
        .timeout(Duration::from_secs(10))
        .build()
        .map_err(|_| ApiError::service_unavailable("owner transport unavailable"))?;
    let mut url = endpoint(&cfg.base_url)?;
    let call = if status {
        url.set_path(&format!(
            "/v1/onboarding/delegated-grants/operations/{}",
            request.operation_id
        ));
        client
            .get(url)
            .query(&[("company_id", request.company_id)])
            .bearer_auth(token)
    } else {
        url.set_path("/v1/onboarding/delegated-grants");
        client.post(url).bearer_auth(token).json(request)
    };
    let response = match call.send().await {
        Ok(r) => r,
        Err(_) => return Ok(RemoteResult::Uncertain),
    };
    let code = response.status();
    if status && code == reqwest::StatusCode::NOT_FOUND {
        return Ok(RemoteResult::Absent);
    }
    if code.is_client_error() {
        return Ok(RemoteResult::Rejected);
    }
    if !code.is_success() {
        return Ok(RemoteResult::Uncertain);
    }
    // Never propagate an owner's response body or transport diagnostics.
    if response.content_length().is_some_and(|n| n > 32 * 1024) {
        return Ok(RemoteResult::Uncertain);
    }
    let mut response = response;
    let mut bytes = Vec::new();
    loop {
        let chunk = match response.chunk().await {
            Ok(v) => v,
            Err(_) => return Ok(RemoteResult::Uncertain),
        };
        let Some(chunk) = chunk else { break };
        if bytes.len() + chunk.len() > 32 * 1024 {
            return Ok(RemoteResult::Uncertain);
        }
        bytes.extend_from_slice(&chunk);
    }
    let result: CompanyGrantResult = match serde_json::from_slice(&bytes) {
        Ok(v) => v,
        Err(_) => return Ok(RemoteResult::Uncertain),
    };
    if result.receipt.request != *request
        || result.receipt.request_sha256 != request_digest(request)?
        || result.receipt.principal_id.is_nil()
        || result.receipt.grant_revision != request.expected_grant_revision + 1
        || result.receipt.direct_grant_active != (request.operation == CompanyGrantOperation::Grant)
        || result.current_access.principal_id != result.receipt.principal_id
        || result.current_access.project.len() > 128
        || result.current_access.group_roles.len() > 16
        || result.current_access.permissions.len() > Permission::ALL.len()
        || result
            .current_access
            .permissions
            .iter()
            .any(|p| !Permission::ALL.iter().any(|v| v.as_str() == p))
        || result.current_access.observed_at > chrono::Utc::now() + chrono::Duration::seconds(5)
        || result.current_access.observed_at < chrono::Utc::now() - chrono::Duration::seconds(30)
    {
        return Ok(RemoteResult::Uncertain);
    }
    Ok(RemoteResult::Verified(Box::new(result)))
}

async fn status_row(state: &AppState, id: Uuid) -> Result<CompanyOperationStatus, ApiError> {
    let row=sqlx::query("SELECT i.request_json,i.request_sha256,COALESCE(o.outcome,'pending') AS outcome,o.receipt_json FROM onboarding_company_intents i LEFT JOIN onboarding_company_outcomes o USING(operation_id) WHERE i.operation_id=$1")
        .bind(id).fetch_optional(&state.pool()?).await.map_err(ApiError::sql)?.ok_or_else(||ApiError::not_found("company operation not found"))?;
    Ok(CompanyOperationStatus {
        request: serde_json::from_value(row.try_get("request_json").map_err(ApiError::sql)?)
            .map_err(|_| ApiError::internal("invalid company intent"))?,
        request_sha256: row.try_get("request_sha256").map_err(ApiError::sql)?,
        outcome: row.try_get("outcome").map_err(ApiError::sql)?,
        receipt: row
            .try_get::<Option<Value>, _>("receipt_json")
            .map_err(ApiError::sql)?
            .map(serde_json::from_value)
            .transpose()
            .map_err(|_| ApiError::internal("invalid company outcome"))?,
        current_access: None,
    })
}
async fn record_outcome(
    state: &AppState,
    actor: &AuthenticatedPrincipal,
    request: &CompanyGrantRequest,
    result: RemoteResult,
) -> Result<CompanyOperationStatus, ApiError> {
    let (outcome, receipt, current) = match result {
        RemoteResult::Verified(r) => ("confirmed", Some(r.receipt), Some(r.current_access)),
        RemoteResult::Rejected => ("rejected", None, None),
        _ => ("uncertain", None, None),
    };
    let mut tx = state
        .pool()?
        .begin()
        .await
        .map_err(|_| effect_unknown(request.operation_id))?;
    // An independently revoked caller cannot suppress an observed remote effect
    // from the journal. This transaction writes observations only, not authority.
    sqlx::query("INSERT INTO onboarding_company_outcomes(operation_id,outcome,receipt_json) VALUES($1,$2,$3) ON CONFLICT(operation_id) DO UPDATE SET outcome=EXCLUDED.outcome,receipt_json=EXCLUDED.receipt_json,updated_at=now() WHERE onboarding_company_outcomes.outcome<>'confirmed' OR (EXCLUDED.outcome='confirmed' AND onboarding_company_outcomes.receipt_json=EXCLUDED.receipt_json)")
        .bind(request.operation_id).bind(outcome).bind(receipt.as_ref().map(|r|json!(r))).execute(&mut *tx).await.map_err(|_|effect_unknown(request.operation_id))?;
    super::super::audit(&mut tx,actor.id,None,"onboarding.company.outcome",json!({"operation_id":request.operation_id,"owner_instance_id":request.owner_instance_id,"outcome":outcome})).await.map_err(|_|effect_unknown(request.operation_id))?;
    tx.commit()
        .await
        .map_err(|_| effect_unknown(request.operation_id))?;
    let mut response = status_row(state, request.operation_id)
        .await
        .map_err(|_| effect_unknown(request.operation_id))?;
    response.current_access = current;
    Ok(response)
}
fn effect_unknown(id: Uuid) -> ApiError {
    ApiError::service_unavailable(&format!(
        "external effect may have occurred; inspect company operation {id}"
    ))
}

pub(crate) async fn company_operation(
    State(state): State<AppState>,
    Extension(actor): Extension<AuthenticatedPrincipal>,
    Json(request): Json<CompanyGrantRequest>,
) -> Result<Json<CompanyOperationStatus>, ApiError> {
    validate_request(&request)?;
    let cfg = owner(&state, &request, true)?.clone();
    let digest = request_digest(&request)?;
    let mut tx = state.pool()?.begin().await.map_err(ApiError::sql)?;
    crate::auth::lock_global_administrator(&state, &mut tx, &actor).await?;
    sqlx::query("SELECT pg_advisory_xact_lock(hashtextextended($1,0))")
        .bind(format!("memory:company-intent:{}", request.operation_id))
        .execute(&mut *tx)
        .await
        .map_err(ApiError::sql)?;
    let old = sqlx::query(
        "SELECT request_sha256,owner_origin FROM onboarding_company_intents WHERE operation_id=$1",
    )
    .bind(request.operation_id)
    .fetch_optional(&mut *tx)
    .await
    .map_err(ApiError::sql)?;
    if let Some(old) = old {
        if old
            .try_get::<String, _>("request_sha256")
            .map_err(ApiError::sql)?
            != digest
            || old
                .try_get::<String, _>("owner_origin")
                .map_err(ApiError::sql)?
                != cfg.base_url
        {
            return Err(conflict());
        }
        tx.commit().await.map_err(ApiError::sql)?;
        return Ok(Json(status_row(&state, request.operation_id).await?));
    }
    sqlx::query("INSERT INTO onboarding_company_intents(operation_id,company_id,owner_instance_id,request_sha256,request_json,owner_origin,actor_id) VALUES($1,$2,$3,$4,$5,$6,$7)")
        .bind(request.operation_id).bind(request.company_id).bind(request.owner_instance_id).bind(digest).bind(json!(request)).bind(&cfg.base_url).bind(actor.id).execute(&mut *tx).await.map_err(ApiError::sql)?;
    super::super::audit(&mut tx,actor.id,None,"onboarding.company.intent",json!({"operation_id":request.operation_id,"owner_instance_id":request.owner_instance_id,"project_id":request.project_id})).await?;
    tx.commit().await.map_err(ApiError::sql)?;
    // Intent is durable and all database locks are released before networking.
    let remote = remote(&cfg, &request, false)
        .await
        .unwrap_or(RemoteResult::Uncertain);
    Ok(Json(
        record_outcome(&state, &actor, &request, remote).await?,
    ))
}
pub(crate) async fn company_operations(
    State(state): State<AppState>,
) -> Result<Json<Value>, ApiError> {
    let ids:Vec<Uuid>=sqlx::query_scalar("SELECT operation_id FROM onboarding_company_intents ORDER BY recorded_at DESC,operation_id LIMIT 100").fetch_all(&state.pool()?).await.map_err(ApiError::sql)?;
    let mut operations = Vec::new();
    for id in ids {
        operations.push(status_row(&state, id).await?);
    }
    Ok(Json(json!({"operations":operations})))
}
pub(crate) async fn company_status(
    State(state): State<AppState>,
    Path(id): Path<Uuid>,
) -> Result<Json<CompanyOperationStatus>, ApiError> {
    // Journal reads intentionally return historical receipt only. Explicit
    // reconciliation reads the owner and provides timestamped current rights.
    Ok(Json(status_row(&state, id).await?))
}
pub(crate) async fn company_reconcile(
    State(state): State<AppState>,
    Extension(actor): Extension<AuthenticatedPrincipal>,
    Path(id): Path<Uuid>,
) -> Result<Json<CompanyOperationStatus>, ApiError> {
    let operation = status_row(&state, id).await?;
    let cfg = owner(&state, &operation.request, false)?.clone();
    let origin: String = sqlx::query_scalar(
        "SELECT owner_origin FROM onboarding_company_intents WHERE operation_id=$1",
    )
    .bind(id)
    .fetch_one(&state.pool()?)
    .await
    .map_err(ApiError::sql)?;
    if origin != cfg.base_url {
        return Err(conflict());
    }
    let mut tx = state.pool()?.begin().await.map_err(ApiError::sql)?;
    crate::auth::lock_global_administrator(&state, &mut tx, &actor).await?;
    tx.commit().await.map_err(ApiError::sql)?;
    let observed = remote(&cfg, &operation.request, true)
        .await
        .unwrap_or(RemoteResult::Uncertain);
    let observed = if matches!(observed, RemoteResult::Absent) {
        owner(&state, &operation.request, true)?;
        remote(&cfg, &operation.request, false)
            .await
            .unwrap_or(RemoteResult::Uncertain)
    } else {
        observed
    };
    Ok(Json(
        record_outcome(&state, &actor, &operation.request, observed).await?,
    ))
}
