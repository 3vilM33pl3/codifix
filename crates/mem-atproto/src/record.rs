// SPDX-License-Identifier: AGPL-3.0-or-later

use crate::{AtUri, MemAtprotoError};
use atrium_api::types::{
    Object, Unknown,
    string::{Cid, Datetime, RecordKey},
};
use serde::{Deserialize, Serialize, de::DeserializeOwned};
use uuid::Uuid;

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct TransportMetadataData {
    pub uri: AtUri,
    pub cid: Cid,
    pub rkey: RecordKey,
}

pub type TransportMetadata = Object<TransportMetadataData>;

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct TransportEnvelopeData {
    pub record: Unknown,
    pub transport: TransportMetadata,
}

pub type TransportEnvelope = Object<TransportEnvelopeData>;

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct CommonFieldsData {
    pub schema_version: u32,
    pub record_id: String,
    pub project_space_id: String,
    pub author: String,
    pub authored_at: Datetime,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub causation_id: Option<String>,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct RelatedMemoryData {
    pub memory_id: String,
    pub relation_type: String,
    pub summary: String,
    pub memory_type: String,
    pub confidence_ppm: u32,
}

pub type RelatedMemory = Object<RelatedMemoryData>;

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct SourceProvenanceData {
    pub status: String,
    pub checked_at: Datetime,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub reason: Option<String>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub resolved_path: Option<String>,
}

pub type SourceProvenance = Object<SourceProvenanceData>;

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct MemoryVersionData {
    #[serde(rename = "$type")]
    pub record_type: String,
    #[serde(flatten)]
    pub common: CommonFieldsData,
    pub memory_id: String,
    pub canonical_id: String,
    pub version_no: u32,
    pub canonical_text: String,
    pub summary: String,
    pub memory_type: String,
    pub importance: i32,
    pub confidence_ppm: u32,
    pub status: String,
    pub tags: Vec<String>,
    pub evidence_refs: Vec<String>,
    pub related_memories: Vec<RelatedMemory>,
    pub created_at: Datetime,
    pub updated_at: Datetime,
    pub is_tombstone: bool,
}

pub type MemoryVersion = Object<MemoryVersionData>;

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct CanonicalStateData {
    #[serde(rename = "$type")]
    pub record_type: String,
    #[serde(flatten)]
    pub common: CommonFieldsData,
    pub canonical_id: String,
    pub memory_id: String,
    pub version_no: u32,
    pub is_tombstone: bool,
    pub status: String,
    pub confidence_ppm: u32,
    pub updated_at: Datetime,
}

pub type CanonicalState = Object<CanonicalStateData>;

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct EvidenceData {
    #[serde(rename = "$type")]
    pub record_type: String,
    #[serde(flatten)]
    pub common: CommonFieldsData,
    pub evidence_id: String,
    pub source_id: String,
    pub memory_id: String,
    pub canonical_id: String,
    pub version_no: u32,
    pub source_kind: String,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub task_id: Option<String>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub file_path: Option<String>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub git_commit: Option<String>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub symbol_name: Option<String>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub symbol_kind: Option<String>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub excerpt: Option<String>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub provenance: Option<SourceProvenance>,
    pub observed_at: Datetime,
}

pub type Evidence = Object<EvidenceData>;

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct RelationData {
    #[serde(rename = "$type")]
    pub record_type: String,
    #[serde(flatten)]
    pub common: CommonFieldsData,
    pub src_canonical_id: String,
    pub relation_type: String,
    pub dst_canonical_id: String,
    pub asserted_at: Datetime,
}

pub type Relation = Object<RelationData>;

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct MembershipData {
    #[serde(rename = "$type")]
    pub record_type: String,
    #[serde(flatten)]
    pub common: CommonFieldsData,
    pub member: String,
    pub capability: String,
}

pub type Membership = Object<MembershipData>;

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct RevocationData {
    #[serde(rename = "$type")]
    pub record_type: String,
    #[serde(flatten)]
    pub common: CommonFieldsData,
    pub member: String,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub capability: Option<String>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub reason: Option<String>,
}

pub type Revocation = Object<RevocationData>;

pub fn confidence_to_ppm(confidence: f32) -> Result<u32, MemAtprotoError> {
    if !confidence.is_finite() || !(0.0..=1.0).contains(&confidence) {
        return Err(MemAtprotoError::ConfidenceRange(confidence));
    }
    let scaled = (f64::from(confidence) * 1_000_000.0).round();
    Ok(scaled as u32)
}

pub fn confidence_from_ppm(confidence_ppm: u32) -> Result<f32, MemAtprotoError> {
    if confidence_ppm > 1_000_000 {
        return Err(MemAtprotoError::ConfidencePpmRange(confidence_ppm));
    }
    Ok((f64::from(confidence_ppm) / 1_000_000.0) as f32)
}

pub fn uuid_rkey(id: Uuid) -> Result<RecordKey, MemAtprotoError> {
    id.to_string()
        .parse::<RecordKey>()
        .map_err(|err| MemAtprotoError::message(err.to_string()))
}

pub fn wrap_unknown_record(record: Unknown, transport: TransportMetadata) -> TransportEnvelope {
    Object::from(TransportEnvelopeData { record, transport })
}

pub fn unwrap_unknown_record(envelope: TransportEnvelope) -> (Unknown, TransportMetadata) {
    (envelope.data.record, envelope.data.transport)
}

pub fn object_to_unknown<T>(object: &Object<T>) -> Result<Unknown, MemAtprotoError>
where
    Object<T>: Serialize,
{
    Ok(serde_json::from_value(serde_json::to_value(object)?)?)
}

pub fn object_from_unknown<T>(unknown: Unknown) -> Result<Object<T>, MemAtprotoError>
where
    Object<T>: DeserializeOwned,
{
    Ok(serde_json::from_value(serde_json::to_value(unknown)?)?)
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::MEMORY_VERSION_NSID;
    use serde_json::json;

    #[test]
    fn confidence_round_trip_stays_within_one_ppm() {
        for value in [0.0_f32, 0.000001, 0.1, 0.5, 0.999999, 1.0] {
            let ppm = confidence_to_ppm(value).unwrap();
            let round_trip = confidence_from_ppm(ppm).unwrap();
            let delta_ppm =
                ((f64::from(value) - f64::from(round_trip)).abs() * 1_000_000.0).round();
            assert!(
                delta_ppm <= 1.0,
                "value={value} ppm={ppm} round_trip={round_trip}"
            );
        }
    }

    #[test]
    fn uuid_record_keys_use_uuid_text_form() {
        let id = Uuid::parse_str("123e4567-e89b-12d3-a456-426614174000").unwrap();
        assert_eq!(
            uuid_rkey(id).unwrap().as_ref(),
            "123e4567-e89b-12d3-a456-426614174000"
        );
    }

    #[test]
    fn object_unknown_round_trip_preserves_extra_fields() {
        let record: MemoryVersion = serde_json::from_value(json!({
            "$type": MEMORY_VERSION_NSID,
            "schemaVersion": 1,
            "recordId": "memory-version:123e4567-e89b-12d3-a456-426614174001:7",
            "projectSpaceId": "project.memory",
            "author": "did:example:alice",
            "authoredAt": "2026-09-04T09:30:00.000000Z",
            "memoryId": "memory:123e4567-e89b-12d3-a456-426614174001",
            "canonicalId": "canonical:123e4567-e89b-12d3-a456-426614174001",
            "versionNo": 7,
            "canonicalText": "Canonical memory text",
            "summary": "Adapter record",
            "memoryType": "implementation",
            "importance": 8,
            "confidencePpm": 750000,
            "status": "active",
            "tags": ["adapter", "offline"],
            "evidenceRefs": ["evidence:123e4567-e89b-12d3-a456-426614174001:123e4567-e89b-12d3-a456-426614174002"],
            "relatedMemories": [{
              "memoryId": "memory:123e4567-e89b-12d3-a456-426614174099",
              "relationType": "supports",
              "summary": "Nested related memory",
              "memoryType": "decision",
              "confidencePpm": 500000,
              "nestedExtra": {"retained": true}
            }],
            "createdAt": "2026-09-04T09:00:00.000000Z",
            "updatedAt": "2026-09-04T09:30:00.000000Z",
            "isTombstone": false,
            "customField": {"nested": true}
        }))
        .unwrap();

        let decoded: MemoryVersion =
            object_from_unknown(object_to_unknown(&record).unwrap()).unwrap();
        let decoded_json = serde_json::to_value(decoded).unwrap();

        assert_eq!(
            decoded_json["customField"]["nested"],
            serde_json::Value::Bool(true)
        );
        assert_eq!(
            decoded_json["relatedMemories"][0]["nestedExtra"]["retained"],
            serde_json::Value::Bool(true)
        );
    }
}
