// SPDX-License-Identifier: AGPL-3.0-or-later
use super::*;
use axum::{Json, Router, extract::State, http::StatusCode, routing::post};
use mem_record::JobStatus;
use std::sync::{Mutex, atomic::AtomicUsize};

#[derive(Clone)]
struct Fake {
    calls: Arc<AtomicUsize>,
    mode: usize,
}
async fn embed(State(fake): State<Fake>, Json(body): Json<Value>) -> (StatusCode, Json<Value>) {
    let call = fake.calls.fetch_add(1, Ordering::SeqCst);
    if (fake.mode == 1 && call == 0) || (fake.mode == 3 && call == 1) {
        return (
            StatusCode::SERVICE_UNAVAILABLE,
            Json(serde_json::json!({"error":"synthetic unavailable"})),
        );
    }
    if fake.mode == 2 && call == 0 {
        tokio::time::sleep(Duration::from_secs(8)).await;
    }
    let n = body["input"].as_array().map_or(1, Vec::len);
    (
        StatusCode::OK,
        Json(
            serde_json::json!({"data":(0..n).map(|i|serde_json::json!({"index":i,"embedding":[0.25,0.75]})).collect::<Vec<_>>()}),
        ),
    )
}

struct Fixture {
    admin: PgPool,
    database: String,
    state: AppState,
    pool: PgPool,
    project: Uuid,
    slug: String,
    operation: Uuid,
    versions: Vec<Uuid>,
    fake: Fake,
    server: JoinHandle<()>,
}
impl Fixture {
    async fn new(mode: usize) -> Option<Self> {
        let admin = mem_test_support::migrated_pool().await?;
        // Workers intentionally claim across projects. Each service fixture needs
        // its own database, otherwise parallel fake providers consume each other's jobs.
        let database = format!("r1_worker_{}", Uuid::new_v4().simple());
        sqlx::query(&format!("CREATE DATABASE {database}"))
            .execute(&admin)
            .await
            .unwrap();
        let mut database_url =
            reqwest::Url::parse(&std::env::var("MEMORY_LAYER_TEST_DATABASE_URL").unwrap()).unwrap();
        database_url.set_path(&database);
        let pool = PgPool::connect(database_url.as_str()).await.unwrap();
        mem_test_support::run_migrations(&pool).await.unwrap();
        let project = Uuid::new_v4();
        let slug = project.to_string();
        let operation = Uuid::new_v4();
        let version = Uuid::new_v4();
        sqlx::query("INSERT INTO projects(id,slug,name,root_path) VALUES($1,$2,'background fixture','/disposable')").bind(project).bind(&slug).execute(&pool).await.unwrap();
        sqlx::query("INSERT INTO memory_entries(id,project_id,canonical_id,version_no,is_tombstone,canonical_text,summary,memory_type,scope,importance,confidence,created_at,updated_at,search_document) VALUES($1,$2,$1,1,false,'Durable input.','Durable input','implementation','project',3,0.9,now(),now(),to_tsvector('english','durable'))")
            .bind(version).bind(project).execute(&pool).await.unwrap();
        let fake = Fake {
            calls: Arc::new(AtomicUsize::new(0)),
            mode,
        };
        let app = Router::new()
            .route("/v1/embeddings", post(embed))
            .with_state(fake.clone());
        let listener = tokio::net::TcpListener::bind("127.0.0.1:0").await.unwrap();
        let addr = listener.local_addr().unwrap();
        let server = tokio::spawn(async move {
            axum::serve(listener, app).await.unwrap();
        });
        let config=serde_json::from_value(serde_json::json!({
            "service":{"bind_addr":"127.0.0.1:0","api_token":"fixture-only"},
            "database":{"url":database_url.as_str()},
            "offline":{"enabled":false},"cluster":{"enabled":false},"reinforcement":{"enabled":false},"consolidation":{"enabled":false},
            "embeddings":{"create_enabled":true,"backends":[{"name":"fixture","provider":"ollama","base_url":format!("http://{addr}/v1"),"model":"fixture","create_enabled":true}]},
            "background_work":{"poll_seconds":1,"lease_seconds":4,"renewal_seconds":1,"initial_delay_seconds":1,"max_delay_seconds":2,"max_attempts":2},
            "mcp":{"enabled":false}
        })).unwrap();
        let state = crate::runtime::build_state(config, Arc::new(Mutex::new(None)))
            .await
            .unwrap();
        Some(Self {
            admin,
            database,
            state,
            pool,
            project,
            slug,
            operation,
            versions: vec![version],
            fake,
            server,
        })
    }
    async fn enqueue(&self, actor: JobActor) -> Uuid {
        let mut tx = self.pool.begin().await.unwrap();
        let work = mem_store::post_curation::enqueue(
            &mut tx,
            self.project,
            self.operation,
            &self.versions,
            &actor,
        )
        .await
        .unwrap();
        tx.commit().await.unwrap();
        work.job_ids[0]
    }
    async fn wait(&self, id: Uuid, status: JobStatus) -> mem_record::JobDetail {
        tokio::time::timeout(Duration::from_secs(20), async {
            loop {
                let detail = jobs::detail(&self.pool, self.project, id).await.unwrap();
                if detail.job.status == status {
                    return detail;
                }
                tokio::time::sleep(Duration::from_millis(50)).await;
            }
        })
        .await
        .expect("worker did not reach expected status")
    }
    async fn cleanup(self) {
        self.server.abort();
        mem_test_support::cleanup_project(&self.pool, &self.slug)
            .await
            .unwrap();
        self.state.pool().unwrap().close().await;
        self.pool.close().await;
        sqlx::query(&format!("DROP DATABASE {} WITH (FORCE)", self.database))
            .execute(&self.admin)
            .await
            .unwrap();
    }
}

