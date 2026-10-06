// SPDX-License-Identifier: AGPL-3.0-or-later
//! Local contribution drafts are user content, never access or submission grants.
//! Correction/replacement operations deliberately have no representation here.
use crate::{MemoryType, OnboardingArea, ValidationError};
use chrono::{DateTime, Utc};
use serde::{Deserialize, Serialize};
use uuid::Uuid;

pub const LAPTOP_CONTRIBUTION_SCHEMA_VERSION: u32 = 1;
pub const LAPTOP_CONTRIBUTION_MAX_TEXT_BYTES: usize = 32_768;
pub const LAPTOP_CONTRIBUTION_MAX_SOURCES: usize = 16;

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct LaptopContributionScope {
    pub company_id: Uuid,
    pub area: OnboardingArea,
    pub owner_instance_id: Uuid,
    pub project_id: Uuid,
    pub space_id: Uuid,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct LaptopContributionBinding {
    pub scope: LaptopContributionScope,
    /// Company service origin, distinct from the project owner's origin.
    pub company_instance_id: Uuid,
    pub device_id: Uuid,
    pub manifest_revision: i64,
    pub manifest_sha256: String,
}
impl LaptopContributionBinding {
    pub fn validate(&self) -> Result<(), ValidationError> {
        if [
            self.scope.company_id,
            self.scope.owner_instance_id,
            self.scope.project_id,
            self.scope.space_id,
            self.company_instance_id,
            self.device_id,
        ]
        .iter()
        .any(Uuid::is_nil)
            || self.scope.area == OnboardingArea::Trust
            || self.manifest_revision <= 0
            || !contribution_sha256_valid(&self.manifest_sha256)
        {
            return Err(ValidationError::new("invalid contribution binding"));
        }
        Ok(())
    }
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum LaptopContributionKind {
    RawCapture,
    PendingAddProposal,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum LaptopContributionSourceKind {
    File,
    Command,
    UserNote,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct LaptopContributionSource {
    pub kind: LaptopContributionSourceKind,
    /// Opaque local provenance; never dereferenced by this contract/store.
    pub reference: String,
    pub excerpt: Option<String>,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(tag = "kind", rename_all = "snake_case", deny_unknown_fields)]
pub enum LaptopContributionPayload {
    RawCapture {
        text: String,
        sources: Vec<LaptopContributionSource>,
    },
    PendingAddProposal {
        canonical_text: String,
        summary: String,
        memory_type: MemoryType,
        sources: Vec<LaptopContributionSource>,
    },
}
impl LaptopContributionPayload {
    pub fn kind(&self) -> LaptopContributionKind {
        match self {
            Self::RawCapture { .. } => LaptopContributionKind::RawCapture,
            Self::PendingAddProposal { .. } => LaptopContributionKind::PendingAddProposal,
        }
    }
    pub fn validate(&self) -> Result<(), ValidationError> {
        let (text, summary, sources) = match self {
            Self::RawCapture { text, sources } => (text, None, sources),
            Self::PendingAddProposal {
                canonical_text,
                summary,
                sources,
                ..
            } => (canonical_text, Some(summary), sources),
        };
        if !bounded_text(text, LAPTOP_CONTRIBUTION_MAX_TEXT_BYTES)
            || summary.is_some_and(|s| !bounded_text(s, 2_048))
            || sources.len() > LAPTOP_CONTRIBUTION_MAX_SOURCES
            || sources.iter().any(|s| {
                !bounded_text(&s.reference, 512)
                    || s.excerpt.as_ref().is_some_and(|e| !bounded_text(e, 2_048))
            })
        {
            return Err(ValidationError::new(
                "invalid or oversized contribution payload",
            ));
        }
        Ok(())
    }
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum LaptopContributionTimeBasis {
    UserRecordedDeviceTime,
}

/// A user's device-time assertion, not a verified company/origin timestamp.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct LaptopContributionTime {
    pub recorded_at: DateTime<Utc>,
    pub basis: LaptopContributionTimeBasis,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct CompanyContributionDraft {
    pub schema_version: u32,
    pub operation_id: Uuid,
    pub binding: LaptopContributionBinding,
    /// Canonical JSON SHA-256 of the complete typed payload, not only its text.
    pub payload_sha256: String,
    pub user_time: LaptopContributionTime,
    pub payload: LaptopContributionPayload,
}
impl CompanyContributionDraft {
    pub fn validate(&self) -> Result<(), ValidationError> {
        if self.schema_version != LAPTOP_CONTRIBUTION_SCHEMA_VERSION
            || self.operation_id.is_nil()
            || !contribution_sha256_valid(&self.payload_sha256)
        {
            return Err(ValidationError::new(
                "invalid contribution identity/version/digest",
            ));
        }
        self.binding.validate()?;
        self.payload.validate()
    }
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum LaptopContributionState {
    Draft,
    Queued,
    Uncertain,
    Accepted,
    Blocked,
}
impl LaptopContributionState {
    pub fn as_str(self) -> &'static str {
        match self {
            Self::Draft => "draft",
            Self::Queued => "queued",
            Self::Uncertain => "uncertain",
            Self::Accepted => "accepted",
            Self::Blocked => "blocked",
        }
    }
    pub fn permits(self, next: Self) -> bool {
        matches!(
            (self, next),
            (Self::Draft, Self::Queued | Self::Blocked)
                | (
                    Self::Queued,
                    Self::Uncertain | Self::Accepted | Self::Blocked
                )
                | (
                    Self::Uncertain,
                    Self::Queued | Self::Accepted | Self::Blocked
                )
                | (Self::Blocked, Self::Queued)
        )
    }
}

/// Fixed codes keep payloads, secrets and arbitrary transport errors out of lists.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum LaptopContributionBlockReason {
    ConsentRequired,
    AccessRevoked,
    ManifestChanged,
    OwnerUnavailable,
    Unsupported,
}

/// Historical owner outcome supplied by a trusted future adapter. This record
/// alone neither verifies transport authenticity nor proves current access.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct LaptopContributionReceipt {
    pub schema_version: u32,
    pub operation_id: Uuid,
    pub binding: LaptopContributionBinding,
    pub request_sha256: String,
    pub payload_sha256: String,
    pub kind: LaptopContributionKind,
    /// Raw capture or pending proposal ID, never a curated acceptance claim.
    pub owner_record_id: Uuid,
    pub owner_committed_at: DateTime<Utc>,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct LaptopContributionTransition {
    pub expected_generation: u64,
    pub expected_state: LaptopContributionState,
    pub next_state: LaptopContributionState,
    pub blocked_reason: Option<LaptopContributionBlockReason>,
    pub receipt: Option<LaptopContributionReceipt>,
}

/// Deliberately contains no body, title, summary, path, excerpt or error text.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct LaptopContributionMetadata {
    pub schema_version: u32,
    pub operation_id: Uuid,
    pub binding: LaptopContributionBinding,
    pub request_sha256: String,
    pub payload_sha256: String,
    pub kind: LaptopContributionKind,
    pub user_time: LaptopContributionTime,
    pub stored_at: DateTime<Utc>,
    pub generation: u64,
    pub state: LaptopContributionState,
    pub blocked_reason: Option<LaptopContributionBlockReason>,
    pub receipt: Option<LaptopContributionReceipt>,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct LaptopContributionHistoryEntry {
    pub generation: u64,
    pub state: LaptopContributionState,
    pub blocked_reason: Option<LaptopContributionBlockReason>,
    pub receipt: Option<LaptopContributionReceipt>,
    pub recorded_at: DateTime<Utc>,
}

pub fn contribution_sha256_valid(value: &str) -> bool {
    value.len() == 64
        && value
            .bytes()
            .all(|b| b.is_ascii_digit() || (b'a'..=b'f').contains(&b))
}
fn bounded_text(value: &str, max: usize) -> bool {
    !value.trim().is_empty()
        && value.len() <= max
        && !value
            .chars()
            .any(|c| c.is_control() && !matches!(c, '\n' | '\r' | '\t'))
}

#[cfg(test)]
mod tests {
    use super::*;
    #[test]
    fn payloads_are_bounded_and_corrections_unknown_fields_rejected() {
        let payload = LaptopContributionPayload::RawCapture {
            text: "offline note".into(),
            sources: vec![],
        };
        assert!(payload.validate().is_ok());
        let mut value = serde_json::to_value(&payload).unwrap();
        value["target_memory_id"] = serde_json::json!(Uuid::new_v4());
        assert!(serde_json::from_value::<LaptopContributionPayload>(value).is_err());
        assert!(
            serde_json::from_str::<LaptopContributionPayload>(
                r#"{"kind":"correction","text":"x"}"#
            )
            .is_err()
        );
        assert!(
            LaptopContributionPayload::RawCapture {
                text: "x".repeat(LAPTOP_CONTRIBUTION_MAX_TEXT_BYTES + 1),
                sources: vec![]
            }
            .validate()
            .is_err()
        );
    }
    #[test]
    fn accepted_is_terminal_and_draft_cannot_be_accepted() {
        use LaptopContributionState::*;
        assert!(!Draft.permits(Accepted));
        for state in [Draft, Queued, Uncertain, Accepted, Blocked] {
            assert!(!Accepted.permits(state));
        }
        assert!(Uncertain.permits(Queued));
        assert!(Queued.permits(Accepted));
        assert!(!contribution_sha256_valid(&"A".repeat(64)));
    }
}
