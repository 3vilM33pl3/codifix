use super::*;
use crate::{
    launcher::RunControl,
    question_queue::{self, QueueOutcome, ServiceExecutionGuard},
    work::{PinnedWork, WorkStep},
};
use codifix_supervisor::action::KnownOutcome;
use codifix_supervisor_store::questions::{
    self, QuestionActor, QuestionSubmission, QueuedQuestion,
};
#[cfg(unix)]
use std::os::unix::fs::PermissionsExt;

fn config(dir: &tempfile::TempDir, profile: &PinnedWork) -> Arc<ServiceExecutionGuard> {
    let body = format!(
        "[service]\nread_only=false\n[database]\nurl='never-used'\n[cluster]\nenabled=false\n[supervisor]\ninstance_id='{}'\narea='workshop'\n[supervisor.interaction]\nmanifest='/operator-only/work.json'\nsha256='{}'\n",
        profile.grant().instance_id,
        profile
            .digest()
            .iter()
            .map(|n| format!("{n:02x}"))
            .collect::<String>()
    );
    let path = dir.path().join("service.toml");
    crate::test_files::write(&path, body.as_bytes());
    let hash = format!("{:x}", Sha256::digest(body.as_bytes()));
    Arc::new(ServiceExecutionGuard::load(&path, &hash, profile).expect("service guard"))
}

async fn question(db: &IsolatedDatabase, profile: &PinnedWork) -> QueuedQuestion {
    question_kind(db, profile, false).await
}

async fn question_kind(
    db: &IsolatedDatabase,
    profile: &PinnedWork,
    session: bool,
) -> QueuedQuestion {
    let principal = Uuid::new_v4();
    let token = Uuid::new_v4();
    sqlx::query("INSERT INTO auth_principals(id,kind,display_name,global_role) VALUES($1,'service_token','Synthetic requester','admin')")
        .bind(principal).execute(&db.pool).await.expect("requester");
    sqlx::query("INSERT INTO auth_service_tokens(id,principal_id,name,token_hash,token_prefix,expires_at) VALUES($1,$2,'Synthetic',decode($3,'hex'),'fake',clock_timestamp()+interval '1 hour')")
        .bind(token).bind(principal).bind(token.simple().to_string()).execute(&db.pool).await.expect("token");
    if session {
        sqlx::query("INSERT INTO auth_web_sessions(id,principal_id,session_hash,csrf_hash,expires_at) VALUES($1,$2,decode($3,'hex'),decode($3,'hex'),clock_timestamp()+interval '1 hour')")
            .bind(token).bind(principal).bind(token.simple().to_string()).execute(&db.pool).await.expect("browser session");
    }
    let text = "What does this reviewed development note establish?";
    let prepared = crate::local_disclosure::LocalDisclosure::new(
        db.pool.clone(),
        profile.grant().clone(),
        text.into(),
    )
    .prepare()
    .await
    .expect("prepare exact sources");
    let input = QuestionSubmission {
        request_id: Uuid::new_v4(),
        project_id: profile.grant().project_id,
        instance_id: profile.grant().instance_id,
        profile_digest: profile.digest(),
        request_digest: digest_hex(prepared.digest()).expect("digest"),
        policy_revision: profile.grant().policy_revision,
        actor: QuestionActor {
            principal_id: principal,
            token_id: (!session).then_some(token),
            session_id: session.then_some(token),
        },
        question: text.into(),
    };
    let mut tx = db.pool.begin().await.expect("tx");
    let result = questions::enqueue_tx(&mut tx, &input)
        .await
        .expect("enqueue");
    tx.commit().await.expect("commit");
    result
}

async fn claimed(db: &IsolatedDatabase, work: &PinnedWork) -> Box<crate::work::ClaimedWork> {
    for _ in 0..3 {
        assert!(matches!(
            work.step(db.pool.clone(), RunControl::default())
                .await
                .expect("advance"),
            WorkStep::State(_)
        ));
    }
    match work
        .step(db.pool.clone(), RunControl::default())
        .await
        .expect("claim")
    {
        WorkStep::Claimed(claim) => claim,
        WorkStep::State(_) => panic!("expected claim"),
    }
}

