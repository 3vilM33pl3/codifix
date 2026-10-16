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
#[cfg(any(windows, test))]
mod wire;
#[cfg(windows)]
pub(crate) use native::send;

#[cfg(test)]
mod tests {
    use super::*;
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
