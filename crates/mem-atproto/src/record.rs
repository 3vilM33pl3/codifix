// SPDX-License-Identifier: AGPL-3.0-or-later

use crate::{
    AtUri, MemAtprotoError, CANONICAL_STATE_NSID, EVIDENCE_NSID, MEMBERSHIP_NSID,
    MEMORY_VERSION_NSID, RELATION_NSID, REVOCATION_NSID,
};
use atrium_api::types::{
    string::{Cid, Datetime, Did, RecordKey},
    Object, Unknown,
};
use serde::{de::DeserializeOwned, Deserialize, Serialize};
use uuid::Uuid;

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct ActorRefData {
    pub did: Did,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub handle: Option<String>,
}

pub type ActorRef = Object<ActorRefData>;

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
pub struct NeutralEnvelopeData {
    pub kind: String,
    pub record: Unknown,
    pub transport: TransportMetadata,
}

pub type NeutralEnvelope = Object<NeutralEnvelopeData>;

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct MemoryVersionData {
    #[serde(rename = "$type")]
    pub record_type: String,
    pub space: String,
    pub canonical_id: Uuid,
    pub version_no: u64,
    pub memory_type: String,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub summary: Option<String>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub content: Option<String>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub confidence_ppm: Option<u32>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub importance: Option<u16>,
    pub captured_at: Datetime,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub writer: Option<ActorRef>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub payload: Option<Unknown>,
}

pub type MemoryVersion = Object<MemoryVersionData>;

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct CanonicalStateData {
    #[serde(rename = "$type")]
    pub record_type: String,
    pub space: String,
    pub canonical_id: Uuid,
    pub head_version_no: u64,
    pub status: String,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub archived_at: Option<Datetime>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub tombstoned_at: Option<Datetime>,
}

pub type CanonicalState = Object<CanonicalStateData>;

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct EvidenceData {
    #[serde(rename = "$type")]
    pub record_type: String,
    pub evidence_id: Uuid,
    pub space: String,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub canonical_id: Option<Uuid>,
    pub source_uri: String,
    pub checked_at: Datetime,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub source_hash: Option<String>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub details: Option<Unknown>,
}

pub type Evidence = Object<EvidenceData>;

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct RelationData {
    #[serde(rename = "$type")]
    pub record_type: String,
    pub space: String,
    pub source_canonical_id: Uuid,
    pub target_canonical_id: Uuid,
    pub relation_type: String,
    pub origin: String,
    pub observed_at: Datetime,
}

pub type Relation = Object<RelationData>;

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct MembershipData {
    #[serde(rename = "$type")]
    pub record_type: String,
    pub membership_id: Uuid,
    pub space: String,
    pub member_did: Did,
    pub capabilities: Vec<String>,
    pub granted_by: Did,
    pub granted_at: Datetime,
}

pub type Membership = Object<MembershipData>;

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct RevocationData {
    #[serde(rename = "$type")]
    pub record_type: String,
    pub revocation_id: Uuid,
    pub space: String,
    pub subject_did: Did,
    pub revoked_by: Did,
    pub revoked_at: Datetime,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub reason: Option<String>,
}

pub type Revocation = Object<RevocationData>;

impl MemoryVersionData {
    pub fn new(
        space: impl Into<String>,
        canonical_id: Uuid,
        version_no: u64,
        memory_type: impl Into<String>,
        captured_at: Datetime,
    ) -> Self {
        Self {
            record_type: MEMORY_VERSION_NSID.to_owned(),
            space: space.into(),
            canonical_id,
            version_no,
            memory_type: memory_type.into(),
            summary: None,
            content: None,
            confidence_ppm: None,
            importance: None,
            captured_at,
            writer: None,
            payload: None,
        }
    }
}

impl CanonicalStateData {
    pub fn new(
        space: impl Into<String>,
        canonical_id: Uuid,
        head_version_no: u64,
        status: impl Into<String>,
    ) -> Self {
        Self {
            record_type: CANONICAL_STATE_NSID.to_owned(),
            space: space.into(),
            canonical_id,
            head_version_no,
            status: status.into(),
            archived_at: None,
            tombstoned_at: None,
        }
    }
}

