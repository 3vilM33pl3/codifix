// SPDX-License-Identifier: AGPL-3.0-or-later

use crate::{EncodedAtRecord, FederationEnvelope, MemAtprotoError, encode_record};
use atrium_xrpc::{HttpClient, XrpcClient, http};
use chrono::{DateTime, Utc};
use http::{Request, Response};
use serde_json::json;
use std::sync::{Arc, Mutex};

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct CapturedRequest {
    pub method: http::Method,
    pub uri: String,
    pub content_type: Option<String>,
    pub body: Vec<u8>,
}

#[derive(Clone, Default)]
pub struct RecordingClient {
    captured: Arc<Mutex<Vec<CapturedRequest>>>,
}

impl RecordingClient {
    pub fn new() -> Self {
        Self::default()
    }

    pub fn captured(&self) -> Vec<CapturedRequest> {
        match self.captured.lock() {
            Ok(guard) => guard.clone(),
            Err(poisoned) => poisoned.into_inner().clone(),
        }
    }
}

impl HttpClient for RecordingClient {
    async fn send_http(
        &self,
        request: Request<Vec<u8>>,
    ) -> Result<Response<Vec<u8>>, Box<dyn std::error::Error + Send + Sync + 'static>> {
        let captured = CapturedRequest {
            method: request.method().clone(),
            uri: request.uri().to_string(),
            content_type: request
                .headers()
                .get(http::header::CONTENT_TYPE)
                .and_then(|value| value.to_str().ok())
                .map(str::to_owned),
            body: request.body().clone(),
        };
        match self.captured.lock() {
            Ok(mut guard) => guard.push(captured),
            Err(poisoned) => poisoned.into_inner().push(captured),
        }

        let response = match request.uri().path() {
            "/xrpc/com.atproto.repo.createRecord" => json!({
                "cid": "bafybeigdyrzt5sfp7udm7hu76uh7y26nf3efuylqabf3oclgtqy55fbzdi",
                "uri": sample_record_uri()?,
                "commit": {
                    "cid": "bafybeie5gq4jxvzmsym6hjlwxej4rwdoxt7wadqvmmwbqi7r27fclha2va",
                    "rev": "3jzfcijpj2z2a"
                }
            }),
            "/xrpc/com.atproto.repo.applyWrites" => json!({
                "commit": {
                    "cid": "bafybeie5gq4jxvzmsym6hjlwxej4rwdoxt7wadqvmmwbqi7r27fclha2va",
                    "rev": "3jzfcijpj2z2a"
                },
                "results": [{
                    "$type": "com.atproto.repo.applyWrites#createResult",
                    "cid": "bafybeigdyrzt5sfp7udm7hu76uh7y26nf3efuylqabf3oclgtqy55fbzdi",
                    "uri": sample_record_uri()?
                }]
            }),
            "/xrpc/com.atproto.repo.listRecords" => json!({
                "cursor": "cursor-1",
                "records": [{
                    "cid": "bafybeigdyrzt5sfp7udm7hu76uh7y26nf3efuylqabf3oclgtqy55fbzdi",
                    "uri": sample_record_uri()?,
                    "value": sample_encoded_record()?.value
                }]
            }),
            other => {
                return Err(Box::new(std::io::Error::other(format!(
                    "unexpected path: {other}"
                ))));
            }
        };

        Ok(Response::builder()
            .status(http::StatusCode::OK)
            .header(http::header::CONTENT_TYPE, "application/json")
            .body(serde_json::to_vec(&response)?)?)
    }
}

impl XrpcClient for RecordingClient {
    fn base_uri(&self) -> String {
        "https://example.test".to_owned()
    }
}

fn sample_memory_version_envelope() -> Result<FederationEnvelope, MemAtprotoError> {
    let record: crate::federation::MemoryVersionRecord = serde_json::from_value(json!({
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
        "related_memories": [],
        "created_at": "2026-09-04T09:00:00Z",
        "updated_at": "2026-09-04T09:30:00Z",
        "is_tombstone": false
    }))?;

    Ok(crate::federation::SharedRecordEnvelope::new(
        crate::federation::RecordLocator::new(
            "memory-version:123e4567-e89b-12d3-a456-426614174001:7",
        )?,
        crate::federation::ProjectSpaceId::new("project.memory")?,
        crate::federation::AuthorIdentity::new("did:example:alice")?,
        DateTime::parse_from_rfc3339("2026-09-04T09:30:00Z")?.with_timezone(&Utc),
        Some(crate::federation::RecordLocator::new(
            "task:123e4567-e89b-12d3-a456-426614174000",
        )?),
        crate::federation::FederationRecord::MemoryVersion(record),
    ))
}

