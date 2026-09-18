use super::*;
use mem_record::SupervisorArea as Area;
use mem_supervisor::{
    StableId, UsdMicros,
    action::{ActionKey, ActionKind},
    budget::ProviderRequest,
    proposal::{Proposal, ProposalKind},
};
use mem_supervisor_runtime::{launcher::SandboxConfig, work::WorkManifest};
use mem_supervisor_store::{
    ActionSubmission, ReadyTransition,
    sources::{self, LocalCollectionGrant, ModelDestination},
};
use mem_test_support::IsolatedDatabase;
use serde_json::{Value, json};
use sha2::{Digest, Sha256};
use sqlx::ConnectOptions;
use std::{
    os::unix::fs::PermissionsExt,
    sync::{Arc, Mutex},
};

struct Fixture {
    db: IsolatedDatabase,
    state: AppState,
    action: Uuid,
    canonical: Uuid,
    token_id: Uuid,
    token: String,
    reader: String,
    other: String,
    file: std::path::PathBuf,
}
fn stable(id: Uuid) -> StableId {
    StableId::new(id.to_string()).unwrap()
}
impl Fixture {
    async fn new() -> Option<Self> {
        let db = IsolatedDatabase::new().await?;
        let instance = Uuid::new_v4();
        let project = Uuid::new_v4();
        let revision = Uuid::new_v4();
        let action = Uuid::new_v4();
        let attempt = Uuid::new_v4();
        let canonical = Uuid::new_v4();
        let config=serde_json::from_value(json!({"service":{"bind_addr":"127.0.0.1:0","api_token":"synthetic-unused","read_only":true},"database":{"url":db.pool.connect_options().to_url_lossy().as_str()},"offline":{"enabled":false},"cluster":{"enabled":false},"reinforcement":{"enabled":false},"consolidation":{"enabled":false},"background_work":{"enabled":false},"mcp":{"enabled":false},"embeddings":{"enabled":false,"create_enabled":false},"supervisor":{"instance_id":instance,"area":"workshop"}})).unwrap();
        let mut state = crate::runtime::build_state(config, Arc::new(Mutex::new(None)))
            .await
            .unwrap();
        state.config.auth.mode = mem_record::AuthMode::MultiUser;
        state.config.service.read_only = false;
        sqlx::query("INSERT INTO projects(id,slug,name,root_path) VALUES($1,'memory','Synthetic','/private-never-disclose')").bind(project).execute(&db.pool).await.unwrap();
        sqlx::query("INSERT INTO memory_entries(id,project_id,canonical_id,version_no,is_tombstone,canonical_text,summary,memory_type,scope,importance,confidence,created_at,updated_at,search_document) VALUES($1,$2,$1,1,false,'SYNTHETIC_REVIEWED_FACT','Reviewed note','decision','project',3,0.8,now(),now(),to_tsvector('synthetic'))").bind(canonical).bind(project).execute(&db.pool).await.unwrap();
        let mut tx = db.pool.begin().await.unwrap();
        let source = sources::prepare_tx(&mut tx, project, &[canonical])
            .await
            .unwrap();
        tx.commit().await.unwrap();
        let grant = LocalCollectionGrant {
            instance_id: instance,
            project_id: project,
            principal_id: Uuid::new_v4(),
            policy_revision: revision,
            destination: ModelDestination::OpenAi,
            expires_at: chrono::Utc::now() + chrono::Duration::hours(1),
            sources: source.iter().map(|s| s.binding.clone()).collect(),
        };
        let question = "SYNTHETIC_PRIVATE_QUESTION";
        let prepared = PreparedRequest::new(
            question,
            &source.into_iter().map(|s| s.text).collect::<Vec<_>>(),
        )
        .unwrap();
        let store = SupervisorStore::new(db.pool.clone());
        store.bind_instance(instance, Area::Workshop).await.unwrap();
        store
            .install_policy(Area::Workshop, revision)
            .await
            .unwrap();
        store
            .set_enabled(Area::Workshop, revision, true)
            .await
            .unwrap();
        store
            .submit(&ActionSubmission {
                id: action,
                key: ActionKey {
                    area: Area::Workshop,
                    source_event: stable(Uuid::new_v4()),
                    kind: ActionKind::ProposeCorrection,
                    policy_revision: stable(revision),
                },
                source_digest: [1; 32],
            })
            .await
            .unwrap();
        for transition in [
            ReadyTransition::Propose,
            ReadyTransition::Authorize,
            ReadyTransition::Queue,
        ] {
            store
                .advance(Area::Workshop, action, transition, [2; 32])
                .await
                .unwrap();
        }
        let claim = store
            .claim(Area::Workshop, action, Uuid::new_v4(), 120)
            .await
            .unwrap();
        let digest: [u8; 32] = (0..32)
            .map(|i| u8::from_str_radix(&prepared.digest()[i * 2..i * 2 + 2], 16).unwrap())
            .collect::<Vec<_>>()
            .try_into()
            .unwrap();
        store
            .bind_model_work(
                &claim,
                instance,
                project,
                digest,
                prepared.evidence_digests(),
            )
            .await
            .unwrap();
        store
            .reserve(
                &claim,
                &ProviderRequest {
                    id: stable(attempt),
                    action_id: stable(action),
                    policy_revision: stable(revision),
                    model: StableId::new("gpt-5.6-sol").unwrap(),
                    worst_case_cost: UsdMicros(100),
                },
                digest,
            )
            .await
            .unwrap();
        store.begin(&claim, attempt).await.unwrap();
        store.settle(attempt, UsdMicros(1), [4; 32]).await.unwrap();
        store
            .publish_proposal(
                &claim,
                &attempt,
                project,
                &Proposal {
                    summary: "SYNTHETIC_PRIVATE_RESPONSE".into(),
                    proposal: ProposalKind::AskHuman,
                    evidence_indices: vec![0],
                },
                prepared.evidence_digests(),
            )
            .await
            .unwrap();
        use std::os::unix::fs::DirBuilderExt;
        let dir = std::env::temp_dir().join(format!("supervisor-profile-{}", Uuid::new_v4()));
        std::fs::DirBuilder::new().mode(0o700).create(&dir).unwrap();
        let file = dir.join("profile.json");
        let manifest = WorkManifest {
            schema_version: 1,
            action_id: action,
            source_event_id: Uuid::new_v4(),
            attempt_id: attempt,
            grant,
            question: question.into(),
            sandbox: SandboxConfig {
                pi: "/unused/pi".into(),
                driver: "/unused/driver".into(),
                driver_sha256: "0".repeat(64),
                private_runtime: "/unused/runtime".into(),
            },
        };
        let bytes = serde_json::to_vec(&manifest).unwrap();
        use std::io::Write;
        use std::os::unix::fs::OpenOptionsExt;
        std::fs::OpenOptions::new()
            .create_new(true)
            .write(true)
            .mode(0o600)
            .open(&file)
            .unwrap()
            .write_all(&bytes)
            .unwrap();
        assert_eq!(
            std::fs::metadata(&file).unwrap().permissions().mode() & 0o777,
            0o600
        );
        state.config.supervisor.as_mut().unwrap().interaction =
            Some(mem_config::SupervisorModelWorkConfig {
                manifest: file.clone(),
                sha256: hex(&Sha256::digest(&bytes)),
            });
        let token = Uuid::new_v4().to_string();
        let reader = Uuid::new_v4().to_string();
        let other = Uuid::new_v4().to_string();
        let token_id = Uuid::new_v4();
        for (credential, id, role) in [
            (&token, token_id, Some("operator")),
            (&reader, Uuid::new_v4(), Some("reader")),
            (&other, Uuid::new_v4(), None),
        ] {
            let principal = Uuid::new_v4();
            sqlx::query("INSERT INTO auth_principals(id,kind,display_name) VALUES($1,'service_token','Synthetic reviewer')").bind(principal).execute(&db.pool).await.unwrap();
            sqlx::query("INSERT INTO auth_service_tokens(id,principal_id,name,token_hash,token_prefix) VALUES($1,$2,'synthetic',$3,'synthetic')").bind(id).bind(principal).bind(crate::auth::hash_secret(credential)).execute(&db.pool).await.unwrap();
            if let Some(role) = role {
                sqlx::query("INSERT INTO auth_project_memberships(id,principal_id,project_id,role) VALUES($1,$2,$3,$4)").bind(Uuid::new_v4()).bind(principal).bind(project).bind(role).execute(&db.pool).await.unwrap();
            }
            if id == token_id {
                sqlx::query("INSERT INTO auth_web_sessions(id,principal_id,session_hash,csrf_hash,expires_at) VALUES($1,$2,$3,$4,clock_timestamp()+interval '1 hour')").bind(Uuid::new_v4()).bind(principal).bind(crate::auth::hash_secret("private-test-session")).bind(crate::auth::hash_secret("private-test-csrf")).execute(&db.pool).await.unwrap();
            }
        }
        Some(Self {
            db,
            state,
            action,
            canonical,
            token_id,
            token,
            reader,
            other,
            file,
        })
    }
    async fn server(&self, readonly: bool) -> (String, tokio::task::JoinHandle<()>) {
        let listener = tokio::net::TcpListener::bind("127.0.0.1:0").await.unwrap();
        let url = format!("http://{}", listener.local_addr().unwrap());
        let mut state = self.state.clone();
        state.config.service.read_only = readonly;
        state.config.auth.public_base_url = Some(url.clone());
        (
            url,
            tokio::spawn(async move {
                axum::serve(listener, crate::routes::build_http_app(state))
                    .await
                    .unwrap();
            }),
        )
    }
    async fn cleanup(self) {
        self.state.pool().unwrap().close().await;
        self.db.cleanup().await.unwrap();
        std::fs::remove_file(&self.file).unwrap();
        std::fs::remove_dir(self.file.parent().unwrap()).unwrap();
    }
}

