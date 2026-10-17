use super::*;

impl Fixture {
    async fn enable_questions(&self) {
        let work = profile(&self.state).await.unwrap();
        sqlx::query("INSERT INTO auth_principals(id,kind,display_name) VALUES($1,'internal','Synthetic question source identity')")
            .bind(work.grant().principal_id).execute(&self.db.pool).await.unwrap();
        sqlx::query("INSERT INTO auth_project_memberships(id,principal_id,project_id,role,permissions_json) VALUES($1,$2,$3,'operator',$4)")
            .bind(Uuid::new_v4()).bind(work.grant().principal_id).bind(work.grant().project_id)
            .bind(json!(["memory_read","loops_run","bundle_export"]))
            .execute(&self.db.pool).await.unwrap();
    }

    /// Synthetic provider completion: never creates a transport or sends input.
    async fn complete_question(&self, id: Uuid) {
        use codifix_supervisor_store::questions;
        let work = profile(&self.state).await.unwrap();
        let mut tx = self.db.pool.begin().await.unwrap();
        let question = questions::read_tx(&mut tx, work.grant().project_id, id)
            .await
            .unwrap()
            .unwrap();
        let evidence = sources::authorize_tx(&mut tx, work.grant()).await.unwrap();
        tx.commit().await.unwrap();
        let prepared = PreparedRequest::new(&question.submission.question, &evidence).unwrap();
        let store = SupervisorStore::new(self.db.pool.clone());
        let source_digest = questions::terms_digest(&question.submission).unwrap();
        store
            .submit(&ActionSubmission {
                id,
                key: ActionKey {
                    area: Area::Workshop,
                    source_event: stable(id),
                    kind: ActionKind::AskQuestion,
                    policy_revision: stable(work.grant().policy_revision),
                },
                source_digest,
            })
            .await
            .unwrap();
        for step in [
            ReadyTransition::Propose,
            ReadyTransition::Authorize,
            ReadyTransition::Queue,
        ] {
            store
                .advance(Area::Workshop, id, step, source_digest)
                .await
                .unwrap();
        }
        let lease = store
            .claim(Area::Workshop, id, Uuid::new_v4(), 120)
            .await
            .unwrap();
        store
            .bind_model_work(
                &lease,
                work.grant().instance_id,
                work.grant().project_id,
                question.submission.request_digest,
                prepared.evidence_digests(),
            )
            .await
            .unwrap();
        store
            .reserve(
                &lease,
                &ProviderRequest {
                    id: stable(question.attempt_id),
                    action_id: stable(id),
                    policy_revision: stable(work.grant().policy_revision),
                    model: StableId::new("gpt-5.6-sol").unwrap(),
                    worst_case_cost: UsdMicros(100),
                },
                question.submission.request_digest,
            )
            .await
            .unwrap();
        store.begin(&lease, question.attempt_id).await.unwrap();
        store
            .settle(question.attempt_id, UsdMicros(1), [4; 32])
            .await
            .unwrap();
        store
            .publish_proposal(
                &lease,
                &question.attempt_id,
                work.grant().project_id,
                &Proposal {
                    summary: "SYNTHETIC_QUEUED_PRIVATE_RESPONSE".into(),
                    proposal: ProposalKind::AskHuman,
                    evidence_indices: vec![0],
                },
                prepared.evidence_digests(),
            )
            .await
            .unwrap();
    }
}

fn browser_post(client: &reqwest::Client, url: &str, path: &str) -> reqwest::RequestBuilder {
    client
        .post(format!("{url}{path}"))
        .header(
            header::COOKIE,
            "codifix_session=private-test-session; codifix_csrf=private-test-csrf",
        )
        .header("x-csrf-token", "private-test-csrf")
        .header(header::ORIGIN, url)
}

