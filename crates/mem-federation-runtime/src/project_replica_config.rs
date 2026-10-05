// SPDX-License-Identifier: AGPL-3.0-or-later
//! Private native-laptop profile. Reading does not provision/migrate a database
//! or authorize a model destination. Never log this object: its local PostgreSQL
//! connection string can contain a credential.
use crate::{DeviceProjectSubscription, Endpoint, Result, RuntimeError};
use serde::{Deserialize, Serialize};
use std::path::Path;
use uuid::Uuid;

#[derive(Clone, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct LocalProjectReplicaProfile {
    pub schema_version: u32,
    pub company: String,
    pub subscription: Uuid,
    pub local_database_url: String,
    pub bootstrap: DeviceProjectSubscription,
    #[serde(default)]
    pub allow_loopback_development: bool,
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
    use mem_federation::{publication::*, subscription::*};
    fn profile() -> LocalProjectReplicaProfile {
        let security = GatewaySecurity::initial(&[37; 32], Default::default()).unwrap();
        LocalProjectReplicaProfile {
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
