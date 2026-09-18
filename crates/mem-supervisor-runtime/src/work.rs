//! An operator-pinned single-work manifest, not a model-selected task queue.
//! Restart observes the same action. Running/terminal actions are never resent.
use crate::{
    launcher::{LaunchError, RunControl, SandboxConfig},
    local_disclosure::LocalDisclosure,
    model::PreparedRequest,
    private_files, strict_json,
    transport::ProposalExecution,
};
use mem_supervisor::{
    Area, StableId,
    action::{ActionKey, ActionKind, ActionState},
};
use mem_supervisor_store::{
    ActionSubmission, LeaseClaim, ReadyTransition, SupervisorStore, sources::LocalCollectionGrant,
};
use serde::{Deserialize, Serialize};
use sqlx::PgPool;
use std::{path::Path, sync::Arc};
use uuid::Uuid;

#[derive(Clone, Deserialize, Serialize)]
#[serde(deny_unknown_fields)]
pub struct WorkManifest {
    pub schema_version: u32,
    pub action_id: Uuid,
    pub source_event_id: Uuid,
    pub attempt_id: Uuid,
    pub grant: LocalCollectionGrant,
    pub question: String,
    pub sandbox: SandboxConfig,
}

pub struct PinnedWork {
    manifest: WorkManifest,
    digest: [u8; 32],
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, thiserror::Error)]
pub enum WorkError {
    #[error("work manifest is invalid, unsafe or mismatched")]
    Manifest,
    #[error("current work authority or sources are unavailable; no new call authorized")]
    Authority,
    #[error("durable work state unavailable; inspect before retrying")]
    Storage,
    #[error("work stopped; no automatic retry")]
    Stopped,
}

pub enum WorkStep {
    State(ActionState),
    Claimed(Box<ClaimedWork>),
}
pub struct ClaimedWork {
    pub store: SupervisorStore,
    pub lease: LeaseClaim,
    pub execution: ProposalExecution,
    pub request: Arc<PreparedRequest>,
    pub disclosure: Arc<LocalDisclosure>,
    pub sandbox: SandboxConfig,
}

impl PinnedWork {
    pub fn grant(&self) -> &LocalCollectionGrant {
        &self.manifest.grant
    }
    pub fn question(&self) -> &str {
        &self.manifest.question
    }
    pub fn digest(&self) -> [u8; 32] {
        self.digest
    }
    /// Read only a private, hash-bound operator file. No HTTP manifest endpoint.
    pub fn load(path: &Path, hash: &str, instance: Uuid, area: Area) -> Result<Self, WorkError> {
        let bytes = private_files::read(path, hash).map_err(|_| WorkError::Manifest)?;
        Self::decode(
            &bytes,
            private_files::digest(hash).map_err(|_| WorkError::Manifest)?,
            instance,
            area,
        )
    }
    fn decode(
        bytes: &[u8],
        digest: [u8; 32],
        instance: Uuid,
        area: Area,
    ) -> Result<Self, WorkError> {
        let value = strict_json::parse(bytes).map_err(|_| WorkError::Manifest)?;
        let manifest: WorkManifest =
            serde_json::from_value(value).map_err(|_| WorkError::Manifest)?;
        if area != Area::Workshop
            || manifest.schema_version != 1
            || manifest.grant.instance_id != instance
            || [
                instance,
                manifest.action_id,
                manifest.source_event_id,
                manifest.attempt_id,
                manifest.grant.project_id,
                manifest.grant.policy_revision,
                manifest.grant.principal_id,
            ]
            .iter()
            .any(Uuid::is_nil)
            || manifest.question.trim().is_empty()
            || manifest.question.len() > 4096
            || manifest.grant.sources.is_empty()
            || manifest.grant.sources.len() > 8
        {
            return Err(WorkError::Manifest);
        }
        private_files::digest(&manifest.sandbox.driver_sha256).map_err(|_| WorkError::Manifest)?;
        Ok(Self { manifest, digest })
    }
    pub fn action_id(&self) -> Uuid {
        self.manifest.action_id
    }
    pub fn attempt_id(&self) -> Uuid {
        self.manifest.attempt_id
    }

