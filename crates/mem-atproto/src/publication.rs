// SPDX-License-Identifier: AGPL-3.0-or-later
//! Schema-v2 private-profile records. This collection is UNPUBLISHED; it does
//! not reinterpret the six legacy spike Lexicons or claim native Spaces support.
use atrium_api::types::{Unknown, string::AtIdentifier};
use mem_federation::{CanonicalJsonCodec, publication::PublicationDelivery};
use serde_json::{Value, json};
use uuid::Uuid;

use crate::{EncodedAtRecord, MemAtprotoError, dag_cbor_cid, uuid_rkey};

pub const COLLECTION: &str = "dev.memory-layer.unspecced.publication";
pub const MAX_RECORD_BYTES: usize = 64 * 1024;

pub fn validate_repository(repository: &str) -> Result<(), MemAtprotoError> {
    repository
        .parse::<atrium_api::types::string::Did>()
        .map_err(|_| MemAtprotoError::message("invalid repository DID"))?;
    Ok(())
}
pub fn create_request(wire: &PublicationWire, repository: &str) -> Result<Value, MemAtprotoError> {
    use atrium_api::{com::atproto::repo::create_record, types::Object};
    let record = wire.record()?;
    let repo = repository
        .parse::<AtIdentifier>()
        .map_err(|_| MemAtprotoError::message("invalid repository"))?;
    Ok(serde_json::to_value(Object::from(
        create_record::InputData {
            collection: record.collection,
            repo,
            record: record.value,
            rkey: Some(record.rkey),
            swap_commit: None,
            validate: Some(false),
        },
    ))?)
}
pub fn verify_readback(
    wire: &PublicationWire,
    repository: &str,
    body: &Value,
) -> Result<bool, MemAtprotoError> {
    let repo = repository
        .parse::<AtIdentifier>()
        .map_err(|_| MemAtprotoError::message("invalid repository"))?;
    let actual: Unknown = serde_json::from_value(body["value"].clone())?;
    let decoded = PublicationWire::decode(actual.clone())?;
    Ok(actual == *wire.raw()
        && decoded.delivery() == wire.delivery()
        && body["uri"] == wire.uri(&repo)?
        && body["cid"] == wire.cid()?)
}

/// Retain the original Atrium object, including extensions at every depth.
/// Only the separately validated known projection may enter the reducer.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct PublicationWire {
    raw: Unknown,
    delivery: PublicationDelivery,
}
impl PublicationWire {
    pub fn encode(delivery: PublicationDelivery) -> Result<Self, MemAtprotoError> {
        Self::decode(serde_json::from_value(
            json!({"$type":COLLECTION,"delivery":delivery}),
        )?)
    }
    pub fn decode(raw: Unknown) -> Result<Self, MemAtprotoError> {
        let mut value = serde_json::to_value(&raw)?;
        if serde_json::to_vec(&value)?.len() > MAX_RECORD_BYTES || value["$type"] != COLLECTION {
            return Err(MemAtprotoError::message(
                "unsupported or oversized publication record",
            ));
        }
        let wire = value
            .get_mut("delivery")
            .ok_or_else(|| MemAtprotoError::message("missing delivery"))?;
        prune(wire, "delivery")?;
        let delivery: PublicationDelivery = serde_json::from_value(wire.clone())?;
        if delivery.envelope.schema_version != 2
            || CanonicalJsonCodec::digest(&delivery.envelope)
                .map_err(|e| MemAtprotoError::message(e.to_string()))?
                != delivery.digest
        {
            return Err(MemAtprotoError::message(
                "unsupported schema or mismatched envelope digest",
            ));
        }
        Ok(Self { raw, delivery })
    }
    pub fn delivery(&self) -> &PublicationDelivery {
        &self.delivery
    }
    pub fn raw(&self) -> &Unknown {
        &self.raw
    }
    pub fn record(&self) -> Result<EncodedAtRecord, MemAtprotoError> {
        // A single space repository can contain identical record UUIDs from two
        // publishers. Scope the deterministic UUID key to all three identities.
        let e = &self.delivery.envelope;
        let key = Uuid::new_v5(
            &e.space.as_uuid(),
            format!("{}:{}", e.publisher.as_uuid(), e.record_id.as_uuid()).as_bytes(),
        );
        Ok(EncodedAtRecord {
            collection: COLLECTION
                .parse()
                .map_err(|_| MemAtprotoError::message("invalid collection"))?,
            rkey: uuid_rkey(key)?,
            value: self.raw.clone(),
        })
    }
    pub fn uri(&self, repository: &AtIdentifier) -> Result<String, MemAtprotoError> {
        Ok(format!(
            "at://{}/{}/{}",
            repository.as_ref(),
            COLLECTION,
            self.record()?.rkey.as_ref()
        ))
    }
    pub fn cid(&self) -> Result<String, MemAtprotoError> {
        Ok(dag_cbor_cid(&self.raw)?.to_string())
    }
}

