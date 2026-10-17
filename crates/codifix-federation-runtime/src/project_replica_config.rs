// SPDX-License-Identifier: AGPL-3.0-or-later
//! Private native-laptop profile. Reading does not provision/migrate a database
//! or authorize a model destination. Never log this object: its local PostgreSQL
//! connection string can contain a credential.
use crate::{DeviceProjectSubscription, Endpoint, Result, RuntimeError};
use serde::{Deserialize, Serialize};
use std::path::Path;
use uuid::Uuid;

#[derive(Debug, Clone, Copy, Default, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum ProjectReplicationMode {
    #[default]
    SharedPublicationsV1,
    CuratedProjectV1,
}
impl ProjectReplicationMode {
    pub fn is_shared(&self) -> bool {
        *self == Self::SharedPublicationsV1
    }
}

/// The company/owner review contract uses RFC 8785, not struct field order.
/// Native clients must bind that same digest without leaking wire types into CLI.
pub fn onboarding_manifest_digest(manifest: &codifix_record::OnboardingProject) -> Result<String> {
    codifix_federation::CanonicalJsonCodec::digest(manifest)
        .map(|digest| digest.as_hex().to_owned())
        .map_err(|_| RuntimeError::Configuration)
}

/// Confirmed control-plane metadata passed to the native provisioner; no local
/// or server database credentials. The provisioner adds its own loopback URL.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct PreparedProjectSubscription {
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub manifest_sha256: Option<String>,
    #[serde(default, skip_serializing_if = "ProjectReplicationMode::is_shared")]
    pub replication_mode: ProjectReplicationMode,
    pub schema_version: u32,
    pub company: String,
    pub subscription: Uuid,
    pub bootstrap: DeviceProjectSubscription,
    #[serde(default)]
    pub allow_loopback_development: bool,
}
impl PreparedProjectSubscription {
    pub fn validate(&self) -> Result<()> {
        // Reuse profile/control-plane validation without inventing a server URL.
        LocalProjectReplicaProfile {
            manifest_sha256: self.manifest_sha256.clone(),
            local_project: None,
            replication_mode: self.replication_mode,
            schema_version: self.schema_version,
            company: self.company.clone(),
            subscription: self.subscription,
            local_database_url: format!(
                "postgresql://validation@127.0.0.1:55432/memory_laptop_{}",
                self.bootstrap.scope.area
            ),
            bootstrap: self.bootstrap.clone(),
            allow_loopback_development: self.allow_loopback_development,
        }
        .validate()
    }
    pub fn read(path: &Path) -> Result<Self> {
        let bytes = crate::private_files::read(path, 64 * 1024)?;
        let prepared: Self =
            serde_json::from_slice(&bytes).map_err(|_| RuntimeError::Configuration)?;
        prepared.validate()?;
        Ok(prepared)
    }
    pub fn create(&self, path: &Path) -> Result<()> {
        self.validate()?;
        let bytes = serde_json::to_vec_pretty(self).map_err(|_| RuntimeError::Configuration)?;
        crate::private_files::create(path, &bytes)
    }
}

#[derive(Clone, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct LocalProjectReplicaProfile {
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub manifest_sha256: Option<String>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub local_project: Option<LocalReplicaProject>,
    #[serde(default, skip_serializing_if = "ProjectReplicationMode::is_shared")]
    pub replication_mode: ProjectReplicationMode,
    pub schema_version: u32,
    pub company: String,
    pub subscription: Uuid,
    pub local_database_url: String,
    pub bootstrap: DeviceProjectSubscription,
    #[serde(default)]
    pub allow_loopback_development: bool,
}
#[derive(Clone, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct LocalReplicaProject {
    pub slug: String,
    pub repository: std::path::PathBuf,
}
impl LocalProjectReplicaProfile {
    pub fn read(path: &Path) -> Result<Self> {
        let bytes = crate::private_files::read(path, 64 * 1024)?;
        let profile: Self =
            serde_json::from_slice(&bytes).map_err(|_| RuntimeError::Configuration)?;
        profile.validate()?;
        Ok(profile)
    }
    pub fn validate(&self) -> Result<()> {
        if !self.replication_mode.is_shared()
            && self
                .manifest_sha256
                .as_ref()
                .is_none_or(|v| v.len() != 64 || !v.bytes().all(|b| b.is_ascii_hexdigit()))
        {
            return Err(RuntimeError::Configuration);
        }
        if let Some(project) = &self.local_project
            && (self.replication_mode.is_shared()
                || project.slug.is_empty()
                || project.slug.len() > 128
                || !project.repository.is_absolute())
        {
            return Err(RuntimeError::Configuration);
        }
        if self.schema_version != 1 || self.subscription.is_nil() {
            return Err(RuntimeError::Configuration);
        }
        self.bootstrap.validate()?;
        let company =
            reqwest::Url::parse(&self.company).map_err(|_| RuntimeError::Configuration)?;
        let loopback = company.host_str().is_some_and(|host| {
            host == "localhost"
                || host
                    .trim_matches(['[', ']'])
                    .parse::<std::net::IpAddr>()
                    .is_ok_and(|ip| ip.is_loopback())
        });
        if company.scheme() == "http" && !(self.allow_loopback_development && loopback) {
            return Err(RuntimeError::Configuration);
        }
        Endpoint::new(&self.company, self.allow_loopback_development && loopback)?;
        let db = reqwest::Url::parse(&self.local_database_url)
            .map_err(|_| RuntimeError::Configuration)?;
        if !matches!(db.scheme(), "postgres" | "postgresql")
            || !db.host_str().is_some_and(|host| {
                host.trim_matches(['[', ']'])
                    .parse::<std::net::IpAddr>()
                    .is_ok_and(|ip| ip.is_loopback())
            })
            || db.port().is_none_or(|port| port == 0)
            || db.username().is_empty()
            || db.query().is_some()
            || db.fragment().is_some()
            || db.path() != format!("/memory_laptop_{}", self.bootstrap.scope.area)
        {
            return Err(RuntimeError::Configuration);
        }
        Ok(())
    }
}

