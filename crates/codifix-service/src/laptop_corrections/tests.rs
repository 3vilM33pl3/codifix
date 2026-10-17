use super::*;
use codifix_config::{LaptopCacheConfig, LaptopCacheSubscription};
use codifix_federation::{
    device_replica::*,
    publication::{ConfidencePpm, InstanceId, SpaceId},
};
use codifix_federation_store::DeviceProjectionStore;
use serde_json::json;

#[tokio::test]
async fn staged_downloaded_corrections_are_scoped_replayable_and_atomic() {
    let Some(mut f) = crate::auth::device::tests::fixture().await else {
        return;
    };
    let selected = LaptopCacheSubscription {
        subscription_id: Uuid::new_v4(),
        device_id: Uuid::new_v4(),
        company_id: Uuid::new_v4(),
        owner_instance_id: Uuid::new_v4(),
        project_id: f.project,
        space_id: Uuid::new_v4(),
        replication_mode: LaptopCacheReplicationMode::CuratedProjectV1,
    };
    let config = &mut f.state.config;
    config.auth.mode = AuthMode::SingleUser;
    config.service.read_only = false;
    config.service.bind_addr = "127.0.0.1:55440".into();
    config.database.url = "postgresql://synthetic@127.0.0.1:55432/memory_laptop_workshop".into();
    config.llm.model.clear();
    config.background_work.internal_automation = false;
    config.provenance.reverify_enabled = false;
    config.procedural.enabled = false;
    config.reinforcement.validation_enabled = false;
    config.mcp.enabled = false;
    config.mcp.http_enabled = false;
    config.laptop_cache = Some(LaptopCacheConfig {
        subscriptions: vec![selected.clone()],
    });
    config.validate_laptop_cache().unwrap();
    let actor = crate::auth::resolve_principal(
        &f.state,
        crate::auth::Credential::ApiToken("synthetic-owner".into()),
    )
    .await
    .unwrap()
    .unwrap();
    let scope = ProjectScope {
        company: selected.company_id,
        area: "workshop".into(),
        owner: InstanceId::new(selected.owner_instance_id).unwrap(),
        project: f.project,
        space: SpaceId::new(selected.space_id).unwrap(),
    };
    let _ = DeviceProjectionStore::initialize(
        f.db.pool.clone(),
        selected.subscription_id,
        selected.device_id,
        scope,
    )
    .await
    .unwrap();
    let binding = LaptopContributionBinding {
        scope: LaptopContributionScope {
            company_id: selected.company_id,
            area: OnboardingArea::Workshop,
            owner_instance_id: selected.owner_instance_id,
            project_id: f.project,
            space_id: selected.space_id,
        },
        device_id: selected.device_id,
        manifest_revision: 1,
        manifest_sha256: "a".repeat(64),
    };
    let mut tx = f.db.pool.begin().await.unwrap();
    codifix_store::device_contributions::bind_project(&mut tx, f.project, &binding)
        .await
        .unwrap();
    tx.commit().await.unwrap();
    let id = Uuid::new_v4();
    let canonical = Uuid::new_v4();
    let now = chrono::Utc::now();
    let record = DeviceReplicaRecord::MemoryVersion(DeviceMemoryVersion {
        id,
        canonical_id: canonical,
        version: 4,
        tombstone: false,
        text: "original owner fact".into(),
        summary: "Original".into(),
        memory_type: MemoryType::Implementation,
        scope: "project".into(),
        author: "original-author".into(),
        authored_at: now,
        updated_at: now,
        tags: vec![],
        unavailable_source_ids: vec![],
    });
    let current = DeviceReplicaRecord::CanonicalState(DeviceCanonicalState {
        canonical_id: canonical,
        current_version_id: id,
        status: MemoryStatus::Active,
        confidence: ConfidencePpm::try_from(800000).unwrap(),
        importance: 3,
        updated_at: now,
        archived_at: None,
    });
    for item in [&record, &current] {
        sqlx::query(
            "INSERT INTO device_replica_records(subscription,record_key,record) VALUES($1,$2,$3)",
        )
        .bind(selected.subscription_id)
        .bind(item.key())
        .bind(json!(item))
        .execute(&f.db.pool)
        .await
        .unwrap();
    }
    let request = LaptopCorrectionRequest {
        operation_id: Uuid::new_v4(),
        source_memory_id: id,
        expected_digest: CanonicalJsonCodec::digest(&record).unwrap().as_hex().into(),
        content: CuratedDeviceContent {
            canonical_text: "corrected owner fact".into(),
            summary: "Correction".into(),
            memory_type: MemoryType::Implementation,
            importance: 3,
            confidence: 0.9,
        },
    };
    let call = |subscription, r| {
        stage(
            State(f.state.clone()),
            Extension(actor.clone()),
            ConnectInfo("127.0.0.1:40000".parse().unwrap()),
            Path(subscription),
            Json(r),
        )
    };
    let (first, duplicate) = tokio::join!(
        call(selected.subscription_id, request.clone()),
        call(selected.subscription_id, request.clone())
    );
    let receipt = first.unwrap().0;
    assert_eq!(
        receipt,
        duplicate.unwrap().0,
        "competing exact staging retries are harmless"
    );
    assert_eq!(
        receipt,
        call(selected.subscription_id, request.clone())
            .await
            .unwrap()
            .0
    );
    assert_eq!(receipt.owner_canonical_id, canonical);
    assert_eq!(receipt.expected_version, 4);
    let mut tx = f.db.pool.begin().await.unwrap();
    let claimed = codifix_store::device_contributions::claim(&mut tx, &binding)
        .await
        .unwrap()
        .unwrap();
    tx.commit().await.unwrap();
    assert!(
        matches!(claimed.contribution.change,CuratedDeviceChange::Correct {target_canonical_id,expected_version:4,..} if target_canonical_id==canonical)
    );
    assert_eq!(claimed.contribution.operation_id, receipt.local_memory_id);
    let mut altered = request.clone();
    altered.content.summary = "altered request".into();
    assert_eq!(
        call(selected.subscription_id, altered)
            .await
            .unwrap_err()
            .status,
        StatusCode::CONFLICT
    );
    assert_eq!(
        call(Uuid::new_v4(), request.clone())
            .await
            .unwrap_err()
            .status,
        StatusCode::FORBIDDEN
    );
    sqlx::query("UPDATE device_replica_records SET record=jsonb_set(record,'{value,current_version_id}',$3) WHERE subscription=$1 AND record_key=$2").bind(selected.subscription_id).bind(current.key()).bind(json!(Uuid::new_v4())).execute(&f.db.pool).await.unwrap();
    let mut stale = request.clone();
    stale.operation_id = Uuid::new_v4();
    assert_eq!(
        call(selected.subscription_id, stale)
            .await
            .unwrap_err()
            .status,
        StatusCode::CONFLICT
    );
    assert_eq!(
        receipt,
        call(selected.subscription_id, request.clone())
            .await
            .unwrap()
            .0,
        "exact replay remains historical even when owner advances"
    );
    sqlx::query(
        "UPDATE device_replica_records SET record=$3 WHERE subscription=$1 AND record_key=$2",
    )
    .bind(selected.subscription_id)
    .bind(current.key())
    .bind(json!(current))
    .execute(&f.db.pool)
    .await
    .unwrap();
    sqlx::query("CREATE FUNCTION fail_staging_event() RETURNS trigger LANGUAGE plpgsql AS $$ BEGIN RAISE EXCEPTION 'synthetic event unavailable'; END $$").execute(&f.db.pool).await.unwrap();
    sqlx::query("CREATE TRIGGER fail_staging_event BEFORE INSERT ON project_timeline_events FOR EACH ROW EXECUTE FUNCTION fail_staging_event()").execute(&f.db.pool).await.unwrap();
    let mut retry = request.clone();
    retry.operation_id = Uuid::new_v4();
    assert!(call(selected.subscription_id, retry.clone()).await.is_err());
    let failed = Uuid::new_v5(&retry.operation_id, b"local-correction-v1");
    assert!(
        !sqlx::query_scalar::<_, bool>("SELECT EXISTS(SELECT 1 FROM memory_entries WHERE id=$1)")
            .bind(failed)
            .fetch_one(&f.db.pool)
            .await
            .unwrap()
    );
    assert!(
        !sqlx::query_scalar::<_, bool>(
            "SELECT EXISTS(SELECT 1 FROM laptop_curated_outbox WHERE operation_id=$1)"
        )
        .bind(failed)
        .fetch_one(&f.db.pool)
        .await
        .unwrap()
    );
    assert_eq!(
        record,
        sqlx::query_scalar::<_, SqlJson<DeviceReplicaRecord>>(
            "SELECT record FROM device_replica_records WHERE subscription=$1 AND record_key=$2"
        )
        .bind(selected.subscription_id)
        .bind(record.key())
        .fetch_one(&f.db.pool)
        .await
        .unwrap()
        .0,
        "downloaded history remains unchanged"
    );
    f.db.cleanup().await.unwrap();
}
