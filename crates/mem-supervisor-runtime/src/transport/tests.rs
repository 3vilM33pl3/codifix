use super::*;
use axum::{
    Json, Router,
    extract::State,
    http::StatusCode,
    response::{IntoResponse, Response},
    routing::post,
};
use mem_supervisor::{
    Area, UsdMicros,
    action::{ActionKey, ActionKind, ActionState},
    budget::AttemptState,
};
use mem_supervisor_store::{ActionSubmission, ReadyTransition};
use mem_test_support::IsolatedDatabase;
use serde_json::{Value, json};
use std::sync::{
    Arc,
    atomic::{AtomicUsize, Ordering},
};

#[derive(Clone, Copy)]
enum Mode {
    Good,
    MalformedUsage,
    BadCitation,
    Revoke,
    Redirect,
    RateLimit,
    RevokeDuringCount,
}
#[derive(Clone)]
struct Fake {
    count: Arc<AtomicUsize>,
    generated: Arc<AtomicUsize>,
    mode: Mode,
    store: SupervisorStore,
    revision: Uuid,
    instance_id: Uuid,
    project_id: Uuid,
}
impl Fake {
    fn execution(&self, revision: Uuid, attempt_id: Uuid) -> ProposalExecution {
        ProposalExecution {
            instance_id: self.instance_id,
            project_id: self.project_id,
            policy_revision: revision,
            attempt_id,
        }
    }
}
async fn count(State(s): State<Fake>, headers: HeaderMap, Json(body): Json<Value>) -> Response {
    assert_eq!(headers[AUTHORIZATION], "Bearer synthetic-only");
    assert_eq!(body["model"], DEFAULT_MODEL);
    assert_eq!(body["tools"], json!([]));
    assert!(body.get("previous_response_id").is_none());
    s.count.fetch_add(1, Ordering::SeqCst);
    if matches!(s.mode, Mode::RevokeDuringCount) {
        s.store
            .revoke_policy(Area::Workshop, s.revision)
            .await
            .expect("revoke");
    }
    Json(json!({"object":"response.input_tokens","input_tokens":100})).into_response()
}
async fn generate(State(s): State<Fake>, headers: HeaderMap, Json(body): Json<Value>) -> Response {
    assert_eq!(headers[AUTHORIZATION], "Bearer synthetic-only");
    assert_eq!(body["model"], DEFAULT_MODEL);
    assert_eq!(body["max_output_tokens"], 800);
    assert_eq!(body["store"], false);
    assert_eq!(body["service_tier"], "default");
    assert_eq!(body["tools"], json!([]));
    s.generated.fetch_add(1, Ordering::SeqCst);
    if matches!(s.mode, Mode::Redirect) {
        return (
            StatusCode::TEMPORARY_REDIRECT,
            [("location", "/v1/responses")],
            "secret redirect body",
        )
            .into_response();
    }
    if matches!(s.mode, Mode::RateLimit) {
        return (StatusCode::TOO_MANY_REQUESTS, "secret provider error").into_response();
    }
    if matches!(s.mode, Mode::Revoke) {
        s.store
            .revoke_policy(Area::Workshop, s.revision)
            .await
            .expect("revoke");
    }
    let mut response = json!({"object":"response","model":DEFAULT_MODEL,"service_tier":"default","status":"completed",
      "usage":{"input_tokens":100,"input_tokens_details":{"cached_tokens":0,"cache_write_tokens":0},
        "output_tokens":20,"output_tokens_details":{"reasoning_tokens":5},"total_tokens":120},
      "output":[{"type":"message","role":"assistant","status":"completed","content":[{"type":"output_text",
        "text":"{\"summary\":\"Ask a human to review\",\"proposal\":\"ask_human\",\"evidence_indices\":[0]}"}]}]});
    if matches!(s.mode, Mode::MalformedUsage) {
        response["usage"] = Value::Null;
    }
    if matches!(s.mode, Mode::BadCitation) {
        response["output"][0]["content"][0]["text"] = json!(
            "{\"summary\":\"wrong citation\",\"proposal\":\"none\",\"evidence_indices\":[99]}"
        );
    }
    Json(response).into_response()
}

async fn fixture(
    mode: Mode,
) -> Option<(
    IsolatedDatabase,
    SupervisorStore,
    LeaseClaim,
    Uuid,
    Fake,
    OpenAiTransport,
    tokio::task::JoinHandle<()>,
)> {
    fixture_kind(mode, ActionKind::StaleKnowledgeReview).await
}

