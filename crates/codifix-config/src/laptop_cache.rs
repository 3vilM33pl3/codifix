// SPDX-License-Identifier: AGPL-3.0-or-later
//! Operator-owned cache view, never inferred from arbitrary database contents.
use crate::AppConfig;
use codifix_record::AuthMode;
use serde::{Deserialize, Serialize};
use uuid::Uuid;

#[derive(Debug, Clone, Copy, Default, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum LaptopCacheReplicationMode {
    #[default]
    SharedPublicationsV1,
    CuratedProjectV1,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct LaptopCacheSubscription {
    pub subscription_id: Uuid,
    pub device_id: Uuid,
    pub company_id: Uuid,
    pub owner_instance_id: Uuid,
    pub project_id: Uuid,
    pub space_id: Uuid,
    #[serde(default)]
    pub replication_mode: LaptopCacheReplicationMode,
}
#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct LaptopCacheConfig {
    pub subscriptions: Vec<LaptopCacheSubscription>,
}
impl AppConfig {
    pub fn validate_laptop_cache(&self) -> Result<(), config::ConfigError> {
        let Some(cache) = &self.laptop_cache else {
            return Ok(());
        };
        let valid_area = self.area.as_ref().is_some_and(|a| {
            a.validate().is_ok() && matches!(a.name.as_str(), "workshop" | "homelab" | "general")
        });
        let loopback = self
            .service
            .bind_addr
            .parse::<std::net::SocketAddr>()
            .is_ok_and(|a| a.ip().is_loopback() && a.port() != 0);
        // This narrow native profile uses a literal loopback socket, explicit
        // port and exact area database. No DNS, URL options or fallback target.
        let database = self.area.as_ref().is_some_and(|area| {
            !self
                .database
                .url
                .chars()
                .any(|c| c.is_whitespace() || c.is_control())
                && url::Url::parse(&self.database.url).is_ok_and(|u| {
                    matches!(u.scheme(), "postgres" | "postgresql")
                        && !u.username().is_empty()
                        && u.query().is_none()
                        && u.fragment().is_none()
                        && u.port().is_some_and(|p| p != 0)
                        && u.host_str().is_some_and(|host| {
                            host.trim_matches(['[', ']'])
                                .parse::<std::net::IpAddr>()
                                .is_ok_and(|ip| ip.is_loopback())
                        })
                        && u.path() == format!("/memory_laptop_{}", area.name)
                })
        });
        let mut seen = std::collections::BTreeSet::new();
        let curated = !cache.subscriptions.is_empty()
            && cache
                .subscriptions
                .iter()
                .all(|s| s.replication_mode == LaptopCacheReplicationMode::CuratedProjectV1);
        let valid_bindings = !cache.subscriptions.is_empty()
            && cache.subscriptions.len() <= 32
            && cache.subscriptions.iter().all(|s| {
                [
                    s.subscription_id,
                    s.device_id,
                    s.company_id,
                    s.owner_instance_id,
                    s.project_id,
                    s.space_id,
                ]
                .iter()
                .all(|id| !id.is_nil())
                    && seen.insert(s.subscription_id)
            });
        if !valid_area
            || !loopback
            || !valid_bindings
            || !database
            || (!curated && !self.service.read_only)
            || self.auth.mode != AuthMode::SingleUser
            || self.cluster.enabled
            || self.background_work.enabled
            || self.background_work.internal_automation
            || (!curated && self.offline.enabled)
            || self.provenance.reverify_enabled
            || self.procedural.enabled
            || (!curated && self.automation.enabled)
            || self.reinforcement.enabled
            || self.reinforcement.validation_enabled
            || self.consolidation.enabled
            || self.embeddings.enabled
            || self.embeddings.create_enabled
            || self.features.llm_curation
            || !self.llm.model.is_empty()
            || self.mcp.enabled
            || self.mcp.http_enabled
            || self.supervisor.is_some()
        {
            return Err(config::ConfigError::Message("laptop cache requires an isolated loopback read-only area, explicit bindings and disabled model/worker/MCP execution".into()));
        }
        Ok(())
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    fn fixture() -> AppConfig {
        serde_json::from_value(serde_json::json!({
            "area":{"name":"workshop","instance_id":Uuid::new_v4()},
            "service":{"bind_addr":"127.0.0.1:55440","read_only":true},
            "database":{"url":"postgresql://laptop:synthetic@127.0.0.1:55432/memory_laptop_workshop"},
            "laptop_cache":{"subscriptions":[{"subscription_id":Uuid::new_v4(),"device_id":Uuid::new_v4(),"company_id":Uuid::new_v4(),"owner_instance_id":Uuid::new_v4(),"project_id":Uuid::new_v4(),"space_id":Uuid::new_v4()}]},
            "cluster":{"enabled":false},"background_work":{"enabled":false,"internal_automation":false},"automation":{"enabled":false},
            "offline":{"enabled":false},"provenance":{"reverify_enabled":false},"procedural":{"enabled":false},
            "reinforcement":{"enabled":false,"validation_enabled":false},"consolidation":{"enabled":false},
            "embeddings":{"enabled":false,"create_enabled":false},"features":{"llm_curation":false},
            "llm":{"model":""},"mcp":{"enabled":false,"http_enabled":false}
        })).unwrap()
    }
    #[test]
    fn laptop_cache_is_opt_in_and_rejects_unbounded_or_active_execution() {
        let config = fixture();
        config.validate_laptop_cache().unwrap();
        for change in [
            |c: &mut AppConfig| c.service.read_only = false,
            |c: &mut AppConfig| c.auth.mode = AuthMode::MultiUser,
            |c: &mut AppConfig| c.background_work.enabled = true,
            |c: &mut AppConfig| c.background_work.internal_automation = true,
            |c: &mut AppConfig| c.offline.enabled = true,
            |c: &mut AppConfig| c.provenance.reverify_enabled = true,
            |c: &mut AppConfig| c.procedural.enabled = true,
            |c: &mut AppConfig| c.reinforcement.enabled = true,
            |c: &mut AppConfig| c.reinforcement.validation_enabled = true,
            |c: &mut AppConfig| c.automation.enabled = true,
            |c: &mut AppConfig| c.consolidation.enabled = true,
            |c: &mut AppConfig| c.embeddings.create_enabled = true,
            |c: &mut AppConfig| c.llm.model = "unapproved-model".into(),
            |c: &mut AppConfig| c.mcp.enabled = true,
            |c: &mut AppConfig| c.service.bind_addr = "0.0.0.0:55440".into(),
            |c: &mut AppConfig| c.area.as_mut().unwrap().name = "trust".into(),
            |c: &mut AppConfig| c.laptop_cache.as_mut().unwrap().subscriptions.clear(),
            |c: &mut AppConfig| {
                c.laptop_cache.as_mut().unwrap().subscriptions[0].device_id = Uuid::nil()
            },
        ] {
            let mut changed = config.clone();
            change(&mut changed);
            assert!(changed.validate_laptop_cache().is_err());
        }
        let mut duplicate = config.clone();
        let same = duplicate.laptop_cache.as_ref().unwrap().subscriptions[0].clone();
        duplicate
            .laptop_cache
            .as_mut()
            .unwrap()
            .subscriptions
            .push(same);
        assert!(duplicate.validate_laptop_cache().is_err());
        let mut legacy = config;
        legacy.laptop_cache = None;
        legacy.service.read_only = false;
        legacy.validate_laptop_cache().unwrap();
    }
    #[test]
    fn laptop_cache_requires_literal_loopback_and_exact_database_before_migrations() {
        for url in [
            "postgresql://laptop:synthetic@127.0.0.1:55432/memory_laptop_workshop",
            "postgres://laptop@[::1]:55432/memory_laptop_workshop",
        ] {
            let mut c = fixture();
            c.database.url = url.into();
            c.validate_laptop_cache().unwrap();
        }
        for url in [
            "postgresql://laptop@server.example.invalid:55432/memory_laptop_workshop",
            "postgresql://laptop@127.0.0.1/memory_laptop_workshop",
            "postgresql://laptop@127.0.0.1:55432/memory_laptop_general",
            "postgresql://laptop@127.0.0.1:55432/shared",
            "postgresql://laptop@127.0.0.1:55432/memory_laptop_workshop?host=other",
            "postgresql://laptop@127.0.0.1:55432/memory_laptop_workshop#option",
            "postgresql://@127.0.0.1:55432/memory_laptop_workshop",
            "postgresql://laptop@remote.example.invalid/shared?application_name=@127.0.0.1:55432/memory_laptop_workshop",
            "postgresql://laptop@remote.example.invalid/shared#@127.0.0.1:55432/memory_laptop_workshop",
        ] {
            let mut c = fixture();
            c.database.url = url.into();
            assert!(c.validate_laptop_cache().is_err());
        }
    }

    #[test]
    fn curated_cache_allows_local_work_but_does_not_enable_model_execution() {
        let mut c = fixture();
        c.laptop_cache.as_mut().unwrap().subscriptions[0].replication_mode =
            LaptopCacheReplicationMode::CuratedProjectV1;
        c.service.read_only = false;
        c.offline.enabled = true;
        c.automation.enabled = true;
        c.validate_laptop_cache().unwrap();
        c.llm.model = "not-authorized".into();
        assert!(c.validate_laptop_cache().is_err());
    }
}
