// SPDX-License-Identifier: AGPL-3.0-or-later

use crate::{
    AtUri, CANONICAL_STATE_NSID, CanonicalState, CanonicalStateData, CommonFieldsData,
    EVIDENCE_NSID, Evidence, EvidenceData, MEMBERSHIP_NSID, MEMORY_VERSION_NSID, MemAtprotoError,
    Membership, MembershipData, MemoryVersion, MemoryVersionData, RELATION_NSID, REVOCATION_NSID,
    RelatedMemoryData, Relation, RelationData, Revocation, RevocationData, SourceProvenance,
    SourceProvenanceData, TransportEnvelope, TransportMetadata, TransportMetadataData,
    confidence_from_ppm, confidence_to_ppm, object_from_unknown, object_to_unknown,
    unwrap_unknown_record, uuid_rkey, wrap_unknown_record,
};
use atrium_api::types::{
    Object, Unknown,
    string::{AtIdentifier, Cid, Datetime, Nsid, RecordKey},
};
use chrono::{DateTime, SecondsFormat, Utc};
use codifix_federation as federation;
use serde::{Deserialize, Serialize, de::DeserializeOwned};
use uuid::Uuid;

pub type FederationEnvelope = federation::SharedRecordEnvelope<federation::FederationRecord>;

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct EncodedAtRecord {
    pub collection: Nsid,
    pub rkey: RecordKey,
    pub value: Unknown,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
struct SourceProvenanceBridge {
    status: String,
    checked_at: DateTime<Utc>,
    reason: Option<String>,
    resolved_path: Option<String>,
}

pub fn collection_for_record(record: &federation::FederationRecord) -> &'static str {
    match record {
        federation::FederationRecord::MemoryVersion(_) => MEMORY_VERSION_NSID,
        federation::FederationRecord::CanonicalState(_) => CANONICAL_STATE_NSID,
        federation::FederationRecord::Evidence(_) => EVIDENCE_NSID,
        federation::FederationRecord::Relation(_) => RELATION_NSID,
        federation::FederationRecord::Membership(_) => MEMBERSHIP_NSID,
        federation::FederationRecord::Revocation(_) => REVOCATION_NSID,
    }
}

pub fn record_locator_rkey(
    record_id: &federation::RecordLocator,
) -> Result<RecordKey, MemAtprotoError> {
    match Uuid::parse_str(record_id.as_str()) {
        Ok(uuid) => uuid_rkey(uuid),
        Err(_) => uuid_rkey(Uuid::new_v5(
            &Uuid::NAMESPACE_URL,
            format!(
                "https://codifix.dev/federation/record-id/{}",
                record_id.as_str()
            )
            .as_bytes(),
        )),
    }
}

pub fn encode_record(envelope: &FederationEnvelope) -> Result<EncodedAtRecord, MemAtprotoError> {
    let collection = collection_for_record(&envelope.record)
        .parse::<Nsid>()
        .map_err(|err| MemAtprotoError::message(err.to_string()))?;
    let value = match &envelope.record {
        federation::FederationRecord::MemoryVersion(record) => {
            object_to_unknown(&Object::from(encode_memory_version(envelope, record)?))?
        }
        federation::FederationRecord::CanonicalState(record) => {
            object_to_unknown(&Object::from(encode_canonical_state(envelope, record)?))?
        }
        federation::FederationRecord::Evidence(record) => {
            object_to_unknown(&Object::from(encode_evidence(envelope, record)?))?
        }
        federation::FederationRecord::Relation(record) => {
            object_to_unknown(&Object::from(encode_relation(envelope, record)?))?
        }
        federation::FederationRecord::Membership(record) => {
            object_to_unknown(&Object::from(encode_membership(envelope, record)?))?
        }
        federation::FederationRecord::Revocation(record) => {
            object_to_unknown(&Object::from(encode_revocation(envelope, record)?))?
        }
    };

    Ok(EncodedAtRecord {
        collection,
        rkey: record_locator_rkey(&envelope.record_id)?,
        value,
    })
}

