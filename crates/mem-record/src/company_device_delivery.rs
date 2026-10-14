// SPDX-License-Identifier: AGPL-3.0-or-later
//! Company-to-owner control plane. No laptop bearer, email or caller-supplied
//! endpoint is transported. These objects do not grant ordinary local access.
use crate::{OnboardingArea, OnboardingDeviceReviewProject, OnboardingProjectSnapshotRequest};
use chrono::{DateTime, Utc};
use serde::{Deserialize, Serialize};
use uuid::Uuid;

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq)]
#[serde(deny_unknown_fields)]
pub struct OwnerDeviceApprovalRequest {
    pub operation_id: Uuid,
    pub company_id: Uuid,
    pub device_request_id: Uuid,
    pub device_id: Uuid,
    /// Client-generated credential fingerprint, never the credential itself.
    pub credential_sha256: String,
    pub owner_instance_id: Uuid,
    pub area: OnboardingArea,
    pub project_id: Uuid,
    pub issuer: String,
    pub subject: String,
    pub manifest_revision: i64,
    pub manifest_sha256: String,
    pub delivery_policy_revision: i64,
    pub expires_at: DateTime<Utc>,
    /// Immutable device consent; omitted legacy approvals remain download-only.
    #[serde(default, skip_serializing_if = "is_false")]
    pub curated_contributions_allowed: bool,
}
fn is_false(value: &bool) -> bool {
    !value
}

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq)]
#[serde(deny_unknown_fields)]
pub struct OwnerDeviceApprovalReceipt {
    pub request: OwnerDeviceApprovalRequest,
    pub request_sha256: String,
    pub human_principal_id: Uuid,
    pub committed_at: DateTime<Utc>,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct OwnerDeviceCatalogRequest {
    pub company_id: Uuid,
    pub owner_instance_id: Uuid,
    pub area: OnboardingArea,
    pub issuer: String,
    pub subject: String,
    pub delivery_policy_revision: i64,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct OwnerDeviceCatalogProject {
    pub project: OnboardingDeviceReviewProject,
    pub manifest_sha256: String,
    #[serde(default, skip_serializing_if = "is_false")]
    pub curated_contributions_available: bool,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct OwnerDeviceCatalogResponse {
    pub request_sha256: String,
    pub projects: Vec<OwnerDeviceCatalogProject>,
    pub observed_at: DateTime<Utc>,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct OwnerDeviceSubscriptionRequest {
    pub approval_id: Uuid,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct OwnerDeviceSnapshotRequest {
    pub approval_id: Uuid,
    pub snapshot: OnboardingProjectSnapshotRequest,
}