async fn fixture_kind(
    mode: Mode,
    kind: ActionKind,
) -> Option<(
    IsolatedDatabase,
    SupervisorStore,
    LeaseClaim,
    Uuid,
    Fake,
    OpenAiTransport,
    tokio::task::JoinHandle<()>,
)> {
    let db = IsolatedDatabase::new().await?;
    let store = SupervisorStore::new(db.pool.clone());
    let instance_id = Uuid::new_v4();
    let project_id = Uuid::new_v4();
    store
        .bind_instance(instance_id, Area::Workshop)
        .await
        .expect("bind instance");
    sqlx::query("INSERT INTO projects(id,slug,name,root_path) VALUES($1,'supervisor-synthetic','Synthetic','/synthetic')")
        .bind(project_id).execute(&db.pool).await.expect("project");
    let revision = Uuid::new_v4();
    store
        .install_policy(Area::Workshop, revision)
        .await
        .expect("policy");
    store
        .set_enabled(Area::Workshop, revision, true)
        .await
        .expect("enabled");
    let id = Uuid::new_v4();
    store
        .submit(&ActionSubmission {
            id,
            key: ActionKey {
                area: Area::Workshop,
                source_event: stable(Uuid::new_v4()).expect("ID"),
                kind,
                policy_revision: stable(revision).expect("ID"),
            },
            source_digest: [1; 32],
        })
        .await
        .expect("submit");
    for t in [
        ReadyTransition::Propose,
        ReadyTransition::Authorize,
        ReadyTransition::Queue,
    ] {
        store
            .advance(Area::Workshop, id, t, [2; 32])
            .await
            .expect("advance");
    }
    let lease = store
        .claim(Area::Workshop, id, Uuid::new_v4(), 120)
        .await
        .expect("claim");
    let fake = Fake {
        count: Arc::new(AtomicUsize::new(0)),
        generated: Arc::new(AtomicUsize::new(0)),
        mode,
        store: store.clone(),
        revision,
        instance_id,
        project_id,
    };
    let router = Router::new()
        .route("/v1/responses/input_tokens", post(count))
        .route("/v1/responses", post(generate))
        .with_state(fake.clone());
    let listener = tokio::net::TcpListener::bind("127.0.0.1:0")
        .await
        .expect("listen");
    let addr = listener.local_addr().expect("addr");
    let server = tokio::spawn(async move {
        axum::serve(listener, router).await.expect("server");
    });
    let mut client =
        OpenAiTransport::build("synthetic-only", &format!("http://{addr}/v1")).expect("client");
    client.semantic_model_time = Some(crate::model::PRICING_REVIEWED_AT);
    Some((db, store, lease, revision, fake, client, server))
}
fn request() -> PreparedRequest {
    PreparedRequest::new(
        "Review this synthetic note",
        &["Fake reviewed context".into()],
    )
    .expect("request")
}

#[tokio::test]
async fn returned_proposal_and_completed_action_are_durable_and_project_scoped() {
    let Some((db, store, lease, revision, fake, client, server)) = fixture(Mode::Good).await else {
        return;
    };
    let project_id = fake.project_id;
    let execution = ProposalExecution {
        instance_id: fake.instance_id,
        policy_revision: revision,
        attempt_id: Uuid::new_v4(),
        project_id,
    };
    let request = request();
    let output = client
        .execute_and_persist(&store, &lease, &execution, &request)
        .await
        .expect("persisted");
    assert_eq!(output.proposal.summary, "Ask a human to review");
    assert_eq!(
        store
            .action(Area::Workshop, lease.action_id())
            .await
            .expect("action")
            .state,
        ActionState::Verified
    );
    let restarted = SupervisorStore::new(db.pool.clone());
    let recovered = restarted
        .private_proposal(Area::Workshop, project_id, lease.action_id())
        .await
        .expect("recover");
    assert_eq!(recovered.proposal.summary, output.proposal.summary);
    assert!(matches!(
        restarted
            .private_proposal(Area::Workshop, Uuid::new_v4(), lease.action_id())
            .await,
        Err(StoreError::NotFound)
    ));
    assert!(matches!(
        restarted
            .private_proposal(Area::Trust, project_id, lease.action_id())
            .await,
        Err(StoreError::NotFound)
    ));
    assert!(
        client
            .execute_and_persist(&restarted, &lease, &execution, &request)
            .await
            .is_err()
    );
    assert_eq!(fake.count.load(Ordering::SeqCst), 1);
    assert_eq!(fake.generated.load(Ordering::SeqCst), 1);
    let audit = serde_json::to_string(
        &restarted
            .journal(Area::Workshop, 0, 500)
            .await
            .expect("journal"),
    )
    .expect("JSON");
    assert!(!audit.contains("Ask a human"));
    assert!(!audit.contains("Fake reviewed"));
    server.abort();
    db.cleanup().await.expect("cleanup");
}

