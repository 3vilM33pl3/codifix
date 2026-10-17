use serde::{Deserialize, Serialize};

use crate::{
    Area, DAILY_AREA_MODEL_CALLS, DAILY_AREA_USD_MICROS, DEFAULT_MODEL, StableId, UsdMicros,
};

/// Locally approved policy. Kept private so callers cannot increase hard caps or
/// silently change a model after approval. New approval means a new revision.
#[derive(Debug, Clone, PartialEq, Eq, Serialize)]
pub struct SupervisorPolicy {
    area: Area,
    revision: StableId,
    enabled: bool,
    revoked: bool,
    model: StableId,
    daily_limit: UsdMicros,
    daily_calls: u32,
}

impl SupervisorPolicy {
    /// A newly constructed policy is disabled until the operator enables it.
    pub fn new(area: Area, revision: StableId) -> Self {
        Self {
            area,
            revision,
            enabled: false,
            revoked: false,
            model: StableId::new(DEFAULT_MODEL).expect("constant model ID is valid"),
            daily_limit: UsdMicros(DAILY_AREA_USD_MICROS),
            daily_calls: DAILY_AREA_MODEL_CALLS,
        }
    }

    pub fn set_enabled(&mut self, enabled: bool) {
        self.enabled = enabled;
    }

    /// Revocation is permanent for this revision; pause/resume cannot undo it.
    pub fn revoke(&mut self) {
        self.revoked = true;
        self.enabled = false;
    }

    /// Limits may be reduced, never raised beyond the independently allocated
    /// $2.50/20-call area ceiling. There is no cross-area budget borrowing.
    pub fn with_limits(mut self, amount: UsdMicros, calls: u32) -> Result<Self, PolicyError> {
        if amount.0 > DAILY_AREA_USD_MICROS || calls > DAILY_AREA_MODEL_CALLS {
            return Err(PolicyError::AboveHardCeiling);
        }
        self.daily_limit = amount;
        self.daily_calls = calls;
        Ok(self)
    }

    pub fn area(&self) -> Area {
        self.area
    }

    pub fn revision(&self) -> &StableId {
        &self.revision
    }

    pub fn model(&self) -> &str {
        self.model.as_str()
    }

    pub fn daily_limit(&self) -> UsdMicros {
        self.daily_limit
    }

    pub fn daily_calls(&self) -> u32 {
        self.daily_calls
    }

    pub fn validate(&self, area: Area, revision: &StableId) -> Result<(), PolicyError> {
        if self.revoked {
            return Err(PolicyError::Revoked);
        }
        if !self.enabled {
            return Err(PolicyError::Disabled);
        }
        if self.area != area {
            return Err(PolicyError::WrongArea);
        }
        if &self.revision != revision {
            return Err(PolicyError::StaleRevision);
        }
        Ok(())
    }

    pub fn validate_model(&self, model: &str) -> Result<(), PolicyError> {
        if model != self.model() {
            return Err(PolicyError::ModelNotApproved);
        }
        Ok(())
    }
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize, thiserror::Error)]
#[serde(rename_all = "snake_case")]
pub enum PolicyError {
    #[error("supervisor policy is disabled")]
    Disabled,
    #[error("supervisor policy revision is permanently revoked")]
    Revoked,
    #[error("policy belongs to a different area")]
    WrongArea,
    #[error("action does not bind the current policy revision")]
    StaleRevision,
    #[error("requested model is not the approved model")]
    ModelNotApproved,
    #[error("policy exceeds the hard per-area ceiling")]
    AboveHardCeiling,
}

/// Exact local standing authorization. Enrollment or remote prose cannot create
/// this value in a runtime: the runtime must load it from its trusted policy store.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct QuestionScope {
    pub requester: StableId,
    pub responder: StableId,
    pub collection: StableId,
    pub purpose: StableId,
    pub onward_destination: StableId,
    pub model: StableId,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize)]
pub struct StandingQuestionPolicy {
    revision: StableId,
    scope: QuestionScope,
    enabled: bool,
    revoked: bool,
}

impl StandingQuestionPolicy {
    pub fn new(revision: StableId, scope: QuestionScope) -> Self {
        Self {
            revision,
            scope,
            enabled: false,
            revoked: false,
        }
    }

    pub fn set_enabled(&mut self, enabled: bool) {
        self.enabled = enabled;
    }

    pub fn revoke(&mut self) {
        self.revoked = true;
        self.enabled = false;
    }

    /// Result preserves the exact question and revision for the action journal.
    /// This does not impersonate the separate F1 exact-question human consent.
    pub fn authorize(
        &self,
        scope: &QuestionScope,
        exact_question: String,
        revision: &StableId,
    ) -> Result<AuthorizedQuestion, QuestionDenial> {
        if self.revoked {
            return Err(QuestionDenial::Revoked);
        }
        if !self.enabled {
            return Err(QuestionDenial::Disabled);
        }
        if revision != &self.revision {
            return Err(QuestionDenial::StaleRevision);
        }
        if scope != &self.scope {
            return Err(QuestionDenial::ScopeMismatch);
        }
        if exact_question.trim().is_empty() || exact_question.len() > 16_384 {
            return Err(QuestionDenial::InvalidQuestion);
        }
        Ok(AuthorizedQuestion {
            policy_revision: self.revision.clone(),
            scope: scope.clone(),
            exact_question,
        })
    }
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize)]
pub struct AuthorizedQuestion {
    policy_revision: StableId,
    scope: QuestionScope,
    exact_question: String,
}

impl AuthorizedQuestion {
    pub fn exact_question(&self) -> &str {
        &self.exact_question
    }

    pub fn policy_revision(&self) -> &StableId {
        &self.policy_revision
    }
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize, thiserror::Error)]
#[serde(rename_all = "snake_case")]
pub enum QuestionDenial {
    #[error("standing question policy is disabled")]
    Disabled,
    #[error("standing question policy revision is permanently revoked")]
    Revoked,
    #[error("standing question revision is stale")]
    StaleRevision,
    #[error("question scope does not exactly match the approved scope")]
    ScopeMismatch,
    #[error("question must be nonempty and bounded")]
    InvalidQuestion,
}
