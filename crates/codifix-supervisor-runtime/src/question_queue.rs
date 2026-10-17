//! Operator-only processing of one durable human question. The application
//! never receives Docker/provider authority, and queue recovery never resends.
use crate::{
    launcher::RunControl,
    operator::{self, Outcome, Report},
    private_files,
    transport::OpenAiTransport,
    work::{PinnedWork, WorkError},
};
use codifix_supervisor::Area;
use codifix_supervisor_store::{SupervisorStore, questions};
use serde::{Deserialize, Serialize};
use sqlx::{PgPool, Postgres, pool::PoolConnection};
use std::{
    path::{Path, PathBuf},
    sync::Arc,
};
use uuid::Uuid;

/// A validated exact service configuration, not configuration discovery. No
/// environment/adjacent-file overrides, credentials or database URLs are used.
/// The same private bytes must still exist at every disclosure and commit.
pub struct ServiceExecutionGuard {
    path: PathBuf,
    sha256: String,
    instance_id: Uuid,
    profile_digest: [u8; 32],
}

// Data-only view of the operator-pinned TOML, never config discovery/platform
// dependencies. Mirrors mem-config::app safety semantics: semantic_time enables
// evaluation, read_only defaults false, clustering defaults true. Unrelated app
// fields are ignored; malformed safety fields fail closed. No env coercions.
#[derive(Deserialize)]
struct ExecutionConfig {
    service: ExecutionService,
    #[serde(default)]
    evaluation: ExecutionEvaluation,
    #[serde(default)]
    cluster: ExecutionCluster,
    supervisor: ExecutionIdentity,
}
#[derive(Deserialize)]
struct ExecutionService {
    #[serde(default)]
    read_only: bool,
}
#[derive(Default, Deserialize)]
#[serde(deny_unknown_fields)]
struct ExecutionEvaluation {
    semantic_time: Option<chrono::DateTime<chrono::Utc>>,
}
#[derive(Deserialize)]
struct ExecutionCluster {
    #[serde(default = "enabled")]
    enabled: bool,
}
fn enabled() -> bool {
    true
}
impl Default for ExecutionCluster {
    fn default() -> Self {
        Self { enabled: true }
    }
}
#[derive(Deserialize)]
struct ExecutionIdentity {
    instance_id: Uuid,
    area: Area,
    interaction: ExecutionInteraction,
}
#[derive(Deserialize)]
struct ExecutionInteraction {
    sha256: String,
}

impl ServiceExecutionGuard {
    pub fn load(path: &Path, hash: &str, work: &PinnedWork) -> Result<Self, WorkError> {
        let bytes = private_files::read(path, hash).map_err(|_| WorkError::Authority)?;
        let config: ExecutionConfig =
            toml::from_str(std::str::from_utf8(&bytes).map_err(|_| WorkError::Authority)?)
                .map_err(|_| WorkError::Authority)?;
        let identity = &config.supervisor;
        let interaction = &identity.interaction;
        if config.service.read_only
            || config.evaluation.semantic_time.is_some()
            || config.cluster.enabled
            || identity.instance_id != work.grant().instance_id
            || identity.area != Area::Workshop
            || private_files::digest(&interaction.sha256).map_err(|_| WorkError::Authority)?
                != work.digest()
        {
            return Err(WorkError::Authority);
        }
        Ok(Self {
            path: path.to_owned(),
            sha256: hash.to_owned(),
            instance_id: work.grant().instance_id,
            profile_digest: work.digest(),
        })
    }

    pub(crate) fn check(&self, instance: Uuid, profile_digest: [u8; 32]) -> Result<(), WorkError> {
        if self.instance_id != instance || self.profile_digest != profile_digest {
            return Err(WorkError::Authority);
        }
        private_files::read(&self.path, &self.sha256)
            .map(|_| ())
            .map_err(|_| WorkError::Authority)
    }
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize)]
#[serde(rename_all = "snake_case")]
pub enum QueueOutcome {
    Idle,
    Busy,
    Blocked,
    CredentialUnavailable,
    StorageUnavailable,
    Stopped,
    Processed,
}

