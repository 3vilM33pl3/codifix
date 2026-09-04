// SPDX-License-Identifier: AGPL-3.0-or-later

use std::{fmt, future::Future, pin::Pin, str::FromStr};

use chrono::{DateTime, Utc};
use mem_record::{
    MemoryRelationType, MemorySourceRecord, MemoryStatus, MemoryType, SourceKind,
    SourceProvenanceRecord,
};
use serde::{Deserialize, Serialize};
use uuid::Uuid;

use crate::{CanonicalJsonCodec, ContentDigest, FederationError};

macro_rules! validated_identifier {
    ($name:ident, $field:literal) => {
        #[derive(Debug, Clone, PartialEq, Eq, PartialOrd, Ord, Hash, Serialize, Deserialize)]
        #[serde(try_from = "String", into = "String")]
        pub struct $name(String);

        impl $name {
            pub fn new(value: impl Into<String>) -> Result<Self, FederationError> {
                Ok(Self(validate_identifier($field, &value.into())?))
            }

            pub fn as_str(&self) -> &str {
                self.0.as_str()
            }
        }

        impl AsRef<str> for $name {
            fn as_ref(&self) -> &str {
                self.as_str()
            }
        }

        impl fmt::Display for $name {
            fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
                f.write_str(self.as_str())
            }
        }

        impl FromStr for $name {
            type Err = FederationError;

            fn from_str(value: &str) -> Result<Self, Self::Err> {
                Self::new(value)
            }
        }

        impl TryFrom<String> for $name {
            type Error = FederationError;

            fn try_from(value: String) -> Result<Self, Self::Error> {
                Self::new(value)
            }
        }

        impl TryFrom<&str> for $name {
            type Error = FederationError;

            fn try_from(value: &str) -> Result<Self, Self::Error> {
                Self::new(value)
            }
        }

        impl From<$name> for String {
            fn from(value: $name) -> Self {
                value.0
            }
        }
    };
}

validated_identifier!(ProjectSpaceId, "project_space_id");
validated_identifier!(SourceIdentity, "source_identity");
validated_identifier!(RecordLocator, "record_locator");
validated_identifier!(AuthorIdentity, "author_identity");
validated_identifier!(PullCursor, "pull_cursor");

#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum SpaceCapability {
    Pull,
    Publish,
    ManageMembership,
}

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq)]
pub struct RelatedMemoryRef {
    pub memory_id: RecordLocator,
    pub relation_type: MemoryRelationType,
    pub summary: String,
    pub memory_type: MemoryType,
    pub confidence: f32,
}

impl RelatedMemoryRef {
    pub fn validate(&self) -> Result<(), FederationError> {
        validate_finite_confidence(self.confidence)
    }
}

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq)]
pub struct MemoryVersionRecord {
    pub memory_id: RecordLocator,
    pub canonical_id: RecordLocator,
    pub version_no: u32,
    pub canonical_text: String,
    pub summary: String,
    pub memory_type: MemoryType,
    pub importance: i32,
    pub confidence: f32,
    pub status: MemoryStatus,
    pub tags: Vec<String>,
    #[serde(default)]
    pub evidence_refs: Vec<RecordLocator>,
    #[serde(default)]
    pub related_memories: Vec<RelatedMemoryRef>,
    pub created_at: DateTime<Utc>,
    pub updated_at: DateTime<Utc>,
    pub is_tombstone: bool,
}

impl MemoryVersionRecord {
    pub fn validate(&self) -> Result<(), FederationError> {
        if self.version_no == 0 {
            return Err(FederationError::InvalidMemoryVersion { value: 0 });
        }
        validate_finite_confidence(self.confidence)?;
        for related in &self.related_memories {
            related.validate()?;
        }
        Ok(())
    }
}

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq)]
pub struct CanonicalStateRecord {
    pub canonical_id: RecordLocator,
    pub memory_id: RecordLocator,
    pub version_no: u32,
    pub is_tombstone: bool,
    pub status: MemoryStatus,
    pub confidence: f32,
    pub updated_at: DateTime<Utc>,
}

