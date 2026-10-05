// SPDX-License-Identifier: AGPL-3.0-or-later
//! Explicit owner delegation, distinct from catalog and federation authority.
use crate::{AuthRole, OnboardingArea};
use chrono::{DateTime, Utc};
use serde::{Deserialize, Serialize};
use uuid::Uuid;

#[derive(Debug, Clone, Copy, Serialize, Deserialize, PartialEq, Eq)]
#[serde(rename_all = "snake_case")]
pub enum CompanyGrantOperation {
    Grant,
    Revoke,
}

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq)]
#[serde(deny_unknown_fields)]
pub struct CompanyGrantRequest {
    pub company_id: Uuid,
    pub operation_id: Uuid,
    pub owner_instance_id: Uuid,
    pub area: OnboardingArea,
    pub project_id: Uuid,
    pub issuer: String,
    pub subject: String,
    pub operation: CompanyGrantOperation,
    pub role: AuthRole,
    pub expected_grant_revision: i64,
    pub delegation_policy_revision: i64,
}

/// Historical transaction outcome, never a claim of current effective access.
#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq)]
#[serde(deny_unknown_fields)]
pub struct CompanyGrantReceipt {
    pub request: CompanyGrantRequest,
    pub request_sha256: String,
    pub principal_id: Uuid,
    pub grant_revision: i64,
    pub direct_grant_active: bool,
    pub committed_at: DateTime<Utc>,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct CompanyCurrentAccess {
    pub observed_at: DateTime<Utc>,
    pub principal_id: Uuid,
    pub disabled: bool,
    pub project: String,
    pub direct_role: Option<AuthRole>,
    pub grant_company_id: Option<Uuid>,
    pub owner_grant_revision: Option<i64>,
    pub group_roles: Vec<AuthRole>,
    pub permissions: Vec<String>,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct CompanyGrantResult {
    pub receipt: CompanyGrantReceipt,
    pub current_access: CompanyCurrentAccess,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct CompanyOperationStatus {
    pub request: CompanyGrantRequest,
    pub request_sha256: String,
    pub outcome: String,
    pub receipt: Option<CompanyGrantReceipt>,
    /// Present only when a fresh owner status was verified, not from the journal.
    pub current_access: Option<CompanyCurrentAccess>,
}
