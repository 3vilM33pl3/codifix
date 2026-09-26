#![allow(clippy::unwrap_used)]
use super::*;
use crate::GatewaySecurity;
use axum::{
    Json, Router,
    extract::{Query, State},
    routing::{get, post},
};
use mem_federation_store::{
    Acceptance, PublishedPage, ReceiptAuthority, ReceiptClaims, SpacePolicy,
};
use std::sync::{Arc, Mutex};

fn config() -> EnrollmentConfig {
    let security = GatewaySecurity::initial(&[7; 32], SpacePolicy::default()).unwrap();
    let record = || RecordId::new(Uuid::new_v4()).unwrap();
    EnrollmentConfig {
        schema_version: 1,
        gateway: "https://gateway.example.test".into(),
        gateway_ca_file: None,
        gateway_ca_sha256: None,
        token_file: "/nonexistent/private/token".into(),
        receipt_keys: security.trust().keys().to_vec(),
        plan: EnrollmentPlan {
            operation_id: Uuid::new_v4(),
            space: SpaceId::new(Uuid::new_v4()).unwrap(),
            authority: InstanceId::new(Uuid::new_v4()).unwrap(),
            author: AuthorIdentity::new("fixture-operator").unwrap(),
            authored_at: Utc::now() - chrono::Duration::minutes(1),
            expires_at: Utc::now() + chrono::Duration::minutes(10),
            expected_sequence: 0,
            expected_tip: None,
            members: vec![MemberEnrollment {
                member: InstanceId::new(Uuid::new_v4()).unwrap(),
                pull_record: record(),
                publish_record: record(),
                ask_record: record(),
                answer_record: record(),
            }],
        },
    }
}

#[test]
fn transport_and_page_failures_are_not_misreported_as_one_proof_error() {
    assert!(matches!(
        map_pull_failure(PullFailure::Unavailable),
        EnrollmentError::GatewayUnavailable
    ));
    assert!(matches!(
        map_pull_failure(PullFailure::Unauthorized),
        EnrollmentError::GatewayUnauthorized
    ));
    assert!(matches!(
        map_pull_failure(PullFailure::PageProof(PageVerificationError::Signature)),
        EnrollmentError::InvalidPageSignature
    ));
    assert!(matches!(
        map_pull_failure(PullFailure::PageProof(PageVerificationError::Receipt)),
        EnrollmentError::InvalidPageReceipt
    ));
    assert!(matches!(
        map_pull_failure(PullFailure::Ordering),
        EnrollmentError::InvalidPageOrder
    ));
}

#[test]
fn default_preview_is_offline_bounded_and_canonical_without_secret_access() {
    let config = config();
    let preview = config.preview().unwrap();
    assert_eq!(preview.network_calls, 0);
    assert_eq!(preview.records.len(), 4);
    assert_eq!(
        preview.review_sha256,
        config.preview().unwrap().review_sha256
    );
    let json = serde_json::to_string(&preview).unwrap();
    assert!(!json.contains("/nonexistent/private/token"));
    assert!(!json.contains("manage_membership"));
    let restored: EnrollmentConfig =
        serde_json::from_str(&serde_json::to_string_pretty(&config).unwrap()).unwrap();
    assert_eq!(restored.digest().unwrap(), config.digest().unwrap());
    let mut changed = config.clone();
    changed.plan.members[0].ask_record = RecordId::new(Uuid::new_v4()).unwrap();
    assert_ne!(config.digest().unwrap(), changed.digest().unwrap());
    for intent in preview.records {
        assert_eq!(intent.envelope.schema_version, 3);
        assert_eq!(intent.envelope.publisher, config.plan.authority);
        assert!(matches!(
            intent.envelope.record,
            PublicationRecord::Membership { .. }
        ));
    }
}

