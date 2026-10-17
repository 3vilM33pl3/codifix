// SPDX-License-Identifier: AGPL-3.0-or-later
use super::*;
use codifix_test_support::fingerprint::benchmark_fingerprint;
use sqlx::ConnectOptions;

#[tokio::test]
async fn sealed_http_queries_authenticate_without_mutating_credentials_or_crossing_projects() {
    let Some(db) = codifix_test_support::IsolatedDatabase::new().await else {
        return;
    };
    let projects = [Uuid::new_v4(), Uuid::new_v4()];
    let mut tokens = vec![];
    let mut sessions = vec![];
    for (index, project) in projects.iter().enumerate() {
        let slug = format!("r3-http-{index}");
        sqlx::query("INSERT INTO projects(id,slug,name,root_path) VALUES($1,$2,$2,'/synthetic')")
            .bind(project)
            .bind(&slug)
            .execute(&db.pool)
            .await
            .unwrap();
        let memory = Uuid::new_v4();
        sqlx::query("INSERT INTO memory_entries
            (id,project_id,canonical_id,version_no,is_tombstone,canonical_text,summary,memory_type,scope,importance,confidence,created_at,updated_at,search_document)
            VALUES($1,$2,$1,1,false,$3,'Settlement reconciliation','implementation','project',3,0.9,
            '2025-12-01T00:00:00Z','2025-12-01T00:00:00Z',to_tsvector('english',$3))")
            .bind(memory).bind(project).bind(format!("Reconcile settlement acknowledgement project-marker-{index}"))
            .execute(&db.pool).await.unwrap();
        let principal = Uuid::new_v4();
        sqlx::query("INSERT INTO auth_principals(id,kind,display_name) VALUES($1,'service_token','synthetic-evaluator')")
            .bind(principal).execute(&db.pool).await.unwrap();
        sqlx::query("INSERT INTO auth_project_memberships(id,principal_id,project_id,role) VALUES($1,$2,$3,'reader')")
            .bind(Uuid::new_v4()).bind(principal).bind(project).execute(&db.pool).await.unwrap();
        let token = format!("mlt_{}{}", Uuid::new_v4(), Uuid::new_v4());
        sqlx::query("INSERT INTO auth_service_tokens(id,principal_id,name,token_hash,token_prefix) VALUES($1,$2,'synthetic',$3,'mlt_synthetic')")
            .bind(Uuid::new_v4()).bind(principal).bind(crate::auth::hash_secret(&token))
            .execute(&db.pool).await.unwrap();
        tokens.push(token);
        let session = Uuid::new_v4().to_string();
        sqlx::query("INSERT INTO auth_web_sessions(id,principal_id,session_hash,csrf_hash,expires_at) VALUES($1,$2,$3,$4,now()+interval '1 hour')")
            .bind(Uuid::new_v4()).bind(principal).bind(crate::auth::hash_secret(&session))
            .bind(crate::auth::hash_secret("synthetic-csrf")).execute(&db.pool).await.unwrap();
        sessions.push(session);
    }
    // Credential expiry remains real-time, not the older semantic scoring time.
    let expired = "mlt_synthetic_expired_token";
    sqlx::query("INSERT INTO auth_service_tokens(id,principal_id,name,token_hash,token_prefix,expires_at)
        SELECT $1,id,'expired',$2,'mlt_synthetic',now()-interval '1 second' FROM auth_principals LIMIT 1")
        .bind(Uuid::new_v4()).bind(crate::auth::hash_secret(expired)).execute(&db.pool).await.unwrap();
    let before = benchmark_fingerprint(&db.pool).await.unwrap();
    let frozen = db.freeze().await.unwrap();
    let reader = frozen.fork_read_only().await.unwrap();
    let mut cfg = config(true);
    cfg.database.url = reader.pool.connect_options().to_url_lossy().to_string();
    cfg.auth.mode = codifix_record::AuthMode::MultiUser;
    cfg.auth.public_base_url = Some("https://synthetic.example.test".into());
    let state = build_state(cfg, Arc::new(Mutex::new(None))).await.unwrap();
    let app = crate::routes::build_http_app(state.clone());
    let listener = tokio::net::TcpListener::bind("127.0.0.1:0").await.unwrap();
    let url = format!("http://{}", listener.local_addr().unwrap());
    let server = tokio::spawn(async move {
        axum::serve(listener, app).await.unwrap();
    });
    // Aborting on unwind prevents test failures from leaving a live service.
    struct Abort(tokio::task::JoinHandle<()>);
    impl Drop for Abort {
        fn drop(&mut self) {
            self.0.abort();
        }
    }
    let server = Abort(server);
    let client = reqwest::Client::builder()
        .no_proxy()
        .timeout(Duration::from_secs(10))
        .build()
        .unwrap();
    for (index, token) in tokens.iter().enumerate() {
        let request = serde_json::json!({"project":format!("r3-http-{index}"),
            "query":"settlement acknowledgement", "top_k":5,
            "answer_mode":"deterministic", "retrieval_mode":"full-memory"});
        let response = client
            .post(format!("{url}/v1/query"))
            .bearer_auth(token)
            .json(&request)
            .send()
            .await
            .unwrap();
        assert_eq!(
            response.status(),
            reqwest::StatusCode::OK,
            "initial scoped-token authentication on sealed database"
        );
        let body = response.text().await.unwrap();
        assert!(body.contains(&format!("project-marker-{index}")));
        assert!(!body.contains(&format!("project-marker-{}", 1 - index)));
        let denied = client
            .post(format!("{url}/v1/query"))
            .bearer_auth(token)
            .json(
                &serde_json::json!({"project":format!("r3-http-{}",1-index),"query":"settlement"}),
            )
            .send()
            .await
            .unwrap();
        assert_eq!(denied.status(), reqwest::StatusCode::FORBIDDEN);
        // Browser reads use the same sealed auth boundary. No mutation/CSRF bypass.
        let browser = client
            .get(format!("{url}/v1/auth/me"))
            .header("Cookie", format!("codifix_session={}", sessions[index]))
            .send()
            .await
            .unwrap();
        assert_eq!(browser.status(), reqwest::StatusCode::OK);
        let write = client.post(format!("{url}/v1/capture/task")).bearer_auth(token)
            .json(&serde_json::json!({"project":format!("r3-http-{index}"),"prompt":"must not persist"})).send().await.unwrap();
        assert_eq!(write.status(), reqwest::StatusCode::FORBIDDEN);
    }
    for token in [expired, "mlt_unknown_synthetic"] {
        let denied = client
            .get(format!("{url}/v1/auth/me"))
            .bearer_auth(token)
            .send()
            .await
            .unwrap();
        assert_eq!(denied.status(), reqwest::StatusCode::UNAUTHORIZED);
    }
    assert_eq!(benchmark_fingerprint(&reader.pool).await.unwrap(), before);
    let audits: i64 = sqlx::query_scalar("SELECT count(*) FROM auth_audit_events WHERE event_type IN ('service_token.seen','session.seen')")
        .fetch_one(&reader.pool).await.unwrap();
    assert_eq!(
        audits, 4,
        "operational audit survives without credential-table writes"
    );
    drop(server);
    state.pool().unwrap().close().await;
    drop(state);
    reader.cleanup().await.unwrap();
    frozen.cleanup().await.unwrap();
}
