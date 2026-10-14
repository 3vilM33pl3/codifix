// SPDX-License-Identifier: AGPL-3.0-or-later
use super::*;
use crate::auth::device::tests::{Fixture, fixture};

pub(crate) async fn setup(f: &mut Fixture) -> (AuthenticatedPrincipal, OwnerDeviceApprovalRequest) {
    sqlx::query("UPDATE auth_project_memberships SET role='operator' WHERE id=$1")
        .bind(f.membership)
        .execute(&f.db.pool)
        .await
        .unwrap();
    let principal_id = Uuid::new_v4();
    let token_id = Uuid::new_v4();
    let secret = format!("mlt_{}{}", Uuid::new_v4().simple(), Uuid::new_v4().simple());
    let owner_instance_id = f.state.config.area.as_ref().unwrap().instance_id;
    let company_id = Uuid::new_v4();
    let issuer = f.state.config.auth.oidc.issuer_url.clone();
    f.state.config.auth.company_delivery_delegations =
        vec![mem_config::CompanyDeliveryDelegationConfig {
            company_id,
            owner_instance_id,
            area: OnboardingArea::Workshop,
            issuer: issuer.clone(),
            principal_id,
            token_id,
            policy_revision: 1,
            projects: vec![f.project],
        }];
    sqlx::query("INSERT INTO auth_principals(id,kind,display_name,global_permissions_json) VALUES($1,'service_token','Company delivery','[]')")
        .bind(principal_id).execute(&f.db.pool).await.unwrap();
    sqlx::query("INSERT INTO auth_service_tokens(id,principal_id,name,token_hash,token_prefix,expires_at) VALUES($1,$2,'Company delivery',$3,'synthetic',now()+interval '1 day')")
        .bind(token_id).bind(principal_id).bind(crate::auth::hash_secret(&secret)).execute(&f.db.pool).await.unwrap();
    sqlx::query("INSERT INTO auth_project_memberships(id,principal_id,project_id,role,permissions_json) VALUES($1,$2,$3,'reader',$4)")
        .bind(Uuid::new_v4()).bind(principal_id).bind(f.project).bind(json!([Permission::ScopedProjectDelivery])).execute(&f.db.pool).await.unwrap();
    let actor = crate::auth::resolve_principal(&f.state, crate::auth::Credential::ApiToken(secret))
        .await
        .unwrap()
        .unwrap();
    let row = sqlx::query("SELECT v.*,p.slug FROM onboarding_project_versions v JOIN projects p ON p.id=v.project_id WHERE v.project_id=$1 ORDER BY v.revision DESC LIMIT 1")
        .bind(f.project).fetch_one(&f.db.pool).await.unwrap();
    let manifest = crate::auth::onboarding::project_row(&row).unwrap();
    let request = OwnerDeviceApprovalRequest {
        operation_id: Uuid::new_v4(),
        company_id,
        device_request_id: Uuid::new_v4(),
        device_id: Uuid::new_v4(),
        credential_sha256: "a".repeat(64),
        owner_instance_id,
        area: OnboardingArea::Workshop,
        project_id: f.project,
        issuer,
        subject: f.human.subject.clone().unwrap(),
        manifest_revision: 1,
        manifest_sha256: digest(&manifest).unwrap(),
        delivery_policy_revision: 1,
        expires_at: chrono::Utc::now() + chrono::Duration::days(1),
        curated_contributions_allowed: false,
    };
    (actor, request)
}
async fn call(
    f: &Fixture,
    actor: &AuthenticatedPrincipal,
    request: &OwnerDeviceApprovalRequest,
) -> Result<OwnerDeviceApprovalReceipt, ApiError> {
    approve(
        State(f.state.clone()),
        Extension(actor.clone()),
        Json(request.clone()),
    )
    .await
    .map(|r| r.0)
}

#[tokio::test]
async fn external_approval_replays_without_creating_local_identity_or_device() {
    let Some(mut f) = fixture().await else { return };
    let (actor, request) = setup(&mut f).await;
    let before: i64 = sqlx::query_scalar("SELECT count(*) FROM auth_principals")
        .fetch_one(&f.db.pool)
        .await
        .unwrap();
    let receipt = call(&f, &actor, &request).await.unwrap();
    assert_eq!(receipt, call(&f, &actor, &request).await.unwrap());
    assert_eq!(receipt.human_principal_id, f.human.id);
    assert_eq!(
        before,
        sqlx::query_scalar::<_, i64>("SELECT count(*) FROM auth_principals")
            .fetch_one(&f.db.pool)
            .await
            .unwrap()
    );
    assert_eq!(
        0,
        sqlx::query_scalar::<_, i64>("SELECT count(*) FROM onboarding_devices WHERE id=$1")
            .bind(request.device_id)
            .fetch_one(&f.db.pool)
            .await
            .unwrap()
    );
    let Json(catalog) = catalog(
        State(f.state.clone()),
        Extension(actor.clone()),
        Json(OwnerDeviceCatalogRequest {
            company_id: request.company_id,
            owner_instance_id: request.owner_instance_id,
            area: request.area,
            issuer: request.issuer.clone(),
            subject: request.subject.clone(),
            delivery_policy_revision: 1,
        }),
    )
    .await
    .unwrap();
    assert_eq!(catalog.projects.len(), 1);
    assert_eq!(catalog.projects[0].manifest_sha256, request.manifest_sha256);
    assert!(catalog.projects[0].project.offline_available);
    assert_eq!(
        catalog.projects[0].project.allowed_roles,
        [AuthRole::Reader]
    );
    assert!(
        !serde_json::to_string(&catalog)
            .unwrap()
            .contains("example/other.git")
    );
    let mut altered = request.clone();
    altered.credential_sha256 = "b".repeat(64);
    assert_eq!(
        call(&f, &actor, &altered).await.unwrap_err().status,
        StatusCode::CONFLICT
    );
    for field in [
        "company", "owner", "area", "subject", "revision", "digest", "expiry",
    ] {
        let mut changed = request.clone();
        changed.operation_id = Uuid::new_v4();
        match field {
            "company" => changed.company_id = Uuid::new_v4(),
            "owner" => changed.owner_instance_id = Uuid::new_v4(),
            "area" => changed.area = OnboardingArea::Trust,
            "subject" => changed.subject = "missing-human".into(),
            "revision" => changed.delivery_policy_revision = 2,
            "digest" => changed.manifest_sha256 = "b".repeat(64),
            _ => changed.expires_at = chrono::Utc::now() - chrono::Duration::seconds(1),
        }
        assert!(call(&f, &actor, &changed).await.is_err(), "{field}");
    }
    let count: i64 =
        sqlx::query_scalar("SELECT count(*) FROM onboarding_delegated_device_approvals")
            .fetch_one(&f.db.pool)
            .await
            .unwrap();
    assert_eq!(count, 1);
    f.db.cleanup().await.unwrap();
}