#[test]
fn unsafe_endpoints_aliases_unknown_fields_and_authority_membership_are_rejected() {
    let original = config();
    for endpoint in [
        "http://127.0.0.1:1",
        "https://user:secret@example.test",
        "https://example.test/path",
        "https://example.test/?q=1",
    ] {
        let mut changed = original.clone();
        changed.gateway = endpoint.into();
        assert!(changed.preview().is_err());
    }
    let mut changed = original.clone();
    changed.plan.members[0].member = changed.plan.authority;
    assert!(changed.preview().is_err());
    let mut changed = original.clone();
    changed.plan.members[0].answer_record = changed.plan.members[0].pull_record;
    assert!(changed.preview().is_err());
    let mut changed = original.clone();
    changed.plan.members.push(changed.plan.members[0].clone());
    assert!(changed.preview().is_err());
    let mut changed = original.clone();
    changed.plan.expected_sequence = 1;
    assert!(changed.preview().is_err());
    let mut unknown = serde_json::to_value(&original).unwrap();
    unknown["plan"]["members"][0]["manage_membership"] = serde_json::json!(true);
    assert!(serde_json::from_value::<EnrollmentConfig>(unknown).is_err());
}

#[tokio::test]
async fn wrong_confirmation_and_expiry_fail_before_token_read_or_network() {
    let mut config = config();
    assert!(matches!(
        config.execute(&"0".repeat(64)).await,
        Err(EnrollmentError::Confirmation)
    ));
    config.plan.authored_at = Utc::now() - chrono::Duration::hours(2);
    config.plan.expires_at = Utc::now() - chrono::Duration::hours(1);
    let digest = config.digest().unwrap();
    assert!(matches!(
        config.execute(digest.as_hex()).await,
        Err(EnrollmentError::Expired)
    ));
}

#[cfg(unix)]
#[test]
fn input_must_be_a_private_regular_explicit_file() {
    use std::io::Write;
    use std::os::unix::fs::{OpenOptionsExt, PermissionsExt, symlink};
    let directory = std::env::temp_dir().join(format!("enrollment-test-{}", Uuid::new_v4()));
    std::fs::create_dir(&directory).unwrap();
    std::fs::set_permissions(&directory, std::fs::Permissions::from_mode(0o700)).unwrap();
    let file = directory.join("config.json");
    std::fs::OpenOptions::new()
        .write(true)
        .create_new(true)
        .mode(0o600)
        .open(&file)
        .unwrap()
        .write_all(&serde_json::to_vec(&config()).unwrap())
        .unwrap();
    assert!(EnrollmentConfig::load(&file).is_ok());
    let link = directory.join("link.json");
    symlink(&file, &link).unwrap();
    assert!(EnrollmentConfig::load(&link).is_err());
    std::fs::set_permissions(&file, std::fs::Permissions::from_mode(0o644)).unwrap();
    assert!(EnrollmentConfig::load(&file).is_err());
    assert!(EnrollmentConfig::load(Path::new("relative.json")).is_err());
    std::fs::remove_file(link).unwrap();
    std::fs::remove_file(file).unwrap();
    std::fs::remove_dir(directory).unwrap();
}

#[cfg(unix)]
#[test]
fn inspect_and_execute_client_use_the_protected_token_loader() {
    use std::{
        fs,
        io::Write,
        os::unix::fs::{OpenOptionsExt, PermissionsExt, symlink},
    };
    let root = tempfile::Builder::new()
        .permissions(fs::Permissions::from_mode(0o700))
        .tempdir()
        .unwrap();
    let mut config = config();
    config.token_file = root.path().join("authority-token");
    let mut token = fs::OpenOptions::new()
        .write(true)
        .create_new(true)
        .mode(0o600)
        .open(&config.token_file)
        .unwrap();
    token
        .write_all(b"synthetic-authority-token-never-sent")
        .unwrap();
    // client() is shared by inspect/execute and constructs no network requests.
    assert!(config.client().is_ok());
    token.set_len(4097).unwrap();
    assert!(config.client().is_err());
    token.set_len(8).unwrap();
    assert!(config.client().is_err());
    fs::write(&config.token_file, b"synthetic-authority-token-never-sent").unwrap();
    assert!(config.client().is_ok());
    fs::rename(&config.token_file, root.path().join("previous")).unwrap();
    symlink(root.path().join("previous"), &config.token_file).unwrap();
    assert!(config.client().is_err());
    fs::remove_file(&config.token_file).unwrap();
    fs::rename(root.path().join("previous"), &config.token_file).unwrap();
    fs::set_permissions(root.path(), fs::Permissions::from_mode(0o755)).unwrap();
    assert!(config.client().is_err());
}

