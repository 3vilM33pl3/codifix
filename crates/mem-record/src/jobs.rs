// SPDX-License-Identifier: AGPL-3.0-or-later

use chrono::{DateTime, Utc};
use serde::{Deserialize, Serialize};
use uuid::Uuid;

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct BackgroundWorkReference {
    pub operation_id: Uuid,
    pub job_ids: Vec<Uuid>,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum JobStatus {
    Queued,
    Running,
    RetryWait,
    Blocked,
    Failed,
    Succeeded,
    Skipped,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum JobKind {
    Embedding,
    SemanticDedup,
    ValidationDue,
    Consolidation,
    ConsolidationSynthesis,
}

/// Safe, enumerated reasons: raw provider/SQL errors never become API output.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum JobFailureCode {
    Transient,
    RateLimited,
    Quota,
    Credentials,
    Authorization,
    Configuration,
    AttemptsExhausted,
    LeaseLost,
    StaleInput,
    InvalidInput,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct JobSummary {
    pub id: Uuid,
    pub project_id: Uuid,
    pub operation_id: Uuid,
    pub kind: JobKind,
    pub status: JobStatus,
    pub generation: i32,
    pub attempts: i32,
    pub created_at: DateTime<Utc>,
    pub available_at: DateTime<Utc>,
    pub lease_until: Option<DateTime<Utc>>,
    pub last_failure: Option<JobFailureCode>,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct JobAttempt {
    pub generation: i32,
    pub attempt: i32,
    pub started_at: DateTime<Utc>,
    pub finished_at: Option<DateTime<Utc>>,
    pub outcome: Option<String>,
    pub failure: Option<JobFailureCode>,
    /// Lease loss cannot establish whether a provider accepted the request.
    pub uncertain: bool,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct JobDetail {
    pub job: JobSummary,
    pub dependencies: Vec<Uuid>,
    pub history: Vec<JobAttempt>,
    pub history_truncated: bool,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct JobList {
    pub jobs: Vec<JobSummary>,
    pub next_after: Option<Uuid>,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct JobRetryRequest {
    pub request_id: Uuid,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct JobRetryResponse {
    pub request_id: Uuid,
    pub job_id: Uuid,
    pub generation: i32,
}
