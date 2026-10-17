//! Admission runs outside Pi and before every provider send. A runtime must
//! durably commit `reserve` and `begin` before sending, with no automatic HTTP
//! retries: each additional send needs its own newly admitted request ID.

use std::collections::BTreeMap;

use serde::{Deserialize, Serialize};

use crate::{
    Area, BudgetDay, StableId, UnixSeconds, UsdMicros,
    policy::{PolicyError, SupervisorPolicy},
};

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct ProviderRequest {
    /// Unique per physical provider attempt, stable across recovery/duplicate events.
    pub id: StableId,
    pub action_id: StableId,
    pub policy_revision: StableId,
    pub model: StableId,
    /// Includes worst-case input/output, tools, reasoning and provider surcharges.
    /// The adapter must prove this bound from explicit provider limits/pricing.
    pub worst_case_cost: UsdMicros,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case", tag = "status")]
pub enum AttemptState {
    Reserved,
    InFlight,
    Uncertain,
    Settled { actual_cost: UsdMicros },
    ReleasedBeforeSend,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize)]
pub struct Attempt {
    request: ProviderRequest,
    state: AttemptState,
}

impl Attempt {
    pub fn request(&self) -> &ProviderRequest {
        &self.request
    }

    pub fn state(&self) -> AttemptState {
        self.state
    }
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
pub struct BudgetTotals {
    pub charged_usd_micros: u128,
    /// Reserved, in-flight and uncertain attempts all retain their full bound.
    pub reserved_usd_micros: u128,
    pub calls: u32,
}

/// One authority per area/UTC day, backed by atomic persistent storage in the
/// future runtime. A fresh in-memory value is not a safe restart/reset mechanism.
#[derive(Debug, Clone, Serialize)]
pub struct BudgetLedger {
    area: Area,
    day: BudgetDay,
    attempts: BTreeMap<StableId, Attempt>,
}

impl BudgetLedger {
    pub fn new(area: Area, day: BudgetDay) -> Self {
        Self {
            area,
            day,
            attempts: BTreeMap::new(),
        }
    }

    pub fn totals(&self) -> BudgetTotals {
        let mut totals = BudgetTotals {
            charged_usd_micros: 0,
            reserved_usd_micros: 0,
            calls: 0,
        };
        for attempt in self.attempts.values() {
            match attempt.state {
                AttemptState::Settled { actual_cost } => {
                    totals.charged_usd_micros += u128::from(actual_cost.0);
                    totals.calls += 1;
                }
                AttemptState::Reserved | AttemptState::InFlight | AttemptState::Uncertain => {
                    totals.reserved_usd_micros += u128::from(attempt.request.worst_case_cost.0);
                    totals.calls += 1;
                }
                AttemptState::ReleasedBeforeSend => {}
            }
        }
        totals
    }

    pub fn attempt(&self, id: &StableId) -> Option<&Attempt> {
        self.attempts.get(id)
    }

    pub fn reserve(
        &mut self,
        policy: &SupervisorPolicy,
        request: ProviderRequest,
        now: UnixSeconds,
    ) -> Result<Admission, BudgetError> {
        self.validate_request(policy, &request, now)?;
        if let Some(existing) = self.attempts.get(&request.id) {
            if existing.request != request {
                return Err(BudgetError::IdempotencyConflict);
            }
            // An existing attempt never authorizes another physical send.
            return Ok(Admission::Existing(existing.state));
        }
        if request.worst_case_cost.0 == 0 {
            return Err(BudgetError::MissingCostBound);
        }
        let totals = self.totals();
        if totals.calls >= policy.daily_calls() {
            return Err(BudgetError::CallLimit);
        }
        let committed = totals.charged_usd_micros + totals.reserved_usd_micros;
        if committed + u128::from(request.worst_case_cost.0) > u128::from(policy.daily_limit().0) {
            return Err(BudgetError::CostLimit);
        }
        self.attempts.insert(
            request.id.clone(),
            Attempt {
                request,
                state: AttemptState::Reserved,
            },
        );
        Ok(Admission::Reserved)
    }

