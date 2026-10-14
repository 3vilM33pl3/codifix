use super::*;
use crate::auth::device::tests::fixture;

#[tokio::test]
async fn curated_receipts_are_atomic_idempotent_and_authority_checked() {
    let Some(mut f) = fixture().await else { return };
    let (actor, mut approval) = crate::auth::company_device_delivery::tests::setup(&mut f).await;
    approval.curated_contributions_allowed = true;
    let _ = crate::auth::company_device_delivery::approve(
        State(f.state.clone()),
        Extension(actor.clone()),
        Json(approval.clone()),
    )
    .await
    .unwrap();
    let input = CuratedDeviceContribution {
        schema_version: 2,
        operation_id: Uuid::new_v4(),
        binding: LaptopContributionBinding {
            scope: LaptopContributionScope {
                company_id: approval.company_id,
                area: approval.area,
                owner_instance_id: approval.owner_instance_id,
                project_id: approval.project_id,
                space_id: Uuid::new_v4(),
            },
            device_id: approval.device_id,
            manifest_revision: approval.manifest_revision,
            manifest_sha256: approval.manifest_sha256.clone(),
        },
        local_memory_id: Uuid::new_v4(),
        authored_at: chrono::Utc::now(),
        change: CuratedDeviceChange::Add {
            content: CuratedDeviceContent {
                canonical_text: "Synthetic curated knowledge".into(),
                summary: "Synthetic fact".into(),
                memory_type: MemoryType::Implementation,
                importance: 3,
                confidence: 0.8,
            },
        },
    };
    let request = OwnerDeviceContributionRequest {
        approval_id: approval.operation_id,
        contribution: input.clone(),
    };
    let mut read_only = approval.clone();
    read_only.operation_id = Uuid::new_v4();
    read_only.curated_contributions_allowed = false;
    assert!(
        serde_json::to_value(&read_only)
            .unwrap()
            .get("curated_contributions_allowed")
            .is_none()
    );
    let _ = crate::auth::company_device_delivery::approve(
        State(f.state.clone()),
        Extension(actor.clone()),
        Json(read_only.clone()),
    )
    .await
    .unwrap();
    let mut legacy = request.clone();
    legacy.approval_id = read_only.operation_id;
    assert_eq!(
        submit(
            State(f.state.clone()),
            Extension(actor.clone()),
            Json(legacy)
        )
        .await
        .unwrap_err()
        .status,
        StatusCode::FORBIDDEN
    );
    let call = |r: OwnerDeviceContributionRequest| {
        submit(State(f.state.clone()), Extension(actor.clone()), Json(r))
    };
    let first = call(request.clone()).await.unwrap().0;
    assert_eq!(first, call(request.clone()).await.unwrap().0);
    let mut altered = request.clone();
    altered.contribution.authored_at += chrono::Duration::seconds(1);
    assert_eq!(
        call(altered).await.unwrap_err().status,
        StatusCode::CONFLICT
    );
    assert_eq!(
        1,
        sqlx::query_scalar::<_, i64>("SELECT count(*) FROM device_curated_contribution_receipts")
            .fetch_one(&f.db.pool)
            .await
            .unwrap()
    );
    let mut correction = request.clone();
    correction.contribution.operation_id = Uuid::new_v4();
    correction.contribution.local_memory_id = Uuid::new_v4();
    let CuratedDeviceChange::Add { content } = input.change else {
        panic!()
    };
    correction.contribution.change = CuratedDeviceChange::Correct {
        target_canonical_id: first.owner_canonical_id.unwrap(),
        expected_version: 2,
        content,
    };
    let proposal = call(correction).await.unwrap().0;
    assert_eq!(proposal.outcome, DeviceContributionOutcome::Conflict);
    assert!(proposal.proposal_id.is_some());
    assert_eq!(
        1,
        sqlx::query_scalar::<_, i64>("SELECT count(*) FROM memory_entries WHERE canonical_id=$1")
            .bind(first.owner_canonical_id)
            .fetch_one(&f.db.pool)
            .await
            .unwrap()
    );
    // A timeline failure rolls back both the accepted content and its receipt.
    sqlx::query("CREATE FUNCTION fail_device_event() RETURNS trigger LANGUAGE plpgsql AS $$ BEGIN RAISE EXCEPTION 'synthetic event unavailable'; END $$").execute(&f.db.pool).await.unwrap();
    sqlx::query("CREATE TRIGGER fail_device_event BEFORE INSERT ON project_timeline_events FOR EACH ROW EXECUTE FUNCTION fail_device_event()").execute(&f.db.pool).await.unwrap();
    let mut retry = request.clone();
    retry.contribution.operation_id = Uuid::new_v4();
    retry.contribution.local_memory_id = Uuid::new_v4();
    assert!(call(retry.clone()).await.is_err());
    assert!(!sqlx::query_scalar::<_,bool>("SELECT EXISTS(SELECT 1 FROM device_curated_contribution_receipts WHERE operation_id=$1)").bind(retry.contribution.operation_id).fetch_one(&f.db.pool).await.unwrap());
    assert_eq!(
        1,
        sqlx::query_scalar::<_, i64>("SELECT count(*) FROM memory_entries WHERE project_id=$1")
            .bind(f.project)
            .fetch_one(&f.db.pool)
            .await
            .unwrap()
    );
    sqlx::query("UPDATE auth_project_memberships SET role='reader' WHERE id=$1")
        .bind(f.membership)
        .execute(&f.db.pool)
        .await
        .unwrap();
    assert_eq!(
        call(request).await.unwrap_err().status,
        StatusCode::FORBIDDEN
    );
    f.db.cleanup().await.unwrap();
}

#[test]
fn wire_accepts_no_raw_or_session_fields() {
    assert!(
        serde_json::from_value::<CuratedDeviceChange>(
            json!({"kind":"raw_capture","text":"secret"})
        )
        .is_err()
    );
    assert!(
        serde_json::from_value::<CuratedDeviceChange>(
            json!({"kind":"add","session":"secret","content":{}})
        )
        .is_err()
    );
}
