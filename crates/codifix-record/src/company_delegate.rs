// SPDX-License-Identifier: AGPL-3.0-or-later
//! Operator confirmation of an existing configuration, not caller-selected authority.
use crate::{AuthRole, OnboardingArea};
use chrono::{DateTime, Utc};
use serde::{Deserialize, Serialize};
use uuid::Uuid;

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct CompanyDelegateProject {
    pub project_id: Uuid,
    pub role_ceiling: AuthRole,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct CompanyDelegateProvisionRequest {
    pub request_id: Uuid,
    pub company_id: Uuid,
    pub owner_instance_id: Uuid,
    pub area: OnboardingArea,
    pub issuer: String,
    pub principal_id: Uuid,
    pub token_id: Uuid,
    pub delegation_policy_revision: i64,
    pub name: String,
    pub projects: Vec<CompanyDelegateProject>,
    pub expires_in_days: u32,
    pub confirmed: bool,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct CompanyDelegateProjectGrant {
    pub project_id: Uuid,
    pub role_ceiling: AuthRole,
    pub permissions: Vec<String>,
}

// Deliberately no Debug: a first-success response contains a one-time secret.
#[derive(Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct CompanyDelegateProvisionResponse {
    pub request_id: Uuid,
    pub company_id: Uuid,
    pub owner_instance_id: Uuid,
    pub area: OnboardingArea,
    pub issuer: String,
    pub principal_id: Uuid,
    pub token_id: Uuid,
    pub delegation_policy_revision: i64,
    pub expires_at: DateTime<Utc>,
    pub projects: Vec<CompanyDelegateProjectGrant>,
    pub replayed: bool,
    pub credential_issued: bool,
    pub token: Option<String>,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct CompanyDelegatePlan {
    pub company_id: Uuid,
    pub owner_instance_id: Uuid,
    pub area: OnboardingArea,
    pub issuer: String,
    pub principal_id: Uuid,
    pub token_id: Uuid,
    pub delegation_policy_revision: i64,
    pub projects: Vec<CompanyDelegateProject>,
    pub status: String,
    pub expires_at: Option<DateTime<Utc>>,
}