pub fn decode_record(encoded: EncodedAtRecord) -> Result<FederationEnvelope, MemAtprotoError> {
    let record_type = unknown_record_type(&encoded.value)?;
    if encoded.collection.as_ref() != record_type {
        return Err(MemAtprotoError::message(format!(
            "collection/type mismatch: collection={} type={record_type}",
            encoded.collection.as_ref()
        )));
    }

    let decoded = match record_type.as_str() {
        MEMORY_VERSION_NSID => decode_memory_version(encoded.value),
        CANONICAL_STATE_NSID => decode_canonical_state(encoded.value),
        EVIDENCE_NSID => decode_evidence(encoded.value),
        RELATION_NSID => decode_relation(encoded.value),
        MEMBERSHIP_NSID => decode_membership(encoded.value),
        REVOCATION_NSID => decode_revocation(encoded.value),
        other => Err(MemAtprotoError::message(format!(
            "unsupported AT record type: {other}"
        ))),
    }?;

    let expected_rkey = record_locator_rkey(&decoded.record_id)?;
    if encoded.rkey != expected_rkey {
        return Err(MemAtprotoError::message(format!(
            "record key mismatch: supplied={} expected={}",
            encoded.rkey.as_ref(),
            expected_rkey.as_ref()
        )));
    }
    Ok(decoded)
}

pub fn attach_transport(
    repo: &AtIdentifier,
    encoded: EncodedAtRecord,
    cid: Cid,
) -> Result<TransportEnvelope, MemAtprotoError> {
    let uri = format!(
        "at://{}/{}/{}",
        repo.as_ref(),
        encoded.collection.as_ref(),
        encoded.rkey.as_ref()
    )
    .parse::<AtUri>()?;

    Ok(wrap_unknown_record(
        encoded.value,
        Object::from(TransportMetadataData {
            uri,
            cid,
            rkey: encoded.rkey,
        }),
    ))
}

pub fn detach_transport(
    envelope: TransportEnvelope,
) -> Result<(FederationEnvelope, TransportMetadata), MemAtprotoError> {
    let (value, transport) = unwrap_unknown_record(envelope);
    let collection = transport
        .data
        .uri
        .collection()
        .cloned()
        .ok_or_else(|| MemAtprotoError::message("transport URI is missing collection"))?;
    let uri_rkey = transport
        .data
        .uri
        .rkey()
        .ok_or_else(|| MemAtprotoError::message("transport URI is missing record key"))?;
    if uri_rkey != &transport.data.rkey {
        return Err(MemAtprotoError::message(
            "transport URI record key does not match transport metadata",
        ));
    }

    let encoded = EncodedAtRecord {
        collection,
        rkey: transport.data.rkey.clone(),
        value,
    };
    Ok((decode_record(encoded)?, transport))
}

fn encode_memory_version(
    envelope: &FederationEnvelope,
    record: &federation::MemoryVersionRecord,
) -> Result<MemoryVersionData, MemAtprotoError> {
    Ok(MemoryVersionData {
        record_type: MEMORY_VERSION_NSID.to_owned(),
        common: common_fields(envelope)?,
        memory_id: record.memory_id.as_str().to_owned(),
        canonical_id: record.canonical_id.as_str().to_owned(),
        version_no: record.version_no,
        canonical_text: record.canonical_text.clone(),
        summary: record.summary.clone(),
        memory_type: enum_string(&record.memory_type)?,
        importance: record.importance,
        confidence_ppm: confidence_to_ppm(record.confidence)?,
        status: enum_string(&record.status)?,
        tags: record.tags.clone(),
        evidence_refs: record
            .evidence_refs
            .iter()
            .map(|locator| locator.as_str().to_owned())
            .collect(),
        related_memories: record
            .related_memories
            .iter()
            .map(|related| {
                Ok(RelatedMemoryData {
                    memory_id: related.memory_id.as_str().to_owned(),
                    relation_type: enum_string(&related.relation_type)?,
                    summary: related.summary.clone(),
                    memory_type: enum_string(&related.memory_type)?,
                    confidence_ppm: confidence_to_ppm(related.confidence)?,
                }
                .into())
            })
            .collect::<Result<Vec<_>, MemAtprotoError>>()?,
        created_at: datetime_from_chrono(record.created_at)?,
        updated_at: datetime_from_chrono(record.updated_at)?,
        is_tombstone: record.is_tombstone,
    })
}

fn encode_canonical_state(
    envelope: &FederationEnvelope,
    record: &federation::CanonicalStateRecord,
) -> Result<CanonicalStateData, MemAtprotoError> {
    Ok(CanonicalStateData {
        record_type: CANONICAL_STATE_NSID.to_owned(),
        common: common_fields(envelope)?,
        canonical_id: record.canonical_id.as_str().to_owned(),
        memory_id: record.memory_id.as_str().to_owned(),
        version_no: record.version_no,
        is_tombstone: record.is_tombstone,
        status: enum_string(&record.status)?,
        confidence_ppm: confidence_to_ppm(record.confidence)?,
        updated_at: datetime_from_chrono(record.updated_at)?,
    })
}

