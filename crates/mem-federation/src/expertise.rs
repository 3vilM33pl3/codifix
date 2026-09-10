// SPDX-License-Identifier: AGPL-3.0-or-later
//! Private expertise contracts. These types establish neither authentication
//! nor human review. Only advertisements may enter the publication log.
use crate::{AuthorIdentity, CanonicalJsonCodec, ContentDigest, SpaceCapability, publication::*};
use chrono::{DateTime, Utc};
use serde::{Deserialize, Serialize};
use std::collections::BTreeSet;
use uuid::Uuid;

pub const EXPERTISE_SCHEMA_VERSION: u32 = 3;
pub const MAX_QUESTION_BYTES: usize = 4096;
pub const MAX_CONTEXT_BYTES: usize = 32 * 1024;
pub const MAX_ANSWER_BYTES: usize = 64 * 1024;
pub const MAX_CITATIONS: usize = 8;
pub const MAX_COLLECTION_ENTRIES: usize = 256;
pub const MAX_COLLECTION_BYTES: usize = 1024 * 1024;

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum AnswerMode {
    Deterministic,
    Model,
}

/// Reviewed metadata only; URLs, discovery instructions and credentials are
/// not part of this contract. Provider/destination names are opaque local IDs.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct CapabilityAdvertisement {
    pub owner: InstanceId,
    pub collection: RecordId,
    pub revision: StateRevision,
    pub collection_digest: ContentDigest,
    pub name: String,
    pub description: String,
    pub topics: Vec<String>,
    pub modes: Vec<AnswerMode>,
    pub requesters: Vec<InstanceId>,
    pub model_destinations: Vec<String>,
    pub output_destinations: Vec<String>,
    pub active: bool,
    pub limits: ExpertiseLimits,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct ExpertiseLimits {
    pub question_bytes: u32,
    pub context_bytes: u32,
    pub answer_bytes: u32,
    pub citations: u32,
    pub deadline_seconds: u32,
}
impl Default for ExpertiseLimits {
    fn default() -> Self {
        Self {
            question_bytes: MAX_QUESTION_BYTES as u32,
            context_bytes: MAX_CONTEXT_BYTES as u32,
            answer_bytes: MAX_ANSWER_BYTES as u32,
            citations: MAX_CITATIONS as u32,
            deadline_seconds: 120,
        }
    }
}
impl ExpertiseLimits {
    pub fn validate(&self) -> Result<(), ExpertiseReject> {
        let ceiling = Self::default();
        if [
            (self.question_bytes, ceiling.question_bytes),
            (self.context_bytes, ceiling.context_bytes),
            (self.answer_bytes, ceiling.answer_bytes),
            (self.citations, ceiling.citations),
            (self.deadline_seconds, ceiling.deadline_seconds),
        ]
        .into_iter()
        .any(|(n, max)| n == 0 || n > max)
        {
            return Err(ExpertiseReject::Limits);
        }
        Ok(())
    }
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, thiserror::Error, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum ExpertiseReject {
    #[error("expertise limits exceeded or invalid input")]
    Limits,
    #[error("expertise permission denied")]
    Unauthorized,
    #[error("collection revision is no longer current")]
    StaleCollection,
    #[error("question expired or invalid lifetime")]
    Expired,
    #[error("immutable expertise binding mismatch")]
    Binding,
    #[error("model or onward destination not permitted")]
    DestinationDenied,
    #[error("answer citation not in selected reviewed evidence")]
    InvalidCitation,
}

pub fn valid_destination(value: &str) -> bool {
    !value.is_empty()
        && value.len() <= 64
        && value
            .bytes()
            .all(|c| c.is_ascii_alphanumeric() || b"_-".contains(&c))
}
fn distinct<T: PartialEq>(values: &[T], max: usize) -> bool {
    values.len() <= max
        && values
            .iter()
            .enumerate()
            .all(|(i, v)| !values[..i].contains(v))
}
impl CapabilityAdvertisement {
    pub fn key(&self) -> RecordKey {
        RecordKey {
            publisher: self.owner,
            id: self.collection,
        }
    }
    pub fn validate(&self) -> Result<(), ExpertiseReject> {
        self.limits.validate()?;
        if self.name.trim().is_empty()
            || self.name.len() > 128
            || self.description.len() > 2048
            || !distinct(&self.topics, 16)
            || self
                .topics
                .iter()
                .any(|s| s.trim().is_empty() || s.len() > 64)
            || self.modes.is_empty()
            || !distinct(&self.modes, 2)
            || !distinct(&self.requesters, 64)
            || !distinct(&self.model_destinations, 16)
            || !distinct(&self.output_destinations, 16)
            || self.output_destinations.is_empty()
            || self
                .model_destinations
                .iter()
                .chain(&self.output_destinations)
                .any(|v| !valid_destination(v))
            || (self.modes.contains(&AnswerMode::Model) && self.model_destinations.is_empty())
        {
            return Err(ExpertiseReject::Limits);
        }
        Ok(())
    }
}

/// Stored only at the origin. Includes exactly what the reviewer sees, never
/// general QuerySource/QueryResult objects or implicit related-source expansion.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct CollectionEntry {
    pub canonical: RecordId,
    pub source: SourceVersionRef,
    pub author: AuthorIdentity,
    pub authored_at: DateTime<Utc>,
    pub content: MemoryContent,
    pub evidence: Vec<ReviewedEvidence>,
}
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct CollectionSnapshot {
    pub space: SpaceId,
    pub owner: InstanceId,
    pub collection: RecordId,
    pub revision: StateRevision,
    pub entries: Vec<CollectionEntry>,
}
impl CollectionSnapshot {
    pub fn digest(&self) -> Result<ContentDigest, ExpertiseReject> {
        self.validate()?;
        CanonicalJsonCodec::digest(self).map_err(|_| ExpertiseReject::Binding)
    }
    pub fn validate(&self) -> Result<(), ExpertiseReject> {
        let mut canonical = BTreeSet::new();
        if self.entries.is_empty() || self.entries.len() > MAX_COLLECTION_ENTRIES {
            return Err(ExpertiseReject::Limits);
        }
        for entry in &self.entries {
            if !canonical.insert(entry.canonical) || entry.evidence.len() > 16 {
                return Err(ExpertiseReject::Limits);
            }
            for evidence in &entry.evidence {
                let snapshot = evidence.snapshot();
                if snapshot.space != self.space
                    || snapshot.stream.origin != self.owner
                    || snapshot.stream.canonical != entry.canonical
                {
                    return Err(ExpertiseReject::Binding);
                }
            }
        }
        if CanonicalJsonCodec::encode(self)
            .map_err(|_| ExpertiseReject::Binding)?
            .len()
            > MAX_COLLECTION_BYTES
        {
            return Err(ExpertiseReject::Limits);
        }
        Ok(())
    }
}