    /// Metadata recovery does not require a provider credential. Never claims,
    /// reads source contents or contacts a provider; changed terms still fail.
    pub async fn observe(&self, pool: PgPool) -> Result<ActionState, WorkError> {
        let m = &self.manifest;
        let store = SupervisorStore::new(pool);
        store
            .reap_expired(Area::Workshop, 100)
            .await
            .map_err(|_| WorkError::Storage)?;
        Ok(store
            .submit(&ActionSubmission {
                id: m.action_id,
                key: ActionKey {
                    area: Area::Workshop,
                    source_event: stable(m.source_event_id)?,
                    kind: ActionKind::ProposeCorrection,
                    policy_revision: stable(m.grant.policy_revision)?,
                },
                source_digest: self.digest,
            })
            .await
            .map_err(|_| WorkError::Storage)?
            .state)
    }

    /// The embedding service checks read-only/evaluation/role before calling.
    /// Competing polls may lose a transition; a later poll reads durable state.
    /// Authority is independently checked again at every disclosure and commit.
    pub async fn step(&self, pool: PgPool, control: RunControl) -> Result<WorkStep, WorkError> {
        if control.stopped() {
            return Err(WorkError::Stopped);
        }
        let m = &self.manifest;
        let store = SupervisorStore::new(pool.clone());
        let transition = match self.observe(pool.clone()).await? {
            ActionState::Observed => Some(ReadyTransition::Propose),
            ActionState::Proposed => Some(ReadyTransition::Authorize),
            ActionState::Authorized => Some(ReadyTransition::Queue),
            ActionState::Queued => None,
            state => return Ok(WorkStep::State(state)),
        };
        let disclosure = Arc::new(
            LocalDisclosure::new(pool, m.grant.clone(), m.question.clone())
                .with_control(control.clone()),
        );
        let request = Arc::new(
            disclosure
                .prepare()
                .await
                .map_err(|_| WorkError::Authority)?,
        );
        if control.stopped() {
            return Err(WorkError::Stopped);
        }
        if let Some(transition) = transition {
            store
                .advance(Area::Workshop, m.action_id, transition, self.digest)
                .await
                .map_err(|_| WorkError::Storage)?;
            return Ok(WorkStep::State(
                store
                    .action(Area::Workshop, m.action_id)
                    .await
                    .map_err(|_| WorkError::Storage)?
                    .state,
            ));
        }
        // One bounded sandbox + broker run. Never renew across a provider send.
        let lease = store
            .claim(Area::Workshop, m.action_id, Uuid::new_v4(), 180)
            .await
            .map_err(|_| WorkError::Storage)?;
        Ok(WorkStep::Claimed(Box::new(ClaimedWork {
            store,
            lease,
            request,
            disclosure,
            execution: ProposalExecution {
                instance_id: m.grant.instance_id,
                policy_revision: m.grant.policy_revision,
                project_id: m.grant.project_id,
                attempt_id: m.attempt_id,
            },
            sandbox: m.sandbox.clone(),
        })))
    }
}
fn stable(id: Uuid) -> Result<StableId, WorkError> {
    StableId::new(id.to_string()).map_err(|_| WorkError::Manifest)
}

impl ClaimedWork {
    /// A child report is not proof of persistence. Check the exact ledger result.
    pub async fn confirm(&self, hash: [u8; 32]) -> Result<(), LaunchError> {
        use sha2::{Digest, Sha256};
        let stored = self
            .store
            .private_proposal(
                Area::Workshop,
                self.execution.project_id,
                self.lease.action_id(),
            )
            .await
            .map_err(|_| LaunchError::Child)?;
        if stored.attempt_id != self.execution.attempt_id
            || stored.policy_revision != self.execution.policy_revision
            || stored.evidence_digests != self.request.evidence_digests()
            || stored.request_digest != private_files::digest(self.request.digest())?
            || self
                .store
                .settled_usage(self.execution.attempt_id)
                .await
                .map_err(|_| LaunchError::Child)?
                .is_none()
        {
            return Err(LaunchError::Child);
        }
        let bytes = serde_json::to_vec(&stored.proposal).map_err(|_| LaunchError::Child)?;
        if <[u8; 32]>::from(Sha256::digest(&bytes)) != hash {
            return Err(LaunchError::Child);
        }
        Ok(())
    }
}
