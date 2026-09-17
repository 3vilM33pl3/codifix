//! Pure contracts for the four-area supervisor. No workers, storage, network,
//! clock, provider configuration, or cryptographic verification live here.
//!
//! A runtime must persist decisions atomically, enforce one ledger per area/day,
//! and validate fencing at every effect boundary. Serialized snapshots are
//! inspection contracts, not bearer capabilities or trusted authorization input.

pub mod action;
pub mod budget;
pub mod lease;
pub mod policy;
pub mod proposal;
pub mod upgrade;

use serde::{Deserialize, Serialize};

pub const CONTRACT_VERSION: u16 = 1;
pub const DEFAULT_MODEL: &str = "gpt-5.6-sol";
pub const DAILY_AREA_USD_MICROS: u64 = 2_500_000;
pub const DAILY_AREA_MODEL_CALLS: u32 = 20;

#[derive(Debug, Clone, Copy, PartialEq, Eq, PartialOrd, Ord, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum Area {
    Trust,
    Homelab,
    Workshop,
    General,
}

/// UTC seconds supplied by the trusted runtime; this library never reads a clock.
#[derive(Debug, Clone, Copy, PartialEq, Eq, PartialOrd, Ord, Serialize, Deserialize)]
#[serde(transparent)]
pub struct UnixSeconds(pub u64);

/// UTC accounting day. All four authorities must use this same day boundary.
#[derive(Debug, Clone, Copy, PartialEq, Eq, PartialOrd, Ord, Serialize, Deserialize)]
#[serde(transparent)]
pub struct BudgetDay(pub u64);

impl UnixSeconds {
    pub fn budget_day(self) -> BudgetDay {
        BudgetDay(self.0 / 86_400)
    }
}

/// Exact millionths of one USD; provider charges must be rounded upward to this
/// unit. Zero is valid for a known free result, never as a stand-in for unknown.
#[derive(Debug, Clone, Copy, PartialEq, Eq, PartialOrd, Ord, Serialize, Deserialize)]
#[serde(transparent)]
pub struct UsdMicros(pub u64);

/// Runtime-issued, stable opaque ID. Construction rejects empty/unbounded IDs.
#[derive(Debug, Clone, PartialEq, Eq, PartialOrd, Ord, Serialize, Deserialize)]
#[serde(try_from = "String", into = "String")]
pub struct StableId(String);

impl StableId {
    pub fn new(value: impl Into<String>) -> Result<Self, InvalidId> {
        let value = value.into();
        if value.trim().is_empty() || value.len() > 256 || value.chars().any(char::is_control) {
            return Err(InvalidId);
        }
        Ok(Self(value))
    }

    pub fn as_str(&self) -> &str {
        &self.0
    }
}

impl TryFrom<String> for StableId {
    type Error = InvalidId;

    fn try_from(value: String) -> Result<Self, Self::Error> {
        Self::new(value)
    }
}

impl From<StableId> for String {
    fn from(value: StableId) -> Self {
        value.0
    }
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, thiserror::Error)]
#[error("stable IDs must contain 1 to 256 bytes and no control characters")]
pub struct InvalidId;