#[tokio::test]
async fn worker_retries_provider_failure_and_completes_independent_stages() {
    let Some(f) = Fixture::new(1).await else {
        return;
    };
    let id = f.enqueue(JobActor::System("local-curation".into())).await;
    let workers = start(f.state.clone()).unwrap();
    let done = f.wait(id, JobStatus::Succeeded).await;
    assert_eq!(done.job.attempts, 2);
    assert_eq!(f.fake.calls.load(Ordering::SeqCst), 2);
    assert!(
        done.history
            .iter()
            .any(|attempt| attempt.failure == Some(Code::Transient))
    );
    let validation = Uuid::new_v5(&f.operation, b"validation_due");
    assert_eq!(
        f.wait(validation, JobStatus::Skipped).await.job.reason,
        Some(JobReason::Disabled)
    );
    f.wait(
        Uuid::new_v5(&f.operation, b"semantic_dedup"),
        JobStatus::Succeeded,
    )
    .await;
    f.wait(
        Uuid::new_v5(&f.operation, b"consolidation"),
        JobStatus::Skipped,
    )
    .await;
    let count: i64 =
        sqlx::query_scalar("SELECT count(*) FROM background_job_outputs WHERE job_id=$1")
            .bind(id)
            .fetch_one(&f.pool)
            .await
            .unwrap();
    assert_eq!(count, 2); // reusable output and atomic version-completed marker
    drop(workers);
    f.cleanup().await;
}

#[tokio::test]
async fn partial_embedding_success_does_not_repeat_completed_versions_on_retry() {
    let Some(mut f) = Fixture::new(3).await else {
        return;
    };
    let version = Uuid::new_v4();
    sqlx::query("INSERT INTO memory_entries(id,project_id,canonical_id,version_no,is_tombstone,canonical_text,summary,memory_type,scope,importance,confidence,created_at,updated_at,search_document) VALUES($1,$2,$1,1,false,'Second durable input.','Second input','implementation','project',3,0.9,now(),now(),to_tsvector('english','second'))").bind(version).bind(f.project).execute(&f.pool).await.unwrap();
    f.versions.push(version);
    let id = f.enqueue(JobActor::System("local-curation".into())).await;
    let workers = start(f.state.clone()).unwrap();
    let done = f.wait(id, JobStatus::Succeeded).await;
    assert_eq!(done.job.attempts, 2);
    assert_eq!(f.fake.calls.load(Ordering::SeqCst), 3);
    let events: i64 = sqlx::query_scalar(
        "SELECT count(*) FROM project_timeline_events WHERE project_id=$1 AND kind='reembed'",
    )
    .bind(f.project)
    .fetch_one(&f.pool)
    .await
    .unwrap();
    assert_eq!(
        events, 2,
        "each committed version gets one event, despite retry"
    );
    drop(workers);
    f.cleanup().await;
}