impl CanonicalStateRecord {
    pub fn validate(&self) -> Result<(), FederationError> {
        if self.version_no == 0 {
            return Err(FederationError::InvalidMemoryVersion { value: 0 });
        }
        validate_finite_confidence(self.confidence)
    }
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct EvidenceRecord {
    pub evidence_id: RecordLocator,
    pub source_id: RecordLocator,
    pub memory_id: RecordLocator,
    pub canonical_id: RecordLocator,
    pub version_no: u32,
    pub source_kind: SourceKind,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub task_id: Option<RecordLocator>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub file_path: Option<String>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub git_commit: Option<String>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub symbol_name: Option<String>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub symbol_kind: Option<String>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub excerpt: Option<String>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub provenance: Option<SourceProvenanceRecord>,
    pub observed_at: DateTime<Utc>,
}

impl EvidenceRecord {
    pub fn from_source(
        evidence_id: RecordLocator,
        memory_id: RecordLocator,
        canonical_id: RecordLocator,
        version_no: u32,
        source: &MemorySourceRecord,
        observed_at: DateTime<Utc>,
    ) -> Result<Self, FederationError> {
        Ok(Self {
            evidence_id,
            source_id: source_record_locator(source.id)?,
            memory_id,
            canonical_id,
            version_no,
            source_kind: source.source_kind.clone(),
            task_id: source.task_id.map(task_record_locator).transpose()?,
            file_path: source.file_path.clone(),
            git_commit: source.git_commit.clone(),
            symbol_name: source.symbol_name.clone(),
            symbol_kind: source.symbol_kind.clone(),
            excerpt: source.excerpt.clone(),
            provenance: source.provenance.clone(),
            observed_at,
        })
    }
}

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq)]
pub struct RelationRecord {
    pub src_canonical_id: RecordLocator,
    pub relation_type: MemoryRelationType,
    pub dst_canonical_id: RecordLocator,
    pub asserted_at: DateTime<Utc>,
}

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq)]
pub struct MembershipRecord {
    pub member: AuthorIdentity,
    pub capability: SpaceCapability,
}

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq)]
pub struct RevocationRecord {
    pub member: AuthorIdentity,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub capability: Option<SpaceCapability>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub reason: Option<String>,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(rename_all = "snake_case", tag = "kind", content = "payload")]
pub enum FederationRecord {
    MemoryVersion(MemoryVersionRecord),
    CanonicalState(CanonicalStateRecord),
    Evidence(EvidenceRecord),
    Relation(RelationRecord),
    Membership(MembershipRecord),
    Revocation(RevocationRecord),
}

impl FederationRecord {
    pub fn validate(&self) -> Result<(), FederationError> {
        match self {
            Self::MemoryVersion(record) => record.validate(),
            Self::CanonicalState(record) => record.validate(),
            Self::Evidence(_) | Self::Relation(_) | Self::Membership(_) | Self::Revocation(_) => {
                Ok(())
            }
        }
    }
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct SharedRecordEnvelope<T> {
    pub schema_version: u32,
    pub record_id: RecordLocator,
    pub project_space_id: ProjectSpaceId,
    pub author: AuthorIdentity,
    pub authored_at: DateTime<Utc>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub causation_id: Option<RecordLocator>,
    pub record: T,
}

impl<T> SharedRecordEnvelope<T> {
    pub fn new(
        record_id: RecordLocator,
        project_space_id: ProjectSpaceId,
        author: AuthorIdentity,
        authored_at: DateTime<Utc>,
        causation_id: Option<RecordLocator>,
        record: T,
    ) -> Self {
        Self {
            schema_version: 1,
            record_id,
            project_space_id,
            author,
            authored_at,
            causation_id,
            record,
        }
    }
}

impl SharedRecordEnvelope<FederationRecord> {
    pub fn record_id(&self) -> &RecordLocator {
        &self.record_id
    }
}

#[derive(Debug, Clone, PartialEq, Eq, Hash, Serialize, Deserialize)]
pub struct DedupKey {
    pub source_identity: SourceIdentity,
    pub record_locator: RecordLocator,
    pub content_digest: ContentDigest,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(try_from = "RawTransportRecord", into = "RawTransportRecord")]
pub struct TransportRecord {
    source_identity: SourceIdentity,
    record_locator: RecordLocator,
    content_digest: ContentDigest,
    envelope: SharedRecordEnvelope<FederationRecord>,
}

impl TransportRecord {
    pub fn new(
        source_identity: SourceIdentity,
        record_locator: RecordLocator,
        envelope: SharedRecordEnvelope<FederationRecord>,
    ) -> Result<Self, FederationError> {
        envelope.record.validate()?;
        let content_digest = CanonicalJsonCodec::digest(&envelope)?;
        Ok(Self {
            source_identity,
            record_locator,
            content_digest,
            envelope,
        })
    }

