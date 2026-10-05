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

/// Separate exact delivery credential, never an imported management authority.
#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct CompanyDeliveryDelegationConfig {
    pub company_id: Uuid,
    pub owner_instance_id: Uuid,
    pub area: OnboardingArea,
    pub issuer: String,
    pub principal_id: Uuid,
    pub token_id: Uuid,
    pub policy_revision: i64,
    pub projects: Vec<Uuid>,
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
    /// Separate server-only reference. Old owner registries remain valid.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub delivery_token_env: Option<String>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub delivery_policy_revision: Option<i64>,
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
    pub company_delivery_delegations: Vec<CompanyDeliveryDelegationConfig>,
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
            company_delivery_delegations: vec![],
            company_onboarding: None,
        }
    }
}

impl AuthConfig {
    /// Delivery is opt-in and exact, with no project-role preset authority.
    pub fn validate_company_delivery_delegations(&self) -> Result<(), ConfigError> {
        let fail = || {
            ConfigError::Message("company delivery delegates require multi-user exact shared HTTPS namespace, separate credential identities and bounded explicit non-Trust scopes".into())
        };
        if let Some(registry) = &self.company_onboarding {
            for owner in &registry.owners {
                if owner.delivery_token_env.is_some() && self.mode != AuthMode::MultiUser {
                    return Err(fail());
                }
                match (&owner.delivery_token_env, owner.delivery_policy_revision) {
                    (None, None) => {}
                    (Some(env), Some(revision))
                        if revision > 0
                            && owner.area != OnboardingArea::Trust
                            && env != &owner.token_env
                            && !env.is_empty()
                            && env.len() <= 128
                            && env.bytes().all(|c| c.is_ascii_alphanumeric() || c == b'_') => {}
                    _ => return Err(fail()),
                }
            }
        }
        if self.company_delivery_delegations.is_empty() {
            return Ok(());
        }
        self.validate_company_delegations()?;
        if self.mode != AuthMode::MultiUser || self.company_delivery_delegations.len() > 64 {
            return Err(fail());
        }
        let mut companies = std::collections::BTreeSet::new();
        let mut principals = std::collections::BTreeSet::new();
        let mut tokens = std::collections::BTreeSet::new();
        for cfg in &self.company_delivery_delegations {
            if cfg.company_id.is_nil()
                || cfg.owner_instance_id.is_nil()
                || cfg.principal_id.is_nil()
                || cfg.token_id.is_nil()
                || cfg.principal_id == cfg.token_id
                || cfg.policy_revision < 1
                || cfg.area == OnboardingArea::Trust
                || cfg.issuer != self.oidc.issuer_url
                || cfg.issuer.ends_with('/')
                || !cfg.issuer.starts_with("https://")
                || cfg.issuer.len() <= 8
                || cfg.issuer.len() > 2048
                || cfg.issuer.chars().any(|c| {
                    c.is_control() || c.is_whitespace() || matches!(c, '@' | '?' | '#' | '\\')
                })
                || !companies.insert(cfg.company_id)
                || !principals.insert(cfg.principal_id)
                || !tokens.insert(cfg.token_id)
                || self.company_delegations.iter().any(|m| {
                    [m.principal_id, m.token_id].contains(&cfg.principal_id)
                        || [m.principal_id, m.token_id].contains(&cfg.token_id)
                })
                || cfg.projects.is_empty()
                || cfg.projects.len() > 256
            {
                return Err(fail());
            }
            let mut projects = std::collections::BTreeSet::new();
            if cfg
                .projects
                .iter()
                .any(|p| p.is_nil() || !projects.insert(*p))
            {
                return Err(fail());
            }
        }
        Ok(())
    }
    /// Fail closed before operator provisioning; these are exact stable
    /// identities, never role presets, email aliases or automatic enrollment.
    pub fn validate_company_delegations(&self) -> Result<(), ConfigError> {
        let fail = || {
            ConfigError::Message("company delegates require unique exact identities, shared HTTPS OIDC namespace and bounded non-Trust project ceilings".into())
        };
        if self.company_delegations.len() > 64 {
            return Err(fail());
        }
        let mut companies = std::collections::BTreeSet::new();
        let mut principals = std::collections::BTreeSet::new();
        let mut tokens = std::collections::BTreeSet::new();
        for cfg in &self.company_delegations {
            if cfg.company_id.is_nil()
                || cfg.principal_id.is_nil()
                || cfg.token_id.is_nil()
                || cfg.principal_id == cfg.token_id
                || cfg.policy_revision < 1
                || cfg.issuer != self.oidc.issuer_url
                || cfg.issuer.ends_with('/')
                || !cfg.issuer.starts_with("https://")
                || cfg.issuer.len() <= 8
                || cfg.issuer.len() > 2048
                || cfg.issuer.chars().any(|c| {
                    c.is_control() || c.is_whitespace() || matches!(c, '@' | '?' | '#' | '\\')
                })
                || !companies.insert(cfg.company_id)
                || !principals.insert(cfg.principal_id)
                || !tokens.insert(cfg.token_id)
                || cfg.projects.is_empty()
                || cfg.projects.len() > 256
            {
                return Err(fail());
            }
            let mut projects = std::collections::BTreeSet::new();
            if cfg.projects.iter().any(|p| {
                p.project_id.is_nil()
                    || !projects.insert(p.project_id)
                    || !matches!(
                        p.role_ceiling,
                        AuthRole::Reader | AuthRole::Writer | AuthRole::Operator
                    )
            }) {
                return Err(fail());
            }
        }
        Ok(())
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

#[cfg(test)]
mod company_delegations_tests {
    use super::*;

    fn configured() -> AuthConfig {
        let mut auth = AuthConfig::default();
        auth.oidc.issuer_url = "https://identity.example/application/o/memory".into();
        auth.company_delegations.push(CompanyDelegationConfig {
            company_id: Uuid::new_v4(),
            issuer: auth.oidc.issuer_url.clone(),
            principal_id: Uuid::new_v4(),
            token_id: Uuid::new_v4(),
            policy_revision: 1,
            projects: vec![CompanyProjectDelegation {
                project_id: Uuid::new_v4(),
                role_ceiling: AuthRole::Operator,
            }],
        });
        auth
    }

    #[test]
    fn company_delegations_are_opt_in_and_exact() {
        assert!(AuthConfig::default().validate_company_delegations().is_ok());
        let auth = configured();
        assert!(auth.validate_company_delegations().is_ok());
        for issuer in [
            "http://identity.example/application/o/memory",
            "https://identity.example/application/o/memory/",
            "https://secret@identity.example/application/o/memory",
            "https://identity.example/application/o/memory?alias=yes",
            "https://identity.example/application/o/memory#alias",
            "https://identity.example/application/o/other",
        ] {
            let mut changed = auth.clone();
            changed.company_delegations[0].issuer = issuer.into();
            assert!(changed.validate_company_delegations().is_err());
        }
    }

    #[test]
    fn company_delegations_reject_duplicate_or_unbounded_authority() {
        let auth = configured();
        for change in [
            (|a: &mut AuthConfig| a.company_delegations[0].company_id = Uuid::nil())
                as fn(&mut AuthConfig),
            |a| a.company_delegations[0].token_id = a.company_delegations[0].principal_id,
            |a| a.company_delegations[0].policy_revision = 0,
            |a| a.company_delegations[0].projects[0].role_ceiling = AuthRole::Admin,
            |a| a.company_delegations[0].projects.clear(),
            |a| {
                let p = a.company_delegations[0].projects[0].clone();
                a.company_delegations[0].projects.push(p);
            },
            |a| a.company_delegations.push(a.company_delegations[0].clone()),
        ] {
            let mut changed = auth.clone();
            change(&mut changed);
            assert!(changed.validate_company_delegations().is_err());
        }
        let mut excessive = auth;
        excessive.company_delegations[0].projects = (0..257)
            .map(|_| CompanyProjectDelegation {
                project_id: Uuid::new_v4(),
                role_ceiling: AuthRole::Reader,
            })
            .collect();
        assert!(excessive.validate_company_delegations().is_err());
    }

    fn delivery_configured() -> AuthConfig {
        let mut auth = configured();
        auth.mode = AuthMode::MultiUser;
        auth.company_delivery_delegations
            .push(CompanyDeliveryDelegationConfig {
                company_id: auth.company_delegations[0].company_id,
                owner_instance_id: Uuid::new_v4(),
                area: OnboardingArea::Workshop,
                issuer: auth.oidc.issuer_url.clone(),
                principal_id: Uuid::new_v4(),
                token_id: Uuid::new_v4(),
                policy_revision: 1,
                projects: vec![auth.company_delegations[0].projects[0].project_id],
            });
        auth
    }

    #[test]
    fn company_delivery_delegations_are_separate_and_exact() {
        assert!(
            AuthConfig::default()
                .validate_company_delivery_delegations()
                .is_ok()
        );
        let auth = delivery_configured();
        assert!(auth.validate_company_delivery_delegations().is_ok());
        for change in [
            (|a: &mut AuthConfig| a.mode = AuthMode::SingleUser) as fn(&mut AuthConfig),
            |a| a.company_delivery_delegations[0].area = OnboardingArea::Trust,
            |a| a.company_delivery_delegations[0].owner_instance_id = Uuid::nil(),
            |a| a.company_delivery_delegations[0].policy_revision = 0,
            |a| a.company_delivery_delegations[0].issuer.push('/'),
            |a| {
                a.company_delivery_delegations[0].principal_id =
                    a.company_delegations[0].principal_id
            },
            |a| a.company_delivery_delegations[0].token_id = a.company_delegations[0].token_id,
            |a| a.company_delivery_delegations[0].principal_id = a.company_delegations[0].token_id,
            |a| a.company_delivery_delegations[0].projects.clear(),
            |a| {
                let id = a.company_delivery_delegations[0].projects[0];
                a.company_delivery_delegations[0].projects.push(id);
            },
            |a| {
                a.company_delivery_delegations
                    .push(a.company_delivery_delegations[0].clone())
            },
        ] {
            let mut changed = auth.clone();
            change(&mut changed);
            assert!(changed.validate_company_delivery_delegations().is_err());
        }
    }

    #[test]
    fn company_owner_delivery_references_are_optional_paired_and_non_secret() {
        let owner:CompanyOwnerConfig=serde_json::from_value(serde_json::json!({
            "owner_instance_id":Uuid::new_v4(),"area":"workshop","base_url":"https://owner.example", "token_env":"MANAGEMENT_TOKEN", "issuer":"https://identity.example/application/o/memory", "delegation_policy_revision":1,
        })).unwrap();
        assert!(owner.delivery_token_env.is_none());
        assert!(owner.delivery_policy_revision.is_none());
        let mut auth = AuthConfig {
            company_onboarding: Some(CompanyOnboardingConfig {
                company_id: Uuid::new_v4(),
                owners: vec![owner],
            }),
            ..AuthConfig::default()
        };
        assert!(auth.validate_company_delivery_delegations().is_ok());
        auth.mode = AuthMode::MultiUser;
        let owner = &mut auth.company_onboarding.as_mut().unwrap().owners[0];
        owner.delivery_token_env = Some("DELIVERY_TOKEN".into());
        assert!(auth.validate_company_delivery_delegations().is_err());
        auth.company_onboarding.as_mut().unwrap().owners[0].delivery_policy_revision = Some(1);
        assert!(auth.validate_company_delivery_delegations().is_ok());
        auth.company_onboarding.as_mut().unwrap().owners[0].delivery_token_env =
            Some("MANAGEMENT_TOKEN".into());
        assert!(auth.validate_company_delivery_delegations().is_err());
        auth.company_onboarding.as_mut().unwrap().owners[0].delivery_token_env =
            Some("DELIVERY_TOKEN".into());
        auth.company_onboarding.as_mut().unwrap().owners[0].area = OnboardingArea::Trust;
        assert!(auth.validate_company_delivery_delegations().is_err());
    }
}