#[tokio::test]
async fn approval_rechecks_human_delegate_and_catalog_on_every_replay() {
    let Some(mut f) = fixture().await else { return };
    let (actor, request) = setup(&mut f).await;
    call(&f, &actor, &request).await.unwrap();
    assert!(call(&f, &f.human, &request).await.is_err());
    let mut other_token = actor.clone();
    other_token.token_id = Some(Uuid::new_v4());
    assert!(call(&f, &other_token, &request).await.is_err());
    sqlx::query("UPDATE auth_service_tokens SET revoked_at=now() WHERE id=$1")
        .bind(actor.token_id)
        .execute(&f.db.pool)
        .await
        .unwrap();
    assert!(call(&f, &actor, &request).await.is_err());
    sqlx::query("UPDATE auth_service_tokens SET revoked_at=NULL WHERE id=$1")
        .bind(actor.token_id)
        .execute(&f.db.pool)
        .await
        .unwrap();
    sqlx::query("UPDATE auth_project_memberships SET permissions_json=$2 WHERE id=$1")
        .bind(f.membership)
        .bind(json!([Permission::MemoryRead]))
        .execute(&f.db.pool)
        .await
        .unwrap();
    assert!(call(&f, &actor, &request).await.is_err());
    sqlx::query("UPDATE auth_project_memberships SET permissions_json=NULL WHERE id=$1")
        .bind(f.membership)
        .execute(&f.db.pool)
        .await
        .unwrap();
    sqlx::query("UPDATE auth_principals SET disabled_at=now() WHERE id=$1")
        .bind(f.human.id)
        .execute(&f.db.pool)
        .await
        .unwrap();
    assert!(call(&f, &actor, &request).await.is_err());
    sqlx::query("UPDATE auth_principals SET disabled_at=NULL WHERE id=$1")
        .bind(f.human.id)
        .execute(&f.db.pool)
        .await
        .unwrap();
    sqlx::query("INSERT INTO onboarding_project_versions(project_id,revision,area,owner_instance_id,offline_allowed,entry_json,reviewed_by) SELECT project_id,2,area,owner_instance_id,offline_allowed,entry_json,reviewed_by FROM onboarding_project_versions WHERE project_id=$1 AND revision=1")
        .bind(f.project).execute(&f.db.pool).await.unwrap();
    assert!(call(&f, &actor, &request).await.is_err());
    f.db.cleanup().await.unwrap();
}

#[tokio::test]
async fn approval_audit_failure_rolls_back_and_capability_broadening_is_denied() {
    let Some(mut f) = fixture().await else { return };
    let (actor, request) = setup(&mut f).await;
    sqlx::query("CREATE FUNCTION deny_device_approval_audit() RETURNS trigger LANGUAGE plpgsql AS $$ BEGIN RAISE EXCEPTION 'synthetic audit failure'; END $$").execute(&f.db.pool).await.unwrap();
    sqlx::query("CREATE TRIGGER deny_device_approval_audit BEFORE INSERT ON auth_audit_events FOR EACH ROW EXECUTE FUNCTION deny_device_approval_audit()").execute(&f.db.pool).await.unwrap();
    assert!(call(&f, &actor, &request).await.is_err());
    assert_eq!(
        0,
        sqlx::query_scalar::<_, i64>("SELECT count(*) FROM onboarding_delegated_device_approvals")
            .fetch_one(&f.db.pool)
            .await
            .unwrap()
    );
    sqlx::query("DROP TRIGGER deny_device_approval_audit ON auth_audit_events")
        .execute(&f.db.pool)
        .await
        .unwrap();
    sqlx::query("UPDATE auth_project_memberships SET permissions_json=$2 WHERE principal_id=$1")
        .bind(actor.id)
        .bind(json!([
            Permission::ScopedProjectDelivery,
            Permission::MemoryRead
        ]))
        .execute(&f.db.pool)
        .await
        .unwrap();
    assert!(call(&f, &actor, &request).await.is_err());
    f.db.cleanup().await.unwrap();
}
