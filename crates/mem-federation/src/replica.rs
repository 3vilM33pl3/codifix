// SPDX-License-Identifier: AGPL-3.0-or-later

use std::collections::{HashMap, HashSet};

use crate::{
    AuthorIdentity, CanonicalJsonCodec, CanonicalStateRecord, ContentDigest, EvidenceRecord,
    FederationError, FederationRecord, MemoryVersionRecord, ProjectSpaceId, RecordLocator,
    RelationRecord, RevocationRecord, SpaceCapability, TransportRecord, validate_finite_confidence,
};

type SourceLocatorKey = (crate::SourceIdentity, RecordLocator);
type MemoryVersionKey = (RecordLocator, u32);
type CanonicalStateKey = (RecordLocator, u32);
type RelationKey = (RecordLocator, mem_record::MemoryRelationType, RecordLocator);

#[derive(Debug, Clone)]
struct StoredMemoryVersion {
    digest: ContentDigest,
    record: MemoryVersionRecord,
    author: String,
    source: String,
    locator: String,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct CanonicalHeadSummary {
    pub canonical_id: String,
    pub version: u32,
    pub publisher: String,
    pub author: String,
    pub source: String,
    pub locator: String,
    pub digest: String,
    pub evidence_count: usize,
    pub tombstone: bool,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct ReplicaSummary {
    pub evidence_count: usize,
    pub memory_version_count: usize,
    pub active_canonical_count: usize,
    pub active_publishers: Vec<String>,
    pub revoked_publishers: Vec<String>,
    pub canonical_heads: Vec<CanonicalHeadSummary>,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub enum DuplicateReason {
    ExactDuplicate,
    SemanticDuplicate,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub enum RejectReason {
    WrongProjectSpace {
        expected: ProjectSpaceId,
        actual: ProjectSpaceId,
    },
    IntegrityMismatch {
        expected: ContentDigest,
        actual: ContentDigest,
    },
    Equivocation {
        source_identity: crate::SourceIdentity,
        record_locator: RecordLocator,
        existing_digest: ContentDigest,
        conflicting_digest: ContentDigest,
    },
    UnauthorizedMembershipChange {
        actor: AuthorIdentity,
    },
    UnauthorizedDataWrite {
        author: AuthorIdentity,
    },
    RevokedAuthor {
        author: AuthorIdentity,
    },
    MemoryVersionConflict {
        canonical_id: RecordLocator,
        version_no: u32,
    },
    CanonicalStateConflict {
        canonical_id: RecordLocator,
        version_no: u32,
    },
    EvidenceConflict {
        evidence_id: RecordLocator,
    },
    RelationConflict {
        src_canonical_id: RecordLocator,
        relation_type: mem_record::MemoryRelationType,
        dst_canonical_id: RecordLocator,
    },
    StaleCanonicalState {
        canonical_id: RecordLocator,
        current_version: u32,
        incoming_version: u32,
    },
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub enum QuarantineReason {
    VersionGap {
        canonical_id: RecordLocator,
        expected: u32,
        actual: u32,
    },
    MissingMemoryVersion {
        canonical_id: RecordLocator,
        version_no: u32,
    },
    MissingEvidence {
        memory_id: RecordLocator,
        evidence_id: RecordLocator,
    },
    MissingCanonical {
        canonical_id: RecordLocator,
    },
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub enum ApplyOutcome {
    Applied,
    Duplicate { reason: DuplicateReason },
    Rejected { reason: RejectReason },
    Quarantined { reason: QuarantineReason },
}

#[derive(Debug, Clone)]
pub struct ReplicaState {
    space_id: ProjectSpaceId,
    authority: AuthorIdentity,
    capabilities: HashMap<AuthorIdentity, HashSet<SpaceCapability>>,
    revoked_publish: HashSet<AuthorIdentity>,
    seen_digests: HashMap<SourceLocatorKey, ContentDigest>,
    memory_versions: HashMap<MemoryVersionKey, StoredMemoryVersion>,
    memory_ids: HashMap<RecordLocator, MemoryVersionRecord>,
    canonical_states: HashMap<RecordLocator, CanonicalStateRecord>,
    canonical_state_versions: HashMap<CanonicalStateKey, ContentDigest>,
    evidence: HashMap<RecordLocator, (ContentDigest, EvidenceRecord)>,
    relations: HashMap<RelationKey, ContentDigest>,
    applied: Vec<TransportRecord>,
}

impl ReplicaState {
    pub fn new(space_id: ProjectSpaceId, authority: AuthorIdentity) -> Self {
        let mut capabilities = HashMap::new();
        capabilities.insert(
            authority.clone(),
            HashSet::from([
                SpaceCapability::Pull,
                SpaceCapability::Publish,
                SpaceCapability::ManageMembership,
            ]),
        );
        Self {
            space_id,
            authority,
            capabilities,
            revoked_publish: HashSet::new(),
            seen_digests: HashMap::new(),
            memory_versions: HashMap::new(),
            memory_ids: HashMap::new(),
            canonical_states: HashMap::new(),
            canonical_state_versions: HashMap::new(),
            evidence: HashMap::new(),
            relations: HashMap::new(),
            applied: Vec::new(),
        }
    }

    pub fn space_id(&self) -> &ProjectSpaceId {
        &self.space_id
    }

    pub fn authority(&self) -> &AuthorIdentity {
        &self.authority
    }

    pub fn apply(&mut self, record: TransportRecord) -> ApplyOutcome {
        let mut candidate = self.clone();
        let outcome = candidate.apply_inner(&record);
        if matches!(outcome, ApplyOutcome::Applied) {
            candidate.record_applied(record);
            *self = candidate;
        }
        outcome
    }

    pub fn replay<I>(&mut self, records: I) -> Vec<ApplyOutcome>
    where
        I: IntoIterator<Item = TransportRecord>,
    {
        records
            .into_iter()
            .map(|record| self.apply(record))
            .collect()
    }

    pub fn has_capability(&self, author: &AuthorIdentity, capability: SpaceCapability) -> bool {
        self.capabilities
            .get(author)
            .is_some_and(|caps| caps.contains(&capability))
    }

    pub fn canonical_state(&self, canonical_id: &RecordLocator) -> Option<&CanonicalStateRecord> {
        self.canonical_states.get(canonical_id)
    }

    pub fn applied_records(&self) -> &[TransportRecord] {
        self.applied.as_slice()
    }

    pub fn summary(&self) -> ReplicaSummary {
        let mut active_publishers = self
            .capabilities
            .iter()
            .filter(|(author, caps)| {
                **author != self.authority && caps.contains(&SpaceCapability::Publish)
            })
            .map(|(author, _)| author.as_str().to_string())
            .collect::<Vec<_>>();
        active_publishers.sort_unstable();

        let mut revoked_publishers = self
            .revoked_publish
            .iter()
            .map(|author| author.as_str().to_string())
            .collect::<Vec<_>>();
        revoked_publishers.sort_unstable();

        let mut canonical_heads = self
            .memory_versions
            .values()
            .fold(
                HashMap::<RecordLocator, &StoredMemoryVersion>::new(),
                |mut latest, stored| {
                    let canonical_id = stored.record.canonical_id.clone();
                    match latest.get(&canonical_id) {
                        Some(current) if current.record.version_no >= stored.record.version_no => {}
                        _ => {
                            latest.insert(canonical_id, stored);
                        }
                    }
                    latest
                },
            )
            .into_values()
            .map(|stored| CanonicalHeadSummary {
                canonical_id: stored.record.canonical_id.as_str().to_string(),
                version: stored.record.version_no,
                publisher: stored.author.clone(),
                author: stored.author.clone(),
                source: stored.source.clone(),
                locator: stored.locator.clone(),
                digest: format!("sha256:{}", stored.digest.as_hex()),
                evidence_count: stored.record.evidence_refs.len(),
                tombstone: stored.record.is_tombstone,
            })
            .collect::<Vec<_>>();
        canonical_heads.sort_unstable_by(|left, right| left.canonical_id.cmp(&right.canonical_id));

        ReplicaSummary {
            evidence_count: self.evidence.len(),
            memory_version_count: self.memory_versions.len(),
            active_canonical_count: canonical_heads
                .iter()
                .filter(|head| !head.tombstone)
                .count(),
            active_publishers,
            revoked_publishers,
            canonical_heads,
        }
    }

    fn apply_inner(&mut self, record: &TransportRecord) -> ApplyOutcome {
        let actual = match CanonicalJsonCodec::digest(record.envelope()) {
            Ok(actual) => actual,
            Err(_) => {
                return ApplyOutcome::Rejected {
                    reason: RejectReason::IntegrityMismatch {
                        expected: record.content_digest().clone(),
                        actual: record.content_digest().clone(),
                    },
                };
            }
        };
        if actual != *record.content_digest() {
            return ApplyOutcome::Rejected {
                reason: RejectReason::IntegrityMismatch {
                    expected: record.content_digest().clone(),
                    actual,
                },
            };
        }

        if record.envelope().project_space_id != self.space_id {
            return ApplyOutcome::Rejected {
                reason: RejectReason::WrongProjectSpace {
                    expected: self.space_id.clone(),
                    actual: record.envelope().project_space_id.clone(),
                },
            };
        }

        let seen_key = (
            record.source_identity().clone(),
            record.record_locator().clone(),
        );
        if let Some(existing_digest) = self.seen_digests.get(&seen_key) {
            if existing_digest == record.content_digest() {
                return ApplyOutcome::Duplicate {
                    reason: DuplicateReason::ExactDuplicate,
                };
            }
            return ApplyOutcome::Rejected {
                reason: RejectReason::Equivocation {
                    source_identity: record.source_identity().clone(),
                    record_locator: record.record_locator().clone(),
                    existing_digest: existing_digest.clone(),
                    conflicting_digest: record.content_digest().clone(),
                },
            };
        }

        match &record.envelope().record {
            FederationRecord::Membership(membership) => self.apply_membership(record, membership),
            FederationRecord::Revocation(revocation) => self.apply_revocation(record, revocation),
            FederationRecord::MemoryVersion(memory_version) => {
                match self.require_publish_capability(record.envelope().author.clone()) {
                    Err(outcome) => outcome,
                    Ok(()) => self.apply_memory_version(record, memory_version),
                }
            }
            FederationRecord::CanonicalState(state) => {
                match self.require_publish_capability(record.envelope().author.clone()) {
                    Err(outcome) => outcome,
                    Ok(()) => self.apply_canonical_state(record, state),
                }
            }
            FederationRecord::Evidence(evidence) => {
                match self.require_publish_capability(record.envelope().author.clone()) {
                    Err(outcome) => outcome,
                    Ok(()) => self.apply_evidence(record, evidence),
                }
            }
            FederationRecord::Relation(relation) => {
                match self.require_publish_capability(record.envelope().author.clone()) {
                    Err(outcome) => outcome,
                    Ok(()) => self.apply_relation(record, relation),
                }
            }
        }
    }

    fn record_applied(&mut self, record: TransportRecord) {
        self.seen_digests.insert(
            (
                record.source_identity().clone(),
                record.record_locator().clone(),
            ),
            record.content_digest().clone(),
        );
        self.applied.push(record);
    }

    fn apply_membership(
        &mut self,
        record: &TransportRecord,
        membership: &crate::MembershipRecord,
    ) -> ApplyOutcome {
        if record.envelope().author != self.authority {
            return ApplyOutcome::Rejected {
                reason: RejectReason::UnauthorizedMembershipChange {
                    actor: record.envelope().author.clone(),
                },
            };
        }
        let caps = self
            .capabilities
            .entry(membership.member.clone())
            .or_default();
        if caps.contains(&membership.capability) {
            return ApplyOutcome::Duplicate {
                reason: DuplicateReason::SemanticDuplicate,
            };
        }
        caps.insert(membership.capability);
        if membership.capability == SpaceCapability::Publish {
            self.revoked_publish.remove(&membership.member);
        }
        ApplyOutcome::Applied
    }

    fn apply_revocation(
        &mut self,
        record: &TransportRecord,
        revocation: &RevocationRecord,
    ) -> ApplyOutcome {
        if record.envelope().author != self.authority {
            return ApplyOutcome::Rejected {
                reason: RejectReason::UnauthorizedMembershipChange {
                    actor: record.envelope().author.clone(),
                },
            };
        }
        let caps = self
            .capabilities
            .entry(revocation.member.clone())
            .or_default();
        let changed = if let Some(capability) = revocation.capability {
            caps.remove(&capability)
        } else {
            let had_any = !caps.is_empty();
            caps.clear();
            had_any
        };
        if !changed {
            return ApplyOutcome::Duplicate {
                reason: DuplicateReason::SemanticDuplicate,
            };
        }
        match revocation.capability {
            Some(SpaceCapability::Publish) | None => {
                self.revoked_publish.insert(revocation.member.clone());
            }
            Some(_) => {}
        }
        ApplyOutcome::Applied
    }

    fn apply_memory_version(
        &mut self,
        record: &TransportRecord,
        memory_version: &MemoryVersionRecord,
    ) -> ApplyOutcome {
        if let Err(error) = validate_finite_confidence(memory_version.confidence) {
            return ApplyOutcome::Rejected {
                reason: match error {
                    FederationError::NonFiniteConfidence { .. } => {
                        RejectReason::MemoryVersionConflict {
                            canonical_id: memory_version.canonical_id.clone(),
                            version_no: memory_version.version_no,
                        }
                    }
                    _ => unreachable!(),
                },
            };
        }
        for related in &memory_version.related_memories {
            if validate_finite_confidence(related.confidence).is_err() {
                return ApplyOutcome::Rejected {
                    reason: RejectReason::MemoryVersionConflict {
                        canonical_id: memory_version.canonical_id.clone(),
                        version_no: memory_version.version_no,
                    },
                };
            }
        }

        let key = (
            memory_version.canonical_id.clone(),
            memory_version.version_no,
        );
        if let Some(existing) = self.memory_versions.get(&key) {
            if existing.digest == *record.content_digest() {
                return ApplyOutcome::Duplicate {
                    reason: DuplicateReason::SemanticDuplicate,
                };
            }
            return ApplyOutcome::Rejected {
                reason: RejectReason::MemoryVersionConflict {
                    canonical_id: memory_version.canonical_id.clone(),
                    version_no: memory_version.version_no,
                },
            };
        }

        let expected = self
            .memory_versions
            .keys()
            .filter(|(canonical_id, _)| canonical_id == &memory_version.canonical_id)
            .map(|(_, version_no)| *version_no)
            .max()
            .map_or(1, |latest| latest + 1);
        if memory_version.version_no != expected {
            return ApplyOutcome::Quarantined {
                reason: QuarantineReason::VersionGap {
                    canonical_id: memory_version.canonical_id.clone(),
                    expected,
                    actual: memory_version.version_no,
                },
            };
        }
        for evidence_id in &memory_version.evidence_refs {
            if !self.evidence.contains_key(evidence_id) {
                return ApplyOutcome::Quarantined {
                    reason: QuarantineReason::MissingEvidence {
                        memory_id: memory_version.memory_id.clone(),
                        evidence_id: evidence_id.clone(),
                    },
                };
            }
        }

        self.memory_ids
            .insert(memory_version.memory_id.clone(), memory_version.clone());
        self.memory_versions.insert(
            key,
            StoredMemoryVersion {
                digest: record.content_digest().clone(),
                record: memory_version.clone(),
                author: record.envelope().author.as_str().to_string(),
                source: record.source_identity().as_str().to_string(),
                locator: record.record_locator().as_str().to_string(),
            },
        );
        ApplyOutcome::Applied
    }

    fn apply_canonical_state(
        &mut self,
        record: &TransportRecord,
        state: &CanonicalStateRecord,
    ) -> ApplyOutcome {
        if validate_finite_confidence(state.confidence).is_err() {
            return ApplyOutcome::Rejected {
                reason: RejectReason::CanonicalStateConflict {
                    canonical_id: state.canonical_id.clone(),
                    version_no: state.version_no,
                },
            };
        }

        let memory = match self
            .memory_versions
            .get(&(state.canonical_id.clone(), state.version_no))
            .map(|stored| &stored.record)
        {
            Some(memory) => memory,
            None => {
                return ApplyOutcome::Quarantined {
                    reason: QuarantineReason::MissingMemoryVersion {
                        canonical_id: state.canonical_id.clone(),
                        version_no: state.version_no,
                    },
                };
            }
        };
        if memory.memory_id != state.memory_id {
            return ApplyOutcome::Rejected {
                reason: RejectReason::CanonicalStateConflict {
                    canonical_id: state.canonical_id.clone(),
                    version_no: state.version_no,
                },
            };
        }
        let version_key = (state.canonical_id.clone(), state.version_no);
        if let Some(existing_digest) = self.canonical_state_versions.get(&version_key) {
            if existing_digest == record.content_digest() {
                return ApplyOutcome::Duplicate {
                    reason: DuplicateReason::SemanticDuplicate,
                };
            }
            return ApplyOutcome::Rejected {
                reason: RejectReason::CanonicalStateConflict {
                    canonical_id: state.canonical_id.clone(),
                    version_no: state.version_no,
                },
            };
        }

        if let Some(current) = self.canonical_states.get(&state.canonical_id)
            && current.version_no >= state.version_no
        {
            return ApplyOutcome::Rejected {
                reason: RejectReason::StaleCanonicalState {
                    canonical_id: state.canonical_id.clone(),
                    current_version: current.version_no,
                    incoming_version: state.version_no,
                },
            };
        }

        self.canonical_state_versions
            .insert(version_key, record.content_digest().clone());
        self.canonical_states
            .insert(state.canonical_id.clone(), state.clone());
        ApplyOutcome::Applied
    }

    fn apply_evidence(
        &mut self,
        record: &TransportRecord,
        evidence: &EvidenceRecord,
    ) -> ApplyOutcome {
        if let Some((existing_digest, _)) = self.evidence.get(&evidence.evidence_id) {
            if existing_digest == record.content_digest() {
                return ApplyOutcome::Duplicate {
                    reason: DuplicateReason::SemanticDuplicate,
                };
            }
            return ApplyOutcome::Rejected {
                reason: RejectReason::EvidenceConflict {
                    evidence_id: evidence.evidence_id.clone(),
                },
            };
        }

        self.evidence.insert(
            evidence.evidence_id.clone(),
            (record.content_digest().clone(), evidence.clone()),
        );
        ApplyOutcome::Applied
    }

    fn apply_relation(
        &mut self,
        record: &TransportRecord,
        relation: &RelationRecord,
    ) -> ApplyOutcome {
        if !self
            .canonical_states
            .contains_key(&relation.src_canonical_id)
        {
            return ApplyOutcome::Quarantined {
                reason: QuarantineReason::MissingCanonical {
                    canonical_id: relation.src_canonical_id.clone(),
                },
            };
        }
        if !self
            .canonical_states
            .contains_key(&relation.dst_canonical_id)
        {
            return ApplyOutcome::Quarantined {
                reason: QuarantineReason::MissingCanonical {
                    canonical_id: relation.dst_canonical_id.clone(),
                },
            };
        }
        let key = (
            relation.src_canonical_id.clone(),
            relation.relation_type.clone(),
            relation.dst_canonical_id.clone(),
        );
        if let Some(existing_digest) = self.relations.get(&key) {
            if existing_digest == record.content_digest() {
                return ApplyOutcome::Duplicate {
                    reason: DuplicateReason::SemanticDuplicate,
                };
            }
            return ApplyOutcome::Rejected {
                reason: RejectReason::RelationConflict {
                    src_canonical_id: relation.src_canonical_id.clone(),
                    relation_type: relation.relation_type.clone(),
                    dst_canonical_id: relation.dst_canonical_id.clone(),
                },
            };
        }
        self.relations.insert(key, record.content_digest().clone());
        ApplyOutcome::Applied
    }

    fn require_publish_capability(&self, author: AuthorIdentity) -> Result<(), ApplyOutcome> {
        if self.has_capability(&author, SpaceCapability::Publish) {
            return Ok(());
        }
        if self.revoked_publish.contains(&author) {
            return Err(ApplyOutcome::Rejected {
                reason: RejectReason::RevokedAuthor { author },
            });
        }
        Err(ApplyOutcome::Rejected {
            reason: RejectReason::UnauthorizedDataWrite { author },
        })
    }
}

#[cfg(test)]
mod tests {
    use chrono::{TimeZone, Utc};
    use mem_record::{
        MemoryEntryResponse, MemoryRelationType, MemorySourceRecord, MemoryStatus, MemoryType,
        SourceKind,
    };
    use proptest::prelude::*;
    use uuid::Uuid;

    use super::*;
    use crate::{
        AuthorIdentity, FederationRecord, MembershipRecord, ProjectSpaceId, ProjectionContext,
        RecordLocator, RevocationRecord, SharedRecordEnvelope, SourceIdentity,
        project_memory_entry,
    };

    fn authority() -> AuthorIdentity {
        AuthorIdentity::new("did:example:authority").expect("authority")
    }

    fn writer() -> AuthorIdentity {
        AuthorIdentity::new("did:example:writer").expect("writer")
    }

    fn source_identity() -> SourceIdentity {
        SourceIdentity::new("did:example:source").expect("source")
    }

    fn project_space_id() -> ProjectSpaceId {
        ProjectSpaceId::new("memory").expect("project")
    }

    fn publish_membership(
        member: AuthorIdentity,
        capability: SpaceCapability,
        locator: &str,
    ) -> TransportRecord {
        TransportRecord::new(
            source_identity(),
            RecordLocator::new(locator).expect("locator"),
            SharedRecordEnvelope::new(
                RecordLocator::new(locator).expect("record id"),
                project_space_id(),
                authority(),
                Utc::now(),
                None,
                FederationRecord::Membership(MembershipRecord { member, capability }),
            ),
        )
        .expect("membership record")
    }

    fn unauthorized_membership(locator: &str) -> TransportRecord {
        TransportRecord::new(
            source_identity(),
            RecordLocator::new(locator).expect("locator"),
            SharedRecordEnvelope::new(
                RecordLocator::new(locator).expect("record id"),
                project_space_id(),
                writer(),
                Utc::now(),
                None,
                FederationRecord::Membership(MembershipRecord {
                    member: writer(),
                    capability: SpaceCapability::Publish,
                }),
            ),
        )
        .expect("membership record")
    }

    fn revoke_publish(member: AuthorIdentity, locator: &str) -> TransportRecord {
        TransportRecord::new(
            source_identity(),
            RecordLocator::new(locator).expect("locator"),
            SharedRecordEnvelope::new(
                RecordLocator::new(locator).expect("record id"),
                project_space_id(),
                authority(),
                Utc::now(),
                None,
                FederationRecord::Revocation(RevocationRecord {
                    member,
                    capability: Some(SpaceCapability::Publish),
                    reason: Some("revoked".to_string()),
                }),
            ),
        )
        .expect("revocation record")
    }

    fn relation_record(locator: &str) -> TransportRecord {
        TransportRecord::new(
            source_identity(),
            RecordLocator::new(locator).expect("locator"),
            SharedRecordEnvelope::new(
                RecordLocator::new(locator).expect("record id"),
                project_space_id(),
                writer(),
                Utc::now(),
                None,
                FederationRecord::Relation(RelationRecord {
                    src_canonical_id: RecordLocator::new("canonical:src").expect("src"),
                    relation_type: MemoryRelationType::DependsOn,
                    dst_canonical_id: RecordLocator::new("canonical:dst").expect("dst"),
                    asserted_at: Utc::now(),
                }),
            ),
        )
        .expect("relation record")
    }

    fn sample_entry(canonical_id: Uuid, version_no: i32) -> MemoryEntryResponse {
        sample_entry_with_tombstone(canonical_id, version_no, false)
    }

    fn sample_entry_with_tombstone(
        canonical_id: Uuid,
        version_no: i32,
        is_tombstone: bool,
    ) -> MemoryEntryResponse {
        let created_at = Utc
            .with_ymd_and_hms(2026, 9, 4, 9, 0, 0)
            .single()
            .expect("valid timestamp");
        let updated_at = Utc
            .with_ymd_and_hms(2026, 9, 4, 10, version_no as u32, 0)
            .single()
            .expect("valid timestamp");
        MemoryEntryResponse {
            id: Uuid::new_v4(),
            project: "memory".to_string(),
            canonical_text: format!("Canonical text v{version_no}"),
            summary: format!("Summary v{version_no}"),
            memory_type: MemoryType::Decision,
            importance: 3,
            confidence: 0.8,
            status: if is_tombstone {
                MemoryStatus::Archived
            } else {
                MemoryStatus::Active
            },
            tags: vec!["federation".to_string()],
            sources: vec![MemorySourceRecord {
                id: Uuid::new_v4(),
                task_id: Some(Uuid::new_v4()),
                file_path: Some(format!("src/v{version_no}.rs")),
                git_commit: Some(format!("commit-{version_no}")),
                symbol_name: None,
                symbol_kind: None,
                source_kind: SourceKind::File,
                excerpt: Some(format!("Excerpt v{version_no}")),
                provenance: None,
            }],
            related_memories: Vec::new(),
            embedding_spaces: Vec::new(),
            created_at,
            updated_at,
            canonical_id,
            version_no,
            is_tombstone,
        }
    }

    fn projected_records(
        author: AuthorIdentity,
        canonical_id: Uuid,
        version_no: i32,
    ) -> Vec<TransportRecord> {
        project_memory_entry(
            &sample_entry(canonical_id, version_no),
            &ProjectionContext {
                source_identity: source_identity(),
                author,
            },
        )
        .expect("projection")
        .into_records()
    }

    fn projected_tombstone_records(
        author: AuthorIdentity,
        canonical_id: Uuid,
        version_no: i32,
    ) -> Vec<TransportRecord> {
        project_memory_entry(
            &sample_entry_with_tombstone(canonical_id, version_no, true),
            &ProjectionContext {
                source_identity: source_identity(),
                author,
            },
        )
        .expect("projection")
        .into_records()
    }

    #[test]
    fn exact_duplicate_is_idempotent() {
        let mut replica = ReplicaState::new(project_space_id(), authority());
        let grant = publish_membership(
            writer(),
            SpaceCapability::Publish,
            "membership:writer:publish",
        );
        assert_eq!(replica.apply(grant), ApplyOutcome::Applied);

        let canonical_id = Uuid::new_v4();
        let evidence = projected_records(writer(), canonical_id, 1)
            .into_iter()
            .next()
            .expect("evidence");
        assert_eq!(replica.apply(evidence.clone()), ApplyOutcome::Applied);
        assert_eq!(
            replica.apply(evidence),
            ApplyOutcome::Duplicate {
                reason: DuplicateReason::ExactDuplicate
            }
        );
    }

    #[test]
    fn same_source_and_locator_with_different_digest_rejects_equivocation() {
        let mut replica = ReplicaState::new(project_space_id(), authority());
        let first = publish_membership(writer(), SpaceCapability::Publish, "membership:shared");
        let second = publish_membership(writer(), SpaceCapability::Pull, "membership:shared");
        assert_eq!(replica.apply(first), ApplyOutcome::Applied);
        match replica.apply(second) {
            ApplyOutcome::Rejected {
                reason: RejectReason::Equivocation { .. },
            } => {}
            other => panic!("unexpected outcome: {other:?}"),
        }
    }

    #[test]
    fn memory_version_gap_is_quarantined() {
        let mut replica = ReplicaState::new(project_space_id(), authority());
        assert_eq!(
            replica.apply(publish_membership(
                writer(),
                SpaceCapability::Publish,
                "membership:writer:publish"
            )),
            ApplyOutcome::Applied
        );

        let canonical_id = Uuid::new_v4();
        let mut records = projected_records(writer(), canonical_id, 2).into_iter();
        let evidence = records.next().expect("evidence");
        let memory_version = records.next().expect("memory version");

        assert_eq!(replica.apply(evidence), ApplyOutcome::Applied);
        assert_eq!(
            replica.apply(memory_version),
            ApplyOutcome::Quarantined {
                reason: QuarantineReason::VersionGap {
                    canonical_id: crate::canonical_locator(canonical_id).expect("canonical"),
                    expected: 1,
                    actual: 2
                }
            }
        );
    }

    #[test]
    fn canonical_state_missing_memory_version_is_quarantined() {
        let mut replica = ReplicaState::new(project_space_id(), authority());
        assert_eq!(
            replica.apply(publish_membership(
                writer(),
                SpaceCapability::Publish,
                "membership:writer:publish"
            )),
            ApplyOutcome::Applied
        );

        let canonical_id = Uuid::new_v4();
        let mut records = projected_records(writer(), canonical_id, 1).into_iter();
        let evidence = records.next().expect("evidence");
        let _memory_version = records.next().expect("memory version");
        let canonical_state = records.next().expect("canonical state");

        assert_eq!(replica.apply(evidence), ApplyOutcome::Applied);
        assert_eq!(
            replica.apply(canonical_state),
            ApplyOutcome::Quarantined {
                reason: QuarantineReason::MissingMemoryVersion {
                    canonical_id: crate::canonical_locator(canonical_id).expect("canonical"),
                    version_no: 1
                }
            }
        );
    }

    #[test]
    fn memory_version_missing_evidence_is_quarantined() {
        let mut replica = ReplicaState::new(project_space_id(), authority());
        assert_eq!(
            replica.apply(publish_membership(
                writer(),
                SpaceCapability::Publish,
                "membership:writer:publish"
            )),
            ApplyOutcome::Applied
        );

        let canonical_id = Uuid::new_v4();
        let mut records = projected_records(writer(), canonical_id, 1).into_iter();
        let _evidence = records.next().expect("evidence");
        let memory_version = records.next().expect("memory version");

        match replica.apply(memory_version) {
            ApplyOutcome::Quarantined {
                reason: QuarantineReason::MissingEvidence { .. },
            } => {}
            other => panic!("unexpected outcome: {other:?}"),
        }
    }

    #[test]
    fn replay_projected_entry_succeeds_with_evidence_first() {
        let mut replica = ReplicaState::new(project_space_id(), authority());
        assert_eq!(
            replica.apply(publish_membership(
                writer(),
                SpaceCapability::Publish,
                "membership:writer:publish"
            )),
            ApplyOutcome::Applied
        );
        let canonical_id = Uuid::new_v4();
        let outcomes = replica.replay(projected_records(writer(), canonical_id, 1));
        assert_eq!(
            outcomes,
            vec![
                ApplyOutcome::Applied,
                ApplyOutcome::Applied,
                ApplyOutcome::Applied
            ]
        );
        assert_eq!(
            replica
                .canonical_state(&crate::canonical_locator(canonical_id).expect("canonical"))
                .expect("state")
                .version_no,
            1
        );
    }

    #[test]
    fn revocation_blocks_later_writes_but_preserves_prior_data() {
        let mut replica = ReplicaState::new(project_space_id(), authority());
        assert_eq!(
            replica.apply(publish_membership(
                writer(),
                SpaceCapability::Publish,
                "membership:writer:publish"
            )),
            ApplyOutcome::Applied
        );
        let canonical_id = Uuid::new_v4();
        assert_eq!(
            replica.replay(projected_records(writer(), canonical_id, 1)),
            vec![
                ApplyOutcome::Applied,
                ApplyOutcome::Applied,
                ApplyOutcome::Applied
            ]
        );

        assert_eq!(
            replica.apply(revoke_publish(writer(), "membership:writer:revoke")),
            ApplyOutcome::Applied
        );

        let evidence = projected_records(writer(), canonical_id, 2)
            .into_iter()
            .next()
            .expect("evidence");
        assert_eq!(
            replica.apply(evidence),
            ApplyOutcome::Rejected {
                reason: RejectReason::RevokedAuthor { author: writer() }
            }
        );
        assert_eq!(
            replica
                .canonical_state(&crate::canonical_locator(canonical_id).expect("canonical"))
                .expect("state")
                .version_no,
            1
        );
    }

    #[test]
    fn only_authority_may_grant_membership() {
        let mut replica = ReplicaState::new(project_space_id(), authority());
        assert_eq!(
            replica.apply(unauthorized_membership("membership:unauthorized")),
            ApplyOutcome::Rejected {
                reason: RejectReason::UnauthorizedMembershipChange { actor: writer() }
            }
        );
    }

    #[test]
    fn pull_only_member_is_unauthorized_not_revoked() {
        let mut replica = ReplicaState::new(project_space_id(), authority());
        assert_eq!(
            replica.apply(publish_membership(
                writer(),
                SpaceCapability::Pull,
                "membership:writer:pull"
            )),
            ApplyOutcome::Applied
        );
        let canonical_id = Uuid::new_v4();
        let evidence = projected_records(writer(), canonical_id, 1)
            .into_iter()
            .next()
            .expect("evidence");
        assert_eq!(
            replica.apply(evidence),
            ApplyOutcome::Rejected {
                reason: RejectReason::UnauthorizedDataWrite { author: writer() }
            }
        );
    }

    #[test]
    fn relation_refs_must_exist() {
        let mut replica = ReplicaState::new(project_space_id(), authority());
        assert_eq!(
            replica.apply(publish_membership(
                writer(),
                SpaceCapability::Publish,
                "membership:writer:publish"
            )),
            ApplyOutcome::Applied
        );
        assert_eq!(
            replica.apply(relation_record("relation:1")),
            ApplyOutcome::Quarantined {
                reason: QuarantineReason::MissingCanonical {
                    canonical_id: RecordLocator::new("canonical:src").expect("src")
                }
            }
        );
    }

    #[test]
    fn tombstone_version_advances_head_and_preserves_history() {
        let mut replica = ReplicaState::new(project_space_id(), authority());
        assert_eq!(
            replica.apply(publish_membership(
                writer(),
                SpaceCapability::Publish,
                "membership:writer:publish"
            )),
            ApplyOutcome::Applied
        );
        let canonical_id = Uuid::new_v4();
        assert_eq!(
            replica.replay(projected_records(writer(), canonical_id, 1)),
            vec![
                ApplyOutcome::Applied,
                ApplyOutcome::Applied,
                ApplyOutcome::Applied
            ]
        );
        assert_eq!(
            replica.replay(projected_tombstone_records(writer(), canonical_id, 2)),
            vec![
                ApplyOutcome::Applied,
                ApplyOutcome::Applied,
                ApplyOutcome::Applied
            ]
        );
        let state = replica
            .canonical_state(&crate::canonical_locator(canonical_id).expect("canonical"))
            .expect("state");
        assert_eq!(state.version_no, 2);
        assert!(state.is_tombstone);
    }

    proptest! {
        #[test]
        fn duplicate_replays_remain_one_applied_record(duplicate_count in 1usize..6) {
            let mut replica = ReplicaState::new(project_space_id(), authority());
            prop_assert_eq!(
                replica.apply(publish_membership(
                    writer(),
                    SpaceCapability::Publish,
                    "membership:writer:publish"
                )),
                ApplyOutcome::Applied
            );
            let canonical_id = Uuid::new_v4();
            let evidence = projected_records(writer(), canonical_id, 1)
                .into_iter()
                .next()
                .expect("evidence");

            let mut outcomes = Vec::new();
            for _ in 0..duplicate_count {
                outcomes.push(replica.apply(evidence.clone()));
            }

            prop_assert_eq!(outcomes[0].clone(), ApplyOutcome::Applied);
            for outcome in outcomes.iter().skip(1) {
                prop_assert_eq!(
                    outcome.clone(),
                    ApplyOutcome::Duplicate {
                        reason: DuplicateReason::ExactDuplicate
                    }
                );
            }
        }

        #[test]
        fn version_permutations_only_accept_contiguous_prefix(
            permutation in prop::collection::vec(1u8..=3, 3).prop_filter(
                "must be a permutation",
                |values| {
                    let mut sorted = values.clone();
                    sorted.sort_unstable();
                    sorted == vec![1, 2, 3]
                }
            )
        ) {
            let mut replica = ReplicaState::new(project_space_id(), authority());
            prop_assert_eq!(
                replica.apply(publish_membership(
                    writer(),
                    SpaceCapability::Publish,
                    "membership:writer:publish"
                )),
                ApplyOutcome::Applied
            );

            let canonical_id = Uuid::new_v4();
            let mut expected_next = 1u32;
            for version in permutation {
                let mut records = projected_records(writer(), canonical_id, i32::from(version)).into_iter();
                let evidence = records.next().expect("evidence");
                let memory = records.next().expect("memory");
                prop_assert_eq!(replica.apply(evidence), ApplyOutcome::Applied);
                let outcome = replica.apply(memory);
                if u32::from(version) == expected_next {
                    prop_assert_eq!(outcome, ApplyOutcome::Applied);
                    expected_next += 1;
                } else {
                    prop_assert_eq!(
                        outcome,
                        ApplyOutcome::Quarantined {
                            reason: QuarantineReason::VersionGap {
                                canonical_id: crate::canonical_locator(canonical_id).expect("canonical"),
                                expected: expected_next,
                                actual: u32::from(version),
                            }
                        }
                    );
                }
            }
        }

        #[test]
        fn replaying_accepted_sequence_twice_is_all_duplicates_on_second_pass(max_version in 1u8..=4) {
            let mut replica = ReplicaState::new(project_space_id(), authority());
            prop_assert_eq!(
                replica.apply(publish_membership(
                    writer(),
                    SpaceCapability::Publish,
                    "membership:writer:publish"
                )),
                ApplyOutcome::Applied
            );
            let canonical_id = Uuid::new_v4();
            let mut records = Vec::new();
            for version in 1..=max_version {
                records.extend(projected_records(writer(), canonical_id, i32::from(version)));
            }

            let first = replica.replay(records.clone());
            prop_assert!(first.iter().all(|outcome| *outcome == ApplyOutcome::Applied));

            let second = replica.replay(records);
            let is_all_duplicates = second.iter().all(|outcome| matches!(
                outcome,
                ApplyOutcome::Duplicate {
                    reason: DuplicateReason::ExactDuplicate
                }
            ));
            prop_assert_eq!(is_all_duplicates, true);
        }
    }
}
