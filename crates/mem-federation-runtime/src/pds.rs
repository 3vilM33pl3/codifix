// SPDX-License-Identifier: AGPL-3.0-or-later
use crate::{Endpoint, Result, RuntimeError, http};
use mem_atproto::publication::PublicationWire;
use mem_federation::publication::PublicationDelivery;
use mem_federation_store::{ImmutablePublisher, PublishFailure};
use serde_json::Value;
use std::{future::Future, pin::Pin};

/// Gateway-only fixed repository binding. No client-supplied repo, credentials,
/// discovery, redirect or overwrite. Do not give this object to member nodes.
#[derive(Clone)]
pub struct PdsPublisher {
    endpoint: Endpoint,
    repository: String,
    token: String,
}
impl PdsPublisher {
    pub fn new(endpoint: Endpoint, repository: String, token: String) -> Result<Self> {
        // Validate DID syntax through the adapter rather than accepting handles
        // that can resolve to another repository between retries.
        if !repository.starts_with("did:") || token.trim().is_empty() {
            return Err(RuntimeError::Configuration);
        }
        mem_atproto::publication::validate_repository(&repository)
            .map_err(|_| RuntimeError::Configuration)?;
        Ok(Self {
            endpoint,
            repository,
            token,
        })
    }
    async fn readback(&self, wire: &PublicationWire) -> std::result::Result<bool, PublishFailure> {
        let key = wire
            .record()
            .map_err(|_| PublishFailure::InvalidReceipt)?
            .rkey
            .as_ref()
            .to_owned();
        let url = self
            .endpoint
            .url("/xrpc/com.atproto.repo.getRecord")
            .map_err(|_| PublishFailure::InvalidReceipt)?;
        let response = self
            .endpoint
            .client
            .get(url)
            .bearer_auth(&self.token)
            .query(&[
                ("repo", self.repository.as_str()),
                ("collection", mem_atproto::publication::COLLECTION),
                ("rkey", key.as_str()),
            ])
            .send()
            .await
            .map_err(|_| PublishFailure::Unavailable)?;
        if response.status().is_client_error() {
            let code = response.status();
            let body: Value = http::json(response, 4096)
                .await
                .map_err(|_| PublishFailure::InvalidReceipt)?;
            if (code.as_u16() == 400 || code.as_u16() == 404) && body["error"] == "RecordNotFound" {
                return Ok(false);
            }
            return Err(PublishFailure::InvalidReceipt);
        }
        if !response.status().is_success() {
            return Err(PublishFailure::Unavailable);
        }
        let body: Value = http::json(response, 128 * 1024)
            .await
            .map_err(|_| PublishFailure::InvalidReceipt)?;
        if !mem_atproto::publication::verify_readback(wire, &self.repository, &body)
            .map_err(|_| PublishFailure::InvalidReceipt)?
        {
            return Err(PublishFailure::Conflict);
        }
        Ok(true)
    }
    pub async fn publish_exact(
        &self,
        delivery: &PublicationDelivery,
    ) -> std::result::Result<(), PublishFailure> {
        let wire = PublicationWire::encode(delivery.clone())
            .map_err(|_| PublishFailure::InvalidReceipt)?;
        if self.readback(&wire).await? {
            return Ok(());
        }
        let body = mem_atproto::publication::create_request(&wire, &self.repository)
            .map_err(|_| PublishFailure::InvalidReceipt)?;
        let url = self
            .endpoint
            .url("/xrpc/com.atproto.repo.createRecord")
            .map_err(|_| PublishFailure::InvalidReceipt)?;
        // Regardless of a success, timeout, conflict or lost reply, only exact
        // read-back with independently calculated DAG-CBOR CID permits success.
        let _response = self
            .endpoint
            .client
            .post(url)
            .bearer_auth(&self.token)
            .json(&body)
            .send()
            .await;
        if self.readback(&wire).await? {
            Ok(())
        } else {
            Err(PublishFailure::Unavailable)
        }
    }
}
impl ImmutablePublisher for PdsPublisher {
    fn publish<'a>(
        &'a self,
        delivery: &'a PublicationDelivery,
    ) -> Pin<Box<dyn Future<Output = std::result::Result<(), PublishFailure>> + Send + 'a>> {
        Box::pin(self.publish_exact(delivery))
    }
}
