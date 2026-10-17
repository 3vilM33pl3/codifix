use super::*;
use crate::local_disclosure::LocalDisclosure;
use codifix_supervisor_store::sources::{self, LocalCollectionGrant, ModelDestination};

pub(super) async fn grant(db: &IsolatedDatabase, fake: &Fake) -> LocalCollectionGrant {
    let principal = Uuid::new_v4();
    sqlx::query("INSERT INTO auth_principals(id,kind,display_name) VALUES($1,'internal','Synthetic Workshop supervisor')").bind(principal).execute(&db.pool).await.expect("principal");
    sqlx::query("INSERT INTO auth_project_memberships(id,principal_id,project_id,role,source,permissions_json) VALUES($1,$2,$3,'reader','direct','[\"memory_read\",\"bundle_export\",\"loops_run\"]'::jsonb)")
        .bind(Uuid::new_v4()).bind(principal).bind(fake.project_id).execute(&db.pool).await.expect("minimal exact permission override");
    let canonical = Uuid::new_v4();
    insert_version(db, fake.project_id, canonical, 1, false).await;
    let mut tx = db.pool.begin().await.expect("tx");
    let sources = sources::prepare_tx(&mut tx, fake.project_id, &[canonical])
        .await
        .expect("selected sources")
        .into_iter()
        .map(|projection| projection.binding)
        .collect();
    tx.commit().await.expect("prepare commit");
    LocalCollectionGrant {
        instance_id: fake.instance_id,
        project_id: fake.project_id,
        principal_id: principal,
        policy_revision: fake.revision,
        destination: ModelDestination::OpenAi,
        expires_at: Utc::now() + chrono::Duration::hours(1),
        sources,
    }
}

async fn insert_version(
    db: &IsolatedDatabase,
    project: Uuid,
    canonical: Uuid,
    version: i32,
    tombstone: bool,
) {
    sqlx::query("INSERT INTO memory_entries(id,project_id,canonical_id,version_no,is_tombstone,canonical_text,summary,memory_type,scope,importance,confidence,created_at,updated_at,search_document) VALUES($1,$2,$3,$4,$5,'Synthetic approved development knowledge only','Synthetic reviewed note','decision','project',3,0.8,now(),now(),to_tsvector('synthetic'))")
        .bind(Uuid::new_v4()).bind(project).bind(canonical).bind(version).bind(tombstone).execute(&db.pool).await.expect("version");
}

#[tokio::test]
async fn exact_local_collection_is_revalidated_and_committed_with_the_proposal() {
    let Some((db, store, lease, revision, fake, client, server)) = fixture(Mode::Good).await else {
        return;
    };
    let grant = grant(&db, &fake).await;
    assert_eq!(
        serde_json::to_value(&grant).expect("grant metadata")["destination"],
        "openai"
    );
    let guard = LocalDisclosure::new(
        db.pool.clone(),
        grant.clone(),
        "Review the approved note".into(),
    );
    let request = Arc::new(guard.prepare().await.expect("current source grant"));
    *fake.expected.write().expect("expected") = Arc::clone(&request);
    let input = request.generation_body().to_string();
    assert!(!input.contains("/synthetic"));
    assert!(!input.contains("raw_capture"));
    assert!(input.contains(&grant.sources[0].version_id.to_string()));
    let execution = fake.execution(revision, Uuid::new_v4());
    let proposal = client
        .execute_guarded_and_persist(&store, &lease, &execution, &request, &guard)
        .await
        .expect("guarded proposal");
    assert_eq!(proposal.evidence_digests, request.evidence_digests());
    assert_eq!(
        store
            .action(Area::Workshop, lease.action_id())
            .await
            .expect("action")
            .state,
        ActionState::Verified
    );
    guard
        .authorize(DisclosurePhase::Deliver, &execution, &request)
        .await
        .expect("still allowed");
    sqlx::query("UPDATE auth_principals SET disabled_at=clock_timestamp() WHERE id=$1")
        .bind(grant.principal_id)
        .execute(&db.pool)
        .await
        .expect("revoke");
    assert!(
        guard
            .authorize(DisclosurePhase::Deliver, &execution, &request)
            .await
            .is_err()
    );
    // Current delivery denial does not erase accepted history or known costs.
    assert!(
        store
            .private_proposal(Area::Workshop, fake.project_id, lease.action_id())
            .await
            .is_ok()
    );
    assert!(
        store
            .settled_usage(execution.attempt_id)
            .await
            .expect("usage")
            .is_some()
    );
    server.abort();
    db.cleanup().await.expect("cleanup");
}

