// SPDX-License-Identifier: AGPL-3.0-or-later
use super::CompanyOrigin;
use anyhow::{Result, bail};
use serde::{Deserialize, Serialize};
use sha2::{Digest, Sha256};
use uuid::Uuid;

pub(super) trait CredentialStore {
    fn get(&self, service: &str, account: &str) -> Result<Option<Vec<u8>>>;
    fn put(&self, service: &str, account: &str, value: &[u8]) -> Result<()>;
    fn delete(&self, service: &str, account: &str) -> Result<()>;
}

pub(super) struct NativeKeychain;
#[cfg(windows)]
#[path = "windows_credentials.rs"]
mod windows;
#[cfg(target_os = "macos")]
impl CredentialStore for NativeKeychain {
    fn get(&self, service: &str, account: &str) -> Result<Option<Vec<u8>>> {
        use security_framework::passwords::{PasswordOptions, generic_password};
        match generic_password(PasswordOptions::new_generic_password(service, account)) {
            Ok(value) => Ok(Some(value)),
            Err(error) if error.code() == -25300 => Ok(None),
            Err(_) => bail!("macOS Keychain read failed; unlock the login keychain and retry"),
        }
    }
    fn put(&self, service: &str, account: &str, value: &[u8]) -> Result<()> {
        security_framework::passwords::set_generic_password(service, account, value).map_err(|_| {
            anyhow::anyhow!("macOS Keychain write failed; no plaintext fallback is available")
        })
    }
    fn delete(&self, service: &str, account: &str) -> Result<()> {
        match security_framework::passwords::delete_generic_password(service, account) {
            Ok(()) => Ok(()),
            Err(error) if error.code() == -25300 => Ok(()),
            Err(_) => bail!(
                "macOS Keychain deletion failed; retry logout to remove the revoked local credential"
            ),
        }
    }
}

#[cfg(not(any(target_os = "macos", windows)))]
impl CredentialStore for NativeKeychain {
    fn get(&self, _: &str, _: &str) -> Result<Option<Vec<u8>>> {
        bail!(
            "company device credentials require macOS Keychain; this platform has no production credential store or plaintext fallback"
        )
    }
    fn put(&self, _: &str, _: &str, _: &[u8]) -> Result<()> {
        bail!("company device credentials require macOS Keychain")
    }
    fn delete(&self, _: &str, _: &str) -> Result<()> {
        bail!("company device credentials require macOS Keychain")
    }
}

// Intentionally no Debug: credentials and the polling secret must never enter
// CLI diagnostics, logs or process arguments.
#[derive(Clone, Serialize, Deserialize)]
pub(super) struct Enrollment {
    pub(super) company: String,
    pub(super) request_id: Uuid,
    pub(super) device_id: Uuid,
    pub(super) name: String,
    pub(super) platform: String,
    pub(super) credential: String,
    pub(super) poll_secret: String,
    pub(super) approved: bool,
    pub(super) pending_logout: bool,
}
pub(super) fn sha256(value: &str) -> String {
    format!("{:x}", Sha256::digest(value.as_bytes()))
}
fn service(origin: &CompanyOrigin) -> String {
    format!("net.memory-layer.company-device:{}", sha256(&origin.0))
}
fn account(enrollment: &Enrollment) -> String {
    enrollment.device_id.to_string()
}

pub(super) fn load(
    store: &impl CredentialStore,
    origin: &CompanyOrigin,
) -> Result<Option<Enrollment>> {
    let Some(index) = store.get(&service(origin), "active-device")? else {
        return Ok(None);
    };
    let id = std::str::from_utf8(&index)
        .ok()
        .and_then(|index| Uuid::parse_str(index).ok())
        .ok_or_else(|| {
            anyhow::anyhow!("Keychain company device index is invalid; no credential was used")
        })?;
    let value = store
        .get(&service(origin), &id.to_string())?
        .ok_or_else(|| {
            anyhow::anyhow!("Keychain company device record is unavailable; no credential was used")
        })?;
    let record: Enrollment = serde_json::from_slice(&value).map_err(|_| {
        anyhow::anyhow!("Keychain company device record is invalid; no credential was used")
    })?;
    if id.is_nil()
        || record.company != origin.0
        || record.device_id != id
        || record.request_id.is_nil()
        || record.credential.len() != 68
        || !record.credential.starts_with("mlt_")
        || !record.credential.as_bytes()[4..]
            .iter()
            .all(u8::is_ascii_hexdigit)
        || record.poll_secret.len() != 64
        || !record
            .poll_secret
            .bytes()
            .all(|value| value.is_ascii_hexdigit())
    {
        bail!("Keychain company/device binding mismatch; no credential was used");
    }
    Ok(Some(record))
}
pub(super) fn load_or_create(
    store: &impl CredentialStore,
    origin: &CompanyOrigin,
    name: &str,
) -> Result<Enrollment> {
    if let Some(record) = load(store, origin)? {
        if record.name != name {
            bail!(
                "saved enrollment has a different device name; resume with its original --name or resolve auth logout first"
            );
        }
        return Ok(record);
    }
    // Two independent OS-random UUID pairs provide separate opaque secrets.
    let record = Enrollment {
        company: origin.0.clone(),
        request_id: Uuid::new_v4(),
        device_id: Uuid::new_v4(),
        name: name.into(),
        platform: std::env::consts::OS.into(),
        credential: format!("mlt_{}{}", Uuid::new_v4().simple(), Uuid::new_v4().simple()),
        poll_secret: format!("{}{}", Uuid::new_v4().simple(), Uuid::new_v4().simple()),
        approved: false,
        pending_logout: false,
    };
    save(store, origin, &record)?;
    // Both records must persist before the first external start request.
    store.put(
        &service(origin),
        "active-device",
        record.device_id.to_string().as_bytes(),
    )?;
    Ok(record)
}
pub(super) fn save(
    store: &impl CredentialStore,
    origin: &CompanyOrigin,
    record: &Enrollment,
) -> Result<()> {
    if record.company != origin.0 {
        bail!("refusing to store a credential under a different company origin");
    }
    let value = serde_json::to_vec(record)
        .map_err(|_| anyhow::anyhow!("could not serialize the Keychain enrollment record"))?;
    store.put(&service(origin), &account(record), &value)
}
pub(super) fn remove(
    store: &impl CredentialStore,
    origin: &CompanyOrigin,
    record: &Enrollment,
) -> Result<()> {
    // Remove the discoverable pointer first. If the secret deletion fails, a
    // retry must still find the record, so restore the pointer before returning.
    store.delete(&service(origin), "active-device")?;
    if let Err(error) = store.delete(&service(origin), &account(record)) {
        store.put(
            &service(origin),
            "active-device",
            record.device_id.to_string().as_bytes(),
        )?;
        return Err(error);
    }
    Ok(())
}
