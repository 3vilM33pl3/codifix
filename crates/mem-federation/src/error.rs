// SPDX-License-Identifier: AGPL-3.0-or-later

use thiserror::Error;

#[derive(Debug, Clone, PartialEq, Eq, Error)]
pub enum FederationError {
    #[error("{field} must be non-empty")]
    EmptyIdentifier { field: &'static str },
    #[error("{field} must not contain whitespace or control characters: {value}")]
    MalformedIdentifier { field: &'static str, value: String },
    #[error("{field} must not be the nil UUID")]
    NilUuid { field: &'static str },
    #[error("confidence must be finite, got {value}")]
    NonFiniteConfidence { value: String },
    #[error("memory version must be >= 1, got {value}")]
    InvalidMemoryVersion { value: i32 },
    #[error("digest must be {expected_len} hex chars, got {actual_len}")]
    InvalidDigestLength {
        expected_len: usize,
        actual_len: usize,
    },
    #[error("digest contains non-hex characters: {value}")]
    InvalidDigestHex { value: String },
    #[error("canonical JSON encoding failed: {message}")]
    CanonicalJson { message: String },
    #[error("record digest mismatch: expected {expected}, computed {actual}")]
    DigestMismatch { expected: String, actual: String },
}