/// Requester identity is separately authenticated by the gateway. Local
/// principal attribution is asserted by that trusted instance, not a remote
/// credential. Neither provider nor output destination is self-authorizing.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct ExpertiseQuestion {
    pub request_id: Uuid,
    pub space: SpaceId,
    pub requester: InstanceId,
    pub principal: Uuid,
    pub capability: RecordKey,
    pub revision: StateRevision,
    pub collection_digest: ContentDigest,
    pub question: String,
    pub mode: AnswerMode,
    pub model_destination: Option<String>,
    pub output_destination: String,
    pub created_at: DateTime<Utc>,
    pub expires_at: DateTime<Utc>,
}
impl ExpertiseQuestion {
    pub fn validate(&self) -> Result<(), ExpertiseReject> {
        if self.request_id.is_nil()
            || self.principal.is_nil()
            || self.question.trim().is_empty()
            || self.question.len() > MAX_QUESTION_BYTES
            || !valid_destination(&self.output_destination)
            || self
                .model_destination
                .as_ref()
                .is_some_and(|d| !valid_destination(d))
            || (self.mode == AnswerMode::Model) != self.model_destination.is_some()
        {
            return Err(ExpertiseReject::Limits);
        }
        let lifetime = self.expires_at.signed_duration_since(self.created_at);
        if lifetime <= chrono::Duration::zero() || lifetime > chrono::Duration::seconds(120) {
            return Err(ExpertiseReject::Expired);
        }
        Ok(())
    }
    pub fn digest(&self) -> Result<ContentDigest, ExpertiseReject> {
        self.validate()?;
        CanonicalJsonCodec::digest(self).map_err(|_| ExpertiseReject::Binding)
    }
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum ExpertiseStatus {
    Queued,
    Running,
    Answered,
    Refused,
    Denied,
    Expired,
    Failed,
    Uncertain,
}
impl ExpertiseStatus {
    pub fn terminal(self) -> bool {
        !matches!(self, Self::Queued | Self::Running)
    }
}

/// A selected, reviewed source snapshot, not a fabricated AT repository URI.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct ExpertiseCitation {
    pub responder: InstanceId,
    pub collection: RecordId,
    pub revision: StateRevision,
    pub canonical: RecordId,
    pub source: SourceVersionRef,
    pub author: AuthorIdentity,
    pub authored_at: DateTime<Utc>,
    pub content_digest: ContentDigest,
    pub excerpt: String,
    pub evidence: Vec<ReviewedEvidence>,
}
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct ExpertiseAnswer {
    pub request_digest: ContentDigest,
    pub status: ExpertiseStatus,
    pub text: String,
    pub citations: Vec<ExpertiseCitation>,
    pub insufficient_evidence: bool,
    pub reason: Option<ExpertiseReject>,
    pub untrusted: bool,
}
impl ExpertiseAnswer {
    /// Validate output against the *selected* evidence, not the entire project
    /// or collection. Models cannot introduce citation IDs or hidden evidence.
    pub fn validate(
        &self,
        question: &ExpertiseQuestion,
        selected: &[ExpertiseCitation],
    ) -> Result<(), ExpertiseReject> {
        if self.request_digest != question.digest()?
            || !self.untrusted
            || !self.status.terminal()
            || self.citations.len() > MAX_CITATIONS
            || !distinct(&self.citations, MAX_CITATIONS)
            || CanonicalJsonCodec::encode(self)
                .map_err(|_| ExpertiseReject::Binding)?
                .len()
                > MAX_ANSWER_BYTES
        {
            return Err(ExpertiseReject::Binding);
        }
        if self.citations.iter().any(|c| {
            !selected.contains(c)
                || c.responder != question.capability.publisher
                || c.collection != question.capability.id
                || c.revision != question.revision
        }) || (self.status == ExpertiseStatus::Answered
            && (self.insufficient_evidence
                || self.citations.is_empty()
                || self.text.trim().is_empty()))
            || (self.status != ExpertiseStatus::Answered && !self.insufficient_evidence)
        {
            return Err(ExpertiseReject::InvalidCitation);
        }
        Ok(())
    }
}

