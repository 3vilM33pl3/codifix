//! Evidence-gated local improvement. No mandate, capability or credential field
//! exists in this API; adopting a candidate cannot expand authority.
use crate::{PolicyError, valid_identifier};
use chrono::{DateTime, Utc};
use serde::{Deserialize, Serialize};
use uuid::Uuid;

#[derive(Clone, Copy, Debug, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum ImprovementKind {
    Prompt,
    Procedure,
    Retrieval,
    ModelProfile,
}
#[derive(Clone, Copy, Debug, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum ImprovementStage {
    Candidate,
    Evaluated,
    LimitedRollout,
    Promoted,
    Reverted,
}
#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct ImprovementEvaluation {
    pub evidence_id: String,
    pub candidate_revision: String,
    pub baseline_revision: String,
    pub held_out_set_id: String,
    pub training_set_id: String,
    pub held_out_cases: u32,
    pub candidate_quality_ppm: u32,
    pub baseline_quality_ppm: u32,
    pub safety_failures: u32,
    pub evaluated_at: DateTime<Utc>,
}
#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct ImprovementGate {
    pub minimum_held_out_cases: u32,
    pub minimum_quality_ppm: u32,
    pub maximum_rollout_ppm: u32,
    pub minimum_rollout_successes: u32,
}
#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct LocalAcceptance {
    pub owner_instance_id: Uuid,
    pub decision_ref: String,
    pub accepted_at: DateTime<Utc>,
}
#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct ImprovementCandidate {
    pub id: Uuid,
    pub owner_instance_id: Uuid,
    pub kind: ImprovementKind,
    pub candidate_revision: String,
    pub baseline_revision: String,
    pub stage: ImprovementStage,
    pub evaluation: Option<ImprovementEvaluation>,
    pub rollout_ppm: u32,
    pub rollout_successes: u32,
    pub rollout_failures: u32,
    pub local_acceptance: Option<LocalAcceptance>,
    pub revert_reason: Option<String>,
}
impl ImprovementCandidate {
    fn validate_gate(&self, gate: &ImprovementGate) -> Result<(), PolicyError> {
        if self.id.is_nil()
            || self.owner_instance_id.is_nil()
            || self.candidate_revision.trim().is_empty()
            || self.baseline_revision.trim().is_empty()
            || self.candidate_revision == self.baseline_revision
            || gate.minimum_held_out_cases == 0
            || gate.minimum_quality_ppm > 1_000_000
            || gate.maximum_rollout_ppm == 0
            || gate.maximum_rollout_ppm > 100_000
            || gate.minimum_rollout_successes == 0
        {
            return Err(PolicyError::Invalid("improvement gate"));
        }
        Ok(())
    }
    pub fn record_evaluation(
        &mut self,
        evaluation: ImprovementEvaluation,
        gate: &ImprovementGate,
        now: DateTime<Utc>,
    ) -> Result<(), PolicyError> {
        self.validate_gate(gate)?;
        if self.stage != ImprovementStage::Candidate {
            return Err(PolicyError::Transition);
        }
        self.check_evaluation(&evaluation, gate, now)?;
        self.evaluation = Some(evaluation);
        self.stage = ImprovementStage::Evaluated;
        Ok(())
    }
    pub fn start_rollout(
        &mut self,
        fraction_ppm: u32,
        acceptance: LocalAcceptance,
        gate: &ImprovementGate,
        now: DateTime<Utc>,
    ) -> Result<(), PolicyError> {
        self.validate_gate(gate)?;
        if self.stage != ImprovementStage::Evaluated || self.evaluation.is_none() {
            return Err(PolicyError::Transition);
        }
        self.check_evaluation(
            self.evaluation
                .as_ref()
                .ok_or(PolicyError::Invalid("missing evaluation"))?,
            gate,
            now,
        )?;
        self.check_acceptance(&acceptance, now)?;
        if fraction_ppm == 0 || fraction_ppm > gate.maximum_rollout_ppm {
            return Err(PolicyError::Invalid("rollout fraction"));
        }
        self.local_acceptance = Some(acceptance);
        self.rollout_ppm = fraction_ppm;
        self.rollout_successes = 0;
        self.rollout_failures = 0;
        self.stage = ImprovementStage::LimitedRollout;
        Ok(())
    }
    pub fn record_rollout_result(&mut self, passed: bool) -> Result<(), PolicyError> {
        if self.stage != ImprovementStage::LimitedRollout {
            return Err(PolicyError::Transition);
        }
        let counter = if passed {
            &mut self.rollout_successes
        } else {
            &mut self.rollout_failures
        };
        *counter = counter
            .checked_add(1)
            .ok_or(PolicyError::Invalid("rollout counter overflow"))?;
        Ok(())
    }
    /// A fresh local decision is required for promotion after observing rollout results.
    pub fn promote(
        &mut self,
        acceptance: LocalAcceptance,
        gate: &ImprovementGate,
        now: DateTime<Utc>,
    ) -> Result<(), PolicyError> {
        self.validate_gate(gate)?;
        self.check_evaluation(
            self.evaluation
                .as_ref()
                .ok_or(PolicyError::Invalid("missing evaluation"))?,
            gate,
            now,
        )?;
        self.check_acceptance(&acceptance, now)?;
        if self.stage != ImprovementStage::LimitedRollout
            || self.rollout_successes < gate.minimum_rollout_successes
            || self.rollout_ppm == 0
            || self.rollout_ppm > gate.maximum_rollout_ppm
            || self.rollout_failures != 0
            || self.local_acceptance.as_ref().is_none_or(|previous| {
                acceptance.decision_ref == previous.decision_ref
                    || acceptance.accepted_at < previous.accepted_at
            })
        {
            return Err(PolicyError::Transition);
        }
        self.local_acceptance = Some(acceptance);
        self.rollout_ppm = 1_000_000;
        self.stage = ImprovementStage::Promoted;
        Ok(())
    }
    pub fn revert(&mut self, reason: String) -> Result<(), PolicyError> {
        if !matches!(
            self.stage,
            ImprovementStage::LimitedRollout | ImprovementStage::Promoted
        ) || reason.trim().is_empty()
        {
            return Err(PolicyError::Transition);
        }
        self.rollout_ppm = 0;
        self.stage = ImprovementStage::Reverted;
        self.revert_reason = Some(reason);
        Ok(())
    }
    fn check_evaluation(
        &self,
        evaluation: &ImprovementEvaluation,
        gate: &ImprovementGate,
        now: DateTime<Utc>,
    ) -> Result<(), PolicyError> {
        if !valid_identifier(&evaluation.evidence_id)
            || !valid_identifier(&evaluation.held_out_set_id)
            || !valid_identifier(&evaluation.training_set_id)
            || evaluation.held_out_set_id == evaluation.training_set_id
            || evaluation.candidate_revision != self.candidate_revision
            || evaluation.baseline_revision != self.baseline_revision
            || evaluation.held_out_cases < gate.minimum_held_out_cases
            || evaluation.safety_failures != 0
            || evaluation.candidate_quality_ppm > 1_000_000
            || evaluation.baseline_quality_ppm > 1_000_000
            || evaluation.candidate_quality_ppm < gate.minimum_quality_ppm
            || evaluation.candidate_quality_ppm < evaluation.baseline_quality_ppm
            || evaluation.evaluated_at > now
        {
            return Err(PolicyError::Invalid("held-out evaluation"));
        }
        Ok(())
    }
    fn check_acceptance(
        &self,
        acceptance: &LocalAcceptance,
        now: DateTime<Utc>,
    ) -> Result<(), PolicyError> {
        if acceptance.owner_instance_id != self.owner_instance_id
            || acceptance.decision_ref.trim().is_empty()
            || acceptance.accepted_at > now
            || self
                .evaluation
                .as_ref()
                .is_none_or(|e| acceptance.accepted_at < e.evaluated_at)
        {
            return Err(PolicyError::Invalid("local acceptance"));
        }
        Ok(())
    }
}

