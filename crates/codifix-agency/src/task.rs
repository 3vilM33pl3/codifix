use crate::{PolicyError, valid_identifier};
use chrono::{DateTime, Utc};
use serde::{Deserialize, Serialize};
use uuid::Uuid;

#[derive(Clone, Copy, Debug, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum TaskStatus {
    Submitted,
    Working,
    InputRequired,
    PlanReviewRequired,
    Completed,
    Failed,
    Canceled,
    Rejected,
}
impl TaskStatus {
    pub fn is_terminal(self) -> bool {
        matches!(
            self,
            Self::Completed | Self::Failed | Self::Canceled | Self::Rejected
        )
    }
}

#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct Clarification {
    pub question: String,
    pub answer: Option<String>,
}
#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct PlanRevision {
    pub revision: u32,
    pub reason: String,
    pub steps: Vec<String>,
    pub accepted: bool,
}
#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct TaskOutcome {
    pub summary: String,
    /// Private references, never included in public discovery projections.
    pub artifact_refs: Vec<String>,
    pub evidence_refs: Vec<String>,
    pub acceptance: Vec<AcceptanceResult>,
}
#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct AcceptanceResult {
    pub criterion: String,
    pub passed: bool,
    pub evidence_ref: String,
}
#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct TaskContract {
    pub id: Uuid,
    pub mandate_id: Uuid,
    pub specialist_id: String,
    pub objective: String,
    pub acceptance_criteria: Vec<String>,
    pub revision: u32,
    pub status: TaskStatus,
    pub clarification: Option<Clarification>,
    pub plan_revision: Option<PlanRevision>,
    pub outcome: Option<TaskOutcome>,
    pub updated_at: DateTime<Utc>,
}
impl TaskContract {
    pub fn validate(&self) -> Result<(), PolicyError> {
        if self.id.is_nil()
            || self.mandate_id.is_nil()
            || !valid_identifier(&self.specialist_id)
            || self.objective.trim().is_empty()
            || self.acceptance_criteria.is_empty()
            || self.acceptance_criteria.iter().any(|s| s.trim().is_empty())
            || self
                .acceptance_criteria
                .iter()
                .collect::<std::collections::BTreeSet<_>>()
                .len()
                != self.acceptance_criteria.len()
        {
            return Err(PolicyError::Invalid("task contract"));
        }
        if self.status == TaskStatus::InputRequired
            && !self
                .clarification
                .as_ref()
                .is_some_and(|c| !c.question.trim().is_empty() && c.answer.is_none())
        {
            return Err(PolicyError::Invalid("clarification state"));
        }
        if self.status == TaskStatus::PlanReviewRequired
            && !self.plan_revision.as_ref().is_some_and(|p| {
                !p.accepted
                    && !p.reason.trim().is_empty()
                    && !p.steps.is_empty()
                    && p.steps.iter().all(|s| !s.trim().is_empty())
                    && p.revision == self.revision
            })
        {
            return Err(PolicyError::Invalid("plan revision state"));
        }
        if self.status != TaskStatus::Completed && self.outcome.is_some() {
            return Err(PolicyError::Invalid("premature outcome"));
        }
        if self.status == TaskStatus::Completed {
            self.validate_outcome(
                self.outcome
                    .as_ref()
                    .ok_or(PolicyError::Invalid("completed outcome"))?,
            )?;
        }
        Ok(())
    }