#[tokio::test]
async fn deterministic_health_and_privileged_installs_never_disclose_to_a_model() {
    for kind in [ActionKind::HealthCheck, ActionKind::InstallMaintenance] {
        let Some((db, store, lease, revision, fake, client, server)) =
            fixture_kind(Mode::Good, kind).await
        else {
            return;
        };
        assert!(matches!(
            client
                .execute(
                    &store,
                    &lease,
                    &fake.execution(revision, Uuid::new_v4()),
                    &request()
                )
                .await,
            Err(TransportError::Authority)
        ));
        assert_eq!(fake.count.load(Ordering::SeqCst), 0);
        assert_eq!(fake.generated.load(Ordering::SeqCst), 0);
        assert_eq!(
            store
                .action(Area::Workshop, lease.action_id())
                .await
                .expect("action")
                .state,
            ActionState::Running
        );
        server.abort();
        db.cleanup().await.expect("cleanup");
    }
}

#[tokio::test]
async fn durable_admission_precedes_one_physical_send_and_duplicate_is_not_reissued() {
    let Some((db, store, lease, revision, fake, client, server)) = fixture(Mode::Good).await else {
        return;
    };
    let attempt = Uuid::new_v4();
    let request = request();
    let proposal = client
        .execute(&store, &lease, &fake.execution(revision, attempt), &request)
        .await
        .expect("proposal");
    assert_eq!(proposal.proposal, crate::model::ProposalKind::AskHuman);
    // A returned proposal is not yet durably recorded as a completed decision.
    assert_eq!(
        store
            .action(Area::Workshop, lease.action_id())
            .await
            .expect("action")
            .state,
        ActionState::Running
    );
    assert!(matches!(
        client
            .execute(&store, &lease, &fake.execution(revision, attempt), &request)
            .await,
        Err(TransportError::ExistingAttempt)
    ));
    assert_eq!(fake.generated.load(Ordering::SeqCst), 1);
    assert_eq!(fake.count.load(Ordering::SeqCst), 1);
    let changed = PreparedRequest::new("Altered payload", &[]).expect("request");
    assert!(matches!(
        client
            .execute(&store, &lease, &fake.execution(revision, attempt), &changed)
            .await,
        Err(TransportError::PermitMismatch)
    ));
    assert_eq!(fake.count.load(Ordering::SeqCst), 1);
    let journal = store
        .journal(Area::Workshop, 0, 500)
        .await
        .expect("journal");
    assert!(
        journal.iter().position(|j| j.event == "begun")
            < journal.iter().position(|j| j.event == "settled")
    );
    let serialized = serde_json::to_string(&journal).expect("journal JSON");
    for private in [
        "synthetic-only",
        "Fake reviewed context",
        "Ask a human",
        "Review this synthetic",
    ] {
        assert!(!serialized.contains(private));
    }
    server.abort();
    db.cleanup().await.expect("cleanup");
}

#[tokio::test]
async fn missing_usage_retains_full_reservation_and_cannot_auto_retry() {
    let Some((db, store, lease, revision, fake, client, server)) =
        fixture(Mode::MalformedUsage).await
    else {
        return;
    };
    let attempt = Uuid::new_v4();
    let request = request();
    assert!(matches!(
        client
            .execute(&store, &lease, &fake.execution(revision, attempt), &request)
            .await,
        Err(TransportError::Model(ModelError::UnknownUsage))
    ));
    let record = store.attempt(attempt).await.expect("attempt");
    assert_eq!(record.state, AttemptState::Uncertain);
    assert_eq!(record.worst_case_cost, UsdMicros(16_500));
    assert!(matches!(
        client
            .execute(&store, &lease, &fake.execution(revision, attempt), &request)
            .await,
        Err(TransportError::ExistingAttempt)
    ));
    assert_eq!(fake.generated.load(Ordering::SeqCst), 1);
    server.abort();
    db.cleanup().await.expect("cleanup");
}

#[tokio::test]
async fn revocation_after_model_execution_accounts_cost_but_refuses_result() {
    let Some((db, store, lease, revision, fake, client, server)) = fixture(Mode::Revoke).await
    else {
        return;
    };
    let attempt = Uuid::new_v4();
    assert!(matches!(
        client
            .execute(
                &store,
                &lease,
                &fake.execution(revision, attempt),
                &request()
            )
            .await,
        Err(TransportError::Authority)
    ));
    assert_eq!(
        store.attempt(attempt).await.expect("attempt").state,
        AttemptState::Settled {
            actual_cost: UsdMicros(800)
        }
    );
    assert_eq!(fake.generated.load(Ordering::SeqCst), 1);
    server.abort();
    db.cleanup().await.expect("cleanup");
}