#[cfg(test)]
#[allow(clippy::unwrap_used)]
mod tests {
    use super::*;
    use crate::GatewaySecurity;
    use codifix_federation::{publication::*, subscription::*};
    fn profile() -> LocalProjectReplicaProfile {
        let security = GatewaySecurity::initial(&[37; 32], Default::default()).unwrap();
        LocalProjectReplicaProfile {
            manifest_sha256: None,
            local_project: None,
            replication_mode: ProjectReplicationMode::SharedPublicationsV1,
            schema_version: 1,
            company: "https://company.example.invalid".into(),
            subscription: Uuid::new_v4(),
            local_database_url: "postgresql://laptop@127.0.0.1:55432/memory_laptop_workshop".into(),
            allow_loopback_development: false,
            bootstrap: DeviceProjectSubscription {
                schema_version: 1,
                device: Uuid::new_v4(),
                catalog_revision: 1,
                authority: InstanceId::new(Uuid::new_v4()).unwrap(),
                receipt_keys: security.trust().keys().to_vec(),
                scope: ProjectScope {
                    company: Uuid::new_v4(),
                    area: "workshop".into(),
                    project: Uuid::new_v4(),
                    owner: InstanceId::new(Uuid::new_v4()).unwrap(),
                    space: SpaceId::new(Uuid::new_v4()).unwrap(),
                },
            },
        }
    }
    #[cfg(unix)]
    #[test]
    fn prepared_metadata_publication_is_atomic_create_only_and_private() {
        use std::{
            fs,
            os::unix::fs::{DirBuilderExt, PermissionsExt, symlink},
        };
        let path =
            std::env::temp_dir().join(format!("memory-subscription-test-{}", Uuid::new_v4()));
        fs::DirBuilder::new().mode(0o700).create(&path).unwrap();
        let fixture = profile();
        let prepared = PreparedProjectSubscription {
            manifest_sha256: None,
            replication_mode: ProjectReplicationMode::SharedPublicationsV1,
            schema_version: 1,
            company: fixture.company,
            subscription: fixture.subscription,
            bootstrap: fixture.bootstrap,
            allow_loopback_development: false,
        };
        let target = path.join("subscription.json");
        prepared.create(&target).unwrap();
        assert_eq!(
            PreparedProjectSubscription::read(&target).unwrap(),
            prepared
        );
        assert_eq!(
            fs::metadata(&target).unwrap().permissions().mode() & 0o777,
            0o600
        );
        let original = fs::read(&target).unwrap();
        let mut changed = prepared.clone();
        changed.subscription = Uuid::new_v4();
        assert!(changed.create(&target).is_err());
        assert_eq!(fs::read(&target).unwrap(), original);
        let alias = path.join("alias.json");
        symlink(&target, &alias).unwrap();
        assert!(PreparedProjectSubscription::read(&alias).is_err());
        assert!(changed.create(&alias).is_err());
        fs::set_permissions(&target, fs::Permissions::from_mode(0o644)).unwrap();
        assert!(PreparedProjectSubscription::read(&target).is_err());
        assert_eq!(fs::read_dir(&path).unwrap().count(), 2);
        fs::remove_dir_all(&path).unwrap();
    }
    #[test]
    fn profile_does_not_allow_server_database_or_origin_fallback() {
        assert!(profile().validate().is_ok());
        for url in [
            "postgresql://laptop@db.example.invalid:5432/memory_laptop_workshop",
            "postgresql://laptop@127.0.0.1:5432/memory",
            "postgresql://laptop@127.0.0.1:5432/memory_laptop_general",
            "postgresql://laptop@127.0.0.1:5432/memory_laptop_workshop?host=db.example.invalid",
            "postgresql://laptop@127.0.0.1/memory_laptop_workshop",
        ] {
            let mut value = profile();
            value.local_database_url = url.into();
            assert!(value.validate().is_err());
        }
        let mut value = profile();
        value.company = "http://company.example.invalid".into();
        value.allow_loopback_development = true;
        assert!(value.validate().is_err());
        value.company = "http://127.0.0.1:4250".into();
        assert!(value.validate().is_ok());
        value.bootstrap.scope.area = "trust".into();
        assert!(value.validate().is_err());
    }
}