#[tokio::test]
async fn revoked_authority_during_provider_execution_fences_every_output() {
    let Some(f) = Fixture::new(2).await else {
        return;
    };
    let principal = Uuid::new_v4();
    let token = Uuid::new_v4();
    sqlx::query("INSERT INTO auth_principals(id,kind,display_name,global_role) VALUES($1,'internal','fixture','operator')").bind(principal).execute(&f.pool).await.unwrap();
    sqlx::query("INSERT INTO auth_service_tokens(id,principal_id,name,token_hash,token_prefix) VALUES($1,$2,'fixture',$3,'fixture')").bind(token).bind(principal).bind(token.as_bytes().as_slice()).execute(&f.pool).await.unwrap();
    let id = f
        .enqueue(JobActor::Credential {
            principal,
            token: Some(token),
            session: None,
        })
        .await;
    let workers = start(f.state.clone()).unwrap();
    tokio::time::timeout(Duration::from_secs(5), async {
        while f.fake.calls.load(Ordering::SeqCst) == 0 {
            tokio::time::sleep(Duration::from_millis(20)).await;
        }
    })
    .await
    .unwrap();
    sqlx::query("UPDATE auth_service_tokens SET revoked_at=now() WHERE id=$1")
        .bind(token)
        .execute(&f.pool)
        .await
        .unwrap();
    let done = f.wait(id, JobStatus::Blocked).await;
    assert_eq!(done.job.last_failure, Some(Code::Authorization));
    let count: i64 =
        sqlx::query_scalar("SELECT count(*) FROM background_job_outputs WHERE job_id=$1")
            .bind(id)
            .fetch_one(&f.pool)
            .await
            .unwrap();
    assert_eq!(count, 0);
    assert!(
        done.history[0].uncertain,
        "revocation prevented an output receipt after the provider accepted work"
    );
    let count: i64 = sqlx::query_scalar("SELECT count(*) FROM memory_chunks WHERE memory_entry_id IN (SELECT id FROM memory_entries WHERE project_id=$1)")
        .bind(f.project)
        .fetch_one(&f.pool)
        .await
        .unwrap();
    assert_eq!(count, 0);
    drop(workers);
    sqlx::query("DELETE FROM auth_principals WHERE id=$1")
        .bind(principal)
        .execute(&f.pool)
        .await
        .unwrap();
    f.cleanup().await;
}

#[tokio::test]
async fn worker_health_stays_stale_on_storage_failure_and_recovers_with_pool() {
    let Some(f) = Fixture::new(0).await else {
        return;
    };
    let id = f.enqueue(JobActor::System("local-curation".into())).await;
    let original = f.state.pool().unwrap();
    original.close().await;
    let workers = start(f.state.clone()).unwrap();
    tokio::time::sleep(Duration::from_millis(1200)).await;
    assert_eq!(f.state.background.last_poll.load(Ordering::Relaxed), 0);
    assert_eq!(f.fake.calls.load(Ordering::SeqCst), 0);
    // Runtime database reconnection replaces the shared pool. No new enqueue
    // or notification is required for an existing supervisor to recover.
    f.state.set_pool(f.pool.clone());
    f.wait(id, JobStatus::Succeeded).await;
    assert!(f.state.background.last_poll.load(Ordering::Relaxed) > 0);
    drop(workers);
    f.cleanup().await;
}