fn encode_evidence(
    envelope: &FederationEnvelope,
    record: &federation::EvidenceRecord,
) -> Result<EvidenceData, MemAtprotoError> {
    Ok(EvidenceData {
        record_type: EVIDENCE_NSID.to_owned(),
        common: common_fields(envelope)?,
        evidence_id: record.evidence_id.as_str().to_owned(),
        source_id: record.source_id.as_str().to_owned(),
        memory_id: record.memory_id.as_str().to_owned(),
        canonical_id: record.canonical_id.as_str().to_owned(),
        version_no: record.version_no,
        source_kind: enum_string(&record.source_kind)?,
        task_id: record
            .task_id
            .as_ref()
            .map(|value| value.as_str().to_owned()),
        file_path: record.file_path.clone(),
        git_commit: record.git_commit.clone(),
        symbol_name: record.symbol_name.clone(),
        symbol_kind: record.symbol_kind.clone(),
        excerpt: record.excerpt.clone(),
        provenance: record
            .provenance
            .as_ref()
            .map(encode_provenance)
            .transpose()?,
        observed_at: datetime_from_chrono(record.observed_at)?,
    })
}

fn encode_relation(
    envelope: &FederationEnvelope,
    record: &federation::RelationRecord,
) -> Result<RelationData, MemAtprotoError> {
    Ok(RelationData {
        record_type: RELATION_NSID.to_owned(),
        common: common_fields(envelope)?,
        src_canonical_id: record.src_canonical_id.as_str().to_owned(),
        relation_type: enum_string(&record.relation_type)?,
        dst_canonical_id: record.dst_canonical_id.as_str().to_owned(),
        asserted_at: datetime_from_chrono(record.asserted_at)?,
    })
}

fn encode_membership(
    envelope: &FederationEnvelope,
    record: &federation::MembershipRecord,
) -> Result<MembershipData, MemAtprotoError> {
    Ok(MembershipData {
        record_type: MEMBERSHIP_NSID.to_owned(),
        common: common_fields(envelope)?,
        member: record.member.as_str().to_owned(),
        capability: enum_string(&record.capability)?,
    })
}

fn encode_revocation(
    envelope: &FederationEnvelope,
    record: &federation::RevocationRecord,
) -> Result<RevocationData, MemAtprotoError> {
    Ok(RevocationData {
        record_type: REVOCATION_NSID.to_owned(),
        common: common_fields(envelope)?,
        member: record.member.as_str().to_owned(),
        capability: record.capability.as_ref().map(enum_string).transpose()?,
        reason: record.reason.clone(),
    })
}

fn decode_memory_version(value: Unknown) -> Result<FederationEnvelope, MemAtprotoError> {
    let object: MemoryVersion = object_from_unknown(value)?;
    let common = object.data.common;
    let record = federation::MemoryVersionRecord {
        memory_id: federation::RecordLocator::new(object.data.memory_id)?,
        canonical_id: federation::RecordLocator::new(object.data.canonical_id)?,
        version_no: object.data.version_no,
        canonical_text: object.data.canonical_text,
        summary: object.data.summary,
        memory_type: parse_string_enum(object.data.memory_type)?,
        importance: object.data.importance,
        confidence: confidence_from_ppm(object.data.confidence_ppm)?,
        status: parse_string_enum(object.data.status)?,
        tags: object.data.tags,
        evidence_refs: object
            .data
            .evidence_refs
            .into_iter()
            .map(federation::RecordLocator::new)
            .collect::<Result<Vec<_>, _>>()?,
        related_memories: object
            .data
            .related_memories
            .into_iter()
            .map(|related| {
                Ok(federation::RelatedMemoryRef {
                    memory_id: federation::RecordLocator::new(related.data.memory_id)?,
                    relation_type: parse_string_enum(related.data.relation_type)?,
                    summary: related.data.summary,
                    memory_type: parse_string_enum(related.data.memory_type)?,
                    confidence: confidence_from_ppm(related.data.confidence_ppm)?,
                })
            })
            .collect::<Result<Vec<_>, MemAtprotoError>>()?,
        created_at: chrono_from_datetime(&object.data.created_at)?,
        updated_at: chrono_from_datetime(&object.data.updated_at)?,
        is_tombstone: object.data.is_tombstone,
    };
    common.into_federation(federation::FederationRecord::MemoryVersion(record))
}

