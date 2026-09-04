// SPDX-License-Identifier: AGPL-3.0-or-later

use serde::{Deserialize, Serialize};
use sha2::{Digest, Sha256};

use crate::FederationError;

const SHA256_HEX_LEN: usize = 64;

#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum DigestAlgorithm {
    Sha256,
}

#[derive(Debug, Clone, PartialEq, Eq, Hash, Serialize, Deserialize)]
#[serde(try_from = "RawContentDigest", into = "RawContentDigest")]
pub struct ContentDigest {
    algorithm: DigestAlgorithm,
    hex: String,
}

impl ContentDigest {
    pub fn new(
        algorithm: DigestAlgorithm,
        hex: impl Into<String>,
    ) -> Result<Self, FederationError> {
        let hex = hex.into().to_ascii_lowercase();
        let expected_len = match algorithm {
            DigestAlgorithm::Sha256 => SHA256_HEX_LEN,
        };
        if hex.len() != expected_len {
            return Err(FederationError::InvalidDigestLength {
                expected_len,
                actual_len: hex.len(),
            });
        }
        if !hex.bytes().all(|byte| byte.is_ascii_hexdigit()) {
            return Err(FederationError::InvalidDigestHex { value: hex });
        }
        Ok(Self { algorithm, hex })
    }

    pub fn sha256_bytes(bytes: &[u8]) -> Self {
        let hex = encode_hex(&Sha256::digest(bytes));
        Self {
            algorithm: DigestAlgorithm::Sha256,
            hex,
        }
    }

    pub fn as_hex(&self) -> &str {
        self.hex.as_str()
    }

    pub fn algorithm(&self) -> DigestAlgorithm {
        self.algorithm
    }
}

#[derive(Debug, Clone, Serialize, Deserialize)]
struct RawContentDigest {
    algorithm: DigestAlgorithm,
    hex: String,
}

impl TryFrom<RawContentDigest> for ContentDigest {
    type Error = FederationError;

    fn try_from(value: RawContentDigest) -> Result<Self, Self::Error> {
        Self::new(value.algorithm, value.hex)
    }
}

impl From<ContentDigest> for RawContentDigest {
    fn from(value: ContentDigest) -> Self {
        Self {
            algorithm: value.algorithm,
            hex: value.hex,
        }
    }
}

fn encode_hex(bytes: &[u8]) -> String {
    const HEX: &[u8; 16] = b"0123456789abcdef";
    let mut output = String::with_capacity(bytes.len() * 2);
    for byte in bytes {
        output.push(char::from(HEX[(byte >> 4) as usize]));
        output.push(char::from(HEX[(byte & 0x0f) as usize]));
    }
    output
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn deserialize_rejects_invalid_hex_length() {
        let error = serde_json::from_str::<ContentDigest>(r#"{"algorithm":"sha256","hex":"abcd"}"#)
            .expect_err("invalid digest length should be rejected");
        assert!(error.to_string().contains("64 hex chars"));
    }

    #[test]
    fn constructor_normalizes_uppercase_hex() {
        let digest = ContentDigest::new(
            DigestAlgorithm::Sha256,
            "AAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAA",
        )
        .expect("digest should normalize");
        assert_eq!(
            digest.as_hex(),
            "aaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaa"
        );
    }
}