#[tokio::test]
async fn queued_question_is_bound_and_recovery_never_reissues_the_completed_attempt() {
    let Some((db, store, initial, _, fake, client, server)) = fixture(Mode::Good).await else {
        return;
    };
    store
        .publish(&initial, KnownOutcome::Failed, [8; 32])
        .await
        .expect("close fixture work");
    let (dir, _, hash) = super::work::manifest(&db, &fake).await;
    let profile = PinnedWork::load(
        &dir.path().join("work.json"),
        &hash,
        fake.instance_id,
        Area::Workshop,
    )
    .expect("profile");
    let guard = config(&dir, &profile);
    let queued = question(&db, &profile).await;
    let id = queued.submission.request_id;
    let attempt = queued.attempt_id;
    assert_eq!(
        question_queue::run_next(
            &profile,
            db.pool.clone(),
            None,
            Arc::clone(&guard),
            RunControl::default()
        )
        .await
        .outcome,
        QueueOutcome::CredentialUnavailable
    );
    assert!(
        store.action(Area::Workshop, id).await.is_err(),
        "no credential must not even create the action"
    );
    let mut altered = queued.clone();
    altered.submission.profile_digest = [0; 32];
    assert!(profile.for_question(altered, Arc::clone(&guard)).is_err());
    let work = profile
        .for_question(queued.clone(), Arc::clone(&guard))
        .expect("derived input");
    assert_eq!(work.attempt_id(), attempt);
    assert_ne!(work.digest(), profile.digest());
    assert_eq!(
        work.digest(),
        questions::terms_digest(&queued.submission).expect("terms")
    );
    let claim = claimed(&db, &work).await;
    *fake.expected.write().expect("expected") = Arc::clone(&claim.request);
    client
        .execute_guarded_and_persist(
            &claim.store,
            &claim.lease,
            &claim.execution,
            &claim.request,
            claim.disclosure.as_ref(),
        )
        .await
        .expect("fake result");
    assert_eq!(fake.generated.load(Ordering::SeqCst), 1);
    assert_eq!(
        store.action(Area::Workshop, id).await.expect("state").state,
        ActionState::Verified
    );
    assert!(
        questions::next(&db.pool, fake.project_id, profile.digest())
            .await
            .expect("next")
            .is_none()
    );
    assert_eq!(
        crate::operator::run(&work, db.pool.clone(), None, RunControl::default())
            .await
            .outcome,
        crate::operator::Outcome::Stored
    );
    assert_eq!(fake.generated.load(Ordering::SeqCst), 1);
    server.abort();
    db.cleanup().await.expect("cleanup");
}

#[tokio::test]
async fn queued_requester_revocation_and_configuration_changes_block_all_disclosure() {
    for change in [
        "token",
        "session",
        "principal",
        "config",
        "request",
        "attempt",
        "expiry",
    ] {
        let Some((db, store, initial, _, fake, _, server)) = fixture(Mode::Good).await else {
            return;
        };
        store
            .publish(&initial, KnownOutcome::Failed, [8; 32])
            .await
            .expect("close fixture work");
        let (dir, _, hash) = super::work::manifest(&db, &fake).await;
        let profile = PinnedWork::load(
            &dir.path().join("work.json"),
            &hash,
            fake.instance_id,
            Area::Workshop,
        )
        .expect("profile");
        let guard = config(&dir, &profile);
        let mut queued = question_kind(&db, &profile, change == "session").await;
        match change {
            "token" => {
                sqlx::query(
                    "UPDATE auth_service_tokens SET revoked_at=clock_timestamp() WHERE id=$1",
                )
                .bind(queued.submission.actor.token_id)
                .execute(&db.pool)
                .await
                .expect("revoke");
            }
            "session" => {
                sqlx::query(
                    "UPDATE auth_web_sessions SET revoked_at=clock_timestamp() WHERE id=$1",
                )
                .bind(queued.submission.actor.session_id)
                .execute(&db.pool)
                .await
                .expect("revoke browser session");
            }
            "principal" => {
                sqlx::query("UPDATE auth_principals SET disabled_at=clock_timestamp() WHERE id=$1")
                    .bind(queued.submission.actor.principal_id)
                    .execute(&db.pool)
                    .await
                    .expect("disable");
            }
            "request" => queued.submission.question.push_str(" changed"),
            "attempt" => queued.attempt_id = Uuid::new_v4(),
            "expiry" => queued.expires_at = Utc::now() - chrono::Duration::seconds(1),
            "config" => {}
            _ => unreachable!(),
        }
        let work = profile
            .for_question(queued, Arc::clone(&guard))
            .expect("derive");
        if change == "config" {
            std::fs::write(dir.path().join("service.toml"), "changed")
                .expect("operator replaced config");
        }
        assert!(
            work.step(db.pool.clone(), RunControl::default())
                .await
                .is_err(),
            "{change}"
        );
        assert_eq!(fake.count.load(Ordering::SeqCst), 0, "{change}");
        assert_eq!(fake.generated.load(Ordering::SeqCst), 0, "{change}");
        server.abort();
        db.cleanup().await.expect("cleanup");
    }
}