fn decode_canonical_state(value: Unknown) -> Result<FederationEnvelope, MemAtprotoError> {
    let object: CanonicalState = object_from_unknown(value)?;
    let common = object.data.common;
    let record = federation::CanonicalStateRecord {
        canonical_id: federation::RecordLocator::new(object.data.canonical_id)?,
        memory_id: federation::RecordLocator::new(object.data.memory_id)?,
        version_no: object.data.version_no,
        is_tombstone: object.data.is_tombstone,
        status: parse_string_enum(object.data.status)?,
        confidence: confidence_from_ppm(object.data.confidence_ppm)?,
        updated_at: chrono_from_datetime(&object.data.updated_at)?,
    };
    common.into_federation(federation::FederationRecord::CanonicalState(record))
}

fn decode_evidence(value: Unknown) -> Result<FederationEnvelope, MemAtprotoError> {
    let object: Evidence = object_from_unknown(value)?;
    let common = object.data.common;
    let record = federation::EvidenceRecord {
        evidence_id: federation::RecordLocator::new(object.data.evidence_id)?,
        source_id: federation::RecordLocator::new(object.data.source_id)?,
        memory_id: federation::RecordLocator::new(object.data.memory_id)?,
        canonical_id: federation::RecordLocator::new(object.data.canonical_id)?,
        version_no: object.data.version_no,
        source_kind: parse_string_enum(object.data.source_kind)?,
        task_id: object
            .data
            .task_id
            .map(federation::RecordLocator::new)
            .transpose()?,
        file_path: object.data.file_path,
        git_commit: object.data.git_commit,
        symbol_name: object.data.symbol_name,
        symbol_kind: object.data.symbol_kind,
        excerpt: object.data.excerpt,
        provenance: object.data.provenance.map(decode_provenance).transpose()?,
        observed_at: chrono_from_datetime(&object.data.observed_at)?,
    };
    common.into_federation(federation::FederationRecord::Evidence(record))
}

fn decode_relation(value: Unknown) -> Result<FederationEnvelope, MemAtprotoError> {
    let object: Relation = object_from_unknown(value)?;
    let common = object.data.common;
    let record = federation::RelationRecord {
        src_canonical_id: federation::RecordLocator::new(object.data.src_canonical_id)?,
        relation_type: parse_string_enum(object.data.relation_type)?,
        dst_canonical_id: federation::RecordLocator::new(object.data.dst_canonical_id)?,
        asserted_at: chrono_from_datetime(&object.data.asserted_at)?,
    };
    common.into_federation(federation::FederationRecord::Relation(record))
}

fn decode_membership(value: Unknown) -> Result<FederationEnvelope, MemAtprotoError> {
    let object: Membership = object_from_unknown(value)?;
    let common = object.data.common;
    let record = federation::MembershipRecord {
        member: federation::AuthorIdentity::new(object.data.member)?,
        capability: parse_string_enum(object.data.capability)?,
    };
    common.into_federation(federation::FederationRecord::Membership(record))
}

fn decode_revocation(value: Unknown) -> Result<FederationEnvelope, MemAtprotoError> {
    let object: Revocation = object_from_unknown(value)?;
    let common = object.data.common;
    let record = federation::RevocationRecord {
        member: federation::AuthorIdentity::new(object.data.member)?,
        capability: object.data.capability.map(parse_string_enum).transpose()?,
        reason: object.data.reason,
    };
    common.into_federation(federation::FederationRecord::Revocation(record))
}

fn common_fields(envelope: &FederationEnvelope) -> Result<CommonFieldsData, MemAtprotoError> {
    Ok(CommonFieldsData {
        schema_version: envelope.schema_version,
        record_id: envelope.record_id.as_str().to_owned(),
        project_space_id: envelope.project_space_id.as_str().to_owned(),
        author: envelope.author.as_str().to_owned(),
        authored_at: datetime_from_chrono(envelope.authored_at)?,
        causation_id: envelope
            .causation_id
            .as_ref()
            .map(|value| value.as_str().to_owned()),
    })
}

