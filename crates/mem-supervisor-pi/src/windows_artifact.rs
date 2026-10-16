// SPDX-License-Identifier: AGPL-3.0-or-later
//! Reviewed native-build provenance. This declares a separately built artifact,
//! never a replacement for the unchanged upstream/Linux release pins.
use crate::{Failure, PI_SOURCE_REVISION};
use serde::{Deserialize, Serialize};
#[cfg(test)]
use sha2::{Digest, Sha256};

pub const NO_TOOLS_PATCH_SHA256: &str =
    "bec724b3d1cef1df4f4cddc50573ce6af1724c743496e7f5e900f4e4a6c815bc";
pub const WINDOWS_BROKER_PATCH_SHA256: &str =
    "61a620316d545b100cf17a83503aba4da46094346c7996c502508b9060aebdf4";

#[derive(Clone, Deserialize, Serialize)]
#[serde(deny_unknown_fields)]
pub struct WindowsArtifact {
    pub source_revision: String,
    pub no_tools_patch_sha256: String,
    pub broker_patch_sha256: String,
    pub binary_sha256: String,
}
impl WindowsArtifact {
    pub fn validate(&self) -> Result<[u8; 32], Failure> {
        if self.source_revision != PI_SOURCE_REVISION
            || self.no_tools_patch_sha256 != NO_TOOLS_PATCH_SHA256
            || self.broker_patch_sha256 != WINDOWS_BROKER_PATCH_SHA256
            || self.binary_sha256.len() != 64
            || !self
                .binary_sha256
                .bytes()
                .all(|c| c.is_ascii_digit() || (b'a'..=b'f').contains(&c))
        {
            return Err(Failure::InvalidConfiguration);
        }
        let mut digest = [0; 32];
        for (index, byte) in digest.iter_mut().enumerate() {
            *byte = u8::from_str_radix(&self.binary_sha256[index * 2..index * 2 + 2], 16)
                .map_err(|_| Failure::InvalidConfiguration)?;
        }
        Ok(digest)
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    #[test]
    fn native_provenance_cannot_change_reviewed_source_or_patch() {
        assert_eq!(
            format!(
                "{:x}",
                Sha256::digest(include_bytes!("../patches/windows-broker-1.patch"))
            ),
            WINDOWS_BROKER_PATCH_SHA256
        );
        let mut value = WindowsArtifact {
            source_revision: PI_SOURCE_REVISION.into(),
            no_tools_patch_sha256: NO_TOOLS_PATCH_SHA256.into(),
            broker_patch_sha256: format!(
                "{:x}",
                Sha256::digest(include_bytes!("../patches/windows-broker-1.patch"))
            ),
            binary_sha256: "0".repeat(64),
        };
        assert_eq!(
            value.validate().expect("synthetic declared digest"),
            [0; 32]
        );
        value.source_revision = "latest".into();
        assert!(value.validate().is_err());
        value.source_revision = PI_SOURCE_REVISION.into();
        value.broker_patch_sha256 = "0".repeat(64);
        assert!(value.validate().is_err());
    }
}
