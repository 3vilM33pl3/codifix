// SPDX-License-Identifier: AGPL-3.0-or-later
//! Separate delivery authority: no management or generic read/export grant.
use crate::OnboardingArea;
use chrono::{DateTime, Utc};
use serde::{Deserialize, Serialize};
use uuid::Uuid;

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct CompanyDeliveryDelegateProvisionRequest {
    pub request_id: Uuid,
    pub company_id: Uuid,
    pub owner_instance_id: Uuid,
    pub area: OnboardingArea,
    pub issuer: String,
    pub principal_id: Uuid,
    pub token_id: Uuid,
    pub delivery_policy_revision: i64,
    pub name: String,
    pub projects: Vec<Uuid>,
    pub expires_in_days: u32,
    pub confirmed: bool,
}

// First success has a one-time secret; deliberately no Debug implementation.
#[derive(Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct CompanyDeliveryDelegateProvisionResponse {
    pub request_id: Uuid,
    pub company_id: Uuid,
    pub owner_instance_id: Uuid,
    pub area: OnboardingArea,
    pub issuer: String,
    pub principal_id: Uuid,
    pub token_id: Uuid,
    pub delivery_policy_revision: i64,
    pub expires_at: DateTime<Utc>,
    pub projects: Vec<Uuid>,
    pub permissions: Vec<String>,
    pub replayed: bool,
    pub credential_issued: bool,
    pub token: Option<String>,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct CompanyDeliveryDelegatePlan {
    pub company_id: Uuid,
    pub owner_instance_id: Uuid,
    pub area: OnboardingArea,
    pub issuer: String,
    pub principal_id: Uuid,
    pub token_id: Uuid,
    pub delivery_policy_revision: i64,
    pub projects: Vec<Uuid>,
    pub permissions: Vec<String>,
    pub status: String,
    pub expires_at: Option<DateTime<Utc>>,
}