    /// Rechecks revocation, model, day and policy revision immediately before
    /// dispatch. The runtime must also atomically verify the current action lease.
    pub fn begin(
        &mut self,
        id: &StableId,
        policy: &SupervisorPolicy,
        now: UnixSeconds,
    ) -> Result<(), BudgetError> {
        let attempt = self.attempts.get(id).ok_or(BudgetError::UnknownAttempt)?;
        self.validate_request(policy, &attempt.request, now)?;
        let totals = self.totals();
        if totals.calls > policy.daily_calls()
            || totals.charged_usd_micros + totals.reserved_usd_micros
                > u128::from(policy.daily_limit().0)
        {
            return Err(BudgetError::ReducedLimit);
        }
        let attempt = self
            .attempts
            .get_mut(id)
            .ok_or(BudgetError::UnknownAttempt)?;
        if attempt.state != AttemptState::Reserved {
            return Err(BudgetError::InvalidTransition);
        }
        attempt.state = AttemptState::InFlight;
        Ok(())
    }

    pub fn mark_uncertain(&mut self, id: &StableId) -> Result<(), BudgetError> {
        let attempt = self
            .attempts
            .get_mut(id)
            .ok_or(BudgetError::UnknownAttempt)?;
        match attempt.state {
            AttemptState::InFlight | AttemptState::Uncertain => {
                attempt.state = AttemptState::Uncertain;
                Ok(())
            }
            _ => Err(BudgetError::InvalidTransition),
        }
    }

    /// Only positive evidence that dispatch never started can free a reservation.
    /// In-flight/uncertain work must be settled using authoritative usage evidence.
    pub fn release_before_send(&mut self, id: &StableId) -> Result<(), BudgetError> {
        let attempt = self
            .attempts
            .get_mut(id)
            .ok_or(BudgetError::UnknownAttempt)?;
        match attempt.state {
            AttemptState::Reserved | AttemptState::ReleasedBeforeSend => {
                attempt.state = AttemptState::ReleasedBeforeSend;
                Ok(())
            }
            _ => Err(BudgetError::InvalidTransition),
        }
    }

    /// Accounting is accepted after revocation/day rollover. If a provider reports
    /// more than its admitted bound, retain the real charge and surface the breach;
    /// rejecting the record would incorrectly make the money available again.
    pub fn settle(
        &mut self,
        id: &StableId,
        actual_cost: UsdMicros,
    ) -> Result<Settlement, BudgetError> {
        let attempt = self
            .attempts
            .get_mut(id)
            .ok_or(BudgetError::UnknownAttempt)?;
        match attempt.state {
            AttemptState::InFlight | AttemptState::Uncertain => {}
            AttemptState::Settled {
                actual_cost: existing,
            } if existing == actual_cost => {
                return Ok(Settlement::AlreadySettled);
            }
            AttemptState::Settled { .. } => return Err(BudgetError::IdempotencyConflict),
            _ => return Err(BudgetError::InvalidTransition),
        }
        attempt.state = AttemptState::Settled { actual_cost };
        Ok(if actual_cost > attempt.request.worst_case_cost {
            Settlement::ExceededReservation
        } else {
            Settlement::Settled
        })
    }

    fn validate_request(
        &self,
        policy: &SupervisorPolicy,
        request: &ProviderRequest,
        now: UnixSeconds,
    ) -> Result<(), BudgetError> {
        policy.validate(self.area, &request.policy_revision)?;
        policy.validate_model(request.model.as_str())?;
        if now.budget_day() != self.day {
            return Err(BudgetError::WrongDay);
        }
        Ok(())
    }
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(
    rename_all = "snake_case",
    tag = "admission",
    content = "attempt_state"
)]
pub enum Admission {
    Reserved,
    Existing(AttemptState),
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum Settlement {
    Settled,
    AlreadySettled,
    ExceededReservation,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, thiserror::Error)]
pub enum BudgetError {
    #[error(transparent)]
    Policy(#[from] PolicyError),
    #[error("accounting day does not match current UTC day")]
    WrongDay,
    #[error("request ID was already used with different terms")]
    IdempotencyConflict,
    #[error("provider request has no positive worst-case cost bound")]
    MissingCostBound,
    #[error("daily model-call ceiling cannot fit this request")]
    CallLimit,
    #[error("daily cost ceiling cannot fit this reservation")]
    CostLimit,
    #[error("current policy limits no longer fit admitted work")]
    ReducedLimit,
    #[error("unknown provider attempt")]
    UnknownAttempt,
    #[error("provider attempt cannot make this transition")]
    InvalidTransition,
}
