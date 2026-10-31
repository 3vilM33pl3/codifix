//! Pure specialist-agency contracts. This crate never grants authority, reads a
//! secret, sends a request, or executes a task. The runtime must authenticate
//! issuers, load current ancestor mandates, and reserve shared budgets atomically.
pub mod a2a;
pub mod discovery;
pub mod evaluation;
pub mod improvement;
pub mod mandate;
pub mod routing;
pub mod task;

pub use mandate::*;
pub use task::*;

use serde::{Deserialize, Serialize};
use std::collections::BTreeSet;
use uuid::Uuid;

pub const SCHEMA_VERSION: u32 = 1;

#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct SpecialistManifest {
    pub schema_version: u32,
    pub instance_id: Uuid,
    pub specialist_id: String,
    pub name: String,
    pub capabilities: BTreeSet<String>,
}

impl SpecialistManifest {
    pub fn validate(&self) -> Result<(), PolicyError> {
        if self.schema_version != SCHEMA_VERSION
            || self.instance_id.is_nil()
            || !valid_identifier(&self.specialist_id)
            || self.name.trim().is_empty()
            || self.name.len() > 200
            || self.capabilities.is_empty()
            || self.capabilities.iter().any(|s| !valid_identifier(s))
        {
            return Err(PolicyError::Invalid("specialist manifest"));
        }
        Ok(())
    }
}

pub(crate) fn valid_identifier(value: &str) -> bool {
    !value.is_empty()
        && value.len() <= 128
        && value
            .bytes()
            .all(|c| c.is_ascii_alphanumeric() || matches!(c, b'-' | b'_' | b'.' | b':'))
}

#[derive(Clone, Debug, PartialEq, Eq, thiserror::Error)]
pub enum PolicyError {
    #[error("invalid {0}")]
    Invalid(&'static str),
    #[error("mandate is {0}")]
    Inactive(&'static str),
    #[error("scope denied: {0}")]
    Scope(&'static str),
    #[error("budget exhausted: {0}")]
    Budget(&'static str),
    #[error("invalid lifecycle transition")]
    Transition,
    #[error("no evaluated model satisfies the policy")]
    NoEligibleModel,
}
