// SPDX-License-Identifier: AGPL-3.0-or-later
use super::*;
use mem_config::{LaptopCacheConfig, LaptopCacheSubscription};
use mem_federation::{
    AuthorIdentity, CanonicalJsonCodec, RecordLocator, publication::*,
    subscription::ProjectStreamReview,
};
use serde_json::{Value, json};

async fn fixture() -> Option<(
    crate::auth::device::tests::Fixture,
    LaptopCacheSubscription,
    AuthenticatedPrincipal,
)> {
    let mut f = crate::auth::device::tests::fixture().await?;
    let binding = LaptopCacheSubscription {
        replication_mode: Default::default(),
        subscription_id: Uuid::new_v4(),
        device_id: Uuid::new_v4(),
        company_id: Uuid::new_v4(),
        owner_instance_id: Uuid::new_v4(),
        project_id: f.project,
        space_id: Uuid::new_v4(),
    };
    f.state.config.auth.mode = AuthMode::SingleUser;
    f.state.config.service.read_only = true;
    f.state.config.service.bind_addr = "127.0.0.1:55440".into();
    f.state.config.llm.model.clear();
    f.state.config.background_work.internal_automation = false;
    f.state.config.provenance.reverify_enabled = false;
    f.state.config.procedural.enabled = false;
    f.state.config.reinforcement.validation_enabled = false;
    f.state.config.mcp.enabled = false;
    f.state.config.mcp.http_enabled = false;
    // The disposable fixture owns the actual pool. This configuration-only URL
    // exercises the guarded native target shape without provisioning a cluster.
    f.state.config.database.url =
        "postgresql://synthetic@127.0.0.1:55432/memory_laptop_workshop".into();
    f.state.config.laptop_cache = Some(LaptopCacheConfig {
        subscriptions: vec![binding.clone()],
    });
    f.state.config.validate_laptop_cache().unwrap();
    let actor = crate::auth::resolve_principal(
        &f.state,
        crate::auth::Credential::ApiToken("synthetic-owner".into()),
    )
    .await
    .unwrap()
    .unwrap();
    Some((f, binding, actor))
}
fn peer() -> ConnectInfo<SocketAddr> {
    ConnectInfo("127.0.0.1:40000".parse().unwrap())
}
#[tokio::test]
async fn curated_replica_search_retains_source_identity_without_inventing_publication_metadata() {
    let Some((mut f, mut binding, actor)) = fixture().await else {
        return;
    };
    binding.replication_mode = LaptopCacheReplicationMode::CuratedProjectV1;
    f.state.config.laptop_cache.as_mut().unwrap().subscriptions[0] = binding.clone();
    f.state.config.service.read_only = false;
    let store = DeviceProjectionStore::initialize(
        f.state.pool().unwrap(),
        binding.subscription_id,
        binding.device_id,
        scope("workshop", &binding).unwrap(),
    )
    .await
    .unwrap();
    let now = chrono::Utc::now();
    let id = Uuid::new_v4();
    let canonical = Uuid::new_v4();
    let source = Uuid::new_v4();
    use mem_federation::device_replica::*;
    let records = [
        DeviceReplicaRecord::MemoryVersion(DeviceMemoryVersion {
            id,
            canonical_id: canonical,
            version: 3,
            tombstone: false,
            text: "diagnosis curated replica text".into(),
            summary: "curated summary".into(),
            memory_type: MemoryType::DomainFact,
            scope: "project".into(),
            author: "original author".into(),
            authored_at: now,
            updated_at: now,
            tags: vec![],
            unavailable_source_ids: vec![source],
        }),
        DeviceReplicaRecord::CanonicalState(DeviceCanonicalState {
            canonical_id: canonical,
            current_version_id: id,
            status: MemoryStatus::Active,
            confidence: ConfidencePpm::try_from(800_000).unwrap(),
            importance: 1,
            updated_at: now,
            archived_at: None,
        }),
    ];
    for record in records {
        sqlx::query(
            "INSERT INTO device_replica_records(subscription,record_key,record) VALUES($1,$2,$3)",
        )
        .bind(binding.subscription_id)
        .bind(record.key())
        .bind(sqlx::types::Json(record))
        .execute(&f.state.pool().unwrap())
        .await
        .unwrap();
    }
    assert_eq!(store.status().await.unwrap().cursor, 0);
    let result = body(
        search(
            State(f.state.clone()),
            Extension(actor.clone()),
            peer(),
            Path(binding.subscription_id),
            Json(LaptopCacheSearchRequest {
                question: "diagnosis".into(),
                limit: 8,
            }),
        )
        .await
        .unwrap(),
    )
    .await;
    let hit = &result["hits"][0];
    assert_eq!(hit["source_record_id"], id.to_string());
    assert_eq!(hit["stream_id"], canonical.to_string());
    assert_eq!(hit["source_version"], 3);
    assert_eq!(hit["origin"], "curated_project");
    assert_eq!(hit["unavailable_source_ids"][0], source.to_string());
    assert!(hit.get("locator").is_none());
    assert!(hit.get("shared_version").is_none());
    for (version, tombstone) in [(1, false), (2, true)] {
        let record = DeviceReplicaRecord::MemoryVersion(DeviceMemoryVersion {
            id: Uuid::new_v4(),
            canonical_id: canonical,
            version,
            tombstone,
            text: format!("historical version {version}"),
            summary: "owner history".into(),
            memory_type: MemoryType::DomainFact,
            scope: "project".into(),
            author: "original author".into(),
            authored_at: now,
            updated_at: now,
            tags: vec![],
            unavailable_source_ids: vec![source],
        });
        sqlx::query(
            "INSERT INTO device_replica_records(subscription,record_key,record) VALUES($1,$2,$3)",
        )
        .bind(binding.subscription_id)
        .bind(record.key())
        .bind(sqlx::types::Json(record))
        .execute(&f.db.pool)
        .await
        .unwrap();
    }
    let first = body(
        history(
            State(f.state.clone()),
            Extension(actor.clone()),
            peer(),
            Path((binding.subscription_id, canonical)),
            Query(LaptopCacheHistoryRequest {
                after_version: 0,
                limit: 1,
            }),
        )
        .await
        .unwrap(),
    )
    .await;
    assert_eq!(first["versions"].as_array().unwrap().len(), 1);
    assert_eq!(first["versions"][0]["version"], 1);
    assert_eq!(first["next_after_version"], 1);
    let second = body(
        history(
            State(f.state.clone()),
            Extension(actor.clone()),
            peer(),
            Path((binding.subscription_id, canonical)),
            Query(LaptopCacheHistoryRequest {
                after_version: 1,
                limit: 2,
            }),
        )
        .await
        .unwrap(),
    )
    .await;
    assert_eq!(second["versions"][0]["tombstone"], true);
    assert_eq!(second["versions"][1]["id"], id.to_string());
    assert_eq!(
        second["versions"][1]["unavailable_source_ids"][0],
        source.to_string()
    );
    assert_eq!(second["next_after_version"], Value::Null);
    assert_eq!(second["read_only"], true);
    assert!(
        history(
            State(f.state.clone()),
            Extension(actor.clone()),
            peer(),
            Path((Uuid::new_v4(), canonical)),
            Query(LaptopCacheHistoryRequest {
                after_version: 0,
                limit: 16
            })
        )
        .await
        .is_err()
    );
    assert!(
        history(
            State(f.state.clone()),
            Extension(actor.clone()),
            ConnectInfo("10.0.0.1:40000".parse().unwrap()),
            Path((binding.subscription_id, canonical)),
            Query(LaptopCacheHistoryRequest {
                after_version: 0,
                limit: 16
            })
        )
        .await
        .is_err()
    );
    let local = Uuid::new_v4();
    sqlx::query("INSERT INTO memory_entries(id,project_id,canonical_id,version_no,canonical_text,summary,memory_type,scope,importance,confidence,created_at,updated_at,search_document) VALUES($1,$2,$1,1,'diagnosis curated replica text','local fact','domain_fact','project',3,0.8,now(),now(),to_tsvector('english','diagnosis curated replica text'))").bind(local).bind(f.project).execute(&f.db.pool).await.unwrap();
    let mut tx = f.db.pool.begin().await.unwrap();
    mem_curate::rebuild_memory_chunks_tx(&mut tx, local)
        .await
        .unwrap();
    tx.commit().await.unwrap();
    let request: QueryRequest =
        serde_json::from_value(json!({"project":"local","query":"diagnosis","top_k":8})).unwrap();
    let answer = crate::laptop_query::query(&f.state, &actor, peer().0, &request)
        .await
        .unwrap();
    assert!(
        answer
            .results
            .iter()
            .any(|r| r.memory_id == id && r.origin.as_ref().is_some_and(|o| o.read_only))
    );
    assert!(answer.results.iter().any(|r| r.memory_id == local));
    sqlx::query("INSERT INTO laptop_curated_outbox(operation_id,local_project_id,local_memory_id,binding,state,receipt) VALUES($1,$2,$1,$3,'accepted',$4)").bind(local).bind(f.project).bind(json!({"device_id":binding.device_id,"scope":{"owner_instance_id":binding.owner_instance_id}})).bind(json!({"owner_memory_id":id})).execute(&f.db.pool).await.unwrap();
    let answer = crate::laptop_query::query(&f.state, &actor, peer().0, &request)
        .await
        .unwrap();
    assert_eq!(answer.results.len(), 1);
    assert_eq!(answer.results[0].memory_id, id);
    assert_eq!(answer.answer_citations[0].memory_id, id);
    sqlx::query("DELETE FROM device_replica_records WHERE subscription=$1 AND record_key=$2")
        .bind(binding.subscription_id)
        .bind(format!("version:{id}"))
        .execute(&f.db.pool)
        .await
        .unwrap();
    let answer = crate::laptop_query::query(&f.state, &actor, peer().0, &request)
        .await
        .unwrap();
    assert!(answer.results.iter().any(|r| r.memory_id == local));
    let mut filtered = request.clone();
    filtered.filters.types.push(MemoryType::Incident);
    assert!(
        crate::laptop_query::query(&f.state, &actor, peer().0, &filtered)
            .await
            .is_err()
    );
    sqlx::query("UPDATE laptop_curated_outbox SET receipt=receipt || $2 WHERE operation_id=$1")
        .bind(local)
        .bind(json!({"owner_canonical_id":canonical}))
        .execute(&f.db.pool)
        .await
        .unwrap();
    sqlx::query("UPDATE device_replica_records SET record=jsonb_set(record,'{value,status}','\"archived\"'::jsonb) WHERE subscription=$1 AND record_key=$2").bind(binding.subscription_id).bind(format!("state:{canonical}")).execute(&f.db.pool).await.unwrap();
    let answer = crate::laptop_query::query(&f.state, &actor, peer().0, &request)
        .await
        .unwrap();
    assert!(
        answer.results.is_empty(),
        "withdrawn accepted source must not reappear as local knowledge"
    );
    // A tombstone is a content version; canonical state may remain active.
    // After pruning the accepted source, that newer downloaded version wins.
    let tombstone_id = Uuid::new_v4();
    let tombstone = DeviceReplicaRecord::MemoryVersion(DeviceMemoryVersion {
        id: tombstone_id,
        canonical_id: canonical,
        version: 4,
        tombstone: true,
        text: String::new(),
        summary: String::new(),
        memory_type: MemoryType::DomainFact,
        scope: "project".into(),
        author: "original-author".into(),
        authored_at: now,
        updated_at: now,
        tags: vec![],
        unavailable_source_ids: vec![],
    });
    sqlx::query(
        "INSERT INTO device_replica_records(subscription,record_key,record) VALUES($1,$2,$3)",
    )
    .bind(binding.subscription_id)
    .bind(tombstone.key())
    .bind(json!(tombstone))
    .execute(&f.db.pool)
    .await
    .unwrap();
    sqlx::query("UPDATE device_replica_records SET record=jsonb_set(jsonb_set(record,'{value,status}','\"active\"'::jsonb),'{value,current_version_id}',$3) WHERE subscription=$1 AND record_key=$2").bind(binding.subscription_id).bind(format!("state:{canonical}")).bind(json!(tombstone_id)).execute(&f.db.pool).await.unwrap();
    let answer = crate::laptop_query::query(&f.state, &actor, peer().0, &request)
        .await
        .unwrap();
    assert!(
        answer.results.is_empty(),
        "downloaded current tombstone must suppress the pruned accepted local copy"
    );
    // The projector retains this metadata when the owner purges tombstoned rows.
    sqlx::query("INSERT INTO device_replica_owner_watermarks(subscription,canonical_id,version) VALUES($1,$2,4)").bind(binding.subscription_id).bind(canonical).execute(&f.db.pool).await.unwrap();
    sqlx::query("UPDATE laptop_curated_outbox SET receipt=receipt || $2 WHERE operation_id=$1")
        .bind(local)
        .bind(json!({"owner_version":3}))
        .execute(&f.db.pool)
        .await
        .unwrap();
    sqlx::query("DELETE FROM device_replica_records WHERE subscription=$1")
        .bind(binding.subscription_id)
        .execute(&f.db.pool)
        .await
        .unwrap();
    let answer = crate::laptop_query::query(&f.state, &actor, peer().0, &request)
        .await
        .unwrap();
    assert!(
        answer.results.is_empty(),
        "retention must not resurrect an acknowledged local copy"
    );
}
#[tokio::test]
async fn curated_memories_browse_merges_scoped_owner_rows_without_echoes() {
    let Some((mut f, mut binding, actor)) = fixture().await else {
        return;
    };
    binding.replication_mode = LaptopCacheReplicationMode::CuratedProjectV1;
    f.state.config.laptop_cache.as_mut().unwrap().subscriptions[0] = binding.clone();
    f.state.config.service.read_only = false;
    let store = DeviceProjectionStore::initialize(
        f.state.pool().unwrap(),
        binding.subscription_id,
        binding.device_id,
        scope("workshop", &binding).unwrap(),
    )
    .await
    .unwrap();
    assert!(store.browse().await.unwrap().is_empty());
    let now = chrono::Utc::now();
    let id = Uuid::new_v4();
    let canonical = Uuid::new_v4();
    let source = Uuid::new_v4();
    use mem_federation::device_replica::*;
    let records = [
        DeviceReplicaRecord::MemoryVersion(DeviceMemoryVersion {
            id,
            canonical_id: canonical,
            version: 3,
            tombstone: false,
            text: "diagnosis curated replica text".into(),
            summary: "curated summary".into(),
            memory_type: MemoryType::DomainFact,
            scope: "project".into(),
            author: "original author".into(),
            authored_at: now,
            updated_at: now,
            tags: vec![],
            unavailable_source_ids: vec![source],
        }),
        DeviceReplicaRecord::CanonicalState(DeviceCanonicalState {
            canonical_id: canonical,
            current_version_id: id,
            status: MemoryStatus::Active,
            confidence: ConfidencePpm::try_from(800_000).unwrap(),
            importance: 1,
            updated_at: now,
            archived_at: None,
        }),
    ];
    for record in records {
        sqlx::query(
            "INSERT INTO device_replica_records(subscription,record_key,record) VALUES($1,$2,$3)",
        )
        .bind(binding.subscription_id)
        .bind(record.key())
        .bind(sqlx::types::Json(record))
        .execute(&f.state.pool().unwrap())
        .await
        .unwrap();
    }
    // The main list is a local-human browse adapter, not a changed repository
    // helper. Exact-text twins stay distinct; accepted device echoes do not.
    let local = Uuid::new_v4();
    let twin = Uuid::new_v4();
    for local_id in [local, twin] {
        sqlx::query("INSERT INTO memory_entries(id,project_id,canonical_text,summary,memory_type,scope,importance,confidence) VALUES($1,$2,'diagnosis curated replica text','curated summary','domain_fact','project',1,0.8)")
            .bind(local_id).bind(f.project).execute(&f.db.pool).await.unwrap();
    }
    let browse = |status, limit, offset| {
        crate::laptop_memories::list(&f.state, &actor, peer().0, "local", status, limit, offset)
    };
    let all = browse(None, 10, 0).await.unwrap();
    assert_eq!(
        crate::laptop_cache::response(&all).unwrap().headers()[header::CACHE_CONTROL],
        "no-store"
    );
    assert_eq!(all.memories.total, 3);
    let remote = all
        .memories
        .items
        .iter()
        .find(|r| r.origin.is_some())
        .unwrap();
    assert_eq!(remote.id, id);
    assert_eq!(remote.version_no, 3);
    assert!(remote.origin.as_ref().unwrap().read_only);
    assert_eq!(remote.origin.as_ref().unwrap().author, "original author");
    assert_eq!(remote.source_count, 1);
    let first_page = browse(None, 1, 0).await.unwrap();
    let second_page = browse(None, 1, 1).await.unwrap();
    assert_eq!(first_page.memories.total, 3);
    assert_eq!(first_page.browse_revision, second_page.browse_revision);
    assert_ne!(
        first_page.memories.items[0].id,
        second_page.memories.items[0].id
    );
    let outbox_binding = json!({"device_id":binding.device_id,"scope":{"owner_instance_id":binding.owner_instance_id}});
    sqlx::query("INSERT INTO laptop_curated_outbox(operation_id,local_project_id,local_memory_id,binding,state,receipt) VALUES($1,$2,$1,$3,'accepted',$4)")
        .bind(local).bind(f.project).bind(outbox_binding)
        .bind(json!({"owner_canonical_id":canonical,"owner_memory_id":id,"owner_version":3}))
        .execute(&f.db.pool).await.unwrap();
    let accepted_view = browse(None, 10, 0).await.unwrap();
    assert_eq!(accepted_view.memories.total, 2);
    assert_ne!(first_page.browse_revision, accepted_view.browse_revision);
    sqlx::query("UPDATE laptop_curated_outbox SET state='conflict' WHERE operation_id=$1")
        .bind(local)
        .execute(&f.db.pool)
        .await
        .unwrap();
    assert_eq!(browse(None, 10, 0).await.unwrap().memories.total, 3);
    sqlx::query("UPDATE laptop_curated_outbox SET state='accepted' WHERE operation_id=$1")
        .bind(local)
        .execute(&f.db.pool)
        .await
        .unwrap();
    assert_eq!(
        crate::repository::fetch_project_memories(&f.db.pool, "local", None, 10, 0)
            .await
            .unwrap()
            .total,
        2
    );
    assert!(
        crate::laptop_memories::list(
            &f.state,
            &actor,
            "10.0.0.1:40000".parse().unwrap(),
            "local",
            None,
            10,
            0
        )
        .await
        .is_err()
    );
    assert_eq!(
        crate::laptop_memories::list(&f.state, &actor, peer().0, "other", None, 10, 0)
            .await
            .unwrap()
            .memories
            .total,
        0
    );
    sqlx::query("UPDATE device_replica_records SET record=jsonb_set(record,'{value,status}','\"archived\"') WHERE subscription=$1 AND record_key=$2")
        .bind(binding.subscription_id).bind(format!("state:{canonical}")).execute(&f.db.pool).await.unwrap();
    assert_eq!(
        browse(Some("active"), 10, 0).await.unwrap().memories.total,
        1
    );
    assert_eq!(
        browse(Some("archived"), 10, 0)
            .await
            .unwrap()
            .memories
            .total,
        1
    );
    sqlx::query("INSERT INTO device_replica_owner_watermarks(subscription,canonical_id,version,inactive) VALUES($1,$2,3,TRUE)")
        .bind(binding.subscription_id).bind(canonical).execute(&f.db.pool).await.unwrap();
    sqlx::query("UPDATE device_replica_records SET record=jsonb_set(record,'{value,tombstone}','true') WHERE subscription=$1 AND record_key=$2")
        .bind(binding.subscription_id).bind(format!("version:{id}")).execute(&f.db.pool).await.unwrap();
    assert_eq!(browse(None, 10, 0).await.unwrap().memories.total, 1);
    sqlx::query("DELETE FROM device_replica_records WHERE subscription=$1")
        .bind(binding.subscription_id)
        .execute(&f.db.pool)
        .await
        .unwrap();
    assert_eq!(
        browse(None, 10, 0).await.unwrap().memories.total,
        1,
        "accepted local copy must not resurrect after owner pruning"
    );
}