impl CommonFieldsData {
    fn into_federation<T>(
        self,
        record: T,
    ) -> Result<federation::SharedRecordEnvelope<T>, MemAtprotoError> {
        Ok(federation::SharedRecordEnvelope {
            schema_version: self.schema_version,
            record_id: federation::RecordLocator::new(self.record_id)?,
            project_space_id: federation::ProjectSpaceId::new(self.project_space_id)?,
            author: federation::AuthorIdentity::new(self.author)?,
            authored_at: chrono_from_datetime(&self.authored_at)?,
            causation_id: self
                .causation_id
                .map(federation::RecordLocator::new)
                .transpose()?,
            record,
        })
    }
}

fn unknown_record_type(value: &Unknown) -> Result<String, MemAtprotoError> {
    serde_json::to_value(value)?
        .get("$type")
        .and_then(|value| value.as_str())
        .map(str::to_owned)
        .ok_or_else(|| MemAtprotoError::message("AT record is missing $type"))
}

fn enum_string<T>(value: &T) -> Result<String, MemAtprotoError>
where
    T: Serialize,
{
    match serde_json::to_value(value)? {
        serde_json::Value::String(value) => Ok(value),
        other => Err(MemAtprotoError::message(format!(
            "expected string enum representation, got {other}"
        ))),
    }
}

fn parse_string_enum<T>(value: String) -> Result<T, MemAtprotoError>
where
    T: DeserializeOwned,
{
    Ok(serde_json::from_value(serde_json::Value::String(value))?)
}

fn encode_provenance(value: &impl Serialize) -> Result<SourceProvenance, MemAtprotoError> {
    let bridge: SourceProvenanceBridge = json_bridge(value)?;
    Ok(Object::from(SourceProvenanceData {
        status: bridge.status,
        checked_at: datetime_from_chrono(bridge.checked_at)?,
        reason: bridge.reason,
        resolved_path: bridge.resolved_path,
    }))
}

fn decode_provenance<T>(value: SourceProvenance) -> Result<T, MemAtprotoError>
where
    T: DeserializeOwned,
{
    json_bridge(&SourceProvenanceBridge {
        status: value.data.status,
        checked_at: chrono_from_datetime(&value.data.checked_at)?,
        reason: value.data.reason,
        resolved_path: value.data.resolved_path,
    })
}

fn json_bridge<From, To>(value: &From) -> Result<To, MemAtprotoError>
where
    From: Serialize,
    To: DeserializeOwned,
{
    Ok(serde_json::from_value(serde_json::to_value(value)?)?)
}

fn datetime_from_chrono(value: DateTime<Utc>) -> Result<Datetime, MemAtprotoError> {
    value
        .to_rfc3339_opts(SecondsFormat::Micros, true)
        .parse()
        .map_err(Into::into)
}