struct Fixture {
    config: EnrollmentConfig,
    security: GatewaySecurity,
    records: Vec<PublicationDelivery>,
    receipts: Vec<AuditReceipt>,
    published: bool,
    forged_page: bool,
    forged_accept: bool,
    forged_sequence: bool,
    posts: usize,
}
#[derive(Deserialize)]
struct Pull {
    after: u32,
    limit: u32,
    nonce: Uuid,
}
async fn pull(
    State(state): State<Arc<Mutex<Fixture>>>,
    Query(query): Query<Pull>,
) -> Json<crate::SignedPage> {
    let f = state.lock().unwrap();
    let count = if f.published { f.records.len() } else { 0 };
    let page = PublishedPage {
        space: f.config.plan.space,
        authority: f.config.plan.authority,
        after: query.after,
        published_through: count as u32,
        records: f
            .records
            .iter()
            .take(count)
            .skip(query.after as usize)
            .take(query.limit as usize)
            .cloned()
            .collect(),
        receipts: f
            .receipts
            .iter()
            .take(count)
            .skip(query.after as usize)
            .take(query.limit as usize)
            .cloned()
            .collect(),
        policy: SpacePolicy::default(),
    };
    let mut signed = f.security.sign_page(page, query.nonce).unwrap();
    if f.forged_page {
        signed.signature[0] ^= 1;
    }
    Json(signed)
}
async fn accept(
    State(state): State<Arc<Mutex<Fixture>>>,
    Json(intent): Json<PublicationIntent>,
) -> Json<Acceptance> {
    let mut f = state.lock().unwrap();
    f.posts += 1;
    if let Some(existing) = f
        .records
        .iter()
        .find(|r| r.envelope.key() == intent.envelope.key())
    {
        return Json(Acceptance {
            outcome: PublicationOutcome::Duplicate,
            delivery: Some(existing.clone()),
        });
    }
    let delivery = PublicationDelivery::new(
        AuthoritySequence::new(f.records.len() as u32 + 1).unwrap(),
        intent.locator,
        intent.envelope,
    )
    .unwrap();
    let receipt = f
        .security
        .sign(ReceiptClaims {
            purpose: "memory-publication-receipt-v1".into(),
            space: f.config.plan.space,
            authority: f.config.plan.authority,
            sequence: delivery.sequence.get(),
            delivery_digest: audit_digest(&delivery).unwrap(),
            previous: f.receipts.last().map(audit_digest).transpose().unwrap(),
            key_generation: 1,
            policy: SpacePolicy::default(),
            transport: serde_json::json!({"synthetic":true}),
        })
        .unwrap();
    f.records.push(delivery.clone());
    f.receipts.push(receipt);
    let mut returned = delivery;
    if f.forged_accept {
        returned.envelope.author = AuthorIdentity::new("forged").unwrap();
    }
    if f.forged_sequence {
        returned.sequence = AuthoritySequence::new(returned.sequence.get() + 100).unwrap();
    }
    Json(Acceptance {
        outcome: PublicationOutcome::Applied,
        delivery: Some(returned),
    })
}
async fn server(
    config: &EnrollmentConfig,
) -> (
    GatewayClient,
    Arc<Mutex<Fixture>>,
    tokio::task::JoinHandle<()>,
) {
    let security = GatewaySecurity::initial(&[7; 32], SpacePolicy::default()).unwrap();
    let state = Arc::new(Mutex::new(Fixture {
        config: config.clone(),
        security: security.clone(),
        records: vec![],
        receipts: vec![],
        published: false,
        forged_page: false,
        forged_accept: false,
        forged_sequence: false,
        posts: 0,
    }));
    let listener = tokio::net::TcpListener::bind("127.0.0.1:0").await.unwrap();
    // Test-only injection into the private helper, never an operator flag.
    let endpoint =
        Endpoint::new(&format!("http://{}", listener.local_addr().unwrap()), true).unwrap();
    let client = GatewayClient::new(
        endpoint,
        "synthetic-credential".repeat(3),
        config.plan.space,
        config.plan.authority,
        security.trust(),
    )
    .unwrap();
    let app = Router::new()
        .route("/v1/pull", get(pull))
        .route("/v1/accept", post(accept))
        .with_state(state.clone());
    let server = tokio::spawn(async move {
        axum::serve(listener, app).await.unwrap();
    });
    (client, state, server)
}