async fn body(response: Response) -> Value {
    assert_eq!(response.headers()[header::CACHE_CONTROL], "no-store");
    serde_json::from_slice(
        &axum::body::to_bytes(response.into_body(), 256 * 1024)
            .await
            .unwrap(),
    )
    .unwrap()
}
async fn seed(
    state: &AppState,
    binding: &LaptopCacheSubscription,
    text: &str,
) -> (Uuid, Vec<PublicationDelivery>) {
    let scope = scope("workshop", binding).unwrap();
    let store = ProjectProjectionStore::initialize(
        state.pool().unwrap(),
        binding.subscription_id,
        binding.device_id,
        scope.clone(),
    )
    .await
    .unwrap();
    assert_eq!(store.cursor().await.unwrap(), 0);
    let stream = StreamId {
        origin: scope.owner,
        canonical: RecordId::new(Uuid::new_v4()).unwrap(),
    };
    let snapshot = EvidenceSnapshot {
        space: scope.space,
        stream,
        public_source_label: "Reviewed runbook excerpt".into(),
        selected_text: "approved evidence bytes".into(),
        observed_at: chrono::Utc::now(),
    };
    let evidence = ReviewedEvidence::new(
        snapshot.clone(),
        ReviewReceipt {
            snapshot_digest: CanonicalJsonCodec::digest(&snapshot).unwrap(),
            reviewer: AuthorIdentity::new("reviewed-expert").unwrap(),
            reviewed_at: chrono::Utc::now(),
        },
    )
    .unwrap();
    let mut records: Vec<PublicationDelivery> = Vec::new();
    let confidence = ConfidencePpm::try_from(800_000).unwrap();
    for (index, record) in [
        PublicationRecord::Evidence(evidence),
        PublicationRecord::MemoryVersion {
            stream,
            publication: MemoryPublication {
                version: SharedVersion::new(2).unwrap(),
                source: SourceVersionRef {
                    record_id: RecordId::new(Uuid::new_v4()).unwrap(),
                    version: SourceVersion::new(7).unwrap(),
                },
                content: MemoryContent {
                    text: text.into(),
                    summary: "approved offline summary".into(),
                    memory_type: MemoryType::DomainFact,
                    confidence,
                    importance: 1,
                    tags: vec![],
                },
                evidence: vec![],
            },
        },
        PublicationRecord::CanonicalState {
            stream,
            state: StatePublication {
                revision: StateRevision::new(3).unwrap(),
                content_version: SharedVersion::new(2).unwrap(),
                status: MemoryStatus::Active,
                confidence,
                tombstone: false,
            },
        },
    ]
    .into_iter()
    .enumerate()
    {
        let mut record = record;
        if let PublicationRecord::MemoryVersion { publication, .. } = &mut record {
            publication.evidence = vec![records[0].envelope.key()];
        }
        records.push(
            PublicationDelivery::new(
                AuthoritySequence::new(index as u32 + 1).unwrap(),
                RecordLocator::new(format!("offline-record:{index}")).unwrap(),
                PublicationEnvelope {
                    schema_version: 2,
                    space: scope.space,
                    publisher: scope.owner,
                    record_id: RecordId::new(Uuid::new_v4()).unwrap(),
                    author: AuthorIdentity::new("original-author").unwrap(),
                    authored_at: chrono::Utc::now(),
                    causation: None,
                    record,
                },
            )
            .unwrap(),
        );
    }
    // Tests seed the already verified local projection, not an untrusted
    // network ingestion path. Existing signature/apply tests exercise that path.
    let projection = json!({"records":records,"reviews":[ProjectStreamReview {stream,review_id:Uuid::new_v4(),records:records.iter().map(|r|r.envelope.key()).collect(),enabled:true}]});
    sqlx::query("UPDATE onboarding_project_projections SET projection=$2,cursor=3,observed_at=clock_timestamp() WHERE subscription=$1").bind(binding.subscription_id).bind(projection).execute(&state.pool().unwrap()).await.unwrap();
    (stream.canonical.as_uuid(), records)
}
#[tokio::test]
async fn selected_local_cache_is_read_only_cited_and_current_snapshot_bounded() {
    let Some((f, binding, actor)) = fixture().await else {
        return;
    };
    let missing = body(
        list(State(f.state.clone()), Extension(actor.clone()), peer())
            .await
            .unwrap(),
    )
    .await;
    assert_eq!(missing["projects"][0]["initialized"], false);
    let (stream, records) = seed(&f.state, &binding, "diagnosis selected approved text").await;
    let mut other = binding.clone();
    other.subscription_id = Uuid::new_v4();
    other.owner_instance_id = Uuid::new_v4();
    seed(&f.state, &other, "diagnosis OTHER-AREA-SECRET").await;
    let cache = body(
        list(State(f.state.clone()), Extension(actor.clone()), peer())
            .await
            .unwrap(),
    )
    .await;
    assert_eq!(cache["projects"].as_array().unwrap().len(), 1);
    let result = body(
        search(
            State(f.state.clone()),
            Extension(actor.clone()),
            peer(),
            Path(binding.subscription_id),
            Json(LaptopCacheSearchRequest {
                question: "diagnosis".into(),
                limit: 8,
            }),
        )
        .await
        .unwrap(),
    )
    .await;
    assert_eq!(result["project"]["cursor"], 3);
    assert_eq!(
        result["project"]["observed_at"],
        cache["projects"][0]["observed_at"]
    );
    assert_eq!(result["hits"][0]["stream_id"], json!(stream));
    assert_eq!(result["hits"][0]["author"], "original-author");
    assert_eq!(result["hits"][0]["source_version"], 7);
    assert_eq!(result["hits"][0]["shared_version"], 2);
    assert_eq!(result["hits"][0]["digest"], records[1].digest.as_hex());
    assert_eq!(
        result["hits"][0]["evidence"][0]["text"],
        "approved evidence bytes"
    );
    assert!(result["hits"][0]["read_only"].as_bool().unwrap());
    assert!(!result.to_string().contains("OTHER-AREA-SECRET"));
    assert_eq!(
        sqlx::query_scalar::<_, i64>("SELECT COUNT(*) FROM onboarding_project_projections")
            .fetch_one(&f.db.pool)
            .await
            .unwrap(),
        2
    );
    sqlx::query("UPDATE onboarding_project_projections SET projection=jsonb_set(projection,'{reviews,0,enabled}','false'::jsonb) WHERE subscription=$1").bind(binding.subscription_id).execute(&f.db.pool).await.unwrap();
    let withdrawn = body(
        search(
            State(f.state.clone()),
            Extension(actor),
            peer(),
            Path(binding.subscription_id),
            Json(LaptopCacheSearchRequest {
                question: "diagnosis".into(),
                limit: 8,
            }),
        )
        .await
        .unwrap(),
    )
    .await;
    assert_eq!(withdrawn["hits"], json!([]));
    f.db.cleanup().await.unwrap();
}
#[tokio::test]
async fn unbound_changed_and_remote_cache_reads_fail_without_fallback_or_initialization() {
    let Some((mut f, binding, actor)) = fixture().await else {
        return;
    };
    assert_eq!(
        search(
            State(f.state.clone()),
            Extension(actor.clone()),
            peer(),
            Path(Uuid::new_v4()),
            Json(LaptopCacheSearchRequest {
                question: "anything".into(),
                limit: 8
            })
        )
        .await
        .unwrap_err()
        .status,
        StatusCode::NOT_FOUND
    );
    let remote = ConnectInfo("192.0.2.30:40000".parse().unwrap());
    assert_eq!(
        list(State(f.state.clone()), Extension(actor.clone()), remote)
            .await
            .unwrap_err()
            .status,
        StatusCode::FORBIDDEN
    );
    let mut unbound = actor.clone();
    unbound.credential_source = CredentialSource::ServiceToken;
    assert_eq!(
        list(State(f.state.clone()), Extension(unbound), peer())
            .await
            .unwrap_err()
            .status,
        StatusCode::FORBIDDEN
    );
    assert_eq!(
        sqlx::query_scalar::<_, i64>("SELECT COUNT(*) FROM onboarding_project_projections")
            .fetch_one(&f.db.pool)
            .await
            .unwrap(),
        0
    );
    seed(&f.state, &binding, "selected").await;
    sqlx::query("UPDATE onboarding_project_projections SET device=$2 WHERE subscription=$1")
        .bind(binding.subscription_id)
        .bind(Uuid::new_v4())
        .execute(&f.db.pool)
        .await
        .unwrap();
    assert_eq!(
        list(State(f.state.clone()), Extension(actor.clone()), peer())
            .await
            .unwrap_err()
            .status,
        StatusCode::SERVICE_UNAVAILABLE
    );
    f.state.config.database.url =
        "postgresql://synthetic@server.example.invalid:5432/memory_laptop_workshop".into();
    assert_eq!(
        list(State(f.state.clone()), Extension(actor), peer())
            .await
            .unwrap_err()
            .status,
        StatusCode::SERVICE_UNAVAILABLE
    );
    f.db.cleanup().await.unwrap();
}
#[tokio::test]
async fn cache_http_requires_session_csrf_and_accepts_only_semantic_read_in_read_only_mode() {
    let Some((f, binding, actor)) = fixture().await else {
        return;
    };
    seed(&f.state, &binding, "cited http diagnosis").await;
    let app = crate::routes::build_http_app(f.state.clone());
    let listener = tokio::net::TcpListener::bind("127.0.0.1:0").await.unwrap();
    let base = format!("http://{}", listener.local_addr().unwrap());
    let server = tokio::spawn(async move {
        axum::serve(
            listener,
            app.into_make_service_with_connect_info::<SocketAddr>(),
        )
        .await
        .unwrap();
    });
    let client = reqwest::Client::new();
    let denied = client
        .get(format!("{base}/v1/onboarding/cache"))
        .send()
        .await
        .unwrap();
    assert_eq!(denied.status(), StatusCode::FORBIDDEN);
    let ok = client
        .get(format!("{base}/v1/onboarding/cache"))
        .header("x-api-token", "synthetic-owner")
        .send()
        .await
        .unwrap();
    assert_eq!(ok.status(), StatusCode::OK);
    assert_eq!(ok.headers()[header::CACHE_CONTROL], "no-store");
    let path = format!(
        "{base}/v1/onboarding/cache/{}/search",
        binding.subscription_id
    );
    let ok = client
        .post(&path)
        .header("x-api-token", "synthetic-owner")
        .json(&json!({"question":"diagnosis","limit":8}))
        .send()
        .await
        .unwrap();
    assert_eq!(ok.status(), StatusCode::OK);
    let (cookie, _) = crate::auth::mint_web_session(&f.state, &f.db.pool, actor.id)
        .await
        .unwrap();
    let browser = client
        .post(&path)
        .header("cookie", format!("memory_session={cookie}"))
        .json(&json!({"question":"diagnosis"}))
        .send()
        .await
        .unwrap();
    assert_eq!(browser.status(), StatusCode::FORBIDDEN);
    server.abort();
    let _ = server.await;
    f.db.cleanup().await.unwrap();
}