/// Stable task bucketing for a bounded canary (at most 10%). This is scheduling,
/// not an authorization check. Task IDs must be assigned by the trusted runtime.
pub fn canary_selected(
    candidate_id: Uuid,
    task_id: Uuid,
    rollout_ppm: u32,
) -> Result<bool, PolicyError> {
    if candidate_id.is_nil() || task_id.is_nil() || rollout_ppm > 100_000 {
        return Err(PolicyError::Invalid("canary selection"));
    }
    let digest = Uuid::new_v5(&candidate_id, task_id.as_bytes());
    let mut prefix = [0u8; 8];
    prefix.copy_from_slice(&digest.as_bytes()[..8]);
    Ok(u64::from_be_bytes(prefix) % 1_000_000 < u64::from(rollout_ppm))
}
impl ImprovementCandidate {
    pub fn selects_task(&self, task_id: Uuid, gate: &ImprovementGate) -> Result<bool, PolicyError> {
        self.validate_gate(gate)?;
        match self.stage {
            ImprovementStage::LimitedRollout => {
                if self.rollout_ppm == 0 || self.rollout_ppm > gate.maximum_rollout_ppm {
                    return Err(PolicyError::Invalid("rollout fraction"));
                }
                canary_selected(self.id, task_id, self.rollout_ppm)
            }
            ImprovementStage::Promoted if self.rollout_ppm == 1_000_000 && !task_id.is_nil() => {
                Ok(true)
            }
            ImprovementStage::Candidate
            | ImprovementStage::Evaluated
            | ImprovementStage::Reverted
                if self.rollout_ppm == 0 && !task_id.is_nil() =>
            {
                Ok(false)
            }
            _ => Err(PolicyError::Invalid("rollout state")),
        }
    }
}
