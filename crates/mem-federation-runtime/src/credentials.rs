// SPDX-License-Identifier: AGPL-3.0-or-later
//! Administrator-owned access credentials, never refresh credentials. Each
//! attempt pins one immutable snapshot. JWT claims are structural checks, not
//! signature verification; enrollment and the fixed authenticated PDS establish
//! the authority of these private operator files.
use crate::{Result, RuntimeError, private_files};
use base64::{Engine, engine::general_purpose::URL_SAFE_NO_PAD};
use serde::{Deserialize, Serialize};
use std::{path::PathBuf, sync::Mutex};

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum CredentialMode {
    Static,
    Rotating,
    Unconfigured,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum CredentialState {
    Usable,
    Expired,
    Invalid,
    StaticUnverified,
    AuthenticationBlocked,
    Unknown,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum MaintenanceState {
    Healthy,
    Refreshing,
    RetryWait,
    Intervention,
    Unknown,
}

/// No tokens, repository contents, file paths or arbitrary operator error text.
/// `publication_usable` means locally eligible to attempt publication, not that
/// the PDS is reachable or has accepted a publication.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct GatewayCredentialStatus {
    pub observed_at: u64,
    pub mode: CredentialMode,
    pub state: CredentialState,
    pub publication_usable: bool,
    pub generation: Option<u64>,
    pub expires_at: Option<u64>,
    pub maintenance_state: MaintenanceState,
    pub maintenance_observed_at: Option<u64>,
    pub last_success: Option<u64>,
    pub retry_at: Option<u64>,
    pub tls_expires_at: Option<u64>,
    pub reason: Option<String>,
}
impl GatewayCredentialStatus {
    pub(crate) fn unknown(now: u64) -> Self {
        Self {
            observed_at: now,
            mode: CredentialMode::Unconfigured,
            state: CredentialState::Unknown,
            publication_usable: false,
            generation: None,
            expires_at: None,
            maintenance_state: MaintenanceState::Unknown,
            maintenance_observed_at: None,
            last_success: None,
            retry_at: None,
            tls_expires_at: None,
            reason: Some("not_observed".into()),
        }
    }
    pub(crate) fn static_token(now: u64) -> Self {
        Self {
            mode: CredentialMode::Static,
            state: CredentialState::StaticUnverified,
            publication_usable: true,
            reason: Some("static_expiry_unknown".into()),
            ..Self::unknown(now)
        }
    }
}

#[derive(Clone, PartialEq, Eq, Deserialize)]
#[serde(deny_unknown_fields)]
pub(crate) struct AccessBundle {
    schema_version: u32,
    repository: String,
    pds: String,
    pub(crate) generation: u64,
    pub(crate) access_token: String,
    issued_at: u64,
    expires_at: u64,
}
#[derive(Deserialize)]
#[serde(deny_unknown_fields)]
struct MaintenanceFile {
    schema_version: u32,
    repository: String,
    pds: String,
    generation: u64,
    observed_at: u64,
    last_success: Option<u64>,
    retry_at: Option<u64>,
    state: MaintenanceState,
    reason: Option<String>,
    tls_expires_at: Option<u64>,
}

pub(crate) struct RotatingCredentials {
    path: PathBuf,
    maintenance_path: Option<PathBuf>,
    repository: String,
    pds: String,
    last: Mutex<Option<AccessBundle>>,
}
impl RotatingCredentials {
    pub(crate) fn new(
        path: PathBuf,
        maintenance_path: Option<PathBuf>,
        repository: String,
        pds: String,
    ) -> Result<Self> {
        if !path.is_absolute()
            || maintenance_path
                .as_ref()
                .is_some_and(|p| !p.is_absolute() || p == &path)
        {
            return Err(RuntimeError::Configuration);
        }
        Ok(Self {
            path,
            maintenance_path,
            repository,
            pds,
            last: Mutex::new(None),
        })
    }

    fn load(&self, now: u64) -> Result<AccessBundle> {
        let bundle: AccessBundle = serde_json::from_slice(&private_files::read(&self.path, 8192)?)
            .map_err(|_| RuntimeError::Configuration)?;
        bundle.validate(&self.repository, &self.pds, now)?;
        let mut last = self.last.lock().map_err(|_| RuntimeError::Configuration)?;
        if let Some(previous) = last.as_ref()
            && (bundle.generation < previous.generation
                || (bundle.generation == previous.generation && bundle != *previous)
                || bundle.issued_at < previous.issued_at)
        {
            return Err(RuntimeError::Configuration);
        }
        *last = Some(bundle.clone());
        Ok(bundle)
    }

    pub(crate) fn snapshot(&self, now: u64) -> Result<AccessBundle> {
        let bundle = self.load(now)?;
        if bundle.expires_at <= now {
            return Err(RuntimeError::Unauthorized);
        }
        Ok(bundle)
    }

    pub(crate) fn status(&self, now: u64) -> GatewayCredentialStatus {
        let mut status = GatewayCredentialStatus {
            mode: CredentialMode::Rotating,
            ..GatewayCredentialStatus::unknown(now)
        };
        match self.load(now) {
            Ok(bundle) => {
                status.generation = Some(bundle.generation);
                status.expires_at = Some(bundle.expires_at);
                status.publication_usable = bundle.expires_at > now;
                status.state = if status.publication_usable {
                    CredentialState::Usable
                } else {
                    CredentialState::Expired
                };
                status.reason = (!status.publication_usable).then(|| "credential_expired".into());
            }
            Err(_) => {
                status.state = CredentialState::Invalid;
                status.reason = Some("credential_file_invalid".into());
            }
        }
        self.maintenance(now, &mut status);
        status
    }

    fn maintenance(&self, now: u64, status: &mut GatewayCredentialStatus) {
        let Some(path) = &self.maintenance_path else {
            return;
        };
        let value = private_files::read(path, 4096)
            .ok()
            .and_then(|bytes| serde_json::from_slice::<MaintenanceFile>(&bytes).ok());
        let Some(value) = value.filter(|v| {
            v.schema_version == 1
                && v.repository == self.repository
                && v.pds == self.pds
                && Some(v.generation) == status.generation
                && v.observed_at <= now
                && v.last_success.is_none_or(|t| t <= v.observed_at)
                && v.reason.as_ref().is_none_or(|s| s.len() <= 64)
        }) else {
            if status.reason.is_none() {
                status.reason = Some("maintenance_status_invalid".into());
            }
            return;
        };
        status.maintenance_observed_at = Some(value.observed_at);
        status.last_success = value.last_success;
        status.tls_expires_at = value.tls_expires_at;
        if now.saturating_sub(value.observed_at) > 180 {
            if status.reason.is_none() {
                status.reason = Some("maintenance_status_stale".into());
            }
            return;
        }
        status.maintenance_state = value.state;
        status.retry_at = value.retry_at;
        if status.reason.is_none() {
            status.reason = match value.state {
                MaintenanceState::Healthy => None,
                MaintenanceState::Refreshing => Some("maintenance_refreshing".into()),
                MaintenanceState::RetryWait => Some("maintenance_retry_wait".into()),
                MaintenanceState::Intervention => Some("maintenance_intervention".into()),
                MaintenanceState::Unknown => Some("maintenance_status_unknown".into()),
            };
        }
    }
}
impl AccessBundle {
    fn validate(&self, repository: &str, pds: &str, now: u64) -> Result<()> {
        if self.schema_version != 1
            || self.repository != repository
            || self.pds != pds
            || self.generation == 0
            || self.generation > 9_007_199_254_740_991
            || self.issued_at > now.saturating_add(30)
            || self.expires_at <= self.issued_at
            || self.expires_at.saturating_sub(self.issued_at) > 86_400
            || self.access_token.len() < 32
            || self.access_token.len() > 4096
        {
            return Err(RuntimeError::Configuration);
        }
        let parts: Vec<_> = self.access_token.split('.').collect();
        if parts.len() != 3 || parts.iter().any(|p| p.is_empty()) {
            return Err(RuntimeError::Configuration);
        }
        let bytes = URL_SAFE_NO_PAD
            .decode(parts[1])
            .map_err(|_| RuntimeError::Configuration)?;
        let claims: serde_json::Value =
            serde_json::from_slice(&bytes).map_err(|_| RuntimeError::Configuration)?;
        if claims["sub"].as_str() != Some(repository)
            || claims["scope"] != "com.atproto.access"
            || claims["exp"].as_u64() != Some(self.expires_at)
            || !claims["aud"]
                .as_str()
                .is_some_and(|s| s.starts_with("did:") && s.len() <= 2048)
        {
            return Err(RuntimeError::Configuration);
        }
        Ok(())
    }
}

pub(crate) fn now_seconds() -> u64 {
    chrono::Utc::now().timestamp().max(0) as u64
}

#[cfg(all(test, unix))]
#[allow(clippy::unwrap_used)]
mod tests {
    use super::*;
    use serde_json::{Value, json};
    use std::{
        fs,
        io::Write,
        os::unix::fs::{OpenOptionsExt, PermissionsExt},
    };

    const REPO: &str = "did:web:private.example.test";
    const PDS: &str = "https://private.example.test";
    const NOW: u64 = 1_800_000_000;

    fn bundle(generation: u64, expiry: u64) -> Value {
        let token = format!(
            "{}.{}.syntheticSignature",
            URL_SAFE_NO_PAD.encode(br#"{"alg":"HS256","typ":"JWT"}"#),
            URL_SAFE_NO_PAD.encode(
                serde_json::to_vec(
                    &json!({"sub":REPO,"aud":REPO,"scope":"com.atproto.access","exp":expiry})
                )
                .unwrap()
            )
        );
        json!({"schema_version":1,"repository":REPO,"pds":PDS,"generation":generation,
            "access_token":token,"issued_at":NOW-3600,"expires_at":expiry})
    }
    fn fixture() -> (tempfile::TempDir, RotatingCredentials) {
        let directory = tempfile::Builder::new()
            .permissions(fs::Permissions::from_mode(0o700))
            .tempdir()
            .unwrap();
        let source = RotatingCredentials::new(
            directory.path().join("session.json"),
            Some(directory.path().join("status.json")),
            REPO.into(),
            PDS.into(),
        )
        .unwrap();
        (directory, source)
    }
    fn replace(source: &RotatingCredentials, value: &Value) {
        let temporary = source.path.with_extension("next");
        let mut file = fs::OpenOptions::new()
            .write(true)
            .create_new(true)
            .mode(0o600)
            .open(&temporary)
            .unwrap();
        file.write_all(&serde_json::to_vec(value).unwrap()).unwrap();
        fs::rename(temporary, &source.path).unwrap();
    }
    #[test]
    fn replacement_is_monotonic_and_existing_snapshot_is_immutable() {
        let (_dir, source) = fixture();
        replace(&source, &bundle(1, NOW + 3600));
        let first = source.snapshot(NOW).unwrap();
        replace(&source, &bundle(2, NOW + 7200));
        let second = source.snapshot(NOW).unwrap();
        assert_eq!(first.generation, 1);
        assert_eq!(second.generation, 2);
        assert_ne!(first.access_token, second.access_token);
        replace(&source, &bundle(1, NOW + 3600));
        assert!(source.snapshot(NOW).is_err());
        assert_eq!(source.status(NOW).state, CredentialState::Invalid);
        replace(&source, &bundle(2, NOW + 3600));
        assert!(source.snapshot(NOW).is_err());
        replace(&source, &bundle(3, NOW + 7200));
        assert!(source.snapshot(NOW).is_ok());
        assert!(source.snapshot(NOW + 7200).is_err());
        assert_eq!(source.status(NOW + 7200).state, CredentialState::Expired);
    }
    #[test]
    fn identity_expiry_unknown_fields_and_jwt_bindings_fail_closed() {
        let (_dir, source) = fixture();
        let good = bundle(1, NOW + 3600);
        for (field, value) in [
            ("repository", json!("did:web:other.test")),
            ("pds", json!("http://169.254.169.254")),
            ("schema_version", json!(2)),
            ("generation", json!(0)),
            ("issued_at", json!(NOW + 31)),
            ("expires_at", json!(NOW + 3601)),
            ("refresh_token", json!("must-never-be-mounted")),
            ("access_token", json!("invalid-token")),
        ] {
            let mut bad = good.clone();
            bad[field] = value;
            replace(&source, &bad);
            assert!(source.snapshot(NOW).is_err(), "{field}");
        }
        replace(&source, &good);
        assert!(source.snapshot(NOW).is_ok());
        fs::set_permissions(&source.path, fs::Permissions::from_mode(0o644)).unwrap();
        assert!(source.snapshot(NOW).is_err());
    }
    #[test]
    fn maintenance_is_fresh_bounded_sanitized_metadata_not_a_token_authority() {
        let (_dir, source) = fixture();
        replace(&source, &bundle(1, NOW + 3600));
        let status_file = source.maintenance_path.as_ref().unwrap();
        let health = json!({"schema_version":1,"repository":REPO,"pds":PDS,"generation":1,
            "observed_at":NOW,"last_success":NOW-10,"retry_at":NOW+60,"state":"retry_wait",
            "reason":"private_secret_never_render_this","tls_expires_at":NOW+86400});
        let mut file = fs::OpenOptions::new()
            .write(true)
            .create_new(true)
            .mode(0o600)
            .open(status_file)
            .unwrap();
        file.write_all(&serde_json::to_vec(&health).unwrap())
            .unwrap();
        let status = source.status(NOW);
        assert_eq!(status.maintenance_state, MaintenanceState::RetryWait);
        assert_eq!(status.reason.as_deref(), Some("maintenance_retry_wait"));
        let serialized = serde_json::to_string(&status).unwrap();
        assert!(!serialized.contains("private_secret"));
        assert!(!serialized.contains("syntheticSignature"));
        let stale = source.status(NOW + 181);
        assert_eq!(stale.maintenance_state, MaintenanceState::Unknown);
        assert_eq!(stale.reason.as_deref(), Some("maintenance_status_stale"));
        assert!(stale.publication_usable); // still-valid access is separate from maintainer health
    }
}