#[tokio::test]
async fn utility_floor_read_fences_concurrent_reward_until_result_commit() {
    let Some(pool) = mem_test_support::migrated_pool().await else {
        return;
    };
    let project = Uuid::new_v4();
    sqlx::query(
        "INSERT INTO projects(id,slug,name,root_path) VALUES($1,$2,'utility lock','/synthetic')",
    )
    .bind(project)
    .bind(project.to_string())
    .execute(&pool)
    .await
    .unwrap();
    let config = mem_config::ProceduralConfig {
        enabled: true,
        utility_floor_enabled: true,
        min_samples: 1,
        utility_floor: 0.0,
        ..Default::default()
    };
    let mut result = pool.begin().await.unwrap();
    assert!(
        !utility_suppressed_tx(&mut result, project, &config)
            .await
            .unwrap()
    );
    let mut reward = pool.begin().await.unwrap();
    let params = mem_reinforce::UtilityParams::default();
    {
        let apply = mem_reinforce::repository::apply_utility_reward_tx(
            &mut reward,
            project,
            "loop",
            mem_loops::LOOP_MEMORY_CONSOLIDATION,
            -1.0,
            &params,
            "fixture",
            serde_json::json!({}),
        );
        tokio::pin!(apply);
        assert!(
            tokio::time::timeout(Duration::from_millis(100), &mut apply)
                .await
                .is_err()
        );
        result.commit().await.unwrap();
        apply.await.unwrap();
    }
    reward.commit().await.unwrap();
    let mut next = pool.begin().await.unwrap();
    assert!(
        utility_suppressed_tx(&mut next, project, &config)
            .await
            .unwrap()
    );
    next.rollback().await.unwrap();
    sqlx::query("DELETE FROM projects WHERE id=$1")
        .bind(project)
        .execute(&pool)
        .await
        .unwrap();
}

#[tokio::test]
async fn queued_manual_consolidation_rechecks_run_cancellation_and_current_settings() {
    let Some(mut f) = Fixture::new(0).await else {
        return;
    };
    crate::repository::handlers::loops::register_builtin_loop_definitions(&f.pool)
        .await
        .unwrap();
    f.state.config.consolidation.enabled = true;
    f.state.config.consolidation.auto_trigger = false;
    let run = Uuid::new_v4();
    let job = Uuid::new_v4();
    sqlx::query("INSERT INTO loop_runs(id,loop_id,definition_version,project_id,scope_type,scope_id,mode,status) VALUES($1,$2,1,$3,'project',$4,'manual','succeeded')")
        .bind(run).bind(mem_loops::LOOP_MEMORY_CONSOLIDATION).bind(f.project).bind(&f.slug).execute(&f.pool).await.unwrap();
    sqlx::query("INSERT INTO loop_settings(id,loop_id,scope_type,scope_id,project_id,enabled,mode) VALUES($1,$2,'project',$3,$4,true,'suggest_only')")
        .bind(Uuid::new_v4()).bind(mem_loops::LOOP_MEMORY_CONSOLIDATION).bind(&f.slug).bind(f.project).execute(&f.pool).await.unwrap();
    let mut tx = f.pool.begin().await.unwrap();
    jobs::enqueue(
        &mut tx,
        &NewJob {
            id: job,
            project_id: f.project,
            operation_id: run,
            kind: JobKind::Consolidation,
            work_key: format!("manual:{run}"),
            actor: JobActor::System("local-curation".into()),
            input: serde_json::json!({"version":1,"run_id":run,"manual":true,"dry_run":true}),
            dependencies: vec![],
        },
    )
    .await
    .unwrap();
    tx.commit().await.unwrap();
    let lease = jobs::claim(&f.pool, policy(&f.state))
        .await
        .unwrap()
        .unwrap();
    jobs::checkpoint_result(authorized(&f.state, &f.pool, &lease).await.unwrap())
        .await
        .unwrap();
    sqlx::query("UPDATE loop_settings SET enabled=false WHERE project_id=$1")
        .bind(f.project)
        .execute(&f.pool)
        .await
        .unwrap();
    assert_eq!(
        classify(&authorized(&f.state, &f.pool, &lease).await.err().unwrap()),
        Code::Authorization
    );
    sqlx::query("UPDATE loop_settings SET enabled=true WHERE project_id=$1")
        .bind(f.project)
        .execute(&f.pool)
        .await
        .unwrap();
    sqlx::query("UPDATE loop_runs SET cancel_requested_at=now() WHERE id=$1")
        .bind(run)
        .execute(&f.pool)
        .await
        .unwrap();
    assert_eq!(
        classify(&authorized(&f.state, &f.pool, &lease).await.err().unwrap()),
        Code::Authorization
    );
    assert_eq!(f.fake.calls.load(Ordering::SeqCst), 0);
    f.cleanup().await;
}