#[tokio::test]
async fn question_intent_is_confirmed_private_idempotent_and_never_calls_a_provider() {
    let Some(f) = Fixture::new().await else {
        return;
    };
    f.enable_questions().await;
    let (url, server) = f.server(false).await;
    let client = reqwest::Client::new();
    let path = "/v1/projects/memory/supervisor/questions";
    let prepare = format!("{path}/prepare");
    let question = json!({"question":"SYNTHETIC_NEW_PRIVATE_QUESTION"});
    assert_eq!(
        client
            .post(format!("{url}{prepare}"))
            .bearer_auth(&f.reader)
            .json(&question)
            .send()
            .await
            .unwrap()
            .status(),
        403
    );
    assert_eq!(
        client
            .post(format!("{url}{prepare}"))
            .bearer_auth(&f.other)
            .json(&question)
            .send()
            .await
            .unwrap()
            .status(),
        403
    );
    assert_eq!(
        client
            .post(format!("{url}{prepare}"))
            .header(header::COOKIE, "codifix_session=private-test-session")
            .json(&question)
            .send()
            .await
            .unwrap()
            .status(),
        403
    );
    let response = browser_post(&client, &url, &prepare)
        .json(&question)
        .send()
        .await
        .unwrap();
    assert_eq!(response.status(), 200);
    assert_eq!(response.headers()[header::CACHE_CONTROL], "no-store");
    let preview: Value = response.json().await.unwrap();
    assert_eq!(preview["model"], "gpt-5.6-sol");
    assert_eq!(preview["destination"], "openai");
    assert_eq!(preview["source_count"], 1);
    assert!(!preview.to_string().contains("SYNTHETIC_REVIEWED_FACT"));
    assert!(!preview.to_string().contains("private-never-disclose"));
    let count: i64 = sqlx::query_scalar("SELECT count(*) FROM supervisor_questions")
        .fetch_one(&f.db.pool)
        .await
        .unwrap();
    assert_eq!(count, 0);
    let id = Uuid::new_v4();
    let request = json!({"request_id":id,"question":question["question"],"profile_digest":preview["profile_digest"],"request_digest":preview["request_digest"]});
    let mut mismatch = request.clone();
    mismatch["request_digest"] = json!("0".repeat(64));
    assert_eq!(
        browser_post(&client, &url, path)
            .json(&mismatch)
            .send()
            .await
            .unwrap()
            .status(),
        409
    );
    let (a, b) = tokio::join!(
        browser_post(&client, &url, path).json(&request).send(),
        browser_post(&client, &url, path).json(&request).send()
    );
    let a = a.unwrap();
    let b = b.unwrap();
    assert_eq!(a.status(), 200);
    assert_eq!(b.status(), 200);
    let status: Value = a.json().await.unwrap();
    assert_eq!(status, b.json::<Value>().await.unwrap());
    assert_eq!(status["request_id"], id.to_string());
    assert_eq!(status["action_id"], id.to_string());
    assert_eq!(status["proposal_available"], false);
    assert_eq!(status["state"], "queued");
    assert!(!status.to_string().contains("PRIVATE_QUESTION"));
    let mut changed = request.clone();
    changed["question"] = json!("SYNTHETIC_DIFFERENT_PRIVATE_QUESTION");
    let altered: Value = browser_post(&client, &url, &prepare)
        .json(&json!({"question":changed["question"]}))
        .send()
        .await
        .unwrap()
        .json()
        .await
        .unwrap();
    changed["request_digest"] = altered["request_digest"].clone();
    assert_eq!(
        browser_post(&client, &url, path)
            .json(&changed)
            .send()
            .await
            .unwrap()
            .status(),
        409
    );
    let listing: Value = client
        .get(format!("{url}{path}"))
        .bearer_auth(&f.token)
        .send()
        .await
        .unwrap()
        .json()
        .await
        .unwrap();
    assert_eq!(listing["questions"].as_array().unwrap().len(), 1);
    assert!(!listing.to_string().contains("PRIVATE_QUESTION"));
    let reader_listing: Value = client
        .get(format!("{url}{path}"))
        .bearer_auth(&f.reader)
        .send()
        .await
        .unwrap()
        .json()
        .await
        .unwrap();
    assert_eq!(reader_listing["questions"], json!([]));
    assert_eq!(
        client
            .get(format!("{url}{path}/{id}"))
            .bearer_auth(&f.reader)
            .send()
            .await
            .unwrap()
            .status(),
        404
    );
    assert_eq!(
        client
            .get(format!("{url}/v1/projects/other/supervisor/questions/{id}"))
            .bearer_auth(&f.token)
            .send()
            .await
            .unwrap()
            .status(),
        403
    );
    let counts:(i64,i64)=sqlx::query_as("SELECT (SELECT count(*) FROM supervisor_questions),(SELECT count(*) FROM supervisor_attempts)").fetch_one(&f.db.pool).await.unwrap();
    assert_eq!(counts, (1, 1)); // Only the pre-existing synthetic proposal attempt.
    let journal = serde_json::to_string(
        &SupervisorStore::new(f.db.pool.clone())
            .journal(Area::Workshop, 0, 100)
            .await
            .unwrap(),
    )
    .unwrap();
    assert!(!journal.contains("PRIVATE_QUESTION"));
    f.complete_question(id).await;
    let proposal_path = format!("{url}/v1/projects/memory/supervisor/proposals/{id}");
    let response = client
        .get(&proposal_path)
        .bearer_auth(&f.token)
        .send()
        .await
        .unwrap();
    assert_eq!(response.status(), 200);
    let result: Value = response.json().await.unwrap();
    assert_eq!(result["question"], question["question"]);
    assert_eq!(result["summary"], "SYNTHETIC_QUEUED_PRIVATE_RESPONSE");
    assert_eq!(
        result["citations"][0]["version_id"],
        f.canonical.to_string()
    );
    let status: Value = client
        .get(format!("{url}{path}/{id}"))
        .bearer_auth(&f.token)
        .send()
        .await
        .unwrap()
        .json()
        .await
        .unwrap();
    assert_eq!(status["state"], "answered");
    assert_eq!(status["proposal_available"], true);
    sqlx::query("UPDATE auth_web_sessions SET revoked_at=clock_timestamp() WHERE session_hash=$1")
        .bind(crate::auth::hash_secret("private-test-session"))
        .execute(&f.db.pool)
        .await
        .unwrap();
    assert_eq!(
        browser_post(&client, &url, path)
            .json(&request)
            .send()
            .await
            .unwrap()
            .status(),
        401
    );
    // A different active credential cannot bypass the original submitter's
    // revoked disclosure credential, even though it can reconcile metadata.
    assert_eq!(
        client
            .get(&proposal_path)
            .bearer_auth(&f.token)
            .send()
            .await
            .unwrap()
            .status(),
        403
    );
    server.abort();
    f.cleanup().await;
}

