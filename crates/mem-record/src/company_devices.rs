// SPDX-License-Identifier: AGPL-3.0-or-later
//! Explicit composite company selections; never owner endpoints or human aliases.
use crate::{OnboardingArea, OwnerDeviceCatalogProject};
use chrono::{DateTime, Utc};
use serde::{Deserialize, Serialize};
use uuid::Uuid;

#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct CompanyDeviceReview {
    pub request_id: Uuid,
    pub device_id: Uuid,
    pub user_code: String,
    pub name: String,
    pub platform: String,
    pub credential_sha256: String,
    pub expires_at: DateTime<Utc>,
    pub company_id: Uuid,
    pub observed_at: DateTime<Utc>,
    pub projects: Vec<OwnerDeviceCatalogProject>,
}
#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq)]
#[serde(deny_unknown_fields)]
pub struct CompanyDeviceSelection {
    pub area: OnboardingArea,
    pub owner_instance_id: Uuid,
    pub project_id: Uuid,
    pub manifest_revision: i64,
    pub manifest_sha256: String,
    pub offline_confirmed: bool,
}
#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq)]
#[serde(deny_unknown_fields)]
pub struct CompanyDeviceApproveRequest {
    pub user_code: String,
    pub confirmed: bool,
    pub projects: Vec<CompanyDeviceSelection>,
}