    pub fn dedup_key(&self) -> DedupKey {
        DedupKey {
            source_identity: self.source_identity.clone(),
            record_locator: self.record_locator.clone(),
            content_digest: self.content_digest.clone(),
        }
    }

    pub fn source_identity(&self) -> &SourceIdentity {
        &self.source_identity
    }

    pub fn record_locator(&self) -> &RecordLocator {
        &self.record_locator
    }

    pub fn content_digest(&self) -> &ContentDigest {
        &self.content_digest
    }

    pub fn envelope(&self) -> &SharedRecordEnvelope<FederationRecord> {
        &self.envelope
    }

    pub fn verify_integrity(&self) -> Result<(), FederationError> {
        let actual = CanonicalJsonCodec::digest(&self.envelope)?;
        if actual == self.content_digest {
            return Ok(());
        }
        Err(FederationError::DigestMismatch {
            expected: self.content_digest.as_hex().to_string(),
            actual: actual.as_hex().to_string(),
        })
    }
}

#[derive(Debug, Clone, Serialize, Deserialize)]
struct RawTransportRecord {
    source_identity: SourceIdentity,
    record_locator: RecordLocator,
    content_digest: ContentDigest,
    envelope: SharedRecordEnvelope<FederationRecord>,
}

impl TryFrom<RawTransportRecord> for TransportRecord {
    type Error = FederationError;

    fn try_from(value: RawTransportRecord) -> Result<Self, Self::Error> {
        let record = Self::new(value.source_identity, value.record_locator, value.envelope)?;
        if record.content_digest != value.content_digest {
            return Err(FederationError::DigestMismatch {
                expected: value.content_digest.as_hex().to_string(),
                actual: record.content_digest.as_hex().to_string(),
            });
        }
        Ok(record)
    }
}

impl From<TransportRecord> for RawTransportRecord {
    fn from(value: TransportRecord) -> Self {
        Self {
            source_identity: value.source_identity,
            record_locator: value.record_locator,
            content_digest: value.content_digest,
            envelope: value.envelope,
        }
    }
}

pub type PublishFuture<'a> = Pin<Box<dyn Future<Output = Result<(), FederationError>> + Send + 'a>>;

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct PullPage {
    pub records: Vec<TransportRecord>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub next_cursor: Option<PullCursor>,
}

pub type PullFuture<'a> =
    Pin<Box<dyn Future<Output = Result<PullPage, FederationError>> + Send + 'a>>;

pub trait ProjectSpaceTransport {
    fn publish(&self, record: TransportRecord) -> PublishFuture<'_>;