#[tokio::test]
async fn shutdown_cancels_owned_work_and_a_new_worker_recovers_lease() {
    let Some(f) = Fixture::new(2).await else {
        return;
    };
    let id = f.enqueue(JobActor::System("local-curation".into())).await;
    let workers = start(f.state.clone()).unwrap();
    tokio::time::timeout(Duration::from_secs(5), async {
        while f.fake.calls.load(Ordering::SeqCst) == 0 {
            tokio::time::sleep(Duration::from_millis(20)).await;
        }
    })
    .await
    .unwrap();
    drop(workers);
    // A fresh supervisor recovers by database polling, without an enqueue notification.
    let replacement = start(f.state.clone()).unwrap();
    let done = f.wait(id, JobStatus::Succeeded).await;
    assert_eq!(done.job.attempts, 2);
    assert!(
        done.history
            .iter()
            .any(|a| a.failure == Some(Code::LeaseLost) && a.uncertain)
    );
    assert_eq!(f.fake.calls.load(Ordering::SeqCst), 2);
    drop(replacement);
    f.cleanup().await;
}

#[tokio::test]
async fn revoked_credential_blocks_worker_before_provider_call() {
    let Some(f) = Fixture::new(0).await else {
        return;
    };
    let principal = Uuid::new_v4();
    let token = Uuid::new_v4();
    sqlx::query("INSERT INTO auth_principals(id,kind,display_name,global_role) VALUES($1,'internal','fixture','operator')").bind(principal).execute(&f.pool).await.unwrap();
    sqlx::query("INSERT INTO auth_service_tokens(id,principal_id,name,token_hash,token_prefix,revoked_at) VALUES($1,$2,'fixture',$3,'fixture',now())")
        .bind(token).bind(principal).bind(token.as_bytes().as_slice()).execute(&f.pool).await.unwrap();
    let id = f
        .enqueue(JobActor::Credential {
            principal,
            token: Some(token),
            session: None,
        })
        .await;
    let workers = start(f.state.clone()).unwrap();
    let done = f.wait(id, JobStatus::Blocked).await;
    assert_eq!(done.job.last_failure, Some(Code::Authorization));
    assert_eq!(f.fake.calls.load(Ordering::SeqCst), 0);
    drop(workers);
    sqlx::query("DELETE FROM auth_principals WHERE id=$1")
        .bind(principal)
        .execute(&f.pool)
        .await
        .unwrap();
    f.cleanup().await;
}

#[test]
fn provider_classification_is_bounded_and_never_exposes_body() {
    for (message, code) in [
        ("429 insufficient_quota secret body", Code::Quota),
        ("429 rate limit", Code::RateLimited),
        ("401 invalid key", Code::Credentials),
        ("403 forbidden", Code::Authorization),
        ("503 unavailable", Code::Transient),
        ("unexpected format", Code::Configuration),
    ] {
        assert_eq!(classify(&anyhow::anyhow!(message)), code);
    }
}

