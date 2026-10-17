// SPDX-License-Identifier: AGPL-3.0-or-later

use chrono::{DateTime, Utc};
use codifix_record::MemoryEntryResponse;

use crate::{
    AuthorIdentity, CanonicalStateRecord, EvidenceRecord, FederationError, FederationRecord,
    MemoryVersionRecord, ProjectSpaceId, RecordLocator, RelatedMemoryRef, SharedRecordEnvelope,
    SourceIdentity, TransportRecord, canonical_locator, canonical_state_locator, evidence_locator,
    memory_id_locator, memory_version_locator, validate_finite_confidence, validate_uuid,
    version_to_u32,
};

#[derive(Debug, Clone)]
pub struct ProjectionContext {
    pub source_identity: SourceIdentity,
    pub author: AuthorIdentity,
}

#[derive(Debug, Clone)]
pub struct ProjectedMemoryEntry {
    pub project_space_id: ProjectSpaceId,
    pub memory_version: TransportRecord,
    pub canonical_state: TransportRecord,
    pub evidence: Vec<TransportRecord>,
}

impl ProjectedMemoryEntry {
    pub fn into_records(self) -> Vec<TransportRecord> {
        let mut records = Vec::with_capacity(self.evidence.len() + 2);
        records.extend(self.evidence);
        records.push(self.memory_version);
        records.push(self.canonical_state);
        records
    }
}

struct EvidenceProjection<'a> {
    default_observed_at: DateTime<Utc>,
    source: &'a codifix_record::MemorySourceRecord,
    project_space_id: &'a ProjectSpaceId,
    source_identity: &'a SourceIdentity,
    author: &'a AuthorIdentity,
    memory_record_id: &'a RecordLocator,
    memory_id: &'a RecordLocator,
    canonical_id: &'a RecordLocator,
    version_no: u32,
    evidence_record_id: RecordLocator,
}

pub fn project_memory_entry(
    entry: &MemoryEntryResponse,
    context: &ProjectionContext,
) -> Result<ProjectedMemoryEntry, FederationError> {
    validate_uuid("memory_id", entry.id)?;
    validate_uuid("canonical_id", entry.canonical_id)?;
    validate_finite_confidence(entry.confidence)?;
    for related in &entry.related_memories {
        validate_uuid("related_memory_id", related.memory_id)?;
        validate_finite_confidence(related.confidence)?;
    }

    let project_space_id = ProjectSpaceId::new(entry.project.clone())?;
    let version_no = version_to_u32(entry.version_no)?;
    let memory_id = memory_id_locator(entry.id)?;
    let canonical_id = canonical_locator(entry.canonical_id)?;
    let memory_record_id = memory_version_locator(entry.canonical_id, entry.version_no)?;
    let state_record_id = canonical_state_locator(entry.canonical_id, entry.version_no)?;

    let mut evidence_refs = Vec::with_capacity(entry.sources.len());
    let mut evidence = Vec::with_capacity(entry.sources.len());
    for source in &entry.sources {
        validate_uuid("source_id", source.id)?;
        let evidence_ref = evidence_locator(entry.id, source.id)?;
        evidence_refs.push(evidence_ref.clone());
        evidence.push(project_evidence_record(EvidenceProjection {
            default_observed_at: entry.updated_at,
            source,
            project_space_id: &project_space_id,
            source_identity: &context.source_identity,
            author: &context.author,
            memory_record_id: &memory_record_id,
            memory_id: &memory_id,
            canonical_id: &canonical_id,
            version_no,
            evidence_record_id: evidence_ref,
        })?);
    }

    let memory_version = TransportRecord::new(
        context.source_identity.clone(),
        memory_record_id.clone(),
        SharedRecordEnvelope::new(
            memory_record_id.clone(),
            project_space_id.clone(),
            context.author.clone(),
            entry.updated_at,
            None,
            FederationRecord::MemoryVersion(MemoryVersionRecord {
                memory_id: memory_id.clone(),
                canonical_id: canonical_id.clone(),
                version_no,
                canonical_text: entry.canonical_text.clone(),
                summary: entry.summary.clone(),
                memory_type: entry.memory_type.clone(),
                importance: entry.importance,
                confidence: entry.confidence,
                status: entry.status.clone(),
                tags: entry.tags.clone(),
                evidence_refs,
                related_memories: entry
                    .related_memories
                    .iter()
                    .map(|related| {
                        Ok(RelatedMemoryRef {
                            memory_id: memory_id_locator(related.memory_id)?,
                            relation_type: related.relation_type.clone(),
                            summary: related.summary.clone(),
                            memory_type: related.memory_type.clone(),
                            confidence: related.confidence,
                        })
                    })
                    .collect::<Result<Vec<_>, FederationError>>()?,
                created_at: entry.created_at,
                updated_at: entry.updated_at,
                is_tombstone: entry.is_tombstone,
            }),
        ),
    )?;

    let canonical_state = TransportRecord::new(
        context.source_identity.clone(),
        state_record_id.clone(),
        SharedRecordEnvelope::new(
            state_record_id,
            project_space_id.clone(),
            context.author.clone(),
            entry.updated_at,
            Some(memory_record_id.clone()),
            FederationRecord::CanonicalState(CanonicalStateRecord {
                canonical_id: canonical_id.clone(),
                memory_id: memory_id.clone(),
                version_no,
                is_tombstone: entry.is_tombstone,
                status: entry.status.clone(),
                confidence: entry.confidence,
                updated_at: entry.updated_at,
            }),
        ),
    )?;

    Ok(ProjectedMemoryEntry {
        project_space_id,
        memory_version,
        canonical_state,
        evidence,
    })
}