#[tokio::test]
async fn queued_requester_revocation_during_generation_retains_billing_without_publication() {
    let Some((db, store, initial, _, fake, client, server)) = fixture(Mode::Good).await else {
        return;
    };
    store
        .publish(&initial, KnownOutcome::Failed, [8; 32])
        .await
        .expect("close fixture work");
    let (dir, _, hash) = super::work::manifest(&db, &fake).await;
    let profile = PinnedWork::load(
        &dir.path().join("work.json"),
        &hash,
        fake.instance_id,
        Area::Workshop,
    )
    .expect("profile");
    let guard = config(&dir, &profile);
    let queued = question(&db, &profile).await;
    let token = queued.submission.actor.token_id;
    let id = queued.submission.request_id;
    let attempt = queued.attempt_id;
    let work = profile.for_question(queued, guard).expect("derive");
    let claim = claimed(&db, &work).await;
    *fake.expected.write().expect("expected") = Arc::clone(&claim.request);
    let pause = Arc::new(tokio::sync::Notify::new());
    *fake.generation_pause.lock().expect("pause") = Some(Arc::clone(&pause));
    let execution = tokio::spawn(async move {
        client
            .execute_guarded_and_persist(
                &claim.store,
                &claim.lease,
                &claim.execution,
                &claim.request,
                claim.disclosure.as_ref(),
            )
            .await
    });
    tokio::time::timeout(Duration::from_secs(5), async {
        while fake.generated.load(Ordering::SeqCst) == 0 {
            tokio::time::sleep(Duration::from_millis(5)).await;
        }
    })
    .await
    .expect("started");
    sqlx::query("UPDATE auth_service_tokens SET revoked_at=clock_timestamp() WHERE id=$1")
        .bind(token)
        .execute(&db.pool)
        .await
        .expect("revoke");
    pause.notify_one();
    assert!(execution.await.expect("execute").is_err());
    assert!(store.settled_usage(attempt).await.expect("usage").is_some());
    assert!(
        store
            .private_proposal(Area::Workshop, fake.project_id, id)
            .await
            .is_err()
    );
    sqlx::query("UPDATE supervisor_actions SET lease_until=clock_timestamp()-interval '1 second' WHERE id=$1").bind(id).execute(&db.pool).await.expect("expire lost worker");
    assert_eq!(
        work.observe(db.pool.clone()).await.expect("recovery"),
        ActionState::Uncertain
    );
    assert_eq!(fake.generated.load(Ordering::SeqCst), 1);
    server.abort();
    db.cleanup().await.expect("cleanup");
}

#[tokio::test]
async fn operator_config_is_rechecked_after_waiting_for_source_locks() {
    let Some((db, store, initial, _, fake, _, server)) = fixture(Mode::Good).await else {
        return;
    };
    store
        .publish(&initial, KnownOutcome::Failed, [8; 32])
        .await
        .expect("close fixture work");
    let (dir, _, hash) = super::work::manifest(&db, &fake).await;
    let profile = PinnedWork::load(
        &dir.path().join("work.json"),
        &hash,
        fake.instance_id,
        Area::Workshop,
    )
    .expect("profile");
    let guard = config(&dir, &profile);
    let queued = question(&db, &profile).await;
    let work = profile.for_question(queued, guard).expect("derive");
    let mut blocker = db.pool.begin().await.expect("source transaction");
    sqlx::query("SELECT canonical_id FROM memory_canonical_state WHERE canonical_id=$1 FOR UPDATE")
        .bind(profile.grant().sources[0].canonical_id)
        .fetch_one(&mut *blocker)
        .await
        .expect("source lock");
    let pool = db.pool.clone();
    let waiting = tokio::spawn(async move { work.step(pool, RunControl::default()).await });
    tokio::time::timeout(Duration::from_secs(5),async {
        loop {
            let blocked:bool=sqlx::query_scalar("SELECT EXISTS(SELECT 1 FROM pg_stat_activity WHERE datname=current_database() AND wait_event_type='Lock' AND query LIKE '%memory_canonical_state%')")
                .fetch_one(&db.pool).await.expect("wait evidence");
            if blocked {break;}
            tokio::time::sleep(Duration::from_millis(10)).await;
        }
    }).await.expect("worker reached source lock");
    std::fs::write(dir.path().join("service.toml"), "changed operator config")
        .expect("configuration changed while blocked");
    blocker.commit().await.expect("release source");
    assert!(waiting.await.expect("worker").is_err());
    assert_eq!(fake.count.load(Ordering::SeqCst), 0);
    assert_eq!(fake.generated.load(Ordering::SeqCst), 0);
    server.abort();
    db.cleanup().await.expect("cleanup");
}

