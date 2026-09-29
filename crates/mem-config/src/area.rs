// SPDX-License-Identifier: AGPL-3.0-or-later

//! Operator-owned, exact area routing. No network discovery or fallback.
use config::ConfigError;
use serde::{Deserialize, Serialize};
use std::{
    collections::BTreeMap,
    path::{Path, PathBuf},
};
use uuid::Uuid;

use crate::{AppConfig, Profile};
#[cfg(test)]
mod tests;

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct AreaIdentityConfig {
    pub name: String,
    pub instance_id: Uuid,
}

impl AreaIdentityConfig {
    pub fn key(&self) -> String {
        format!("{}-{}", self.name, self.instance_id)
    }

    pub fn validate(&self) -> Result<(), ConfigError> {
        if self.name.is_empty()
            || self.name.len() > 32
            || !self
                .name
                .bytes()
                .all(|c| c.is_ascii_lowercase() || c.is_ascii_digit() || c == b'-')
            || self.instance_id.is_nil()
        {
            return Err(error(
                "area requires a lowercase name (1..32 characters) and non-nil instance_id",
            ));
        }
        Ok(())
    }

    /// Each area owns a distinct offline file, including when two service
    /// configurations previously shared a directory or explicit offline path.
    /// Existing unscoped queues are intentionally left for operator reconciliation.
    pub fn offline_path(&self, base: &Path) -> PathBuf {
        let suffix = format!("-{}", self.key());
        let stem = base
            .file_stem()
            .and_then(|s| s.to_str())
            .unwrap_or("memory-layer");
        if stem.ends_with(&suffix) {
            return base.to_owned();
        }
        let extension = base
            .extension()
            .and_then(|s| s.to_str())
            .map(|ext| format!(".{ext}"))
            .unwrap_or_default();
        base.with_file_name(format!("{stem}{suffix}{extension}"))
    }
}

