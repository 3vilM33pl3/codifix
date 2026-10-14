// SPDX-License-Identifier: AGPL-3.0-or-later
//! Curated device contributions. Version one raw drafts remain local-only.
use crate::{LaptopContributionBinding, MemoryType, ValidationError};
use chrono::{DateTime, Utc};
use serde::{Deserialize, Serialize};
use uuid::Uuid;

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct CuratedDeviceContent {
    pub canonical_text: String,
    pub summary: String,
    pub memory_type: MemoryType,
    pub importance: i32,
    pub confidence: f32,
}
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
#[serde(tag = "kind", rename_all = "snake_case", deny_unknown_fields)]
pub enum CuratedDeviceChange {
    Add {
        content: CuratedDeviceContent,
    },
    Correct {
        target_canonical_id: Uuid,
        expected_version: i32,
        content: CuratedDeviceContent,
    },
}
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct CuratedDeviceContribution {
    pub schema_version: u32,
    pub operation_id: Uuid,
    pub binding: LaptopContributionBinding,
    pub local_memory_id: Uuid,
    pub authored_at: DateTime<Utc>,
    pub change: CuratedDeviceChange,
}
impl CuratedDeviceContribution {
    pub fn validate(&self) -> Result<(), ValidationError> {
        self.binding.validate()?;
        let content = match &self.change {
            CuratedDeviceChange::Add { content } => content,
            CuratedDeviceChange::Correct {
                target_canonical_id,
                expected_version,
                content,
            } => {
                if target_canonical_id.is_nil() || *expected_version < 1 {
                    return Err(ValidationError::new("invalid correction version"));
                }
                content
            }
        };
        if self.schema_version != 2
            || self.operation_id.is_nil()
            || self.local_memory_id.is_nil()
            || content.canonical_text.trim().is_empty()
            || content.canonical_text.len() > 32768
            || content.summary.trim().is_empty()
            || content.summary.len() > 2048
            || !(1..=5).contains(&content.importance)
            || !content.confidence.is_finite()
            || !(0.0..=1.0).contains(&content.confidence)
            || [&content.canonical_text, &content.summary].iter().any(|s| {
                s.chars()
                    .any(|c| c.is_control() && !matches!(c, '\n' | '\r' | '\t'))
            })
        {
            return Err(ValidationError::new("invalid curated device contribution"));
        }
        Ok(())
    }
}
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct OwnerDeviceContributionRequest {
    pub approval_id: Uuid,
    pub contribution: CuratedDeviceContribution,
}
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct OwnerDeviceContributionStatusRequest {
    pub approval_id: Uuid,
    pub operation_id: Uuid,
}
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum DeviceContributionOutcome {
    Accepted,
    Proposed,
    Conflict,
}
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct DeviceContributionReceipt {
    pub schema_version: u32,
    pub operation_id: Uuid,
    pub binding: LaptopContributionBinding,
    pub local_memory_id: Uuid,
    /// Canonical SHA-256 of the complete CuratedDeviceContribution. The owner
    /// additionally retains the exact approval identity in its receipt journal.
    pub request_sha256: String,
    pub human_principal_id: Uuid,
    pub outcome: DeviceContributionOutcome,
    pub owner_memory_id: Option<Uuid>,
    pub owner_canonical_id: Option<Uuid>,
    pub owner_version: Option<i32>,
    pub proposal_id: Option<Uuid>,
    pub committed_at: DateTime<Utc>,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum DeviceContributionDecision {
    Approved,
    Rejected,
}
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct DeviceContributionResolution {
    pub operation_id: Uuid,
    pub proposal_id: Uuid,
    pub request_sha256: String,
    pub decision: DeviceContributionDecision,
    pub owner_memory_id: Option<Uuid>,
    pub owner_canonical_id: Option<Uuid>,
    pub owner_version: Option<i32>,
    pub reviewer: String,
    pub decided_at: DateTime<Utc>,
}
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct DeviceContributionStatus {
    /// Original immutable submission result, even after a proposal decision.
    pub receipt: DeviceContributionReceipt,
    pub resolution: Option<DeviceContributionResolution>,
}

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct LaptopCorrectionRequest {
    pub operation_id: Uuid,
    pub source_memory_id: Uuid,
    /// SHA-256 of the exact downloaded DeviceReplicaRecord, without a prefix.
    pub expected_digest: String,
    pub content: CuratedDeviceContent,
}
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct LaptopCorrectionReceipt {
    pub operation_id: Uuid,
    pub subscription_id: Uuid,
    pub local_memory_id: Uuid,
    pub local_canonical_id: Uuid,
    pub owner_canonical_id: Uuid,
    pub expected_version: i32,
    pub request_sha256: String,
    pub staged_at: DateTime<Utc>,
}
