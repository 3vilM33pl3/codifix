// SPDX-License-Identifier: AGPL-3.0-or-later
//! Origin-owned publication domain. This is not an authentication or storage
//! layer. The legacy spike schema is intentionally not silently reinterpreted.

mod state;
pub use state::*;
mod policy;
pub use policy::*;

use std::num::NonZeroU32;

use chrono::{DateTime, Utc};
use mem_record::{MemoryRelationType, MemoryStatus, MemoryType};
use serde::{Deserialize, Serialize};
use thiserror::Error;
use uuid::Uuid;

use crate::{
    AuthorIdentity, CanonicalJsonCodec, ContentDigest, FederationError, RecordLocator,
    SpaceCapability,
};

pub const PUBLICATION_SCHEMA_VERSION: u32 = 2;

#[derive(Debug, Clone, PartialEq, Eq, Error)]
pub enum PublicationError {
    #[error("identifier must be a non-nil UUID")]
    NilIdentity,
    #[error("counter must be positive")]
    ZeroCounter,
    #[error("confidence must be finite and between zero and one")]
    InvalidConfidence,
    #[error("reviewed snapshot does not match its review digest")]
    ReviewMismatch,
    #[error(transparent)]
    Encoding(#[from] FederationError),
}

macro_rules! identity {
    ($name:ident) => {
        #[derive(
            Debug, Clone, Copy, PartialEq, Eq, PartialOrd, Ord, Hash, Serialize, Deserialize,
        )]
        #[serde(try_from = "Uuid", into = "Uuid")]
        pub struct $name(Uuid);
        impl $name {
            pub fn new(id: Uuid) -> Result<Self, PublicationError> {
                if id.is_nil() {
                    Err(PublicationError::NilIdentity)
                } else {
                    Ok(Self(id))
                }
            }
            pub fn as_uuid(self) -> Uuid {
                self.0
            }
        }
        impl TryFrom<Uuid> for $name {
            type Error = PublicationError;
            fn try_from(id: Uuid) -> Result<Self, Self::Error> {
                Self::new(id)
            }
        }
        impl From<$name> for Uuid {
            fn from(id: $name) -> Self {
                id.0
            }
        }
    };
}
identity!(InstanceId);
identity!(SpaceId);
identity!(RecordId);

macro_rules! counter {
    ($name:ident) => {
        #[derive(Debug, Clone, Copy, PartialEq, Eq, PartialOrd, Ord, Serialize, Deserialize)]
        #[serde(transparent)]
        pub struct $name(NonZeroU32);
        impl $name {
            pub fn new(value: u32) -> Result<Self, PublicationError> {
                NonZeroU32::new(value)
                    .map(Self)
                    .ok_or(PublicationError::ZeroCounter)
            }
            pub fn get(self) -> u32 {
                self.0.get()
            }
        }
    };
}
counter!(SharedVersion);
counter!(SourceVersion);
counter!(StateRevision);
counter!(AuthoritySequence);

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(try_from = "u32", into = "u32")]
pub struct ConfidencePpm(u32);

impl ConfidencePpm {
    pub fn from_confidence(value: f32) -> Result<Self, PublicationError> {
        if !value.is_finite() || !(0.0..=1.0).contains(&value) {
            return Err(PublicationError::InvalidConfidence);
        }
        Self::try_from((f64::from(value) * 1_000_000.0).round() as u32)
    }
    pub fn get(self) -> u32 {
        self.0
    }
    pub fn as_confidence(self) -> f32 {
        self.0 as f32 / 1_000_000.0
    }
}
impl TryFrom<u32> for ConfidencePpm {
    type Error = PublicationError;
    fn try_from(value: u32) -> Result<Self, Self::Error> {
        if value <= 1_000_000 {
            Ok(Self(value))
        } else {
            Err(PublicationError::InvalidConfidence)
        }
    }
}
impl From<ConfidencePpm> for u32 {
    fn from(value: ConfidencePpm) -> Self {
        value.0
    }
}