#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct AreaTarget {
    pub instance_id: Uuid,
    /// Absolute local config path. Omit for a remote-only area such as Trust
    /// on Monolith: direct local commands then fail with an actionable message.
    pub config: Option<PathBuf>,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct AreaRoute {
    pub area: String,
    pub repository: Option<PathBuf>,
    pub project: Option<String>,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct AreaRouting {
    pub version: u32,
    pub default_area: String,
    pub areas: BTreeMap<String, AreaTarget>,
    #[serde(default)]
    pub routes: Vec<AreaRoute>,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct ResolvedArea {
    pub identity: AreaIdentityConfig,
    pub config: PathBuf,
}

fn error(message: impl Into<String>) -> ConfigError {
    ConfigError::Message(message.into())
}

impl AreaRouting {
    pub fn read(path: &Path) -> Result<Self, ConfigError> {
        let metadata = std::fs::metadata(path)
            .map_err(|e| error(format!("read area routing {}: {e}", path.display())))?;
        if !metadata.is_file() {
            return Err(error("area routing must be a regular file"));
        }
        #[cfg(unix)]
        {
            use std::os::unix::fs::{MetadataExt, PermissionsExt};
            if metadata.permissions().mode() & 0o022 != 0 {
                return Err(error("area routing must not be group/world writable"));
            }
            let output = std::process::Command::new("id")
                .arg("-u")
                .output()
                .map_err(|e| error(format!("resolve routing owner: {e}")))?;
            let uid = String::from_utf8_lossy(&output.stdout)
                .trim()
                .parse::<u32>()
                .map_err(|_| error("cannot determine routing owner"))?;
            if !output.status.success() || (metadata.uid() != 0 && metadata.uid() != uid) {
                return Err(error("area routing must belong to this user or root"));
            }
        }
        let bytes = std::fs::read(path).map_err(|e| error(format!("read area routing: {e}")))?;
        let routing: Self = serde_json::from_slice(&bytes)
            .map_err(|e| error(format!("parse area routing: {e}")))?;
        routing.validate()?;
        Ok(routing)
    }

    pub fn validate(&self) -> Result<(), ConfigError> {
        if self.version != 1
            || self.default_area != "general"
            || !self.areas.contains_key("general")
        {
            return Err(error(
                "area routing version must be 1 and default_area must name general",
            ));
        }
        let mut identities = std::collections::BTreeSet::new();
        let mut configs = std::collections::BTreeSet::new();
        for (name, target) in &self.areas {
            AreaIdentityConfig {
                name: name.clone(),
                instance_id: target.instance_id,
            }
            .validate()?;
            if !identities.insert(target.instance_id) {
                return Err(error("each area must have a distinct instance identity"));
            }
            if let Some(path) = &target.config
                && (!path.is_absolute() || !configs.insert(normalized_path(path)))
            {
                return Err(error("area configs must have distinct absolute paths"));
            }
        }
        for route in &self.routes {
            if !self.areas.contains_key(&route.area)
                || (route.repository.is_none() && route.project.is_none())
                || route.repository.as_ref().is_some_and(|p| !p.is_absolute())
                || route.project.as_ref().is_some_and(|p| p.trim().is_empty())
            {
                return Err(error(
                    "area route requires a known area and an absolute repository and/or nonempty project",
                ));
            }
        }
        Ok(())
    }

    pub fn resolve(
        &self,
        explicit: Option<&str>,
        repository: Option<&Path>,
        project: Option<&str>,
    ) -> Result<ResolvedArea, ConfigError> {
        self.validate()?;
        let area = if let Some(area) = explicit {
            area.to_owned()
        } else {
            let repository = repository.map(repository_identity);
            let mut exact = std::collections::BTreeSet::new();
            let mut project_only = std::collections::BTreeSet::new();
            for route in &self.routes {
                let project_matches = route.project.as_deref().is_none_or(|p| Some(p) == project);
                if let Some(repo) = &route.repository {
                    if project_matches && repository.as_ref() == Some(&repository_identity(repo)) {
                        exact.insert(route.area.clone());
                    }
                } else if project_matches {
                    project_only.insert(route.area.clone());
                }
            }
            let matched = if exact.is_empty() {
                project_only
            } else {
                exact
            };
            if matched.len() > 1 {
                return Err(error(
                    "ambiguous area mapping; specify --area or correct the operator routing file",
                ));
            }
            matched
                .into_iter()
                .next()
                .unwrap_or_else(|| self.default_area.clone())
        };
        let target = self
            .areas
            .get(&area)
            .ok_or_else(|| error(format!("unknown area {area}")))?;
        let config = target.config.clone().ok_or_else(|| error(format!("area {area} is remote-only; use explicit federation expertise, or run local writes on its host")))?;
        if !config.is_file() {
            return Err(error(format!(
                "area {area} config is unavailable: {}; no fallback was attempted",
                config.display()
            )));
        }
        Ok(ResolvedArea {
            identity: AreaIdentityConfig {
                name: area,
                instance_id: target.instance_id,
            },
            config,
        })
    }
}

pub fn normalized_path(path: &Path) -> PathBuf {
    std::fs::canonicalize(path).unwrap_or_else(|_| path.to_path_buf())
}

fn repository_identity(path: &Path) -> PathBuf {
    mem_platform::git_common_dir(path).unwrap_or_else(|| normalized_path(path))
}

impl ResolvedArea {
    pub fn load(&self, profile: Profile) -> Result<AppConfig, ConfigError> {
        let config = AppConfig::load_with_profile(Some(self.config.clone()), profile)?;
        if config.area.as_ref() != Some(&self.identity) {
            return Err(error(format!(
                "area {} config identity does not match its routing binding",
                self.identity.name
            )));
        }
        Ok(config)
    }
}

/// Only the user/operator location is discovered; repositories cannot install
/// a routing policy. An explicitly selected missing file is an error at read.
pub fn discover_area_routing_path(explicit: Option<PathBuf>, profile: Profile) -> Option<PathBuf> {
    if explicit.is_some() {
        return explicit;
    }
    if let Some(path) = std::env::var_os("MEMORY_LAYER_AREA_ROUTING") {
        return Some(path.into());
    }
    if profile == Profile::Dev {
        return None;
    }
    let path = mem_platform::preferred_global_config_path().with_file_name("area-routing.json");
    path.exists().then_some(path)
}
