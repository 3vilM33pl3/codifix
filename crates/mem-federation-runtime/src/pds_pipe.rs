// SPDX-License-Identifier: AGPL-3.0-or-later
//! Administrator-bound local PDS transport. No pipe name or principal is taken
//! from a DID, record, participant request or remote discovery response.
use crate::{Result, RuntimeError};
use serde::Deserialize;

#[derive(Clone, Debug, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct PdsNamedPipe {
    pub name: String,
    pub server_sid: String,
    pub client_sid: String,
}

#[cfg(any(windows, test))]
fn environment_bytes(
    bytes: &[u8],
    expected: &str,
) -> Result<std::collections::BTreeMap<String, String>> {
    use sha2::{Digest, Sha256};
    if expected.len() != 64 || format!("{:x}", Sha256::digest(bytes)) != expected {
        return Err(RuntimeError::Configuration);
    }
    let values: std::collections::BTreeMap<String, String> =
        serde_json::from_slice(bytes).map_err(|_| RuntimeError::Configuration)?;
    if values.is_empty()
        || values.len() > 64
        || values.iter().any(|(name, value)| {
            !name.starts_with("PDS_")
                || name.len() > 128
                || !name
                    .bytes()
                    .all(|c| c.is_ascii_uppercase() || c.is_ascii_digit() || c == b'_')
                || value.len() > 8192
                || value.contains('\0')
        })
    {
        return Err(RuntimeError::Configuration);
    }
    Ok(values)
}

impl PdsNamedPipe {
    pub fn validate(&self) -> Result<()> {
        let name = self
            .name
            .strip_prefix(r"\\.\pipe\memory-pds-")
            .ok_or(RuntimeError::Configuration)?;
        if name.is_empty()
            || name.len() > 96
            || !name.bytes().all(|b| b.is_ascii_alphanumeric() || b == b'-')
        {
            return Err(RuntimeError::Configuration);
        }
        for sid in [&self.server_sid, &self.client_sid] {
            // Explicit user/service SIDs only; never Everyone, Users, Guests,
            // Authenticated Users or a caller-selected arbitrary group.
            let (tail, expected) = if let Some(tail) = sid.strip_prefix("S-1-5-21-") {
                (tail, 4)
            } else if let Some(tail) = sid.strip_prefix("S-1-5-80-") {
                (tail, 5)
            } else {
                return Err(RuntimeError::Configuration);
            };
            let parts: Vec<_> = tail.split('-').collect();
            if parts.len() != expected
                || parts.iter().any(|value| {
                    value
                        .parse::<u32>()
                        .ok()
                        .is_none_or(|number| number.to_string() != *value)
                })
            {
                return Err(RuntimeError::Configuration);
            }
        }
        Ok(())
    }
}

#[cfg(windows)]
mod native;
#[cfg(windows)]
mod server;
#[cfg(windows)]
pub use server::serve;
#[cfg(windows)]
pub use server::{PdsPipeStatus, check_status};
#[cfg(any(windows, test))]
mod wire;
#[cfg(windows)]
pub(crate) use native::send;

#[cfg(test)]
mod tests {
    use super::*;
    #[test]
    fn private_environment_binds_exact_bytes_and_rejects_non_pds_or_unbounded_values() {
        use sha2::{Digest, Sha256};
        let bytes = br#"{"PDS_ADMIN_PASSWORD":"synthetic-only"}"#;
        let digest = format!("{:x}", Sha256::digest(bytes));
        assert_eq!(
            environment_bytes(bytes, &digest).expect("bound environment")["PDS_ADMIN_PASSWORD"],
            "synthetic-only"
        );
        assert!(environment_bytes(bytes, &"0".repeat(64)).is_err());
        for value in [
            serde_json::json!({"OPENAI_API_KEY":"never"}),
            serde_json::json!({"pds_lowercase":"no"}),
            serde_json::json!({"PDS_ADMIN_PASSWORD":"a".repeat(8193)}),
            serde_json::json!({"PDS_BAD":"nul\0value"}),
            serde_json::json!({}),
        ] {
            let bytes = serde_json::to_vec(&value).expect("fixture");
            assert!(environment_bytes(&bytes, &format!("{:x}", Sha256::digest(&bytes))).is_err());
        }
    }
    fn config() -> PdsNamedPipe {
        PdsNamedPipe {
            name: r"\\.\pipe\memory-pds-company-1".into(),
            server_sid: "S-1-5-80-1-2-3-4-5".into(),
            client_sid: "S-1-5-21-1-2-3-1001".into(),
        }
    }
    #[test]
    fn local_pipe_names_and_explicit_principals_only() {
        config().validate().expect("synthetic test fixture");
        for name in [
            r"\\remote\pipe\memory-pds-a",
            r"\\.\pipe\arbitrary",
            r"\\.\pipe\memory-pds-",
            r"\\.\pipe\memory-pds-a\other",
            r"\\.\pipe\memory-pds-a:b",
        ] {
            let mut value = config();
            value.name = name.into();
            assert!(value.validate().is_err());
        }
        for sid in [
            "S-1-1-0",
            "S-1-5-11",
            "S-1-5-32-545",
            "S-1-5-21-1;bad",
            "S-1-5-80-",
            "S-1-5-21-1--3-1001",
            "S-1-5-21-01-2-3-1001",
            "S-1-5-80-1-2-3-4-4294967296",
        ] {
            let mut value = config();
            value.client_sid = sid.into();
            assert!(value.validate().is_err());
        }
    }
}
