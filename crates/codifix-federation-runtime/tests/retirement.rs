// SPDX-License-Identifier: AGPL-3.0-or-later
#![allow(clippy::unwrap_used)]
use codifix_atproto_verify::{DetachedVerifier, fixtures::retired_attestation};
use codifix_federation::{
    AuthorIdentity, CanonicalJsonCodec, RecordLocator, SpaceCapability, publication::*,
};
use codifix_federation_runtime::*;
use codifix_federation_store::*;
use codifix_record::{MemoryType, Permission, PermissionSet};
use serde_json::json;
use sqlx::types::Json;
use std::{
    collections::BTreeSet,
    sync::{
        Arc,
        atomic::{AtomicUsize, Ordering},
    },
};
use uuid::Uuid;

fn instance() -> InstanceId {
    InstanceId::new(Uuid::new_v4()).unwrap()
}
fn id() -> RecordId {
    RecordId::new(Uuid::new_v4()).unwrap()
}
struct Model(AtomicUsize);
impl ExternalModel for Model {
    fn id(&self) -> &str {
        "fake"
    }
    fn answer(
        &self,
        _: ModelRequest,
    ) -> std::pin::Pin<
        Box<
            dyn std::future::Future<Output = codifix_federation_runtime::Result<String>>
                + Send
                + '_,
        >,
    > {
        self.0.fetch_add(1, Ordering::SeqCst);
        Box::pin(async { Ok("never called".into()) })
    }
}
#[tokio::test]
async fn retired_history_is_verified_offline_and_old_execution_never_reaches_egress() {
    let Some(db) = codifix_test_support::IsolatedDatabase::new().await else {
        return;
    };
    let pool = &db.pool;
    let authority = instance();
    let receiver = instance();
    let space = SpaceId::new(Uuid::new_v4()).unwrap();
    let project = Uuid::new_v4();
    let principal = Uuid::new_v4();
    sqlx::query("INSERT INTO projects(id,slug,name,root_path) VALUES($1,$2,'retirement fixture','/synthetic')").bind(project).bind(project.to_string()).execute(pool).await.unwrap();
    sqlx::query("INSERT INTO auth_principals(id,kind,display_name,global_role) VALUES($1,'internal','fixture','admin')").bind(principal).execute(pool).await.unwrap();
    let access = LocalProjectAccess {
        principal,
        project,
        permissions: PermissionSet::from_permissions(&Permission::ALL),
        read_only: false,
    };
    let mut tx = pool.begin().await.unwrap();
    let grant = LocalSpaceGrant {
        principal,
        project,
        space,
        instance: receiver,
        capabilities: vec![SpaceCapability::Pull],
    };
    set_local_grant(&mut tx, &access, &grant, true)
        .await
        .unwrap();
    tx.commit().await.unwrap();
    let stream = StreamId {
        origin: authority,
        canonical: id(),
    };
    let envelope = PublicationEnvelope {
        schema_version: 2,
        space,
        publisher: authority,
        record_id: id(),
        author: AuthorIdentity::new("original-author").unwrap(),
        authored_at: chrono::Utc::now(),
        causation: None,
        record: PublicationRecord::MemoryVersion {
            stream,
            publication: MemoryPublication {
                version: SharedVersion::new(1).unwrap(),
                source: SourceVersionRef {
                    record_id: id(),
                    version: SourceVersion::new(1).unwrap(),
                },
                content: MemoryContent {
                    text: "retained diagnosis".into(),
                    summary: "diagnosis".into(),
                    memory_type: MemoryType::Decision,
                    confidence: ConfidencePpm::try_from(800_000).unwrap(),
                    importance: 3,
                    tags: vec![],
                },
                evidence: vec![],
            },
        },
    };
    let delivery = PublicationDelivery::new(
        AuthoritySequence::new(1).unwrap(),
        RecordLocator::new("publication:original-history").unwrap(),
        envelope,
    )
    .unwrap();
    let mut receipt = AuditReceipt {
        claims: ReceiptClaims {
            purpose: "memory-publication-receipt-v1".into(),
            space,
            authority,
            sequence: 1,
            delivery_digest: audit_digest(&delivery).unwrap(),
            previous: None,
            key_generation: 1,
            policy: SpacePolicy::default(),
            transport: json!({"fixture":"original"}),
        },
        signature: vec![],
    };
    let bytes = CanonicalJsonCodec::encode(&receipt.claims).unwrap();
    let (key, sig) = retired_attestation(&bytes);
    receipt.signature = sig;
    let keys = vec![ReceiptKey {
        generation: 1,
        first_sequence: 1,
        public_key: key.clone(),
    }];
    assert!(
        DetachedVerifier::new(key)
            .unwrap()
            .verify(&bytes, &receipt.signature)
            .is_err()
    );
    assert!(
        ReceiptTrust::new(keys.clone())
            .unwrap()
            .verify_receipt(authority, &delivery, &receipt)
            .is_err()
    );
    let replica = ReplicaStore::initialize(pool.clone(), space, receiver, authority)
        .await
        .unwrap();
    replica
        .receive(authority, std::slice::from_ref(&delivery))
        .await
        .unwrap();
    // This fixture simulates existing pre-cutover verified rows, never active ingestion.
    sqlx::query("INSERT INTO federation_replica_trust(space,receiver,keys) VALUES($1,$2,$3)")
        .bind(space.as_uuid())
        .bind(receiver.as_uuid())
        .bind(Json(&keys))
        .execute(pool)
        .await
        .unwrap();
    sqlx::query("INSERT INTO federation_replica_receipts(space,receiver,sequence,receipt) VALUES($1,$2,1,$3)").bind(space.as_uuid()).bind(receiver.as_uuid()).bind(Json(&receipt)).execute(pool).await.unwrap();
    // Existing private laptop rows are retained independently of live publication.
    use codifix_federation::{device_replica::*, subscription::ProjectScope};
    let device = Uuid::new_v4();
    let subscription = Uuid::new_v4();
    let scope = ProjectScope {
        company: Uuid::new_v4(),
        area: "workshop".into(),
        owner: authority,
        project,
        space,
    };
    let view = DeviceProjectionStore::initialize(pool.clone(), subscription, device, scope.clone())
        .await
        .unwrap();
    let now = chrono::Utc::now();
    let remote_version = Uuid::new_v4();
    let version = DeviceReplicaRecord::MemoryVersion(DeviceMemoryVersion {
        id: remote_version,
        canonical_id: stream.canonical.as_uuid(),
        version: 1,
        tombstone: false,
        text: "retained device diagnosis".into(),
        summary: "diagnosis".into(),
        memory_type: MemoryType::Decision,
        scope: "project".into(),
        author: "original-author".into(),
        authored_at: now,
        updated_at: now,
        tags: vec![],
        unavailable_source_ids: vec![Uuid::new_v4()],
    });
    let canonical = DeviceReplicaRecord::CanonicalState(DeviceCanonicalState {
        canonical_id: stream.canonical.as_uuid(),
        current_version_id: remote_version,
        status: codifix_record::MemoryStatus::Active,
        confidence: ConfidencePpm::try_from(800_000).unwrap(),
        importance: 3,
        updated_at: now,
        archived_at: None,
    });
    for record in [&version, &canonical] {
        sqlx::query(
            "INSERT INTO device_replica_records(subscription,record_key,record) VALUES($1,$2,$3)",
        )
        .bind(subscription)
        .bind(record.key())
        .bind(Json(record))
        .execute(pool)
        .await
        .unwrap();
    }
    let hit_digest = view.search("retained", 5).await.unwrap()[0].digest.clone();
    let old_binding = codifix_record::LaptopContributionBinding {
        scope: codifix_record::LaptopContributionScope {
            company_id: scope.company,
            area: codifix_record::OnboardingArea::Workshop,
            owner_instance_id: authority.as_uuid(),
            project_id: project,
            space_id: space.as_uuid(),
        },
        device_id: device,
        manifest_revision: 1,
        manifest_sha256: "a".repeat(64),
    };
    let mut tx = pool.begin().await.unwrap();
    codifix_store::device_contributions::bind_project(&mut tx, project, &old_binding)
        .await
        .unwrap();
    tx.commit().await.unwrap();
    let local_memory = Uuid::new_v4();
    let owner_canonical = Uuid::new_v4();
    sqlx::query("INSERT INTO memory_entries(id,project_id,canonical_id,version_no,canonical_text,summary,memory_type,scope,importance,confidence,created_at,updated_at,search_document) VALUES($1,$2,$1,1,'preserved local','local','decision','project',3,0.8,now(),now(),to_tsvector('local'))").bind(local_memory).bind(project).execute(pool).await.unwrap();
    sqlx::query("INSERT INTO laptop_curated_origins VALUES($1,$2,7)")
        .bind(local_memory)
        .bind(owner_canonical)
        .execute(pool)
        .await
        .unwrap();
    let calls = Arc::new(AtomicUsize::new(0));
    let count = calls.clone();
    let app = axum::Router::new().fallback(axum::routing::any(move || {
        let count = count.clone();
        async move {
            count.fetch_add(1, Ordering::SeqCst);
            axum::http::StatusCode::SERVICE_UNAVAILABLE
        }
    }));
    let listener = tokio::net::TcpListener::bind("127.0.0.1:0").await.unwrap();
    let endpoint =
        Endpoint::new(&format!("http://{}", listener.local_addr().unwrap()), true).unwrap();
    let task = tokio::spawn(async move { axum::serve(listener, app).await.unwrap() });
    let client = GatewayClient::new(
        endpoint,
        "a".repeat(64),
        space,
        authority,
        ReceiptTrust::new(keys.clone()).unwrap(),
    )
    .unwrap();
    let sync = InstanceSync::new(pool.clone(), client.clone(), receiver)
        .await
        .unwrap();
    let history: String = sqlx::query_scalar("SELECT codifix_retained_history_sha256($1,$2)")
        .bind(space.as_uuid())
        .bind(receiver.as_uuid())
        .fetch_one(pool)
        .await
        .unwrap();
    let checkpoint = RetiredReplicaCheckpoint {
        receiver,
        authority,
        cursor: 1,
        history_sha256: history.clone(),
        keys: keys.clone(),
    };
    sqlx::query("INSERT INTO codifix_retired_federation_spaces(space,local_instance,manifest_sha256,replicas,manifest) VALUES($1,$2,$3,$4,'{}')").bind(space.as_uuid()).bind(receiver.as_uuid()).bind("a".repeat(64)).bind(Json(vec![checkpoint])).execute(pool).await.unwrap();
    sqlx::query("UPDATE laptop_curated_bindings SET enabled=FALSE WHERE local_project_id=$1")
        .bind(project)
        .execute(pool)
        .await
        .unwrap();
    let retained = view.search("retained", 5).await.unwrap();
    assert_eq!(retained[0].digest, hit_digest);
    assert_eq!(retained[0].version.id, remote_version);
    assert!(view.require_active().await.is_err());
    assert!(
        DeviceProjectionStore::initialize(pool.clone(), subscription, device, scope.clone())
            .await
            .is_err()
    );
    let mut tx = pool.begin().await.unwrap();
    assert!(
        codifix_store::device_contributions::bind_project(&mut tx, project, &old_binding)
            .await
            .is_err()
    );
    assert!(
        codifix_store::device_contributions::claim(&mut tx, &old_binding)
            .await
            .is_err()
    );
    tx.rollback().await.unwrap();
    let context = retired_context(pool, space, &access, &[stream])
        .await
        .unwrap();
    assert_eq!(context.records, vec![delivery.clone()]);
    assert_eq!(context.receipts, vec![receipt.clone()]);
    assert_eq!(context.citations[0].author.as_str(), "original-author");
    assert!(sync.cycle(&access).await.is_err());
    assert!(sync.catch_up().await.is_err());
    assert!(sync.gateway_status().await.is_err());
    assert!(
        InstanceSync::new(pool.clone(), client, receiver)
            .await
            .is_err()
    );
    assert!(replica.enroll_trust(&keys).await.is_err());
    assert!(
        replica
            .receive_with_receipts(authority, &[delivery], &[receipt])
            .await
            .is_err()
    );
    assert!(
        retry_source_work(pool, &access, space, receiver)
            .await
            .is_err()
    );
    let mut tx = pool.begin().await.unwrap();
    assert!(
        set_local_grant(&mut tx, &access, &grant, true)
            .await
            .is_err()
    );
    tx.rollback().await.unwrap();
    let page = DeviceReplicaPage {
        request: DeviceReplicaRequest {
            schema_version: 1,
            nonce: Uuid::new_v4(),
            device,
            scope: scope.clone(),
            catalog_revision: 1,
            after: 0,
            through: None,
            limit: 1,
        },
        through: 0,
        next: 0,
        entries: vec![],
    };
    assert!(sync.attest_device_replica(&page).await.is_err());
    let model = Model(AtomicUsize::new(0));
    assert!(
        sync.answer_shared(
            &access,
            &[stream],
            &BTreeSet::from(["fake".into()]),
            &model,
            "question"
        )
        .await
        .is_err()
    );
    assert_eq!(calls.load(Ordering::SeqCst), 0);
    assert_eq!(model.0.load(Ordering::SeqCst), 0);
    let mut denied = access.clone();
    denied.permissions = PermissionSet::EMPTY;
    assert!(
        retired_context(pool, space, &denied, &[stream])
            .await
            .is_err()
    );
    let after: String = sqlx::query_scalar("SELECT codifix_retained_history_sha256($1,$2)")
        .bind(space.as_uuid())
        .bind(receiver.as_uuid())
        .fetch_one(pool)
        .await
        .unwrap();
    assert_eq!(after, history);
    // A fresh space starts empty and only explicitly enrolled work is accepted.
    let gateway = GatewayStore::new(pool.clone());
    let new_space = SpaceId::new(Uuid::new_v4()).unwrap();
    let fence = gateway.initialize(new_space, authority).await.unwrap();
    let intent = PublicationIntent {
        locator: RecordLocator::new("publication:new-enrolment").unwrap(),
        envelope: PublicationEnvelope {
            schema_version: 2,
            space: new_space,
            publisher: authority,
            record_id: id(),
            author: AuthorIdentity::new("operator").unwrap(),
            authored_at: chrono::Utc::now(),
            causation: None,
            record: PublicationRecord::Membership {
                member: receiver,
                capability: SpaceCapability::Pull,
            },
        },
    };
    assert_eq!(
        gateway
            .accept(fence, authority, &intent)
            .await
            .unwrap()
            .outcome,
        PublicationOutcome::Applied
    );
    assert_eq!(
        gateway
            .accept(fence, authority, &intent)
            .await
            .unwrap()
            .outcome,
        PublicationOutcome::Duplicate
    );
    let mut new_binding = old_binding.clone();
    new_binding.scope.space_id = new_space.as_uuid();
    new_binding.manifest_revision = 2;
    new_binding.manifest_sha256 = "b".repeat(64);
    let mut tx = pool.begin().await.unwrap();
    for _ in 0..2 {
        codifix_store::device_contributions::rebind_retired_project(
            &mut tx,
            project,
            &old_binding,
            &new_binding,
            &"c".repeat(64),
        )
        .await
        .unwrap();
    }
    tx.commit().await.unwrap();
    let origin:(Uuid,i32)=sqlx::query_as("SELECT owner_canonical_id,owner_version FROM laptop_curated_origins WHERE local_canonical_id=$1").bind(local_memory).fetch_one(pool).await.unwrap();
    assert_eq!(origin, (owner_canonical, 7));
    let (frozen, status): (Json<codifix_record::LaptopContributionBinding>, String) =
        sqlx::query_as("SELECT binding,state FROM laptop_curated_outbox WHERE local_memory_id=$1")
            .bind(local_memory)
            .fetch_one(pool)
            .await
            .unwrap();
    assert_eq!(frozen.0, old_binding);
    assert_eq!(status, "queued");
    let mut next_scope = scope.clone();
    next_scope.space = new_space;
    let new_view =
        DeviceProjectionStore::initialize(pool.clone(), Uuid::new_v4(), device, next_scope)
            .await
            .unwrap();
    assert!(new_view.search("retained", 5).await.unwrap().is_empty());
    assert_eq!(
        view.search("retained", 5).await.unwrap()[0].digest,
        hit_digest
    );
    // Even a changed pinned key schedule fails before returning historical bytes.
    sqlx::query("UPDATE federation_replica_trust SET keys='[]' WHERE space=$1 AND receiver=$2")
        .bind(space.as_uuid())
        .bind(receiver.as_uuid())
        .execute(pool)
        .await
        .unwrap();
    assert!(
        retired_context(pool, space, &access, &[stream])
            .await
            .is_err()
    );
    task.abort();
    db.cleanup().await.unwrap();
}
