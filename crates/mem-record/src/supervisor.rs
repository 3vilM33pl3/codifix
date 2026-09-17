// SPDX-License-Identifier: AGPL-3.0-or-later
//! Metadata-only inspection, never execution authority or a knowledge payload.
use chrono::{DateTime, NaiveDate, Utc};
pub use mem_supervisor::{Area as SupervisorArea, action::ActionState as SupervisorActionState};
use serde::{Deserialize, Serialize};
use uuid::Uuid;

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct SupervisorResponse {
    pub read_only: bool,
    /// A persisted enabled policy is not evidence of an active worker.
    pub execution: SupervisorExecution,
    /// Absent when no durable supervisor identity is configured.
    pub snapshot: Option<SupervisorSnapshot>,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum SupervisorExecution {
    NotIntegrated,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct SupervisorSnapshot {
    pub instance_id: Uuid,
    pub area: Option<SupervisorArea>,
    pub observed_at: DateTime<Utc>,
    pub policy: Option<SupervisorPolicySummary>,
    pub budget: Option<SupervisorBudgetSummary>,
    pub action_counts: Vec<SupervisorActionCount>,
    pub journal: Vec<SupervisorJournalEntry>,
    /// Opaque decimal string prevents browser integer precision loss.
    pub next_after: String,
    pub has_more: bool,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct SupervisorPolicySummary {
    pub revision: Uuid,
    pub enabled: bool,
    pub revoked: bool,
    pub model: String,
    pub daily_usd_micros: String,
    pub daily_calls: u32,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct SupervisorBudgetSummary {
    pub day: NaiveDate,
    pub charged_usd_micros: String,
    /// Includes reserved, in-flight and uncertain charges; not free capacity.
    pub reserved_usd_micros: String,
    pub calls: u32,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct SupervisorActionCount {
    pub state: SupervisorActionState,
    pub count: String,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct SupervisorJournalEntry {
    pub sequence: String,
    pub action_id: Option<Uuid>,
    pub attempt_id: Option<Uuid>,
    pub policy_revision: Uuid,
    pub event: SupervisorJournalEvent,
    pub recorded_at: DateTime<Utc>,
}

/// Closed vocabulary: stored prose must never leak through an operational feed.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum SupervisorJournalEvent {
    PolicyInstalled,
    PolicyEnabled,
    PolicyPaused,
    PolicyRevoked,
    PolicyReduced,
    Observed,
    Proposed,
    Authorized,
    Queued,
    Claimed,
    Renewed,
    Verified,
    Failed,
    Denied,
    Expired,
    Cancelled,
    Uncertain,
    Reserved,
    Begun,
    ReleasedBeforeSend,
    Settled,
    SettledOverBound,
    ModelContextBound,
}