    /// General transitions cannot bypass clarification, plan review, or completion checks.
    pub fn transition(&mut self, next: TaskStatus, now: DateTime<Utc>) -> Result<(), PolicyError> {
        self.validate()?;
        let allowed = matches!(
            (self.status, next),
            (
                TaskStatus::Submitted,
                TaskStatus::Working | TaskStatus::Rejected | TaskStatus::Canceled
            ) | (
                TaskStatus::Working,
                TaskStatus::Failed | TaskStatus::Canceled | TaskStatus::Rejected
            ) | (
                TaskStatus::InputRequired | TaskStatus::PlanReviewRequired,
                TaskStatus::Canceled | TaskStatus::Failed
            )
        );
        if !allowed || now < self.updated_at {
            return Err(PolicyError::Transition);
        }
        self.status = next;
        self.updated_at = now;
        Ok(())
    }
    pub fn clarify(&mut self, question: String, now: DateTime<Utc>) -> Result<(), PolicyError> {
        self.validate()?;
        if self.status != TaskStatus::Working || question.trim().is_empty() || now < self.updated_at
        {
            return Err(PolicyError::Transition);
        }
        self.clarification = Some(Clarification {
            question,
            answer: None,
        });
        self.status = TaskStatus::InputRequired;
        self.updated_at = now;
        Ok(())
    }
    pub fn answer_clarification(
        &mut self,
        answer: String,
        now: DateTime<Utc>,
    ) -> Result<(), PolicyError> {
        self.validate()?;
        if self.status != TaskStatus::InputRequired
            || answer.trim().is_empty()
            || now < self.updated_at
        {
            return Err(PolicyError::Transition);
        }
        self.clarification
            .as_mut()
            .ok_or(PolicyError::Invalid("clarification"))?
            .answer = Some(answer);
        self.status = TaskStatus::Working;
        self.updated_at = now;
        Ok(())
    }
    /// A revision changes the work plan only. Authority and acceptance criteria are immutable here.
    pub fn revise_plan(
        &mut self,
        reason: String,
        steps: Vec<String>,
        now: DateTime<Utc>,
    ) -> Result<(), PolicyError> {
        self.validate()?;
        if self.status != TaskStatus::Working
            || reason.trim().is_empty()
            || steps.is_empty()
            || steps.iter().any(|s| s.trim().is_empty())
            || now < self.updated_at
        {
            return Err(PolicyError::Transition);
        }
        let revision = self
            .revision
            .checked_add(1)
            .ok_or(PolicyError::Invalid("revision overflow"))?;
        self.revision = revision;
        self.plan_revision = Some(PlanRevision {
            revision,
            reason,
            steps,
            accepted: false,
        });
        self.status = TaskStatus::PlanReviewRequired;
        self.updated_at = now;
        Ok(())
    }
    /// Runtime must authenticate the accepting local owner before invoking this method.
    pub fn accept_plan(&mut self, now: DateTime<Utc>) -> Result<(), PolicyError> {
        self.validate()?;
        if self.status != TaskStatus::PlanReviewRequired || now < self.updated_at {
            return Err(PolicyError::Transition);
        }
        self.plan_revision
            .as_mut()
            .ok_or(PolicyError::Invalid("plan revision"))?
            .accepted = true;
        self.status = TaskStatus::Working;
        self.updated_at = now;
        Ok(())
    }
    pub fn finish(&mut self, outcome: TaskOutcome, now: DateTime<Utc>) -> Result<(), PolicyError> {
        self.validate()?;
        if self.status != TaskStatus::Working || now < self.updated_at {
            return Err(PolicyError::Transition);
        }
        self.validate_outcome(&outcome)?;
        self.outcome = Some(outcome);
        self.status = TaskStatus::Completed;
        self.updated_at = now;
        Ok(())
    }
    fn validate_outcome(&self, outcome: &TaskOutcome) -> Result<(), PolicyError> {
        if outcome
            .artifact_refs
            .iter()
            .chain(&outcome.evidence_refs)
            .any(|r| r.trim().is_empty())
            || outcome.acceptance.iter().any(|a| {
                !outcome.evidence_refs.contains(&a.evidence_ref)
                    && !outcome.artifact_refs.contains(&a.evidence_ref)
            })
            || outcome.summary.trim().is_empty()
            || outcome.acceptance.len() != self.acceptance_criteria.len()
            || self.acceptance_criteria.iter().any(|criterion| {
                outcome
                    .acceptance
                    .iter()
                    .filter(|a| {
                        &a.criterion == criterion && a.passed && !a.evidence_ref.trim().is_empty()
                    })
                    .count()
                    != 1
            })
        {
            return Err(PolicyError::Invalid("acceptance evidence"));
        }
        Ok(())
    }
}