fn sample_encoded_record() -> Result<EncodedAtRecord, MemAtprotoError> {
    encode_record(&sample_memory_version_envelope()?)
}

fn sample_record_uri() -> Result<String, MemAtprotoError> {
    let encoded = sample_encoded_record()?;
    Ok(format!(
        "at://did:web:memory.example/{}/{}",
        encoded.collection.as_ref(),
        encoded.rkey.as_ref()
    ))
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::{
        APPLY_WRITES_REQUEST_GOLDEN, CREATE_RECORD_REQUEST_GOLDEN, LIST_RECORDS_REQUEST_URI_GOLDEN,
    };
    use atrium_api::{
        client::AtpServiceClient,
        com::atproto::repo::{apply_writes, create_record, list_records},
        types::{LimitedNonZeroU8, Object, string::AtIdentifier},
    };
    use futures::executor::block_on;

    fn sample_repo() -> AtIdentifier {
        "did:web:memory.example".parse::<AtIdentifier>().unwrap()
    }

    #[test]
    fn create_record_uses_expected_method_path_and_json_body() {
        block_on(async {
            let client = RecordingClient::new();
            let api = AtpServiceClient::new(client.clone());
            let encoded = sample_encoded_record().unwrap();
            let output = api
                .service
                .com
                .atproto
                .repo
                .create_record(Object::from(create_record::InputData {
                    collection: encoded.collection.clone(),
                    record: encoded.value.clone(),
                    repo: sample_repo(),
                    rkey: Some(encoded.rkey.clone()),
                    swap_commit: None,
                    validate: Some(true),
                }))
                .await
                .unwrap();

            assert_eq!(output.data.uri, sample_record_uri().unwrap());

            let captured = client.captured();
            let request = &captured[0];
            assert_eq!(request.method, http::Method::POST);
            assert_eq!(
                request.uri,
                "https://example.test/xrpc/com.atproto.repo.createRecord"
            );
            assert_eq!(request.content_type.as_deref(), Some("application/json"));

            let body: serde_json::Value = serde_json::from_slice(&request.body).unwrap();
            let expected: serde_json::Value =
                serde_json::from_str(CREATE_RECORD_REQUEST_GOLDEN).unwrap();
            assert_eq!(body, expected);
        });
    }

    #[test]
    fn apply_writes_uses_expected_method_path_and_json_body() {
        block_on(async {
            let client = RecordingClient::new();
            let api = AtpServiceClient::new(client.clone());
            let encoded = sample_encoded_record().unwrap();
            let output = api
                .service
                .com
                .atproto
                .repo
                .apply_writes(Object::from(apply_writes::InputData {
                    repo: sample_repo(),
                    swap_commit: None,
                    validate: Some(true),
                    writes: vec![apply_writes::InputWritesItem::Create(Box::new(
                        Object::from(apply_writes::CreateData {
                            collection: encoded.collection.clone(),
                            rkey: Some(encoded.rkey.clone()),
                            value: encoded.value.clone(),
                        }),
                    ))],
                }))
                .await
                .unwrap();

            assert_eq!(output.data.results.unwrap().len(), 1);

            let captured = client.captured();
            let request = &captured[0];
            assert_eq!(request.method, http::Method::POST);
            assert_eq!(
                request.uri,
                "https://example.test/xrpc/com.atproto.repo.applyWrites"
            );

            let body: serde_json::Value = serde_json::from_slice(&request.body).unwrap();
            let expected: serde_json::Value =
                serde_json::from_str(APPLY_WRITES_REQUEST_GOLDEN).unwrap();
            assert_eq!(body, expected);
        });
    }

    #[test]
    fn list_records_uses_expected_method_path_query_and_response_shape() {
        block_on(async {
            let client = RecordingClient::new();
            let api = AtpServiceClient::new(client.clone());
            let encoded = sample_encoded_record().unwrap();
            let output = api
                .service
                .com
                .atproto
                .repo
                .list_records(Object::from(list_records::ParametersData {
                    collection: encoded.collection.clone(),
                    cursor: Some("cursor-0".to_owned()),
                    limit: Some(LimitedNonZeroU8::<100>::try_from(10u8).unwrap()),
                    repo: sample_repo(),
                    reverse: Some(true),
                }))
                .await
                .unwrap();

            assert_eq!(output.data.cursor.as_deref(), Some("cursor-1"));
            assert_eq!(output.data.records.len(), 1);
            assert_eq!(
                output.data.records[0].data.uri,
                sample_record_uri().unwrap()
            );

            let captured = client.captured();
            let request = &captured[0];
            assert_eq!(request.method, http::Method::GET);
            assert_eq!(request.uri, LIST_RECORDS_REQUEST_URI_GOLDEN.trim());
        });
    }
}
