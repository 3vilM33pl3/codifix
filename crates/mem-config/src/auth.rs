// SPDX-License-Identifier: AGPL-3.0-or-later

#[allow(unused_imports)]
use std::{
    collections::HashMap,
    env, fmt,
    path::{Path, PathBuf},
    time::Duration,
};

#[allow(unused_imports)]
use chrono::{DateTime, Utc};
#[allow(unused_imports)]
use config::{Config, ConfigError, Environment, File, FileFormat};
#[allow(unused_imports)]
use mem_platform::discover_existing_global_config_path;
#[allow(unused_imports)]
use mem_record::*;
#[allow(unused_imports)]
use serde::{Deserialize, Serialize};
#[allow(unused_imports)]
use thiserror::Error;
#[allow(unused_imports)]
use uuid::Uuid;

#[allow(unused_imports)]
use super::*;

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct OidcAuthConfig {
    #[serde(default)]
    pub issuer_url: String,
    #[serde(default)]
    pub client_id: String,
    #[serde(default = "default_oidc_client_secret_env")]
    pub client_secret_env: String,
    #[serde(default = "default_oidc_scopes")]
    pub scopes: Vec<String>,
    #[serde(default = "default_oidc_groups_claim")]
    pub groups_claim: String,
}

impl Default for OidcAuthConfig {
    fn default() -> Self {
        Self {
            issuer_url: String::new(),
            client_id: String::new(),
            client_secret_env: default_oidc_client_secret_env(),
            scopes: default_oidc_scopes(),
            groups_claim: default_oidc_groups_claim(),
        }
    }
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct AuthGroupMappingRule {
    pub group: String,
    pub role: AuthRole,
    #[serde(default)]
    pub global: bool,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub project: Option<String>,
}

#[derive(Debug, Clone, Default, Serialize, Deserialize)]
pub struct AuthGroupMappingsConfig {
    #[serde(default)]
    pub rules: Vec<AuthGroupMappingRule>,
}

/// Opt-in server-only Authentik API access. The token must have only the
/// configured user/invitation permissions; it is never returned by the API.
#[derive(Debug, Clone, Default, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct AuthentikManagementConfig {
    #[serde(default)]
    pub enabled: bool,
    #[serde(default)]
    pub base_url: String,
    #[serde(default)]
    pub token_env: String,
    #[serde(default)]
    pub invitation_flow: Option<Uuid>,
    /// Newly created users go only under this provider-owned restricted path.
    #[serde(default)]
    pub user_path: String,
}

/// Owner-enrolled authority. A company credential is not a global administrator.
#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct CompanyProjectDelegation {
    pub project_id: Uuid,
    pub role_ceiling: AuthRole,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct CompanyDelegationConfig {
    pub company_id: Uuid,
    /// Exact shared OIDC namespace, also equal to this owner's configured issuer.
    pub issuer: String,
    pub principal_id: Uuid,
    pub token_id: Uuid,
    pub policy_revision: i64,
    pub projects: Vec<CompanyProjectDelegation>,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct CompanyOwnerConfig {
    pub owner_instance_id: Uuid,
    pub area: OnboardingArea,
    pub base_url: String,
    /// Server-only environment reference; never a browser/device credential.
    pub token_env: String,
    pub issuer: String,
    pub delegation_policy_revision: i64,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct CompanyOnboardingConfig {
    pub company_id: Uuid,
    pub owners: Vec<CompanyOwnerConfig>,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct AuthConfig {
    #[serde(default)]
    pub mode: AuthMode,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub public_base_url: Option<String>,
    #[serde(default = "default_auth_session_ttl")]
    #[serde(with = "humantime_serde")]
    pub session_ttl: Duration,
    #[serde(default)]
    pub multi_user_legacy_token_enabled: bool,
    #[serde(default)]
    pub oidc: OidcAuthConfig,
    #[serde(default)]
    pub group_mappings: AuthGroupMappingsConfig,
    #[serde(default)]
    pub authentik_management: AuthentikManagementConfig,
    #[serde(default)]
    pub company_delegations: Vec<CompanyDelegationConfig>,
    #[serde(default)]
    pub company_onboarding: Option<CompanyOnboardingConfig>,
}

impl Default for AuthConfig {
    fn default() -> Self {
        Self {
            mode: AuthMode::SingleUser,
            public_base_url: None,
            session_ttl: default_auth_session_ttl(),
            multi_user_legacy_token_enabled: false,
            oidc: OidcAuthConfig::default(),
            group_mappings: AuthGroupMappingsConfig::default(),
            authentik_management: AuthentikManagementConfig::default(),
            company_delegations: vec![],
            company_onboarding: None,
        }
    }
}

pub fn default_auth_session_ttl() -> Duration {
    Duration::from_secs(12 * 60 * 60)
}

pub fn default_oidc_client_secret_env() -> String {
    "MEMORY_LAYER_OIDC_CLIENT_SECRET".to_string()
}

pub fn default_oidc_scopes() -> Vec<String> {
    vec![
        "openid".to_string(),
        "profile".to_string(),
        "email".to_string(),
    ]
}

pub fn default_oidc_groups_claim() -> String {
    "groups".to_string()
}