#[tokio::test]
async fn failed_timeline_appends_never_broadcast_committed_activity() {
    let Some(f) = Fixture::new(0).await else {
        return;
    };
    let mut receiver = f.state.events.subscribe();
    crate::repository::events::notify_project_changed(
        &f.state,
        f.slug.clone(),
        None,
        ActivityKind::Checkpoint,
        "\0".into(),
        None,
    );
    assert!(
        tokio::time::timeout(Duration::from_millis(100), receiver.recv())
            .await
            .is_err()
    );
    assert!(f.state.recent_activity.lock().unwrap().is_empty());
    crate::repository::events::notify_project_changed(
        &f.state,
        f.slug.clone(),
        None,
        ActivityKind::Checkpoint,
        "committed".into(),
        None,
    );
    let event = tokio::time::timeout(Duration::from_secs(2), receiver.recv())
        .await
        .unwrap()
        .unwrap();
    assert!(event.seq.is_some());
    assert_eq!(event.summary, "committed");
    f.cleanup().await;
}

#[tokio::test]
async fn jobs_http_inspection_retry_and_project_policy_are_real_and_sanitized() {
    let Some(mut f) = Fixture::new(0).await else {
        return;
    };
    let id = f.enqueue(JobActor::System("local-curation".into())).await;
    let principal = Uuid::new_v4();
    let token = Uuid::new_v4().to_string();
    sqlx::query("INSERT INTO auth_principals(id,kind,display_name) VALUES($1,'service_token','job operator')").bind(principal).execute(&f.pool).await.unwrap();
    sqlx::query("INSERT INTO auth_project_memberships(id,principal_id,project_id,role) VALUES($1,$2,$3,'operator')").bind(Uuid::new_v4()).bind(principal).bind(f.project).execute(&f.pool).await.unwrap();
    sqlx::query("INSERT INTO auth_service_tokens(id,principal_id,name,token_hash,token_prefix) VALUES($1,$2,'fixture',$3,'fixture')")
        .bind(Uuid::new_v4()).bind(principal).bind(crate::auth::hash_secret(&token)).execute(&f.pool).await.unwrap();
    f.state.config.auth.mode = mem_record::AuthMode::MultiUser;
    let listener = tokio::net::TcpListener::bind("127.0.0.1:0").await.unwrap();
    let base = format!("http://{}/v1/projects", listener.local_addr().unwrap());
    let origin = base.trim_end_matches("/v1/projects").to_owned();
    f.state.config.auth.public_base_url = Some(origin.clone());
    let app = crate::routes::build_http_app(f.state.clone());
    let server = tokio::spawn(async move {
        axum::serve(
            listener,
            app.into_make_service_with_connect_info::<std::net::SocketAddr>(),
        )
        .await
        .unwrap();
    });
    let client = reqwest::Client::new();
    let url = format!("{base}/{}/jobs", f.slug);
    let ingest_url = format!("{}/v1/activity", base.trim_end_matches("/v1/projects"));
    let mut activity = serde_json::json!({"activity":"checkpoint","project":f.slug,"checkpoint":{"project":f.slug,"repo_root":"/fixture","marked_at":chrono::Utc::now(),"note":"checkpoint receipt"}});
    let logged: Value = client
        .post(&ingest_url)
        .bearer_auth(&token)
        .json(&activity)
        .send()
        .await
        .unwrap()
        .error_for_status()
        .unwrap()
        .json()
        .await
        .unwrap();
    let receipt: (Option<String>, String) =
        sqlx::query_as("SELECT actor_id,summary FROM project_timeline_events WHERE seq=$1")
            .bind(logged["seq"].as_i64().unwrap())
            .fetch_one(&f.pool)
            .await
            .unwrap();
    assert_eq!(receipt.0, Some(principal.to_string()));
    assert!(receipt.1.contains("checkpoint receipt"));
    activity["checkpoint"]["note"] = serde_json::json!("\0");
    assert_eq!(
        client
            .post(&ingest_url)
            .bearer_auth(&token)
            .json(&activity)
            .send()
            .await
            .unwrap()
            .status(),
        StatusCode::SERVICE_UNAVAILABLE
    );
    let before_cursor = logged["seq"].as_i64().unwrap();
    // The existing activity endpoint exposes commit-safe ordered recovery.
    let activity_url = format!("{base}/{}/activities", f.slug);
    let mut first = f.pool.begin().await.unwrap();
    let first_event = TimelineEvent::new(
        f.project,
        ActivityKind::Checkpoint,
        "earlier transaction".into(),
    );
    let first_seq = mem_store::events::append(&mut first, &first_event)
        .await
        .unwrap();
    let second_pool = f.pool.clone();
    let project = f.project;
    let mut second = tokio::spawn(async move {
        let mut tx = second_pool.begin().await.unwrap();
        let event = TimelineEvent::new(
            project,
            ActivityKind::Checkpoint,
            "later transaction".into(),
        );
        let seq = mem_store::events::append(&mut tx, &event).await.unwrap();
        tx.commit().await.unwrap();
        seq
    });
    assert!(
        tokio::time::timeout(Duration::from_millis(50), &mut second)
            .await
            .is_err()
    );
    let pending: Value = client
        .get(format!("{activity_url}?after_seq={before_cursor}&limit=1"))
        .bearer_auth(&token)
        .send()
        .await
        .unwrap()
        .error_for_status()
        .unwrap()
        .json()
        .await
        .unwrap();
    assert_eq!(pending["items"].as_array().unwrap().len(), 0);
    first.commit().await.unwrap();
    let second_seq = second.await.unwrap();
    assert!(second_seq > first_seq);
    let first_page: Value = client
        .get(format!("{activity_url}?after_seq={before_cursor}&limit=1"))
        .bearer_auth(&token)
        .send()
        .await
        .unwrap()
        .error_for_status()
        .unwrap()
        .json()
        .await
        .unwrap();
    assert_eq!(first_page["items"][0]["seq"], first_seq);
    assert_eq!(first_page["cursor"]["has_more"], true);
    assert_eq!(first_page["cursor"]["retention_gap"], false);
    let next_page: Value = client
        .get(format!("{activity_url}?after_seq={first_seq}&limit=1"))
        .bearer_auth(&token)
        .send()
        .await
        .unwrap()
        .error_for_status()
        .unwrap()
        .json()
        .await
        .unwrap();
    assert_eq!(next_page["items"][0]["seq"], second_seq);
    sqlx::query("DELETE FROM project_timeline_events WHERE id=$1")
        .bind(first_event.id)
        .execute(&f.pool)
        .await
        .unwrap();
    let gap: Value = client
        .get(format!("{activity_url}?after_seq=0"))
        .bearer_auth(&token)
        .send()
        .await
        .unwrap()
        .error_for_status()
        .unwrap()
        .json()
        .await
        .unwrap();
    assert_eq!(gap["cursor"]["retention_gap"], true);
    assert_eq!(gap["cursor"]["deleted_through"], first_seq);
    for query in [
        "after_seq=-1",
        "after_seq=0&limit=201",
        "after_seq=0&kind=checkpoint",
    ] {
        assert_eq!(
            client
                .get(format!("{activity_url}?{query}"))
                .bearer_auth(&token)
                .send()
                .await
                .unwrap()
                .status(),
            StatusCode::BAD_REQUEST
        );
    }
    let page: Value = client
        .get(format!("{url}?limit=1"))
        .bearer_auth(&token)
        .send()
        .await
        .unwrap()
        .error_for_status()
        .unwrap()
        .json()
        .await
        .unwrap();
    assert_eq!(page["jobs"].as_array().unwrap().len(), 1);
    assert!(page["next_after"].is_string());
    assert_eq!(page["stats"]["queued"], 4);
    let detail = client
        .get(format!("{url}/{id}"))
        .bearer_auth(&token)
        .send()
        .await
        .unwrap()
        .text()
        .await
        .unwrap();
    assert!(!detail.contains("Durable input"));
    assert!(!detail.contains("config_fingerprint"));
    assert!(!detail.contains("cluster_snapshot"));
    let detail: Value = serde_json::from_str(&detail).unwrap();
    assert_eq!(detail["stages"].as_array().unwrap().len(), 4);
    assert_eq!(
        client
            .get(format!("{base}/another-project/jobs"))
            .bearer_auth(&token)
            .send()
            .await
            .unwrap()
            .status(),
        StatusCode::FORBIDDEN
    );
    sqlx::query("UPDATE background_jobs SET status='blocked',last_failure='quota' WHERE id=$1")
        .bind(id)
        .execute(&f.pool)
        .await
        .unwrap();
    // Only independent validation remains ready. Once it finishes, children
    // waiting for the blocked embedding must not inflate oldest-ready age.
    let validation = Uuid::new_v5(&f.operation, b"validation_due");
    sqlx::query("UPDATE background_jobs SET status='skipped' WHERE id=$1")
        .bind(validation)
        .execute(&f.pool)
        .await
        .unwrap();
    let waiting: Value = client
        .get(&url)
        .bearer_auth(&token)
        .send()
        .await
        .unwrap()
        .error_for_status()
        .unwrap()
        .json()
        .await
        .unwrap();
    assert_eq!(waiting["stats"]["queued"], 2);
    assert!(waiting["stats"]["oldest_ready_age_seconds"].is_null());
    let request = serde_json::json!({"request_id":Uuid::new_v4()});
    let (session, csrf) = crate::auth::mint_web_session(&f.state, &f.pool, principal)
        .await
        .unwrap();
    let cookies = format!("memory_session={session}; memory_csrf={csrf}");
    for (origin_header, token_header) in [
        (origin.as_str(), None),
        ("https://untrusted.invalid", Some(csrf.as_str())),
        (origin.as_str(), Some("wrong-token")),
    ] {
        let mut browser = client
            .post(format!("{url}/{id}/retry"))
            .header("cookie", &cookies)
            .header("origin", origin_header)
            .json(&request);
        if let Some(token_header) = token_header {
            browser = browser.header("x-csrf-token", token_header);
        }
        assert_eq!(
            browser.send().await.unwrap().status(),
            StatusCode::FORBIDDEN
        );
    }
    assert_eq!(
        jobs::detail(&f.pool, f.project, id)
            .await
            .unwrap()
            .job
            .generation,
        1
    );
    assert_eq!(
        client
            .post(format!("{url}/{id}/retry"))
            .header("cookie", &cookies)
            .header("origin", &origin)
            .header("x-csrf-token", &csrf)
            .json(&request)
            .send()
            .await
            .unwrap()
            .status(),
        StatusCode::OK
    );
    for _ in 0..2 {
        let response = client
            .post(format!("{url}/{id}/retry"))
            .header("cookie", &cookies)
            .header("origin", &origin)
            .header("x-csrf-token", &csrf)
            .json(&request)
            .send()
            .await
            .unwrap();
        assert_eq!(
            response.status(),
            StatusCode::OK,
            "{}",
            response.text().await.unwrap()
        );
    }
    assert_eq!(
        jobs::detail(&f.pool, f.project, id)
            .await
            .unwrap()
            .job
            .generation,
        2
    );
    assert_eq!(
        client
            .post(format!("{url}/{id}/retry"))
            .bearer_auth(&token)
            .json(&serde_json::json!({"request_id":Uuid::new_v4()}))
            .send()
            .await
            .unwrap()
            .status(),
        StatusCode::CONFLICT
    );
    server.abort();
    f.state.config.service.read_only = true;
    let listener = tokio::net::TcpListener::bind("127.0.0.1:0").await.unwrap();
    let address = listener.local_addr().unwrap();
    let app = crate::routes::build_http_app(f.state.clone());
    let readonly = tokio::spawn(async move {
        axum::serve(
            listener,
            app.into_make_service_with_connect_info::<std::net::SocketAddr>(),
        )
        .await
        .unwrap();
    });
    assert_eq!(
        client
            .post(format!(
                "http://{address}/v1/projects/{}/jobs/{id}/retry",
                f.slug
            ))
            .bearer_auth(&token)
            .json(&request)
            .send()
            .await
            .unwrap()
            .status(),
        StatusCode::FORBIDDEN
    );
    readonly.abort();
    sqlx::query("DELETE FROM auth_principals WHERE id=$1")
        .bind(principal)
        .execute(&f.pool)
        .await
        .unwrap();
    f.cleanup().await;
}