#[tokio::test]
async fn private_results_require_current_project_access_and_reviews_are_atomic_idempotent_not_execution()
 {
    let Some(f) = Fixture::new().await else {
        return;
    };
    let (url, server) = f.server(false).await;
    let client = reqwest::Client::new();
    let endpoint = format!("{url}/v1/projects/memory/supervisor/proposals/{}", f.action);
    assert_eq!(
        client
            .get(&endpoint)
            .bearer_auth(&f.other)
            .send()
            .await
            .unwrap()
            .status(),
        403
    );
    let response = client
        .get(&endpoint)
        .bearer_auth(&f.reader)
        .send()
        .await
        .unwrap();
    assert_eq!(response.status(), 200);
    assert_eq!(response.headers()[header::CACHE_CONTROL], "no-store");
    let detail: Value = response.json().await.unwrap();
    assert_eq!(detail["summary"], "SYNTHETIC_PRIVATE_RESPONSE");
    assert_eq!(detail["can_review"], false);
    assert_eq!(
        detail["citations"][0]["version_id"],
        f.canonical.to_string()
    );
    assert!(!detail.to_string().contains("/private-never-disclose"));
    let request = json!({"request_id":Uuid::new_v4(),"proposal_digest":detail["proposal_digest"],"decision":"approve"});
    let review = format!("{endpoint}/review");
    assert_eq!(
        client
            .post(&review)
            .bearer_auth(&f.reader)
            .json(&request)
            .send()
            .await
            .unwrap()
            .status(),
        403
    );
    assert_eq!(
        client
            .post(&review)
            .header(
                header::COOKIE,
                "memory_session=private-test-session; memory_csrf=private-test-csrf"
            )
            .json(&request)
            .send()
            .await
            .unwrap()
            .status(),
        403
    );
    let (a, b) = tokio::join!(
        client
            .post(&review)
            .bearer_auth(&f.token)
            .json(&request)
            .send(),
        client
            .post(&review)
            .bearer_auth(&f.token)
            .json(&request)
            .send()
    );
    let a = a.unwrap();
    let b = b.unwrap();
    assert_eq!(a.status(), 200);
    assert_eq!(b.status(), 200);
    assert_eq!(
        a.json::<Value>().await.unwrap(),
        b.json::<Value>().await.unwrap()
    );
    let mut changed = request.clone();
    changed["decision"] = json!("reject");
    assert_eq!(
        client
            .post(&review)
            .bearer_auth(&f.token)
            .json(&changed)
            .send()
            .await
            .unwrap()
            .status(),
        409
    );
    let counts:(i64,i64,i64)=sqlx::query_as("SELECT (SELECT count(*) FROM supervisor_proposal_reviews),(SELECT count(*) FROM supervisor_actions),(SELECT count(*) FROM supervisor_attempts)").fetch_one(&f.db.pool).await.unwrap();
    assert_eq!(counts, (1, 1, 1));
    let feed = serde_json::to_string(
        &SupervisorStore::new(f.db.pool.clone())
            .journal(Area::Workshop, 0, 100)
            .await
            .unwrap(),
    )
    .unwrap();
    assert!(!feed.contains("SYNTHETIC_PRIVATE"));
    sqlx::query("UPDATE auth_service_tokens SET revoked_at=clock_timestamp() WHERE id=$1")
        .bind(f.token_id)
        .execute(&f.db.pool)
        .await
        .unwrap();
    assert_eq!(
        client
            .get(&endpoint)
            .bearer_auth(&f.token)
            .send()
            .await
            .unwrap()
            .status(),
        401
    );
    server.abort();
    f.cleanup().await;
}