#[tokio::test]
async fn pending_is_not_published_and_exact_retry_preserves_identity_and_verifies_signed_history() {
    let config = config();
    let (client, state, server) = server(&config).await;
    let report = config
        .execute_with(&client, config.digest().unwrap())
        .await
        .unwrap();
    assert_eq!(report.status, "accepted_publication_pending");
    assert_eq!(report.accepted.len(), 4);
    assert!(report.confirmed_published_records.is_empty());
    assert_eq!(state.lock().unwrap().records.len(), 4);
    state.lock().unwrap().published = true;
    let report = config
        .execute_with(&client, config.digest().unwrap())
        .await
        .unwrap();
    assert_eq!(report.status, "published_proven");
    assert_eq!(report.confirmed_published_records.len(), 4);
    assert_eq!(report.observed.members[0].capabilities.len(), 4);
    assert_eq!(state.lock().unwrap().records.len(), 4);
    assert_eq!(state.lock().unwrap().posts, 8);
    server.abort();
}

#[tokio::test]
async fn forged_pages_fail_before_write_and_forged_acceptance_is_unconfirmed() {
    let config = config();
    let (client, state, server) = server(&config).await;
    state.lock().unwrap().forged_page = true;
    assert!(matches!(
        config.execute_with(&client, config.digest().unwrap()).await,
        Err(EnrollmentError::InvalidPageSignature)
    ));
    assert_eq!(state.lock().unwrap().posts, 0);
    state.lock().unwrap().forged_page = false;
    state.lock().unwrap().forged_accept = true;
    assert!(matches!(
        config.execute_with(&client, config.digest().unwrap()).await,
        Err(EnrollmentError::Unconfirmed)
    ));
    assert_eq!(state.lock().unwrap().records.len(), 1);
    server.abort();
}

#[tokio::test]
async fn unrelated_published_change_requires_new_review_before_additional_writes() {
    let config = config();
    let (client, state, server) = server(&config).await;
    state.lock().unwrap().published = true;
    config
        .execute_with(&client, config.digest().unwrap())
        .await
        .unwrap();
    let mut next = config.clone();
    next.plan.operation_id = Uuid::new_v4();
    next.plan.members[0].pull_record = RecordId::new(Uuid::new_v4()).unwrap();
    assert!(matches!(
        next.execute_with(&client, next.digest().unwrap()).await,
        Err(EnrollmentError::ChangedHistory)
    ));
    assert_eq!(state.lock().unwrap().posts, 4);
    // A new plan must pin the observed receipt tip, not just the count.
    let snapshot = config.snapshot(&client).await.unwrap();
    next.plan.expected_sequence = 4;
    next.plan.expected_tip = Some(ContentDigest::sha256_bytes(b"wrong"));
    assert!(matches!(
        next.check_baseline(&snapshot, &next.intents().unwrap()),
        Err(EnrollmentError::ChangedHistory)
    ));
    next.plan.expected_tip = Some(audit_digest(snapshot.receipts.last().unwrap()).unwrap());
    assert!(
        next.check_baseline(&snapshot, &next.intents().unwrap())
            .is_ok()
    );
    server.abort();
}

#[tokio::test]
async fn acceptance_sequence_must_match_the_signed_publication() {
    let config = config();
    let (client, state, server) = server(&config).await;
    {
        let mut fixture = state.lock().unwrap();
        fixture.published = true;
        fixture.forged_sequence = true;
    }
    assert!(matches!(
        config.execute_with(&client, config.digest().unwrap()).await,
        Err(EnrollmentError::InvalidProof)
    ));
    server.abort();
}