#[tokio::test]
async fn invalid_citation_is_billed_but_not_accepted_as_a_proposal() {
    let Some((db, store, lease, revision, fake, client, server)) = fixture(Mode::BadCitation).await
    else {
        return;
    };
    let attempt = Uuid::new_v4();
    assert!(matches!(
        client
            .execute(
                &store,
                &lease,
                &fake.execution(revision, attempt),
                &request()
            )
            .await,
        Err(TransportError::Model(ModelError::InvalidEvidence))
    ));
    assert_eq!(
        store.attempt(attempt).await.expect("attempt").state,
        AttemptState::Settled {
            actual_cost: UsdMicros(800)
        }
    );
    server.abort();
    db.cleanup().await.expect("cleanup");
}

#[tokio::test]
async fn redirects_and_rate_limits_never_trigger_hidden_retries_or_error_body_leaks() {
    for mode in [Mode::Redirect, Mode::RateLimit] {
        let Some((db, store, lease, revision, fake, client, server)) = fixture(mode).await else {
            return;
        };
        let attempt = Uuid::new_v4();
        let result = client
            .execute(
                &store,
                &lease,
                &fake.execution(revision, attempt),
                &request(),
            )
            .await;
        assert!(matches!(result, Err(TransportError::ProviderRejected)));
        let error = result.err().expect("error");
        assert!(!format!("{error:?} {error}").contains("secret"));
        assert_eq!(
            store.attempt(attempt).await.expect("attempt").state,
            AttemptState::Uncertain
        );
        assert_eq!(fake.generated.load(Ordering::SeqCst), 1);
        server.abort();
        db.cleanup().await.expect("cleanup");
    }
}

#[test]
fn invalid_secret_or_binding_never_enters_an_error_message() {
    assert!(matches!(
        OpenAiTransport::build("secret\nheader", BASE),
        Err(TransportError::CredentialUnavailable)
    ));
    assert!(matches!(
        OpenAiTransport::build("", BASE),
        Err(TransportError::CredentialUnavailable)
    ));
    assert!(digest_hex("not-a-hash").is_err());
}

#[tokio::test]
async fn mismatched_instance_or_missing_project_never_discloses_even_to_token_counter() {
    let Some((db, store, lease, revision, fake, client, server)) = fixture(Mode::Good).await else {
        return;
    };
    let mut execution = fake.execution(revision, Uuid::new_v4());
    execution.instance_id = Uuid::new_v4();
    assert!(matches!(
        client.execute(&store, &lease, &execution, &request()).await,
        Err(TransportError::Authority)
    ));
    execution.instance_id = fake.instance_id;
    execution.project_id = Uuid::new_v4();
    assert!(matches!(
        client.execute(&store, &lease, &execution, &request()).await,
        Err(TransportError::Authority)
    ));
    assert_eq!(fake.count.load(Ordering::SeqCst), 0);
    assert_eq!(fake.generated.load(Ordering::SeqCst), 0);
    server.abort();
    db.cleanup().await.expect("cleanup");
}

#[tokio::test]
async fn changed_project_under_bound_action_is_rejected_before_input_counting() {
    let Some((db, store, lease, revision, fake, client, server)) = fixture(Mode::Good).await else {
        return;
    };
    let input = request();
    store
        .bind_model_work(
            &lease,
            fake.instance_id,
            fake.project_id,
            digest_hex(input.digest()).expect("digest"),
            input.evidence_digests(),
        )
        .await
        .expect("bind");
    let other_project = Uuid::new_v4();
    sqlx::query("INSERT INTO projects(id,slug,name,root_path) VALUES($1,'other','Other','/synthetic-other')")
        .bind(other_project).execute(&db.pool).await.expect("project");
    let mut execution = fake.execution(revision, Uuid::new_v4());
    execution.project_id = other_project;
    assert!(matches!(
        client.execute(&store, &lease, &execution, &input).await,
        Err(TransportError::Authority)
    ));
    assert_eq!(fake.count.load(Ordering::SeqCst), 0);
    assert_eq!(fake.generated.load(Ordering::SeqCst), 0);
    server.abort();
    db.cleanup().await.expect("cleanup");
}

#[tokio::test]
async fn revocation_during_token_counting_prevents_generation() {
    let Some((db, store, lease, revision, fake, client, server)) =
        fixture(Mode::RevokeDuringCount).await
    else {
        return;
    };
    assert!(matches!(
        client
            .execute(
                &store,
                &lease,
                &fake.execution(revision, Uuid::new_v4()),
                &request()
            )
            .await,
        Err(TransportError::Authority)
    ));
    assert_eq!(fake.count.load(Ordering::SeqCst), 1);
    assert_eq!(fake.generated.load(Ordering::SeqCst), 0);
    server.abort();
    db.cleanup().await.expect("cleanup");
}