impl EvidenceData {
    pub fn new(
        evidence_id: Uuid,
        space: impl Into<String>,
        source_uri: impl Into<String>,
        checked_at: Datetime,
    ) -> Self {
        Self {
            record_type: EVIDENCE_NSID.to_owned(),
            evidence_id,
            space: space.into(),
            canonical_id: None,
            source_uri: source_uri.into(),
            checked_at,
            source_hash: None,
            details: None,
        }
    }
}

impl RelationData {
    pub fn new(
        space: impl Into<String>,
        source_canonical_id: Uuid,
        target_canonical_id: Uuid,
        relation_type: impl Into<String>,
        origin: impl Into<String>,
        observed_at: Datetime,
    ) -> Self {
        Self {
            record_type: RELATION_NSID.to_owned(),
            space: space.into(),
            source_canonical_id,
            target_canonical_id,
            relation_type: relation_type.into(),
            origin: origin.into(),
            observed_at,
        }
    }
}

impl MembershipData {
    pub fn new(
        membership_id: Uuid,
        space: impl Into<String>,
        member_did: Did,
        capabilities: Vec<String>,
        granted_by: Did,
        granted_at: Datetime,
    ) -> Self {
        Self {
            record_type: MEMBERSHIP_NSID.to_owned(),
            membership_id,
            space: space.into(),
            member_did,
            capabilities,
            granted_by,
            granted_at,
        }
    }
}

impl RevocationData {
    pub fn new(
        revocation_id: Uuid,
        space: impl Into<String>,
        subject_did: Did,
        revoked_by: Did,
        revoked_at: Datetime,
    ) -> Self {
        Self {
            record_type: REVOCATION_NSID.to_owned(),
            revocation_id,
            space: space.into(),
            subject_did,
            revoked_by,
            revoked_at,
            reason: None,
        }
    }
}

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

pub fn wrap_record<T>(
    kind: impl Into<String>,
    record: Object<T>,
    transport: TransportMetadata,
) -> Result<NeutralEnvelope, MemAtprotoError>
where
    Object<T>: Serialize,
{
    Ok(Object::from(NeutralEnvelopeData {
        kind: kind.into(),
        record: object_to_unknown(&record)?,
        transport,
    }))
}

pub fn unwrap_record<T>(
    envelope: NeutralEnvelope,
) -> Result<(Object<T>, TransportMetadata), MemAtprotoError>
where
    Object<T>: DeserializeOwned,
{
    Ok((
        object_from_unknown(envelope.data.record)?,
        envelope.data.transport,
    ))
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
    use serde_json::json;

    fn sample_transport() -> TransportMetadata {
        serde_json::from_value(json!({
            "uri": "at://did:web:memory.example/dev.memory-layer.unspecced.memoryVersion/123e4567-e89b-12d3-a456-426614174000",
            "cid": "bafybeigdyrzt5sfp7udm7hu76uh7y26nf3efuylqabf3oclgtqy55fbzdi",
            "rkey": "123e4567-e89b-12d3-a456-426614174000"
        }))
        .unwrap()
    }

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
    fn envelope_round_trip_preserves_unknown_fields() {
        let record: MemoryVersion = serde_json::from_value(json!({
            "$type": MEMORY_VERSION_NSID,
            "space": "memory/demo",
            "canonicalId": "123e4567-e89b-12d3-a456-426614174001",
            "versionNo": 7,
            "memoryType": "implementation",
            "capturedAt": "2026-09-04T09:30:00.000000Z",
            "summary": "Adapter record",
            "payload": {"k": "v"},
            "customField": {"nested": true}
        }))
        .unwrap();

        let envelope =
            wrap_record(MEMORY_VERSION_NSID, record.clone(), sample_transport()).unwrap();
        let (decoded, transport) = unwrap_record::<MemoryVersionData>(envelope).unwrap();

        assert_eq!(decoded.data, record.data);
        assert_eq!(
            transport.data.rkey.as_ref(),
            "123e4567-e89b-12d3-a456-426614174000"
        );

        let decoded_json = serde_json::to_value(decoded).unwrap();
        assert_eq!(
            decoded_json["customField"]["nested"],
            serde_json::Value::Bool(true)
        );
    }
}