/// Metadata only; questions, source contents and provider responses never reach
/// stdout or operational journals.
#[derive(Debug, Serialize)]
pub struct QueueReport {
    pub outcome: QueueOutcome,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub work: Option<Report>,
}

fn report(outcome: QueueOutcome) -> QueueReport {
    QueueReport {
        outcome,
        work: None,
    }
}

/// A session lock must never be returned to the pool with its connection. On
/// cancellation/drop, close the socket so PostgreSQL releases the lock too.
struct QueueSlot(PoolConnection<Postgres>);
impl Drop for QueueSlot {
    fn drop(&mut self) {
        self.0.close_on_drop();
    }
}

async fn slot(pool: &PgPool) -> Result<Option<QueueSlot>, WorkError> {
    let mut connection = QueueSlot(pool.acquire().await.map_err(|_| WorkError::Storage)?);
    // Fixed namespace for this Workshop database; never caller-selected.
    let acquired: bool = sqlx::query_scalar("SELECT pg_try_advisory_lock(1835363695,1937076338)")
        .fetch_one(&mut *connection.0)
        .await
        .map_err(|_| WorkError::Storage)?;
    if acquired {
        Ok(Some(connection))
    } else {
        Ok(None)
    }
}

/// At most one queued question and one physical sandbox launch. No polling
/// daemon, retries, provisioning or policy enablement is performed here.
pub async fn run_next(
    profile: &PinnedWork,
    pool: PgPool,
    transport: Option<OpenAiTransport>,
    guard: Arc<ServiceExecutionGuard>,
    control: RunControl,
) -> QueueReport {
    if control.stopped() {
        return report(QueueOutcome::Stopped);
    }
    if guard
        .check(profile.grant().instance_id, profile.digest())
        .is_err()
    {
        return report(QueueOutcome::Blocked);
    }
    let _slot = match slot(&pool).await {
        Ok(Some(slot)) => slot,
        Ok(None) => return report(QueueOutcome::Busy),
        Err(_) => return report(QueueOutcome::StorageUnavailable),
    };
    if SupervisorStore::new(pool.clone())
        .reap_expired(Area::Workshop, 100)
        .await
        .is_err()
    {
        return report(QueueOutcome::StorageUnavailable);
    }
    // Missing credentials never claim work or consume/deny a user's question.
    let Some(transport) = transport else {
        return report(QueueOutcome::CredentialUnavailable);
    };
    let question = match questions::next(&pool, profile.grant().project_id, profile.digest()).await
    {
        Ok(Some(question)) => question,
        Ok(None) => return report(QueueOutcome::Idle),
        Err(_) => return report(QueueOutcome::StorageUnavailable),
    };
    let id = question.submission.request_id;
    let work = match profile.for_question(question, guard) {
        Ok(work) => work,
        Err(_) => {
            let _ = questions::deny(&pool, id).await;
            return report(QueueOutcome::Blocked);
        }
    };
    let result = operator::run(&work, pool.clone(), Some(transport), control).await;
    if result.outcome == Outcome::AuthorityUnavailable {
        // A terminal metadata denial avoids retrying a revoked/expired request.
        // No attempt identity changes, refund or provider resend is implied.
        if questions::deny(&pool, id).await.is_err() {
            return report(QueueOutcome::StorageUnavailable);
        }
    }
    QueueReport {
        outcome: QueueOutcome::Processed,
        work: Some(result),
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::{launcher::SandboxConfig, work::WorkManifest};
    use codifix_supervisor_store::sources::{
        LocalCollectionGrant, ModelDestination, SourceBinding,
    };
    use sha2::{Digest, Sha256};

    fn private(path: &Path, text: &str) -> String {
        crate::test_files::write(path, text.as_bytes());
        format!("{:x}", Sha256::digest(text.as_bytes()))
    }

    fn fixture() -> (tempfile::TempDir, PinnedWork, String) {
        let dir = crate::test_files::directory();
        let id = Uuid::new_v4();
        let manifest = WorkManifest {
            schema_version: 1,
            action_id: Uuid::new_v4(),
            source_event_id: Uuid::new_v4(),
            attempt_id: Uuid::new_v4(),
            grant: LocalCollectionGrant {
                instance_id: id,
                project_id: Uuid::new_v4(),
                principal_id: Uuid::new_v4(),
                policy_revision: Uuid::new_v4(),
                destination: ModelDestination::OpenAi,
                expires_at: chrono::Utc::now() + chrono::Duration::hours(1),
                sources: vec![SourceBinding {
                    canonical_id: Uuid::new_v4(),
                    version_id: Uuid::new_v4(),
                    version: 1,
                    evidence_digest: [3; 32],
                }],
            },
            question: "Synthetic original question".into(),
            sandbox: SandboxConfig {
                pi: "/unused-pi".into(),
                driver: "/unused-driver".into(),
                driver_sha256: "0".repeat(64),
                private_runtime: dir.path().into(),
                windows_artifacts: None,
            },
        };
        let hash = private(
            &dir.path().join("work.json"),
            &serde_json::to_string(&manifest).expect("manifest"),
        );
        let profile = PinnedWork::load(&dir.path().join("work.json"), &hash, id, Area::Workshop)
            .expect("profile");
        let config = format!(
            "[service]\nread_only=false\napi_token='synthetic-not-disclosed'\n[database]\nurl='ignored-no-network-selection'\n[cluster]\nenabled=false\n[supervisor]\ninstance_id='{id}'\narea='workshop'\n[supervisor.interaction]\nmanifest='/in-container/work.json'\nsha256='{hash}'\n"
        );
        (dir, profile, config)
    }

    #[test]
    fn config_guard_rejects_modes_identity_and_every_changed_byte() {
        let (dir, profile, config) = fixture();
        let path = dir.path().join("service.toml");
        let hash = private(&path, &config);
        let guard = ServiceExecutionGuard::load(&path, &hash, &profile).expect("guard");
        assert!(
            guard
                .check(profile.grant().instance_id, profile.digest())
                .is_ok()
        );
        assert!(guard.check(Uuid::new_v4(), profile.digest()).is_err());
        assert!(guard.check(profile.grant().instance_id, [9; 32]).is_err());
        for changed in [
            config.replace("read_only=false", "read_only=true"),
            config.replace("enabled=false", "enabled=true"),
            config.replace("[cluster]\nenabled=false\n", ""),
            config.replace("enabled=false", ""),
            config.replace("read_only=false", "read_only='false'"),
            format!("{config}\n[evaluation]\nsemantic_time='2026-09-19T00:00:00Z'\n"),
            format!("{config}\n[evaluation]\nsemantic_time='not-a-timestamp'\n"),
            format!("{config}\n[evaluation]\nunknown=true\n"),
            config.replace("area='workshop'", "area='general'"),
            config.replace(
                &profile.grant().instance_id.to_string(),
                &Uuid::new_v4().to_string(),
            ),
            config.replace("sha256='", "sha256='wrong"),
        ] {
            let new_hash = private(&path, &changed);
            assert!(ServiceExecutionGuard::load(&path, &new_hash, &profile).is_err());
            assert!(
                guard
                    .check(profile.grant().instance_id, profile.digest())
                    .is_err()
            );
        }
    }

    #[tokio::test]
    async fn queue_slot_closes_its_connection_and_releases_after_cancellation() {
        let Some(db) = codifix_test_support::IsolatedDatabase::new().await else {
            return;
        };
        let held = slot(&db.pool).await.expect("claim").expect("slot");
        assert!(slot(&db.pool).await.expect("contender").is_none());
        drop(held);
        let recovered = tokio::time::timeout(std::time::Duration::from_secs(5), async {
            loop {
                if let Some(slot) = slot(&db.pool).await.expect("recover") {
                    break slot;
                }
                tokio::time::sleep(std::time::Duration::from_millis(10)).await;
            }
        })
        .await
        .expect("closed connection releases session lock");
        drop(recovered);
        db.cleanup().await.expect("cleanup");
    }
}
