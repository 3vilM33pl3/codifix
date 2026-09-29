// SPDX-License-Identifier: AGPL-3.0-or-later
//! Internal rehearsal restricted answering and delivery. Not a generic agent/task
//! worker. There is no query capture, tool execution or recursive RPC.
mod model;
use crate::AppState;
use mem_federation::publication::LocalProjectAccess;
use mem_federation_runtime::InstanceSync;
use mem_record::Permission;
pub(super) use model::ModelConfig;
use sqlx::{Postgres, Transaction};
use std::time::Duration;
use uuid::Uuid;

#[cfg(test)]
pub(super) fn start(
    state: AppState,
    sync: InstanceSync,
    principal: Uuid,
    project: Uuid,
) -> tokio::task::JoinHandle<()> {
    start_projects(state, sync, vec![(project, principal)])
}

pub(super) fn start_projects(
    state: AppState,
    sync: InstanceSync,
    answerers: Vec<(Uuid, Uuid)>,
) -> tokio::task::JoinHandle<()> {
    tokio::spawn(async move {
        // Both loops belong to this task: abort/drop cancels both without
        // detached work. A slow answer cannot starve a live delivery challenge.
        tokio::join!(
            run(&state, &sync, &answerers, false),
            run(&state, &sync, &answerers, true)
        );
    })
}
async fn run(state: &AppState, sync: &InstanceSync, answerers: &[(Uuid, Uuid)], answer: bool) {
    loop {
        let work = async {
            if answer {
                answer_cycle(state, sync, answerers).await
            } else {
                cycle(state, sync, answerers).await
            }
        };
        let outcome = tokio::time::timeout(Duration::from_secs(12), work).await;
        if !matches!(outcome, Ok(Ok(()))) {
            tracing::warn!(answer_stage = answer, "expertise work paused or denied");
        }
        // DB state is authoritative. Lost polls/leases resume with a fresh
        // reader challenge; an old approval is never automatically reused.
        tokio::time::sleep(Duration::from_secs(2)).await;
    }
}
async fn answer_cycle(
    state: &AppState,
    sync: &InstanceSync,
    answerers: &[(Uuid, Uuid)],
) -> anyhow::Result<()> {
    let pool = state
        .pool()
        .map_err(|_| anyhow::anyhow!("database unavailable"))?;
    let mut tx = pool.begin().await?;
    sqlx::query("SET LOCAL lock_timeout='2s'")
        .execute(&mut *tx)
        .await?;
    sqlx::query("SET LOCAL statement_timeout='3s'")
        .execute(&mut *tx)
        .await?;
    let Some(lease) = sync.claim_expertise_question().await? else {
        tx.rollback().await?;
        return Ok(());
    };
    let (project, principal) = collection_binding(&mut tx, &lease.question, answerers).await?;
    let current = access(state, &mut tx, principal, project).await?;
    let work = async {
        output_policy(state, project, &lease.question)?;
        let (snapshot, ad) = mem_federation_store::eligible_question_collection(
            &mut tx,
            &current,
            lease.question.capability.publisher,
            &lease.question,
        )
        .await?;
        let selected =
            mem_federation_runtime::expertise_selected_context(&lease.question, &ad, &snapshot)?;
        let answer = if lease.question.mode == mem_federation::expertise::AnswerMode::Model
            && !selected.insufficient_evidence
        {
            model::answer(state, sync, project, &lease, selected).await?
        } else {
            selected
        };
        output_policy(state, project, &lease.question)?;
        // Recompute eligible citations while source and authority locks are held.
        mem_federation_runtime::validate_expertise_delivery(
            &lease.question,
            &ad,
            &snapshot,
            &answer,
        )?;
        let answer =
            mem_federation_store::persist_expertise_answer(&mut tx, project, &lease, &answer)
                .await?;
        tx.commit().await?;
        // Result survives a crash here. Recovered delivery still needs fresh policy.
        let mut tx = pool.begin().await?;
        sqlx::query("SET LOCAL lock_timeout='2s'")
            .execute(&mut *tx)
            .await?;
        sqlx::query("SET LOCAL statement_timeout='3s'")
            .execute(&mut *tx)
            .await?;
        let current = access(state, &mut tx, principal, project).await?;
        output_policy(state, project, &lease.question)?;
        sync.complete_expertise_question(&mut tx, &current, &lease, &answer)
            .await?;
        tx.rollback().await?;
        Ok::<(), anyhow::Error>(())
    };
    tokio::pin!(work);
    tokio::select! {
        result=&mut work=>result,
        _=tokio::time::sleep(Duration::from_secs(10))=>{
            sync.renew_expertise_question(&lease).await?;
            // The encompassing 12-second deadline bounds this deterministic
            // stage below its original 30-second lease even after renewal.
            work.await
        }
    }
}
async fn cycle(
    state: &AppState,
    sync: &InstanceSync,
    answerers: &[(Uuid, Uuid)],
) -> anyhow::Result<()> {
    let pool = state
        .pool()
        .map_err(|_| anyhow::anyhow!("database unavailable"))?;
    let mut tx = pool.begin().await?;
    sqlx::query("SET LOCAL lock_timeout='2s'")
        .execute(&mut *tx)
        .await?;
    sqlx::query("SET LOCAL statement_timeout='3s'")
        .execute(&mut *tx)
        .await?;
    if let Some(lease) = sync.claim_expertise_delivery().await? {
        let (project, principal) = collection_binding(&mut tx, &lease.question, answerers).await?;
        let access = access(state, &mut tx, principal, project).await?;
        output_policy(state, project, &lease.question)?;
        sync.authorize_expertise_delivery(&mut tx, &access, &lease)
            .await?;
    }
    tx.rollback().await?;
    Ok(())
}

