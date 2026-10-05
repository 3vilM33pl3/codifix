// SPDX-License-Identifier: AGPL-3.0-or-later
//! Device enrollment never carries a raw service credential in a response.
use crate::{AuthRole, OnboardingProject};
use chrono::{DateTime, Utc};
use serde::{Deserialize, Serialize};
use uuid::Uuid;

#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct OnboardingDeviceStartRequest {
    pub request_id: Uuid,
    pub device_id: Uuid,
    pub name: String,
    pub platform: String,
    pub credential_sha256: String,
    pub poll_sha256: String,
}
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct OnboardingDeviceStartResponse {
    pub request_id: Uuid,
    pub user_code: String,
    pub expires_at: DateTime<Utc>,
    pub poll_interval_seconds: u32,
    pub verification_uri_complete: String,
}
#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct OnboardingDevicePollRequest {
    pub request_id: Uuid,
    pub poll_secret: String,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct OnboardingDeviceGrant {
    pub manifest: OnboardingProject,
    pub role: AuthRole,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct OnboardingDevicePollResponse {
    pub status: String,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub device_id: Option<Uuid>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub principal_id: Option<Uuid>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub projects: Option<Vec<OnboardingDeviceGrant>>,
}
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct OnboardingDeviceReviewProject {
    pub manifest: OnboardingProject,
    pub allowed_roles: Vec<AuthRole>,
    pub offline_available: bool,
}
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct OnboardingDeviceReview {
    pub request_id: Uuid,
    pub device_id: Uuid,
    pub user_code: String,
    pub name: String,
    pub platform: String,
    pub credential_sha256: String,
    pub expires_at: DateTime<Utc>,
    pub projects: Vec<OnboardingDeviceReviewProject>,
}
#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq)]
#[serde(deny_unknown_fields)]
pub struct OnboardingDeviceSelection {
    pub project_id: Uuid,
    pub role: AuthRole,
    pub revision: i64,
}
#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct OnboardingDeviceApproveRequest {
    pub user_code: String,
    pub projects: Vec<OnboardingDeviceSelection>,
    pub confirmed: bool,
}
