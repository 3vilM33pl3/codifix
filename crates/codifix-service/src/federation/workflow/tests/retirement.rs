// SPDX-License-Identifier: AGPL-3.0-or-later
use super::*;
use codifix_federation_store::{
    AuditReceipt, ReceiptClaims, ReceiptKey, ReplicaStore, RetiredReplicaCheckpoint, SpacePolicy,
    audit_digest, set_local_grant,
};
use sqlx::types::Json;

#[tokio::test]
async fn retired_http_context_uses_current_project_scope_without_an_active_registry() {
    let Some(f) = Fixture::new().await else {
        return;
    };
    let old = SpaceId::new(Uuid::new_v4()).unwrap();
    let authority = f.runtime.authority;
    let receiver = f.runtime.instance;
    let stream = StreamId {
        origin: authority,
        canonical: RecordId::new(Uuid::new_v4()).unwrap(),
    };
    let item = intent(
        old,
        authority,
        PublicationRecord::MemoryVersion {
            stream,
            publication: MemoryPublication {
                version: SharedVersion::new(1).unwrap(),
                source: SourceVersionRef {
                    record_id: RecordId::new(Uuid::new_v4()).unwrap(),
                    version: SourceVersion::new(1).unwrap(),
                },
                content: MemoryContent {
                    text: "retained HTTP diagnosis".into(),
                    summary: "retained".into(),
                    memory_type: codifix_record::MemoryType::Decision,
                    confidence: ConfidencePpm::try_from(800_000).unwrap(),
                    importance: 3,
                    tags: vec![],
                },
                evidence: vec![],
            },
        },
    );
    let delivery = PublicationDelivery::new(
        AuthoritySequence::new(1).unwrap(),
        item.locator,
        item.envelope,
    )
    .unwrap();
    let mut receipt = AuditReceipt {
        claims: ReceiptClaims {
            purpose: "memory-publication-receipt-v1".into(),
            space: old,
            authority,
            sequence: 1,
            delivery_digest: audit_digest(&delivery).unwrap(),
            previous: None,
            key_generation: 1,
            policy: SpacePolicy::default(),
            transport: json!({}),
        },
        signature: vec![],
    };
    let (key, signature) = codifix_atproto_verify::fixtures::retired_attestation(
        &CanonicalJsonCodec::encode(&receipt.claims).unwrap(),
    );
    receipt.signature = signature;
    let keys = vec![ReceiptKey {
        generation: 1,
        first_sequence: 1,
        public_key: key,
    }];
    let replica = ReplicaStore::initialize(f.pool.clone(), old, receiver, authority)
        .await
        .unwrap();
    replica
        .receive(authority, std::slice::from_ref(&delivery))
        .await
        .unwrap();
    sqlx::query("INSERT INTO federation_replica_trust(space,receiver,keys) VALUES($1,$2,$3)")
        .bind(old.as_uuid())
        .bind(receiver.as_uuid())
        .bind(Json(&keys))
        .execute(&f.pool)
        .await
        .unwrap();
    sqlx::query("INSERT INTO federation_replica_receipts(space,receiver,sequence,receipt) VALUES($1,$2,1,$3)").bind(old.as_uuid()).bind(receiver.as_uuid()).bind(Json(&receipt)).execute(&f.pool).await.unwrap();
    let access = crate::auth::federation_local_access(&f.state, f.root_id, f.runtime.project)
        .await
        .unwrap();
    let grant = LocalSpaceGrant {
        principal: f.reader_id,
        project: f.runtime.project,
        space: old,
        instance: receiver,
        capabilities: vec![SpaceCapability::Pull],
    };
    let mut tx = f.pool.begin().await.unwrap();
    set_local_grant(&mut tx, &access, &grant, true)
        .await
        .unwrap();
    tx.commit().await.unwrap();
    let history: String = sqlx::query_scalar("SELECT codifix_retained_history_sha256($1,$2)")
        .bind(old.as_uuid())
        .bind(receiver.as_uuid())
        .fetch_one(&f.pool)
        .await
        .unwrap();
    let checkpoint = RetiredReplicaCheckpoint {
        receiver,
        authority,
        cursor: 1,
        history_sha256: history,
        keys,
    };
    sqlx::query("INSERT INTO codifix_retired_federation_spaces(space,local_instance,manifest_sha256,replicas,manifest) VALUES($1,$2,$3,$4,'{}')").bind(old.as_uuid()).bind(receiver.as_uuid()).bind("a".repeat(64)).bind(Json(vec![checkpoint])).execute(&f.pool).await.unwrap();
    *f.state.federation.write().unwrap() = None;
    let url = format!(
        "{}/v1/projects/{}/federation/{}/context",
        f.url,
        f.slug,
        old.as_uuid()
    );
    let client = reqwest::Client::new();
    let body = json!({"streams":[stream]});
    let response = client
        .post(&url)
        .bearer_auth(&f.reader)
        .json(&body)
        .send()
        .await
        .unwrap();
    assert_eq!(response.status(), StatusCode::OK);
    let retained: Value = response.json().await.unwrap();
    assert_eq!(retained["records"][0]["digest"], json!(delivery.digest));
    // No old worker registry is needed, but current local permissions and exact
    // project token scope remain necessary even when signed history is retained.
    assert_eq!(
        client
            .post(&url)
            .bearer_auth(&f.writer)
            .json(&body)
            .send()
            .await
            .unwrap()
            .status(),
        StatusCode::FORBIDDEN
    );
    assert_eq!(
        client.post(&url).json(&body).send().await.unwrap().status(),
        StatusCode::UNAUTHORIZED
    );
    let other = Uuid::new_v4();
    sqlx::query("INSERT INTO projects(id,slug,name,root_path) VALUES($1,$2,'other','/other')")
        .bind(other)
        .bind(other.to_string())
        .execute(&f.pool)
        .await
        .unwrap();
    let (_, other_token) = actor(&f.pool, other, "reader").await;
    assert_eq!(
        client
            .post(&url)
            .bearer_auth(other_token)
            .json(&body)
            .send()
            .await
            .unwrap()
            .status(),
        StatusCode::FORBIDDEN
    );
}