/// Identical canonical UUIDs at two independent origins are different streams.
#[derive(Debug, Clone, Copy, PartialEq, Eq, PartialOrd, Ord, Hash, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct StreamId {
    pub origin: InstanceId,
    pub canonical: RecordId,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, PartialOrd, Ord, Hash, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct RecordKey {
    pub publisher: InstanceId,
    pub id: RecordId,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct SourceVersionRef {
    pub record_id: RecordId,
    pub version: SourceVersion,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct MemoryContent {
    pub text: String,
    pub summary: String,
    pub memory_type: MemoryType,
    pub confidence: ConfidencePpm,
    pub importance: i32,
    pub tags: Vec<String>,
}

/// Only explicitly selected evidence. No automatic conversion from a local
/// source/capture/session object exists. The label is public, reviewed text.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct EvidenceSnapshot {
    pub space: SpaceId,
    pub stream: StreamId,
    pub public_source_label: String,
    pub selected_text: String,
    pub observed_at: DateTime<Utc>,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct ReviewReceipt {
    pub snapshot_digest: ContentDigest,
    pub reviewer: AuthorIdentity,
    pub reviewed_at: DateTime<Utc>,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(try_from = "RawReviewedEvidence", into = "RawReviewedEvidence")]
pub struct ReviewedEvidence {
    snapshot: EvidenceSnapshot,
    review: ReviewReceipt,
}

impl ReviewedEvidence {
    /// Bind a separately established review to exact canonical snapshot bytes.
    /// The service must authenticate/persist the review; hash agreement alone
    /// neither authenticates the reviewer nor proves that a human reviewed it.
    pub fn new(
        snapshot: EvidenceSnapshot,
        review: ReviewReceipt,
    ) -> Result<Self, PublicationError> {
        if CanonicalJsonCodec::digest(&snapshot)? != review.snapshot_digest {
            return Err(PublicationError::ReviewMismatch);
        }
        Ok(Self { snapshot, review })
    }
    pub fn snapshot(&self) -> &EvidenceSnapshot {
        &self.snapshot
    }
    pub fn review(&self) -> &ReviewReceipt {
        &self.review
    }
    pub fn published_snapshot_bytes(&self) -> Result<Vec<u8>, PublicationError> {
        Ok(CanonicalJsonCodec::encode(&self.snapshot)?)
    }
}
#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
struct RawReviewedEvidence {
    snapshot: EvidenceSnapshot,
    review: ReviewReceipt,
}
impl TryFrom<RawReviewedEvidence> for ReviewedEvidence {
    type Error = PublicationError;
    fn try_from(value: RawReviewedEvidence) -> Result<Self, Self::Error> {
        Self::new(value.snapshot, value.review)
    }
}
impl From<ReviewedEvidence> for RawReviewedEvidence {
    fn from(value: ReviewedEvidence) -> Self {
        Self {
            snapshot: value.snapshot,
            review: value.review,
        }
    }
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct MemoryPublication {
    pub version: SharedVersion,
    pub source: SourceVersionRef,
    pub content: MemoryContent,
    pub evidence: Vec<RecordKey>,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct StatePublication {
    pub revision: StateRevision,
    pub content_version: SharedVersion,
    pub status: MemoryStatus,
    pub confidence: ConfidencePpm,
    pub tombstone: bool,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct RelationPublication {
    pub id: RecordId,
    pub source: StreamId,
    pub target: StreamId,
    pub relation_type: MemoryRelationType,
    pub revision: StateRevision,
    pub active: bool,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct CorrectionProposal {
    pub target: StreamId,
    pub base_version: SharedVersion,
    pub content: MemoryContent,
    pub evidence: Vec<RecordKey>,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(
    tag = "kind",
    content = "payload",
    rename_all = "snake_case",
    deny_unknown_fields
)]
pub enum PublicationRecord {
    Expertise(crate::expertise::CapabilityAdvertisement),
    Membership {
        member: InstanceId,
        capability: SpaceCapability,
    },
    Revocation {
        member: InstanceId,
        capability: Option<SpaceCapability>,
    },
    Evidence(ReviewedEvidence),
    MemoryVersion {
        stream: StreamId,
        publication: MemoryPublication,
    },
    CanonicalState {
        stream: StreamId,
        state: StatePublication,
    },
    Relation(RelationPublication),
    Proposal(CorrectionProposal),
    /// `None` rejects; `Some` accepts exactly the proposal with origin-owned
    /// local provenance. Application and proposal resolution are atomic.
    ResolveProposal {
        proposal: RecordKey,
        source: Option<SourceVersionRef>,
    },
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct PublicationEnvelope {
    pub schema_version: u32,
    pub space: SpaceId,
    pub publisher: InstanceId,
    pub record_id: RecordId,
    pub author: AuthorIdentity,
    pub authored_at: DateTime<Utc>,
    pub causation: Option<RecordKey>,
    pub record: PublicationRecord,
}
impl PublicationEnvelope {
    /// V2 bytes retain their original meaning. New capabilities and semantic
    /// kinds are valid only under the explicit V3 contract.
    pub fn supported_schema(&self) -> bool {
        let expertise = matches!(
            &self.record,
            PublicationRecord::Expertise(_)
                | PublicationRecord::Membership {
                    capability: SpaceCapability::AskExpertise | SpaceCapability::AnswerExpertise,
                    ..
                }
                | PublicationRecord::Revocation {
                    capability: Some(
                        SpaceCapability::AskExpertise | SpaceCapability::AnswerExpertise
                    ),
                    ..
                }
        );
        self.schema_version == crate::expertise::EXPERTISE_SCHEMA_VERSION
            || (self.schema_version == PUBLICATION_SCHEMA_VERSION && !expertise)
    }
    pub fn key(&self) -> RecordKey {
        RecordKey {
            publisher: self.publisher,
            id: self.record_id,
        }
    }
}

/// Untrusted received metadata; only the caller-supplied authenticated identity
/// at `apply` is trusted. A digest is integrity, not authentication.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct PublicationDelivery {
    pub sequence: AuthoritySequence,
    pub locator: RecordLocator,
    pub digest: ContentDigest,
    pub envelope: PublicationEnvelope,
}
impl PublicationDelivery {
    pub fn new(
        sequence: AuthoritySequence,
        locator: RecordLocator,
        envelope: PublicationEnvelope,
    ) -> Result<Self, PublicationError> {
        let digest = CanonicalJsonCodec::digest(&envelope)?;
        Ok(Self {
            sequence,
            locator,
            digest,
            envelope,
        })
    }
}
