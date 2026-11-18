// SPDX-License-Identifier: AGPL-3.0-or-later
//! Read-only bounded companion observations. No raw agency payloads or credentials.
use chrono::{DateTime, Utc};
use serde::{Deserialize, Serialize};
use uuid::Uuid;

#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize)]
pub struct CompanionStatus {
    pub enabled: bool,
    pub area: Option<String>,
    pub instance_id: Option<Uuid>,
    /// disabled, unknown, fresh or stale. A heartbeat is process presence only.
    pub state: String,
    pub last_heartbeat_at: Option<DateTime<Utc>>,
    pub version: Option<String>,
    pub exchange_access: bool,
}
#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize)]
pub struct ExchangeSummary {
    pub task_id: Uuid,
    pub peer: String,
    /// incoming or outgoing
    pub direction: String,
    pub specialist_id: String,
    pub objective: String,
    pub status: String,
    pub created_at: DateTime<Utc>,
    pub updated_at: DateTime<Utc>,
    /// False means only the latest legacy snapshot survives.
    pub journal_available: bool,
}
#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize)]
pub struct ExchangePage {
    pub exchanges: Vec<ExchangeSummary>,
    /// Opaque bounded keyset cursor; pass verbatim as before.
    pub next_before: Option<String>,
}
#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize)]
pub struct ExchangeEvent {
    pub sequence: i64,
    /// request, clarification, answer, result, status or snapshot
    pub kind: String,
    pub body: String,
    pub recorded_at: DateTime<Utc>,
}
#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize)]
pub struct ExchangeDetail {
    pub exchange: ExchangeSummary,
    pub events: Vec<ExchangeEvent>,
    pub events_truncated: bool,
    /// Peer-reported results are never independent verification.
    pub independently_verified: bool,
}