fn project_evidence_record(
    projection: EvidenceProjection<'_>,
) -> Result<TransportRecord, FederationError> {
    let observed_at = projection
        .source
        .provenance
        .as_ref()
        .map(|provenance| provenance.checked_at)
        .unwrap_or(projection.default_observed_at);
    let evidence_record = EvidenceRecord::from_source(
        projection.evidence_record_id.clone(),
        projection.memory_id.clone(),
        projection.canonical_id.clone(),
        projection.version_no,
        projection.source,
        observed_at,
    )?;
    TransportRecord::new(
        projection.source_identity.clone(),
        projection.evidence_record_id.clone(),
        SharedRecordEnvelope::new(
            projection.evidence_record_id,
            projection.project_space_id.clone(),
            projection.author.clone(),
            observed_at,
            Some(projection.memory_record_id.clone()),
            FederationRecord::Evidence(evidence_record),
        ),
    )
}

#[cfg(test)]
mod tests {
    use chrono::TimeZone;
    use codifix_record::{
        MemoryRelationType, MemorySourceRecord, MemoryStatus, MemoryType, RelatedMemorySummary,
        SourceKind, SourceProvenanceRecord, SourceProvenanceStatus,
    };
    use uuid::Uuid;

    use super::*;

    fn sample_entry() -> MemoryEntryResponse {
        let checked_at = Utc
            .with_ymd_and_hms(2026, 9, 4, 10, 0, 0)
            .single()
            .expect("valid timestamp");
        let created_at = Utc
            .with_ymd_and_hms(2026, 9, 4, 9, 0, 0)
            .single()
            .expect("valid timestamp");
        let updated_at = Utc
            .with_ymd_and_hms(2026, 9, 4, 11, 0, 0)
            .single()
            .expect("valid timestamp");
        MemoryEntryResponse {
            id: Uuid::new_v4(),
            project: "memory".to_string(),
            canonical_text: "ATProto adapter runs over a pure federation core.".to_string(),
            summary: "Pure federation core".to_string(),
            memory_type: MemoryType::Architecture,
            importance: 4,
            confidence: 0.9,
            status: MemoryStatus::Active,
            tags: vec!["federation".to_string(), "protocol".to_string()],
            sources: vec![MemorySourceRecord {
                id: Uuid::new_v4(),
                task_id: Some(Uuid::new_v4()),
                file_path: Some("docs/spec.md".to_string()),
                git_commit: Some("abc123".to_string()),
                symbol_name: None,
                symbol_kind: None,
                source_kind: SourceKind::File,
                excerpt: Some("Pure transport-neutral reducer.".to_string()),
                provenance: Some(SourceProvenanceRecord {
                    status: SourceProvenanceStatus::Verified,
                    checked_at,
                    reason: None,
                    resolved_path: Some("/repo/docs/spec.md".to_string()),
                }),
            }],
            related_memories: vec![RelatedMemorySummary {
                memory_id: Uuid::new_v4(),
                relation_type: MemoryRelationType::DependsOn,
                summary: "Reducer invariants".to_string(),
                memory_type: MemoryType::Decision,
                confidence: 0.7,
            }],
            embedding_spaces: Vec::new(),
            created_at,
            updated_at,
            canonical_id: Uuid::new_v4(),
            version_no: 1,
            is_tombstone: false,
        }
    }

    fn sample_context() -> ProjectionContext {
        ProjectionContext {
            source_identity: SourceIdentity::new("did:example:replica").expect("source"),
            author: AuthorIdentity::new("did:example:alice").expect("author"),
        }
    }

    #[test]
    fn projection_preserves_fields_and_dependency_order() {
        let entry = sample_entry();
        let projected = project_memory_entry(&entry, &sample_context()).expect("projection");

        assert_eq!(projected.project_space_id.as_str(), "memory");
        match &projected.memory_version.envelope().record {
            FederationRecord::MemoryVersion(memory) => {
                assert_eq!(memory.summary, entry.summary);
                assert_eq!(memory.evidence_refs.len(), 1);
                assert_eq!(memory.related_memories.len(), 1);
            }
            other => panic!("unexpected record: {other:?}"),
        }

        let records = projected.into_records();
        assert_eq!(records.len(), 3);
        assert!(matches!(
            records[0].envelope().record,
            FederationRecord::Evidence(_)
        ));
        assert!(matches!(
            records[1].envelope().record,
            FederationRecord::MemoryVersion(_)
        ));
        assert!(matches!(
            records[2].envelope().record,
            FederationRecord::CanonicalState(_)
        ));
    }

    #[test]
    fn projection_rejects_non_finite_confidence() {
        let mut entry = sample_entry();
        entry.confidence = f32::NAN;
        let error = project_memory_entry(&entry, &sample_context())
            .expect_err("non-finite confidence should fail");
        match error {
            FederationError::NonFiniteConfidence { .. } => {}
            other => panic!("unexpected error: {other:?}"),
        }
    }
}
