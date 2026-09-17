use serde::{Deserialize, Serialize};

use crate::{
    Area, StableId, UnixSeconds,
    lease::{Lease, LeaseAuthority, LeaseError},
    policy::{PolicyError, SupervisorPolicy},
};

#[derive(Debug, Clone, Copy, PartialEq, Eq, PartialOrd, Ord, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum ActionKind {
    HealthCheck,
    StaleKnowledgeReview,
    AskQuestion,
    ProposeCorrection,
    ReviewRelease,
    InstallMaintenance,
}

/// Stable event/policy identity. Storage must uniquely constrain this composite
/// key. Redelivery keeps the same key; attempts do not create fresh actions.
#[derive(Debug, Clone, PartialEq, Eq, PartialOrd, Ord, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct ActionKey {
    pub area: Area,
    pub source_event: StableId,
    pub kind: ActionKind,
    pub policy_revision: StableId,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum ActionState {
    Observed,
    Proposed,
    Authorized,
    Queued,
    Running,
    Verified,
    Denied,
    Failed,
    Expired,
    Cancelled,
    Uncertain,
}

/// State machine for one action. A runtime supplies immutable evidence references
/// and persists each change and history entry in one transaction.
#[derive(Debug, Clone, Serialize)]
pub struct Action {
    id: StableId,
    key: ActionKey,
    state: ActionState,
    evidence: Vec<StableId>,
    lease: LeaseAuthority,
}

impl Action {
    pub fn observe(id: StableId, key: ActionKey, evidence: StableId) -> Self {
        let lease = LeaseAuthority::new(id.clone());
        Self {
            id,
            key,
            state: ActionState::Observed,
            evidence: vec![evidence],
            lease,
        }
    }

    pub fn id(&self) -> &StableId {
        &self.id
    }

    pub fn key(&self) -> &ActionKey {
        &self.key
    }

    pub fn state(&self) -> ActionState {
        self.state
    }

    pub fn evidence(&self) -> &[StableId] {
        &self.evidence
    }

    pub fn propose(&mut self) -> Result<(), ActionError> {
        self.advance(ActionState::Observed, ActionState::Proposed)
    }

    /// Only validates the common policy. Kind-specific permission (question
    /// scope, release eligibility, human approval) must be checked by the runtime
    /// in the same transaction; this method is not an external effect permit.
    pub fn authorize(&mut self, policy: &SupervisorPolicy) -> Result<(), ActionError> {
        policy.validate(self.key.area, &self.key.policy_revision)?;
        self.advance(ActionState::Proposed, ActionState::Authorized)
    }

    pub fn queue(&mut self, policy: &SupervisorPolicy) -> Result<(), ActionError> {
        policy.validate(self.key.area, &self.key.policy_revision)?;
        self.advance(ActionState::Authorized, ActionState::Queued)
    }

    pub fn claim(
        &mut self,
        policy: &SupervisorPolicy,
        worker: StableId,
        now: UnixSeconds,
        ttl_seconds: u64,
    ) -> Result<Lease, ActionError> {
        policy.validate(self.key.area, &self.key.policy_revision)?;
        if self.state != ActionState::Queued {
            return Err(ActionError::InvalidTransition);
        }
        let lease = self.lease.claim(worker, now, ttl_seconds)?;
        self.state = ActionState::Running;
        Ok(lease)
    }

    pub fn validate_effect(
        &self,
        lease: &Lease,
        policy: &SupervisorPolicy,
        now: UnixSeconds,
    ) -> Result<(), ActionError> {
        policy.validate(self.key.area, &self.key.policy_revision)?;
        self.require_running()?;
        self.lease.validate(lease, now)?;
        Ok(())
    }

    pub fn renew(
        &mut self,
        lease: &Lease,
        now: UnixSeconds,
        ttl_seconds: u64,
    ) -> Result<Lease, ActionError> {
        self.require_running()?;
        Ok(self.lease.renew(lease, now, ttl_seconds)?)
    }

    pub fn finish(
        &mut self,
        lease: &Lease,
        now: UnixSeconds,
        outcome: KnownOutcome,
        evidence: StableId,
    ) -> Result<(), ActionError> {
        self.require_running()?;
        self.lease.validate(lease, now)?;
        self.lease.revoke()?;
        self.state = outcome.state();
        self.evidence.push(evidence);
        Ok(())
    }

    /// Revocation/cancellation cannot establish whether an in-flight effect ran.
    pub fn stop(&mut self, reason: StopReason, evidence: StableId) -> Result<(), ActionError> {
        match self.state {
            ActionState::Observed
            | ActionState::Proposed
            | ActionState::Authorized
            | ActionState::Queued => {
                self.lease.revoke()?;
                self.state = match reason {
                    StopReason::Denied => ActionState::Denied,
                    StopReason::Cancelled => ActionState::Cancelled,
                    StopReason::Expired => ActionState::Expired,
                };
            }
            ActionState::Running => {
                self.lease.revoke()?;
                self.state = ActionState::Uncertain;
            }
            _ => return Err(ActionError::InvalidTransition),
        }
        self.evidence.push(evidence);
        Ok(())
    }

    pub fn expire_lease(
        &mut self,
        now: UnixSeconds,
        evidence: StableId,
    ) -> Result<(), ActionError> {
        self.require_running()?;
        if !self.lease.is_expired(now) {
            return Err(ActionError::LeaseStillLive);
        }
        self.stop(StopReason::Expired, evidence)
    }

    /// Only a trusted reconciler with authoritative effect evidence may call this.
    /// Uncertain work cannot be queued or claimed again automatically.
    pub fn reconcile(
        &mut self,
        outcome: KnownOutcome,
        evidence: StableId,
    ) -> Result<(), ActionError> {
        if self.state != ActionState::Uncertain {
            return Err(ActionError::InvalidTransition);
        }
        self.state = outcome.state();
        self.evidence.push(evidence);
        Ok(())
    }

    fn require_running(&self) -> Result<(), ActionError> {
        if self.state != ActionState::Running {
            return Err(ActionError::InvalidTransition);
        }
        Ok(())
    }

    fn advance(&mut self, from: ActionState, to: ActionState) -> Result<(), ActionError> {
        if self.state != from {
            return Err(ActionError::InvalidTransition);
        }
        self.state = to;
        Ok(())
    }
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum KnownOutcome {
    Verified,
    Failed,
}

impl KnownOutcome {
    fn state(self) -> ActionState {
        match self {
            Self::Verified => ActionState::Verified,
            Self::Failed => ActionState::Failed,
        }
    }
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum StopReason {
    Denied,
    Cancelled,
    Expired,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, thiserror::Error)]
pub enum ActionError {
    #[error(transparent)]
    Policy(#[from] PolicyError),
    #[error(transparent)]
    Lease(#[from] LeaseError),
    #[error("action cannot make this transition")]
    InvalidTransition,
    #[error("action lease is still live")]
    LeaseStillLive,
}
