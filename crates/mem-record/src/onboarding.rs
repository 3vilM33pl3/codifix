// SPDX-License-Identifier: AGPL-3.0-or-later
//! Administrative onboarding metadata; never a credential or delivery grant.
use chrono::{DateTime, Utc};
use serde::{Deserialize, Serialize};
use uuid::Uuid;
use crate::{AuthPrincipalResponse, AuthRole};

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct OnboardingCapabilities {
    pub schema_version: u32,
    pub idp_management_enabled: bool,
    pub invitation_enabled: bool,
    pub idp_provider: Option<String>,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct OnboardingUser {
    pub principal: AuthPrincipalResponse,
    pub disabled_at: Option<DateTime<Utc>>,
    pub created_at: DateTime<Utc>,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct OnboardingUserRequest {
    pub issuer: String,
    pub subject: String,
    pub display_name: String,
    pub email: Option<String>,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct OnboardingAccessGrant {
    pub source: String,
    pub role: AuthRole,
    pub group: Option<String>,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct OnboardingProjectAccess {
    pub project: String,
    pub permissions: Vec<String>,
    pub grants: Vec<OnboardingAccessGrant>,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct OnboardingGroupRule {
    pub group: String,
    pub role: AuthRole,
    pub global: bool,
    pub project: Option<String>,
    pub matched: bool,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct OnboardingEffectiveAccess {
    pub principal_id: Uuid,
    pub disabled: bool,
    pub global_permissions: Vec<String>,
    pub projects: Vec<OnboardingProjectAccess>,
    pub group_rules: Vec<OnboardingGroupRule>,
}

#[derive(Debug, Clone, Copy, Serialize, Deserialize, PartialEq, Eq)]
#[serde(rename_all = "snake_case")]
pub enum OnboardingArea { General, Work, Personal, Trust }

impl OnboardingArea {
    pub fn as_str(self) -> &'static str {
        match self { Self::General => "general", Self::Work => "work", Self::Personal => "personal", Self::Trust => "trust" }
    }
}

#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct OnboardingPinnedTool {
    pub name: String,
    pub version: String,
    pub url: String,
    pub sha256: String,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct OnboardingDestination {
    pub name: String,
    pub url: String,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct OnboardingProjectEntry {
    pub area: OnboardingArea,
    pub owner_instance_id: Uuid,
    pub repository_url: String,
    pub default_branch: String,
    pub offline_allowed: bool,
    pub tools: Vec<OnboardingPinnedTool>,
    pub destinations: Vec<OnboardingDestination>,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct OnboardingProjectReviewRequest {
    pub expected_revision: i64,
    pub entry: OnboardingProjectEntry,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct OnboardingProject {
    pub project_id: Uuid,
    pub project: String,
    #[serde(flatten)]
    pub entry: OnboardingProjectEntry,
    pub revision: i64,
    pub reviewed_by: Uuid,
    pub reviewed_at: DateTime<Utc>,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct OnboardingDevice {
    pub id: Uuid,
    pub principal_id: Uuid,
    pub name: String,
    pub platform: String,
    pub created_at: DateTime<Utc>,
    pub last_seen_at: Option<DateTime<Utc>>,
    pub revoked_at: Option<DateTime<Utc>>,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct OnboardingIdpUserRequest {
    pub username: String,
    pub display_name: String,
    pub email: String,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct OnboardingIdpOperation {
    pub id: Uuid,
    pub operation: String,
    pub outcome: String,
    pub provider_user_id: Option<i64>,
    /// Metadata only: use the provider's invitation administration for delivery.
    pub invitation_id: Option<Uuid>,
}