#[tokio::test]
async fn stale_wrong_scope_broad_identity_and_expired_grants_never_disclose_to_counter() {
    for change in [
        "source",
        "tombstone",
        "withdraw",
        "project",
        "instance",
        "principal",
        "global",
        "permissions",
        "expired",
        "destination",
    ] {
        let Some((db, store, lease, revision, fake, client, server)) = fixture(Mode::Good).await
        else {
            return;
        };
        let original = grant(&db, &fake).await;
        let valid = LocalDisclosure::new(db.pool.clone(), original.clone(), "Review".into());
        let request = Arc::new(valid.prepare().await.expect("prepare"));
        *fake.expected.write().expect("expected") = Arc::clone(&request);
        let mut changed = original.clone();
        match change {
            "source" => {
                insert_version(
                    &db,
                    fake.project_id,
                    original.sources[0].canonical_id,
                    2,
                    false,
                )
                .await
            }
            "tombstone" => {
                insert_version(
                    &db,
                    fake.project_id,
                    original.sources[0].canonical_id,
                    2,
                    true,
                )
                .await
            }
            "withdraw" => {
                sqlx::query(
                    "UPDATE memory_canonical_state SET status='archived' WHERE canonical_id=$1",
                )
                .bind(original.sources[0].canonical_id)
                .execute(&db.pool)
                .await
                .expect("withdraw");
            }
            "project" => changed.project_id = Uuid::new_v4(),
            "instance" => changed.instance_id = Uuid::new_v4(),
            "principal" => {
                sqlx::query("UPDATE auth_principals SET disabled_at=clock_timestamp() WHERE id=$1")
                    .bind(original.principal_id)
                    .execute(&db.pool)
                    .await
                    .expect("disable");
            }
            "global" => {
                sqlx::query("UPDATE auth_principals SET global_role='admin' WHERE id=$1")
                    .bind(original.principal_id)
                    .execute(&db.pool)
                    .await
                    .expect("global");
            }
            "permissions" => {
                sqlx::query("UPDATE auth_project_memberships SET permissions_json='[\"memory_read\"]'::jsonb WHERE principal_id=$1").bind(original.principal_id).execute(&db.pool).await.expect("revoke permission");
            }
            "expired" => changed.expires_at = Utc::now() - chrono::Duration::seconds(1),
            "destination" => {
                let mut wrong = serde_json::to_value(&changed).expect("metadata");
                wrong["destination"] = json!("unknown");
                assert!(serde_json::from_value::<LocalCollectionGrant>(wrong).is_err());
                server.abort();
                db.cleanup().await.expect("cleanup");
                continue;
            }
            _ => unreachable!(),
        }
        let guard = LocalDisclosure::new(db.pool.clone(), changed, "Review".into());
        let execution = fake.execution(revision, Uuid::new_v4());
        assert!(
            client
                .execute_guarded_and_persist(&store, &lease, &execution, &request, &guard)
                .await
                .is_err(),
            "{change}"
        );
        assert_eq!(fake.count.load(Ordering::SeqCst), 0, "{change}");
        assert_eq!(fake.generated.load(Ordering::SeqCst), 0, "{change}");
        assert!(matches!(
            store.attempt(execution.attempt_id).await,
            Err(StoreError::NotFound)
        ));
        server.abort();
        db.cleanup().await.expect("cleanup");
    }
}