fn prune(value: &mut Value, shape: &str) -> Result<(), MemAtprotoError> {
    if value.is_null() {
        return Ok(());
    } // Option fields; required nulls fail serde.
    let fields: &[(&str, &str)] = match shape {
        "delivery" => &[
            ("sequence", ""),
            ("locator", ""),
            ("digest", "digest"),
            ("envelope", "envelope"),
        ],
        "digest" => &[("algorithm", ""), ("hex", "")],
        "envelope" => &[
            ("schema_version", ""),
            ("space", ""),
            ("publisher", ""),
            ("record_id", ""),
            ("author", ""),
            ("authored_at", ""),
            ("causation", "key"),
            ("record", "record"),
        ],
        "key" => &[("publisher", ""), ("id", "")],
        "stream" => &[("origin", ""), ("canonical", "")],
        "source" => &[("record_id", ""), ("version", "")],
        "content" => &[
            ("text", ""),
            ("summary", ""),
            ("memory_type", ""),
            ("confidence", ""),
            ("importance", ""),
            ("tags", ""),
        ],
        "publication" => &[
            ("version", ""),
            ("source", "source"),
            ("content", "content"),
            ("evidence", "keys"),
        ],
        "state" => &[
            ("revision", ""),
            ("content_version", ""),
            ("status", ""),
            ("confidence", ""),
            ("tombstone", ""),
        ],
        "snapshot" => &[
            ("space", ""),
            ("stream", "stream"),
            ("public_source_label", ""),
            ("selected_text", ""),
            ("observed_at", ""),
        ],
        "review" => &[
            ("snapshot_digest", "digest"),
            ("reviewer", ""),
            ("reviewed_at", ""),
        ],
        "membership" | "revocation" => &[("member", ""), ("capability", "")],
        "evidence" => &[("snapshot", "snapshot"), ("review", "review")],
        "memory_version" => &[("stream", "stream"), ("publication", "publication")],
        "canonical_state" => &[("stream", "stream"), ("state", "state")],
        "relation" => &[
            ("id", ""),
            ("source", "stream"),
            ("target", "stream"),
            ("relation_type", ""),
            ("revision", ""),
            ("active", ""),
        ],
        "proposal" => &[
            ("target", "stream"),
            ("base_version", ""),
            ("content", "content"),
            ("evidence", "keys"),
        ],
        "resolve_proposal" => &[("proposal", "key"), ("source", "source")],
        "keys" => {
            let values = value
                .as_array_mut()
                .ok_or_else(|| MemAtprotoError::message("invalid key array"))?;
            for value in values {
                prune(value, "key")?;
            }
            return Ok(());
        }
        "record" => {
            let kind = value["kind"]
                .as_str()
                .ok_or_else(|| MemAtprotoError::message("missing record kind"))?
                .to_owned();
            if ![
                "membership",
                "revocation",
                "evidence",
                "memory_version",
                "canonical_state",
                "relation",
                "proposal",
                "resolve_proposal",
            ]
            .contains(&kind.as_str())
            {
                return Err(MemAtprotoError::message("unknown semantic record kind"));
            }
            let map = value
                .as_object_mut()
                .ok_or_else(|| MemAtprotoError::message("invalid record"))?;
            map.retain(|key, _| key == "kind" || key == "payload");
            return prune(
                map.get_mut("payload")
                    .ok_or_else(|| MemAtprotoError::message("missing payload"))?,
                &kind,
            );
        }
        _ => return Err(MemAtprotoError::message("invalid schema projection")),
    };
    let map = value
        .as_object_mut()
        .ok_or_else(|| MemAtprotoError::message("invalid record object"))?;
    map.retain(|key, _| fields.iter().any(|(name, _)| key == name));
    for (name, child) in fields {
        if !child.is_empty()
            && let Some(value) = map.get_mut(*name)
        {
            prune(value, child)?;
        }
    }
    Ok(())
}