fn chrono_from_datetime(value: &Datetime) -> Result<DateTime<Utc>, MemAtprotoError> {
    Ok(DateTime::parse_from_rfc3339(value.as_str())?.with_timezone(&Utc))
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::federation::CanonicalJsonCodec;
    use serde_json::json;

    fn sample_envelope(
        record_id: &str,
        record: federation::FederationRecord,
    ) -> FederationEnvelope {
        federation::SharedRecordEnvelope::new(
            federation::RecordLocator::new(record_id).unwrap(),
            federation::ProjectSpaceId::new("project.memory").unwrap(),
            federation::AuthorIdentity::new("did:example:alice").unwrap(),
            DateTime::parse_from_rfc3339("2026-09-04T09:30:00Z")
                .unwrap()
                .with_timezone(&Utc),
            Some(
                federation::RecordLocator::new("task:123e4567-e89b-12d3-a456-426614174000")
                    .unwrap(),
            ),
            record,
        )
    }

    fn sample_memory_version_envelope() -> FederationEnvelope {
        let record: federation::MemoryVersionRecord = serde_json::from_value(json!({
            "memory_id": "memory:123e4567-e89b-12d3-a456-426614174001",
            "canonical_id": "canonical:123e4567-e89b-12d3-a456-426614174001",
            "version_no": 7,
            "canonical_text": "Canonical memory text",
            "summary": "Adapter record",
            "memory_type": "implementation",
            "importance": 8,
            "confidence": 0.75,
            "status": "active",
            "tags": ["adapter", "offline"],
            "evidence_refs": ["evidence:123e4567-e89b-12d3-a456-426614174001:123e4567-e89b-12d3-a456-426614174002"],
            "related_memories": [{
                "memory_id": "memory:123e4567-e89b-12d3-a456-426614174099",
                "relation_type": "supports",
                "summary": "Related memory",
                "memory_type": "decision",
                "confidence": 0.5
            }],
            "created_at": "2026-09-04T09:00:00Z",
            "updated_at": "2026-09-04T09:30:00Z",
            "is_tombstone": false
        }))
        .unwrap();
        sample_envelope(
            "memory-version:123e4567-e89b-12d3-a456-426614174001:7",
            federation::FederationRecord::MemoryVersion(record),
        )
    }

    fn sample_canonical_state_envelope() -> FederationEnvelope {
        let record: federation::CanonicalStateRecord = serde_json::from_value(json!({
            "canonical_id": "canonical:123e4567-e89b-12d3-a456-426614174001",
            "memory_id": "memory:123e4567-e89b-12d3-a456-426614174001",
            "version_no": 7,
            "is_tombstone": false,
            "status": "active",
            "confidence": 0.75,
            "updated_at": "2026-09-04T09:30:00Z"
        }))
        .unwrap();
        sample_envelope(
            "canonical-state:123e4567-e89b-12d3-a456-426614174001:7",
            federation::FederationRecord::CanonicalState(record),
        )
    }

    fn sample_evidence_envelope() -> FederationEnvelope {
        let record: federation::EvidenceRecord = serde_json::from_value(json!({
            "evidence_id": "evidence:123e4567-e89b-12d3-a456-426614174001:123e4567-e89b-12d3-a456-426614174002",
            "source_id": "source:123e4567-e89b-12d3-a456-426614174002",
            "memory_id": "memory:123e4567-e89b-12d3-a456-426614174001",
            "canonical_id": "canonical:123e4567-e89b-12d3-a456-426614174001",
            "version_no": 7,
            "source_kind": "file",
            "task_id": "task:123e4567-e89b-12d3-a456-426614174000",
            "file_path": "src/lib.rs",
            "git_commit": "deadbeef",
            "symbol_name": "encode_record",
            "symbol_kind": "function",
            "excerpt": "adapter boundary",
            "provenance": {
                "status": "verified",
                "checked_at": "2026-09-04T10:00:00Z",
                "reason": "present",
                "resolved_path": "/repo/src/lib.rs"
            },
            "observed_at": "2026-09-04T09:35:00Z"
        }))
        .unwrap();
        sample_envelope(
            "evidence:123e4567-e89b-12d3-a456-426614174001:123e4567-e89b-12d3-a456-426614174002",
            federation::FederationRecord::Evidence(record),
        )
    }

    fn sample_relation_envelope() -> FederationEnvelope {
        let record: federation::RelationRecord = serde_json::from_value(json!({
            "src_canonical_id": "canonical:123e4567-e89b-12d3-a456-426614174001",
            "relation_type": "supports",
            "dst_canonical_id": "canonical:123e4567-e89b-12d3-a456-426614174099",
            "asserted_at": "2026-09-04T09:40:00Z"
        }))
        .unwrap();
        sample_envelope(
            "relation:123e4567-e89b-12d3-a456-426614174001:supports:123e4567-e89b-12d3-a456-426614174099",
            federation::FederationRecord::Relation(record),
        )
    }

    fn sample_membership_envelope() -> FederationEnvelope {
        let record: federation::MembershipRecord = serde_json::from_value(json!({
            "member": "did:example:bob",
            "capability": "publish"
        }))
        .unwrap();
        sample_envelope(
            "membership:did:example:bob:publish",
            federation::FederationRecord::Membership(record),
        )
    }

    fn sample_revocation_envelope() -> FederationEnvelope {
        let record: federation::RevocationRecord = serde_json::from_value(json!({
            "member": "did:example:bob",
            "capability": "publish",
            "reason": "membership removed"
        }))
        .unwrap();
        sample_envelope(
            "revocation:did:example:bob:publish",
            federation::FederationRecord::Revocation(record),
        )
    }

    fn assert_round_trip(original: FederationEnvelope) {
        let encoded = encode_record(&original).unwrap();
        let decoded = decode_record(encoded).unwrap();
        assert_eq!(
            CanonicalJsonCodec::encode(&original).unwrap(),
            CanonicalJsonCodec::encode(&decoded).unwrap()
        );
    }

    fn assert_lexicon_matches_encoded_record(encoded: &EncodedAtRecord) {
        let lexicon = crate::UNSPECCED_LEXICONS
            .iter()
            .map(|raw| serde_json::from_str::<serde_json::Value>(raw.json).unwrap())
            .find(|lexicon| lexicon["id"] == encoded.collection.as_ref())
            .unwrap();
        assert_eq!(
            lexicon["defs"]["main"]["key"],
            serde_json::Value::String("any".to_owned())
        );

        let required = lexicon["defs"]["main"]["record"]["required"]
            .as_array()
            .unwrap();
        let properties = lexicon["defs"]["main"]["record"]["properties"]
            .as_object()
            .unwrap();
        let serialized = serde_json::to_value(&encoded.value).unwrap();
        let object = serialized.as_object().unwrap();

        for field in required {
            let field = field.as_str().unwrap();
            assert!(
                object.contains_key(field),
                "serialized record for {} is missing required lexicon field {field}",
                encoded.collection.as_ref()
            );
        }

        for key in object.keys().filter(|key| key.as_str() != "$type") {
            assert!(
                properties.contains_key(key),
                "serialized record for {} has no lexicon property for {key}",
                encoded.collection.as_ref()
            );
        }
    }

    #[test]
    fn memory_version_round_trip_preserves_canonical_federation_json() {
        assert_round_trip(sample_memory_version_envelope());
    }

    #[test]
    fn canonical_state_round_trip_preserves_canonical_federation_json() {
        assert_round_trip(sample_canonical_state_envelope());
    }

    #[test]
    fn evidence_round_trip_preserves_canonical_federation_json() {
        assert_round_trip(sample_evidence_envelope());
    }

    #[test]
    fn relation_round_trip_preserves_canonical_federation_json() {
        assert_round_trip(sample_relation_envelope());
    }

    #[test]
    fn membership_round_trip_preserves_canonical_federation_json() {
        assert_round_trip(sample_membership_envelope());
    }

    #[test]
    fn revocation_round_trip_preserves_canonical_federation_json() {
        assert_round_trip(sample_revocation_envelope());
    }

    #[test]
    fn transport_round_trip_preserves_uri_cid_and_rkey() {
        let envelope = sample_memory_version_envelope();
        let encoded = encode_record(&envelope).unwrap();
        let expected_rkey = encoded.rkey.clone();
        let transport = attach_transport(
            &"did:web:memory.example".parse::<AtIdentifier>().unwrap(),
            encoded,
            "bafybeigdyrzt5sfp7udm7hu76uh7y26nf3efuylqabf3oclgtqy55fbzdi"
                .parse::<Cid>()
                .unwrap(),
        )
        .unwrap();

        let (decoded, metadata) = detach_transport(transport).unwrap();
        assert_eq!(
            CanonicalJsonCodec::encode(&envelope).unwrap(),
            CanonicalJsonCodec::encode(&decoded).unwrap()
        );
        assert_eq!(metadata.data.rkey, expected_rkey);
        assert_eq!(
            metadata.data.uri.as_str(),
            format!(
                "at://did:web:memory.example/{}/{}",
                MEMORY_VERSION_NSID,
                expected_rkey.as_ref()
            )
        );
    }

    #[test]
    fn decode_record_rejects_mismatched_rkey() {
        let mut encoded = encode_record(&sample_memory_version_envelope()).unwrap();
        encoded.rkey =
            uuid_rkey(Uuid::parse_str("123e4567-e89b-12d3-a456-426614174fff").unwrap()).unwrap();

        let error = decode_record(encoded).unwrap_err();
        assert!(error.to_string().contains("record key mismatch"));
    }

    #[test]
    fn decode_record_rejects_non_uuid_record_keys() {
        let encoded = EncodedAtRecord {
            collection: MEMORY_VERSION_NSID.parse::<Nsid>().unwrap(),
            rkey: "not-a-uuid-rkey".parse::<RecordKey>().unwrap(),
            value: encode_record(&sample_memory_version_envelope())
                .unwrap()
                .value,
        };

        let error = decode_record(encoded).unwrap_err();
        assert!(error.to_string().contains("record key mismatch"));
    }

    #[test]
    fn lexicons_match_serialized_sample_records() {
        for envelope in [
            sample_memory_version_envelope(),
            sample_canonical_state_envelope(),
            sample_evidence_envelope(),
            sample_relation_envelope(),
            sample_membership_envelope(),
            sample_revocation_envelope(),
        ] {
            assert_lexicon_matches_encoded_record(&encode_record(&envelope).unwrap());
        }
    }
}