#[tokio::test]
async fn changes_during_generation_are_billed_but_never_accepted_as_current_knowledge() {
    for change in [
        "source",
        "tombstone",
        "principal",
        "permissions",
        "shutdown",
    ] {
        let Some((db, store, lease, revision, fake, client, server)) = fixture(Mode::Good).await
        else {
            return;
        };
        let grant = grant(&db, &fake).await;
        let control = crate::launcher::RunControl::default();
        let guard = LocalDisclosure::new(db.pool.clone(), grant.clone(), "Review".into())
            .with_control(control.clone());
        let request = Arc::new(guard.prepare().await.expect("prepare"));
        *fake.expected.write().expect("expected") = Arc::clone(&request);
        let pause = Arc::new(tokio::sync::Notify::new());
        *fake.generation_pause.lock().expect("pause") = Some(Arc::clone(&pause));
        let execution = fake.execution(revision, Uuid::new_v4());
        let attempt = execution.attempt_id;
        let job_store = store.clone();
        let job_lease = lease.clone();
        let work = tokio::spawn(async move {
            client
                .execute_guarded_and_persist(&job_store, &job_lease, &execution, &request, &guard)
                .await
        });
        tokio::time::timeout(Duration::from_secs(5), async {
            while fake.generated.load(Ordering::SeqCst) == 0 {
                tokio::time::sleep(Duration::from_millis(5)).await;
            }
        })
        .await
        .expect("generation began");
        match change {
            "shutdown" => control.stop(),
            "source" => {
                insert_version(
                    &db,
                    fake.project_id,
                    grant.sources[0].canonical_id,
                    2,
                    false,
                )
                .await
            }
            "tombstone" => {
                insert_version(&db, fake.project_id, grant.sources[0].canonical_id, 2, true).await
            }
            "principal" => {
                sqlx::query("UPDATE auth_principals SET disabled_at=clock_timestamp() WHERE id=$1")
                    .bind(grant.principal_id)
                    .execute(&db.pool)
                    .await
                    .expect("disable");
            }
            "permissions" => {
                sqlx::query("DELETE FROM auth_project_memberships WHERE principal_id=$1")
                    .bind(grant.principal_id)
                    .execute(&db.pool)
                    .await
                    .expect("revoke");
            }
            _ => unreachable!(),
        }
        pause.notify_one();
        assert!(work.await.expect("work").is_err(), "{change}");
        assert_eq!(
            store
                .settled_usage(attempt)
                .await
                .expect("known billing")
                .expect("usage")
                .total_tokens,
            120
        );
        assert!(
            store
                .private_proposal(Area::Workshop, fake.project_id, lease.action_id())
                .await
                .is_err()
        );
        assert_eq!(fake.generated.load(Ordering::SeqCst), 1);
        server.abort();
        db.cleanup().await.expect("cleanup");
    }
}

#[tokio::test]
async fn source_and_authority_locks_survive_proposal_write_until_the_same_commit() {
    let Some((db, store, lease, revision, fake, client, server)) = fixture(Mode::Good).await else {
        return;
    };
    let grant = grant(&db, &fake).await;
    let guard = LocalDisclosure::new(db.pool.clone(), grant.clone(), "Review".into());
    let request = Arc::new(guard.prepare().await.expect("prepare"));
    *fake.expected.write().expect("expected") = Arc::clone(&request);
    let execution = fake.execution(revision, Uuid::new_v4());
    let proposal = client
        .execute(&store, &lease, &execution, &request)
        .await
        .expect("settled result");
    let mut tx = db.pool.begin().await.expect("tx");
    sources::authorize_tx(&mut tx, &grant)
        .await
        .expect("hold authority and source locks");
    SupervisorStore::publish_proposal_tx(
        &mut tx,
        &lease,
        &execution.attempt_id,
        execution.project_id,
        &proposal,
        request.evidence_digests(),
    )
    .await
    .expect("tentative result");
    let mut other = db.pool.begin().await.expect("contender");
    sqlx::query("SET LOCAL lock_timeout='50ms'")
        .execute(&mut *other)
        .await
        .expect("lock timeout");
    assert!(
        sqlx::query("UPDATE auth_principals SET disabled_at=clock_timestamp() WHERE id=$1")
            .bind(grant.principal_id)
            .execute(&mut *other)
            .await
            .is_err()
    );
    other.rollback().await.expect("rollback contender");
    // Roll back after the result write: neither private proposal nor Verified leaks.
    tx.rollback().await.expect("rollback result");
    assert!(
        store
            .private_proposal(Area::Workshop, fake.project_id, lease.action_id())
            .await
            .is_err()
    );
    assert_eq!(
        store
            .action(Area::Workshop, lease.action_id())
            .await
            .expect("action")
            .state,
        ActionState::Running
    );
    guard
        .publish(&lease, &execution, &request, &proposal)
        .await
        .expect("atomic authorized commit");
    assert_eq!(
        store
            .action(Area::Workshop, lease.action_id())
            .await
            .expect("action")
            .state,
        ActionState::Verified
    );
    server.abort();
    db.cleanup().await.expect("cleanup");
}