async fn collection_binding(
    tx: &mut Transaction<'_, Postgres>,
    question: &mem_federation::expertise::ExpertiseQuestion,
    answerers: &[(Uuid, Uuid)],
) -> anyhow::Result<(Uuid, Uuid)> {
    // Gateway addressing is instance/collection, not a caller-selected local
    // project. Resolve the owner from our own immutable collection registration.
    // More than one project with the same collection ID is ambiguous and denied.
    let projects: Vec<Uuid> = sqlx::query_scalar("SELECT project_id FROM federation_expertise_collections WHERE space=$1 AND owner=$2 AND collection=$3 LIMIT 2 FOR SHARE")
        .bind(question.space.as_uuid()).bind(question.capability.publisher.as_uuid())
        .bind(question.capability.id.as_uuid()).fetch_all(&mut **tx).await?;
    anyhow::ensure!(
        projects.len() == 1,
        "expertise collection project unavailable or ambiguous"
    );
    answerers
        .iter()
        .find(|(project, _)| *project == projects[0])
        .copied()
        .ok_or_else(|| anyhow::anyhow!("expertise collection has no configured project answerer"))
}
fn output_policy(
    state: &AppState,
    project: Uuid,
    q: &mem_federation::expertise::ExpertiseQuestion,
) -> anyhow::Result<()> {
    let registry = state
        .federation
        .read()
        .expect("federation registry poisoned");
    let runtime = registry
        .as_ref()
        .and_then(|runtime| runtime.for_project(project))
        .ok_or_else(|| anyhow::anyhow!("expertise policy unavailable"))?;
    anyhow::ensure!(
        runtime.project == project
            && runtime.instance == q.capability.publisher
            && runtime
                .expertise_output_destinations
                .contains(&q.output_destination),
        "expertise output policy denied"
    );
    if q.mode == mem_federation::expertise::AnswerMode::Model {
        let config = runtime
            .expertise_model
            .as_ref()
            .ok_or_else(|| anyhow::anyhow!("expertise model disabled"))?;
        config.validate_consent(q)?;
    }
    Ok(())
}
pub(super) async fn access(
    state: &AppState,
    tx: &mut Transaction<'_, Postgres>,
    principal: Uuid,
    project: Uuid,
) -> anyhow::Result<LocalProjectAccess> {
    // A departed requester, human session, global administrator or group-based
    // identity cannot become this worker implicitly. Current per-project auth
    // is read under the same lock held through result-release authorization.
    let scoped: Option<bool> = sqlx::query_scalar("SELECT kind='internal' AND global_role IS NULL AND (global_permissions_json IS NULL OR global_permissions_json='[]'::jsonb) AND groups_json='[]'::jsonb FROM auth_principals WHERE id=$1 AND disabled_at IS NULL FOR SHARE")
        .bind(principal).fetch_optional(&mut **tx).await?;
    anyhow::ensure!(
        scoped == Some(true),
        "expertise service identity must be project-scoped and internal"
    );
    let permissions = crate::auth::background_permissions(
        state,
        tx,
        &mem_store::jobs::JobActor::Principal(principal),
        project,
    )
    .await
    .map_err(|_| anyhow::anyhow!("expertise current execution authority denied"))?;
    anyhow::ensure!(
        permissions.contains(Permission::MemoryRead)
            && permissions.contains(Permission::BundleExport),
        "expertise project permissions denied"
    );
    Ok(LocalProjectAccess {
        project,
        principal,
        permissions,
        read_only: state.config.service.read_only,
    })
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::sync::{Arc, Mutex};
    #[tokio::test]
    async fn collection_address_selects_only_its_configured_project_answerer() {
        let Some(db) = mem_test_support::IsolatedDatabase::new().await else {
            return;
        };
        let project = Uuid::new_v4();
        let other = Uuid::new_v4();
        let answerer = Uuid::new_v4();
        let now = chrono::Utc::now();
        let q: mem_federation::expertise::ExpertiseQuestion = serde_json::from_value(serde_json::json!({
            "request_id": Uuid::new_v4(), "space": Uuid::new_v4(), "requester": Uuid::new_v4(),
            "principal": Uuid::new_v4(), "capability": {"publisher": Uuid::new_v4(), "id": Uuid::new_v4()},
            "revision": 1, "collection_digest": {"algorithm":"sha256", "hex":"a".repeat(64)},
            "question":"synthetic", "mode":"deterministic", "model_destination":null,
            "output_destination":"local_human", "created_at":now, "expires_at": now + chrono::Duration::seconds(120)
        })).unwrap();
        let mut tx = db.pool.begin().await.unwrap();
        for p in [project, other] {
            sqlx::query("INSERT INTO federation_expertise_collections(space,owner,project_id,collection,revision) VALUES($1,$2,$3,$4,1)")
                .bind(q.space.as_uuid()).bind(q.capability.publisher.as_uuid()).bind(p)
                .bind(q.capability.id.as_uuid()).execute(&mut *tx).await.unwrap();
            if p == project {
                assert!(
                    collection_binding(&mut tx, &q, &[(other, Uuid::new_v4())])
                        .await
                        .is_err()
                );
                assert_eq!(
                    collection_binding(
                        &mut tx,
                        &q,
                        &[(project, answerer), (other, Uuid::new_v4())]
                    )
                    .await
                    .unwrap(),
                    (project, answerer)
                );
            } else {
                assert!(
                    collection_binding(&mut tx, &q, &[(project, answerer)])
                        .await
                        .is_err()
                );
            }
        }
        tx.rollback().await.unwrap();
        db.cleanup().await.unwrap();
    }
    #[tokio::test]
    async fn worker_authority_is_current_project_scoped_and_locked_through_release() {
        let Some(db) = mem_test_support::IsolatedDatabase::new().await else {
            return;
        };
        // This test only uses the state for frozen configuration/policy, never
        // the host's configuration or model providers.
        let config = serde_json::from_value(serde_json::json!({"service":{"bind_addr":"127.0.0.1:0","api_token":"synthetic"},
            "database":{"url":std::env::var("MEMORY_LAYER_TEST_DATABASE_URL").unwrap()},"offline":{"enabled":false},"cluster":{"enabled":false},"reinforcement":{"enabled":false}})).unwrap();
        let state = crate::runtime::build_state(config, Arc::new(Mutex::new(None)))
            .await
            .unwrap();
        let principal = Uuid::new_v4();
        let project = Uuid::new_v4();
        sqlx::query(
            "INSERT INTO projects(id,slug,name,root_path) VALUES($1,$2,'fixture','/synthetic')",
        )
        .bind(project)
        .bind(project.to_string())
        .execute(&db.pool)
        .await
        .unwrap();
        sqlx::query(
            "INSERT INTO auth_principals(id,kind,display_name) VALUES($1,'internal','fixture')",
        )
        .bind(principal)
        .execute(&db.pool)
        .await
        .unwrap();
        let mut tx = db.pool.begin().await.unwrap();
        assert!(access(&state, &mut tx, principal, project).await.is_err());
        tx.rollback().await.unwrap();
        sqlx::query("INSERT INTO auth_project_memberships(id,principal_id,project_id,role,source) VALUES($1,$2,$3,'operator','direct')")
            .bind(Uuid::new_v4()).bind(principal).bind(project).execute(&db.pool).await.unwrap();
        let mut tx = db.pool.begin().await.unwrap();
        access(&state, &mut tx, principal, project).await.unwrap();
        let pool = db.pool.clone();
        let mut revoked = tokio::spawn(async move {
            sqlx::query(
                "DELETE FROM auth_project_memberships WHERE principal_id=$1 AND project_id=$2",
            )
            .bind(principal)
            .bind(project)
            .execute(&pool)
            .await
            .unwrap();
        });
        assert!(
            tokio::time::timeout(Duration::from_millis(100), &mut revoked)
                .await
                .is_err()
        );
        tx.rollback().await.unwrap();
        revoked.await.unwrap();
        let mut tx = db.pool.begin().await.unwrap();
        assert!(access(&state, &mut tx, principal, project).await.is_err());
        tx.rollback().await.unwrap();
        sqlx::query("UPDATE auth_principals SET global_role='admin' WHERE id=$1")
            .bind(principal)
            .execute(&db.pool)
            .await
            .unwrap();
        let mut tx = db.pool.begin().await.unwrap();
        assert!(access(&state, &mut tx, principal, project).await.is_err());
        tx.rollback().await.unwrap();
        db.cleanup().await.unwrap();
    }
}