#[tokio::test]
async fn question_admission_rejects_readonly_changed_sources_invalid_input_and_lost_authority() {
    let Some(f) = Fixture::new().await else {
        return;
    };
    f.enable_questions().await;
    let client = reqwest::Client::new();
    let prepare = "/v1/projects/memory/supervisor/questions/prepare";
    let question = json!({"question":"SYNTHETIC_NEW_PRIVATE_QUESTION"});
    let (url, server) = f.server(true).await;
    assert_eq!(
        browser_post(&client, &url, prepare)
            .json(&question)
            .send()
            .await
            .unwrap()
            .status(),
        403
    );
    server.abort();
    let (url, server) = f.server(false).await;
    for invalid in [
        " ".to_owned(),
        "q".repeat(4097),
        "control\u{001b}".to_owned(),
        "carriage\r".to_owned(),
    ] {
        assert_eq!(
            browser_post(&client, &url, prepare)
                .json(&json!({"question":invalid}))
                .send()
                .await
                .unwrap()
                .status(),
            400
        );
    }
    assert_eq!(
        browser_post(&client, &url, prepare)
            .json(&json!({"question":"valid","provider":"different"}))
            .send()
            .await
            .unwrap()
            .status(),
        422
    );
    // Direct LoopsRun without current export consent is insufficient.
    sqlx::query("UPDATE auth_project_memberships SET permissions_json=$1 WHERE principal_id=(SELECT principal_id FROM auth_service_tokens WHERE id=$2)")
        .bind(json!(["memory_read","loops_run"])).bind(f.token_id).execute(&f.db.pool).await.unwrap();
    assert_eq!(
        browser_post(&client, &url, prepare)
            .json(&question)
            .send()
            .await
            .unwrap()
            .status(),
        403
    );
    sqlx::query("UPDATE auth_project_memberships SET permissions_json=NULL WHERE principal_id=(SELECT principal_id FROM auth_service_tokens WHERE id=$1)")
        .bind(f.token_id).execute(&f.db.pool).await.unwrap();
    sqlx::query("UPDATE memory_canonical_state SET status='archived' WHERE canonical_id=$1")
        .bind(f.canonical)
        .execute(&f.db.pool)
        .await
        .unwrap();
    assert_eq!(
        browser_post(&client, &url, prepare)
            .json(&question)
            .send()
            .await
            .unwrap()
            .status(),
        403
    );
    let count: i64 = sqlx::query_scalar("SELECT count(*) FROM supervisor_questions")
        .fetch_one(&f.db.pool)
        .await
        .unwrap();
    assert_eq!(count, 0);
    server.abort();
    f.cleanup().await;
}

#[tokio::test]
async fn question_intent_commit_failure_is_atomic_and_sanitized() {
    let Some(f) = Fixture::new().await else {
        return;
    };
    f.enable_questions().await;
    let (url, server) = f.server(false).await;
    let client = reqwest::Client::new();
    let path = "/v1/projects/memory/supervisor/questions";
    let question = "SYNTHETIC_ATOMIC_PRIVATE_QUESTION";
    let preview: Value = browser_post(&client, &url, &format!("{path}/prepare"))
        .json(&json!({"question":question}))
        .send()
        .await
        .unwrap()
        .json()
        .await
        .unwrap();
    sqlx::raw_sql("CREATE FUNCTION fail_question() RETURNS trigger LANGUAGE plpgsql AS $$ BEGIN RAISE EXCEPTION 'SYNTHETIC_PRIVATE_DATABASE_DETAIL'; END $$; CREATE CONSTRAINT TRIGGER fail_question AFTER INSERT ON supervisor_questions DEFERRABLE INITIALLY DEFERRED FOR EACH ROW EXECUTE FUNCTION fail_question();").execute(&f.db.pool).await.unwrap();
    let request = json!({"request_id":Uuid::new_v4(),"question":question,"profile_digest":preview["profile_digest"],"request_digest":preview["request_digest"]});
    let response = browser_post(&client, &url, path)
        .json(&request)
        .send()
        .await
        .unwrap();
    assert_eq!(response.status(), 503);
    assert!(
        !response
            .text()
            .await
            .unwrap()
            .contains("PRIVATE_DATABASE_DETAIL")
    );
    let count: i64 = sqlx::query_scalar("SELECT count(*) FROM supervisor_questions")
        .fetch_one(&f.db.pool)
        .await
        .unwrap();
    assert_eq!(count, 0);
    server.abort();
    f.cleanup().await;
}