    fn pull(&self, space_id: &ProjectSpaceId, cursor: Option<&PullCursor>) -> PullFuture<'_>;
}

pub fn canonical_locator(canonical_id: Uuid) -> Result<RecordLocator, FederationError> {
    validate_uuid("canonical_id", canonical_id)?;
    RecordLocator::new(format!("canonical:{canonical_id}"))
}

pub fn memory_id_locator(memory_id: Uuid) -> Result<RecordLocator, FederationError> {
    validate_uuid("memory_id", memory_id)?;
    RecordLocator::new(format!("memory:{memory_id}"))
}

pub fn memory_version_locator(
    canonical_id: Uuid,
    version_no: i32,
) -> Result<RecordLocator, FederationError> {
    validate_uuid("canonical_id", canonical_id)?;
    let version_no = version_to_u32(version_no)?;
    RecordLocator::new(format!("memory-version:{canonical_id}:{version_no}"))
}

pub fn canonical_state_locator(
    canonical_id: Uuid,
    version_no: i32,
) -> Result<RecordLocator, FederationError> {
    validate_uuid("canonical_id", canonical_id)?;
    let version_no = version_to_u32(version_no)?;
    RecordLocator::new(format!("canonical-state:{canonical_id}:{version_no}"))
}

pub fn source_record_locator(source_id: Uuid) -> Result<RecordLocator, FederationError> {
    validate_uuid("source_id", source_id)?;
    RecordLocator::new(format!("source:{source_id}"))
}

pub fn task_record_locator(task_id: Uuid) -> Result<RecordLocator, FederationError> {
    validate_uuid("task_id", task_id)?;
    RecordLocator::new(format!("task:{task_id}"))
}

pub fn evidence_locator(
    memory_id: Uuid,
    source_id: Uuid,
) -> Result<RecordLocator, FederationError> {
    validate_uuid("memory_id", memory_id)?;
    validate_uuid("source_id", source_id)?;
    RecordLocator::new(format!("evidence:{memory_id}:{source_id}"))
}

pub(crate) fn validate_identifier(
    field: &'static str,
    value: &str,
) -> Result<String, FederationError> {
    let trimmed = value.trim();
    if trimmed.is_empty() {
        return Err(FederationError::EmptyIdentifier { field });
    }
    if trimmed
        .chars()
        .any(|character| character.is_whitespace() || character.is_control())
    {
        return Err(FederationError::MalformedIdentifier {
            field,
            value: value.to_string(),
        });
    }
    Ok(trimmed.to_string())
}

pub(crate) fn validate_finite_confidence(value: f32) -> Result<(), FederationError> {
    if value.is_finite() {
        return Ok(());
    }
    Err(FederationError::NonFiniteConfidence {
        value: value.to_string(),
    })
}

pub(crate) fn validate_uuid(field: &'static str, value: Uuid) -> Result<(), FederationError> {
    if value.is_nil() {
        return Err(FederationError::NilUuid { field });
    }
    Ok(())
}

pub(crate) fn version_to_u32(version_no: i32) -> Result<u32, FederationError> {
    u32::try_from(version_no)
        .map_err(|_| FederationError::InvalidMemoryVersion { value: version_no })
        .and_then(|version| {
            if version == 0 {
                Err(FederationError::InvalidMemoryVersion { value: version_no })
            } else {
                Ok(version)
            }
        })
}

#[cfg(test)]
mod tests {
    use chrono::Utc;

    use super::*;

    #[test]
    fn identifier_deserialize_rejects_whitespace() {
        let error = serde_json::from_str::<ProjectSpaceId>("\"bad id\"")
            .expect_err("whitespace should be rejected");
        assert!(error.to_string().contains("whitespace"));
    }

    #[test]
    fn transport_integrity_detects_tampering() {
        let envelope = SharedRecordEnvelope::new(
            RecordLocator::new("record:1").expect("record id"),
            ProjectSpaceId::new("project.alpha").expect("space id"),
            AuthorIdentity::new("did:example:alice").expect("author"),
            Utc::now(),
            None,
            FederationRecord::Membership(MembershipRecord {
                member: AuthorIdentity::new("did:example:bob").expect("member"),
                capability: SpaceCapability::Publish,
            }),
        );
        let mut record = TransportRecord::new(
            SourceIdentity::new("did:example:source").expect("source"),
            RecordLocator::new("transport:1").expect("locator"),
            envelope,
        )
        .expect("record");

        record.content_digest = ContentDigest::new(
            crate::DigestAlgorithm::Sha256,
            "bbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbb",
        )
        .expect("digest");

        let error = record
            .verify_integrity()
            .expect_err("tampering should be detected");
        match error {
            FederationError::DigestMismatch { .. } => {}
            other => panic!("unexpected error: {other:?}"),
        }
    }
}