#[cfg(unix)]
#[tokio::test]
#[ignore = "requires pinned patched Pi/driver and explicit disposable PostgreSQL"]
async fn operator_run_next_uses_real_pi_with_fake_provider_and_restricted_database_role() {
    use crate::launcher::SandboxConfig;
    let (db, store, initial, _, fake, client, server) =
        fixture(Mode::Good).await.expect("required database");
    store
        .publish(&initial, KnownOutcome::Failed, [8; 32])
        .await
        .expect("close fixture work");
    let (dir, mut manifest, _) = super::work::manifest(&db, &fake).await;
    let driver = dir.path().join("driver");
    std::fs::copy(
        std::env::var_os("CODIFIX_PI_SANDBOX_DRIVER").expect("driver"),
        &driver,
    )
    .expect("stage driver");
    std::fs::set_permissions(&driver, std::fs::Permissions::from_mode(0o500))
        .expect("private driver");
    manifest.sandbox = SandboxConfig {
        pi: std::env::var_os("CODIFIX_PI_PATCHED_ARTIFACT")
            .expect("pinned patched Pi")
            .into(),
        driver_sha256: format!(
            "{:x}",
            Sha256::digest(std::fs::read(&driver).expect("driver bytes"))
        ),
        driver,
        private_runtime: dir.path().into(),
        windows_artifacts: None,
    };
    let hash = super::work::save(&dir, &manifest);
    let profile = PinnedWork::load(
        &dir.path().join("work.json"),
        &hash,
        fake.instance_id,
        Area::Workshop,
    )
    .expect("profile");
    let guard = config(&dir, &profile);
    let queued = question_kind(&db, &profile, true).await;
    let expected = crate::local_disclosure::LocalDisclosure::new(
        db.pool.clone(),
        profile.grant().clone(),
        queued.submission.question.clone(),
    )
    .prepare()
    .await
    .expect("approved input");
    *fake.expected.write().expect("expected") = Arc::new(expected);
    let role = format!("question_test_{}", Uuid::new_v4().simple());
    sqlx::raw_sql(&format!(
        "CREATE ROLE {role} NOLOGIN NOSUPERUSER NOCREATEDB NOCREATEROLE"
    ))
    .execute(&db.pool)
    .await
    .expect("isolated role");
    sqlx::raw_sql(
        &include_str!("../../../operator-worker-grants.sql").replace("workshop_worker", &role),
    )
    .execute(&db.pool)
    .await
    .expect("operator grants");
    let selected = role.clone();
    let restricted = sqlx::postgres::PgPoolOptions::new()
        .max_connections(4)
        .after_connect(move |connection, _| {
            let statement = format!("SET ROLE {selected}");
            Box::pin(async move {
                sqlx::query(&statement).execute(connection).await?;
                Ok(())
            })
        })
        .connect_with((*db.pool.connect_options()).clone())
        .await
        .expect("restricted pool");
    assert!(
        sqlx::query("UPDATE auth_web_sessions SET revoked_at=clock_timestamp() WHERE false")
            .execute(&restricted)
            .await
            .is_err()
    );
    assert!(
        sqlx::query("UPDATE memory_entries SET canonical_text=canonical_text WHERE false")
            .execute(&restricted)
            .await
            .is_err()
    );
    let base = client.base.clone();
    let result = question_queue::run_next(
        &profile,
        restricted.clone(),
        Some(client),
        Arc::clone(&guard),
        RunControl::default(),
    )
    .await;
    assert_eq!(result.outcome, QueueOutcome::Processed);
    assert_eq!(
        result.work.expect("work").outcome,
        crate::operator::Outcome::Completed
    );
    assert_eq!(fake.generated.load(Ordering::SeqCst), 1);
    assert!(
        store
            .settled_usage(queued.attempt_id)
            .await
            .expect("usage")
            .is_some()
    );
    let client = OpenAiTransport::build("synthetic-only", &base).expect("fake client");
    assert_eq!(
        question_queue::run_next(
            &profile,
            restricted.clone(),
            Some(client),
            guard,
            RunControl::default()
        )
        .await
        .outcome,
        QueueOutcome::Idle
    );
    assert_eq!(
        fake.generated.load(Ordering::SeqCst),
        1,
        "restart never invokes the provider again"
    );
    restricted.close().await;
    sqlx::raw_sql(&format!("DROP OWNED BY {role}; DROP ROLE {role};"))
        .execute(&db.pool)
        .await
        .expect("isolated role cleanup");
    server.abort();
    db.cleanup().await.expect("cleanup");
}
