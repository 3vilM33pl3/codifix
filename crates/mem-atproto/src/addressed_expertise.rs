// SPDX-License-Identifier: AGPL-3.0-or-later
//! Unpublished, addressed expertise records. They are never part of the shared
//! publication reducer or its ordered pull. The gateway alone may read/write
//! this collection and must enforce authenticated recipient filtering.
use atrium_api::types::{Unknown, string::AtIdentifier};
use mem_federation::expertise::{AddressedExpertisePayload, AddressedExpertiseRecord};
use serde_json::{Value, json};
use uuid::Uuid;

use crate::{EncodedAtRecord, MemAtprotoError, dag_cbor_cid, uuid_rkey};

pub const QUESTION_COLLECTION: &str = "dev.memory-layer.unspecced.expertiseQuestion";
pub const ANSWER_COLLECTION: &str = "dev.memory-layer.unspecced.expertiseAnswer";
pub const MAX_RECORD_BYTES: usize = 64 * 1024;

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct AddressedExpertiseWire {
    raw: Unknown,
    addressed: AddressedExpertiseRecord,
}

impl AddressedExpertiseWire {
    pub fn encode(addressed: AddressedExpertiseRecord) -> Result<Self, MemAtprotoError> {
        addressed
            .validate()
            .map_err(|error| MemAtprotoError::message(error.to_string()))?;
        let collection = collection_for(&addressed.payload);
        Self::decode(serde_json::from_value(
            json!({"$type":collection,"addressed":addressed}),
        )?)
    }

    pub fn decode(raw: Unknown) -> Result<Self, MemAtprotoError> {
        let value = serde_json::to_value(&raw)?;
        if serde_json::to_vec(&value)?.len() > MAX_RECORD_BYTES {
            return Err(MemAtprotoError::message(
                "oversized addressed expertise record",
            ));
        }
        let addressed: AddressedExpertiseRecord = serde_json::from_value(
            value
                .get("addressed")
                .cloned()
                .ok_or_else(|| MemAtprotoError::message("missing addressed payload"))?,
        )?;
        addressed
            .validate()
            .map_err(|error| MemAtprotoError::message(error.to_string()))?;
        if value["$type"] != collection_for(&addressed.payload) {
            return Err(MemAtprotoError::message(
                "addressed expertise collection/type mismatch",
            ));
        }
        Ok(Self { raw, addressed })
    }

    pub fn addressed(&self) -> &AddressedExpertiseRecord {
        &self.addressed
    }

    pub fn raw(&self) -> &Unknown {
        &self.raw
    }

    pub fn record(&self) -> Result<EncodedAtRecord, MemAtprotoError> {
        let addressed = &self.addressed;
        let collection = collection_for(&addressed.payload);
        let key = Uuid::new_v5(
            &addressed.space.as_uuid(),
            format!(
                "{}:{}:{}:{}",
                addressed.sender.as_uuid(),
                addressed.recipient.as_uuid(),
                addressed.request_id,
                collection
            )
            .as_bytes(),
        );
        Ok(EncodedAtRecord {
            collection: collection
                .parse()
                .map_err(|_| MemAtprotoError::message("invalid addressed collection"))?,
            rkey: uuid_rkey(key)?,
            value: self.raw.clone(),
        })
    }

    pub fn uri(&self, repository: &AtIdentifier) -> Result<String, MemAtprotoError> {
        let record = self.record()?;
        Ok(format!(
            "at://{}/{}/{}",
            repository.as_ref(),
            record.collection.as_ref(),
            record.rkey.as_ref()
        ))
    }

    pub fn cid(&self) -> Result<String, MemAtprotoError> {
        Ok(dag_cbor_cid(&self.raw)?.to_string())
    }
}

pub fn collection_for(payload: &AddressedExpertisePayload) -> &'static str {
    match payload {
        AddressedExpertisePayload::Question(_) => QUESTION_COLLECTION,
        AddressedExpertisePayload::Answer(_) => ANSWER_COLLECTION,
    }
}

pub fn create_request(
    wire: &AddressedExpertiseWire,
    repository: &str,
) -> Result<Value, MemAtprotoError> {
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
    wire: &AddressedExpertiseWire,
    repository: &str,
    body: &Value,
) -> Result<bool, MemAtprotoError> {
    let repo = repository
        .parse::<AtIdentifier>()
        .map_err(|_| MemAtprotoError::message("invalid repository"))?;
    let actual: Unknown = serde_json::from_value(body["value"].clone())?;
    let decoded = AddressedExpertiseWire::decode(actual.clone())?;
    Ok(actual == *wire.raw()
        && decoded.addressed() == wire.addressed()
        && body["uri"] == wire.uri(&repo)?
        && body["cid"] == wire.cid()?)
}