#[tokio::test]
async fn private_results_fail_closed_for_withdrawal_and_readonly_reviews_and_failed_commit() {
    let Some(f) = Fixture::new().await else {
        return;
    };
    let (url, server) = f.server(true).await;
    let client = reqwest::Client::new();
    let path = format!("/v1/projects/memory/supervisor/proposals/{}", f.action);
    let detail: Value = client
        .get(format!("{url}{path}"))
        .bearer_auth(&f.token)
        .send()
        .await
        .unwrap()
        .json()
        .await
        .unwrap();
    assert_eq!(detail["can_review"], false);
    let request = json!({"request_id":Uuid::new_v4(),"proposal_digest":detail["proposal_digest"],"decision":"reject"});
    assert_eq!(
        client
            .post(format!("{url}{path}/review"))
            .bearer_auth(&f.token)
            .json(&request)
            .send()
            .await
            .unwrap()
            .status(),
        403
    );
    server.abort();
    let (url, server) = f.server(false).await;
    sqlx::raw_sql("CREATE FUNCTION fail_review() RETURNS trigger LANGUAGE plpgsql AS $$ BEGIN RAISE EXCEPTION 'SYNTHETIC_PRIVATE_DATABASE_DETAIL'; END $$; CREATE CONSTRAINT TRIGGER fail_review AFTER INSERT ON supervisor_proposal_reviews DEFERRABLE INITIALLY DEFERRED FOR EACH ROW EXECUTE FUNCTION fail_review();").execute(&f.db.pool).await.unwrap();
    let failed = client
        .post(format!("{url}{path}/review"))
        .bearer_auth(&f.token)
        .json(&request)
        .send()
        .await
        .unwrap();
    assert_eq!(failed.status(), 503);
    assert!(!failed.text().await.unwrap().contains("SYNTHETIC_PRIVATE"));
    let n: i64 = sqlx::query_scalar("SELECT count(*) FROM supervisor_proposal_reviews")
        .fetch_one(&f.db.pool)
        .await
        .unwrap();
    assert_eq!(n, 0);
    sqlx::query("UPDATE memory_canonical_state SET status='archived' WHERE canonical_id=$1")
        .bind(f.canonical)
        .execute(&f.db.pool)
        .await
        .unwrap();
    assert_eq!(
        client
            .get(format!("{url}{path}"))
            .bearer_auth(&f.token)
            .send()
            .await
            .unwrap()
            .status(),
        409
    );
    server.abort();
    f.cleanup().await;
}