impl PublicationState {
    /// Current authority state is an input, not obtained by consulting a wall
    /// clock/network. The gateway supplies real time, never a caller override.
    pub fn authorize_expertise(
        &self,
        authenticated: InstanceId,
        question: &ExpertiseQuestion,
        now: DateTime<Utc>,
    ) -> Result<&CapabilityAdvertisement, ExpertiseReject> {
        question.validate()?;
        if authenticated != question.requester
            || question.space != self.space()
            || !self.has_capability(question.requester, SpaceCapability::AskExpertise)
            || !self.has_capability(question.requester, SpaceCapability::Pull)
            || !self.has_capability(
                question.capability.publisher,
                SpaceCapability::AnswerExpertise,
            )
            || !self.has_capability(question.capability.publisher, SpaceCapability::Pull)
        {
            return Err(ExpertiseReject::Unauthorized);
        }
        if now >= question.expires_at || question.created_at > now {
            return Err(ExpertiseReject::Expired);
        }
        let ad = self
            .expertise(question.capability)
            .ok_or(ExpertiseReject::Unauthorized)?;
        if !ad.active || !ad.requesters.contains(&authenticated) {
            return Err(ExpertiseReject::Unauthorized);
        }
        if ad.revision != question.revision || ad.collection_digest != question.collection_digest {
            return Err(ExpertiseReject::StaleCollection);
        }
        if !ad.modes.contains(&question.mode)
            || !ad
                .output_destinations
                .contains(&question.output_destination)
            || question
                .model_destination
                .as_ref()
                .is_some_and(|id| !ad.model_destinations.contains(id))
        {
            return Err(ExpertiseReject::DestinationDenied);
        }
        if question.question.len() > ad.limits.question_bytes as usize
            || question
                .expires_at
                .signed_duration_since(question.created_at)
                > chrono::Duration::seconds(i64::from(ad.limits.deadline_seconds))
        {
            return Err(ExpertiseReject::Limits);
        }
        Ok(ad)
    }
}
