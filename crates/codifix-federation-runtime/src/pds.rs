// SPDX-License-Identifier: AGPL-3.0-or-later
use crate::{Endpoint, Result, RuntimeError, http};
use codifix_atproto::{addressed_expertise::AddressedExpertiseWire, publication::PublicationWire};
use codifix_atproto_verify::{
    MAX_CAR_BYTES, PinnedRepository, VerifiedDeletion, VerifiedRepository,
};
use codifix_federation::expertise::AddressedExpertiseRecord;
use codifix_federation::publication::{PublicationDelivery, SpaceId};
use codifix_federation_store::{ImmutablePublisher, PublishFailure};
use serde_json::Value;
use std::{
    future::Future,
    pin::Pin,
    sync::Arc,
    time::{SystemTime, UNIX_EPOCH},
};

/// Gateway-only fixed repository binding. No client-supplied repo, credentials,
/// discovery, redirect or overwrite. Do not give this object to member nodes.
#[derive(Clone)]
pub struct PdsPublisher {
    endpoint: Endpoint,
    repository: String,
    token: String,
    generation: u64,
    rotating: Option<Arc<crate::credentials::RotatingCredentials>>,
    space: SpaceId,
    verifier: PinnedRepository,
    pub(crate) signing_key: String,
    verification_slot: Arc<tokio::sync::Semaphore>,
}
impl PdsPublisher {
    pub fn new(
        endpoint: Endpoint,
        repository: String,
        token: String,
        signing_key: String,
        space: SpaceId,
    ) -> Result<Self> {
        // Validate DID syntax through the adapter rather than accepting handles
        // that can resolve to another repository between retries.
        if !repository.starts_with("did:") || token.trim().is_empty() {
            return Err(RuntimeError::Configuration);
        }
        codifix_atproto::publication::validate_repository(&repository)
            .map_err(|_| RuntimeError::Configuration)?;
        let verifier = PinnedRepository::new(repository.clone(), signing_key.clone())
            .map_err(|_| RuntimeError::Configuration)?;
        Ok(Self {
            endpoint,
            repository,
            token,
            generation: 0,
            rotating: None,
            space,
            verifier,
            signing_key,
            verification_slot: Arc::new(tokio::sync::Semaphore::new(1)),
        })
    }
    /// Access-only bundle; the gateway never receives a refresh token. A missing
    /// or expired file keeps the process inspectable but prevents work claims.
    pub fn with_rotating_credentials(
        endpoint: Endpoint,
        repository: String,
        session_file: std::path::PathBuf,
        maintenance_file: Option<std::path::PathBuf>,
        signing_key: String,
        space: SpaceId,
    ) -> Result<Self> {
        let rotating = crate::credentials::RotatingCredentials::new(
            session_file,
            maintenance_file,
            repository.clone(),
            endpoint.origin().to_owned(),
        )?;
        let mut publisher = Self::new(
            endpoint,
            repository,
            "unloaded-access-only-credential".into(),
            signing_key,
            space,
        )?;
        publisher.rotating = Some(Arc::new(rotating));
        Ok(publisher)
    }
    pub fn credential_status(&self) -> crate::GatewayCredentialStatus {
        let now = crate::credentials::now_seconds();
        self.rotating.as_ref().map_or_else(
            || crate::GatewayCredentialStatus::static_token(now),
            |source| source.status(now),
        )
    }
    pub(crate) fn credential_snapshot(&self) -> Result<Self> {
        let mut snapshot = self.clone();
        if let Some(source) = &self.rotating {
            let bundle = source.snapshot(crate::credentials::now_seconds())?;
            snapshot.token = bundle.access_token;
            snapshot.generation = bundle.generation;
            snapshot.rotating = None;
        }
        Ok(snapshot)
    }
    pub(crate) fn credential_generation(&self) -> u64 {
        self.generation
    }
    fn authentication_failure(&self) -> PublishFailure {
        PublishFailure::Authentication {
            generation: self.generation,
        }
    }
    async fn readback(
        &self,
        wire: &PublicationWire,
    ) -> std::result::Result<Option<VerifiedRepository>, PublishFailure> {
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
            .get(url)
            .bearer_auth(&self.token)
            .query(&[
                ("repo", self.repository.as_str()),
                ("collection", codifix_atproto::publication::COLLECTION),
                ("rkey", key.as_str()),
            ])
            .send()
            .await
            .map_err(|_| PublishFailure::Unavailable)?;
        if response.status().is_client_error() {
            let code = response.status();
            if code == reqwest::StatusCode::UNAUTHORIZED {
                return Err(self.authentication_failure());
            }
            if code == reqwest::StatusCode::FORBIDDEN {
                return Err(PublishFailure::AuthorizationDenied);
            }
            let body: Value = http::json(response, 4096)
                .await
                .map_err(|_| PublishFailure::InvalidReceipt)?;
            if (code.as_u16() == 400 || code.as_u16() == 404) && body["error"] == "RecordNotFound" {
                return Ok(None);
            }
            return Err(PublishFailure::InvalidReceipt);
        }
        if !response.status().is_success() {
            return Err(PublishFailure::Unavailable);
        }
        let body: Value = http::json(response, 128 * 1024)
            .await
            .map_err(|_| PublishFailure::InvalidReceipt)?;
        if !codifix_atproto::publication::verify_readback(wire, &self.repository, &body)
            .map_err(|_| PublishFailure::InvalidReceipt)?
        {
            return Err(PublishFailure::Conflict);
        }
        Ok(Some(self.verify_repository(wire).await?))
    }
    async fn verify_repository(
        &self,
        wire: &PublicationWire,
    ) -> std::result::Result<VerifiedRepository, PublishFailure> {
        // Bound download + CPU concurrency across publisher clones. Move the
        // permit into the blocking task so cancellation cannot release it early.
        let permit = self
            .verification_slot
            .clone()
            .acquire_owned()
            .await
            .map_err(|_| PublishFailure::Unavailable)?;
        let response = self
            .endpoint
            .get(
                self.endpoint
                    .url("/xrpc/com.atproto.sync.getRepo")
                    .map_err(|_| PublishFailure::InvalidReceipt)?,
            )
            .bearer_auth(&self.token)
            .query(&[("did", self.repository.as_str())])
            .send()
            .await
            .map_err(|_| PublishFailure::Unavailable)?;
        if !response.status().is_success() {
            if response.status() == reqwest::StatusCode::UNAUTHORIZED {
                return Err(self.authentication_failure());
            }
            if response.status() == reqwest::StatusCode::FORBIDDEN {
                return Err(PublishFailure::AuthorizationDenied);
            }
            return Err(PublishFailure::Unavailable);
        }
        let bytes = http::bytes(response, MAX_CAR_BYTES)
            .await
            .map_err(|_| PublishFailure::InvalidReceipt)?;
        let verifier = self.verifier.clone();
        let wire = wire.clone();
        let now = SystemTime::now()
            .duration_since(UNIX_EPOCH)
            .map_err(|_| PublishFailure::InvalidReceipt)?
            .as_micros();
        let now = u64::try_from(now).map_err(|_| PublishFailure::InvalidReceipt)?;
        tokio::task::spawn_blocking(move || {
            let _permit = permit;
            verifier.verify_full_car(&bytes, &wire, now)
        })
        .await
        .map_err(|_| PublishFailure::InvalidReceipt)?
        .map_err(|error| {
            tracing::warn!(reason = %error, "PDS repository verification failed");
            PublishFailure::InvalidReceipt
        })
    }
    pub async fn publish_exact(
        &self,
        delivery: &PublicationDelivery,
    ) -> std::result::Result<(), PublishFailure> {
        self.publish_verified(delivery).await.map(|_| ())
    }
    pub async fn publish_verified(
        &self,
        delivery: &PublicationDelivery,
    ) -> std::result::Result<VerifiedRepository, PublishFailure> {
        let snapshot = self
            .credential_snapshot()
            .map_err(|_| self.authentication_failure())?;
        snapshot.publish_verified_snapshot(delivery).await
    }
    async fn publish_verified_snapshot(
        &self,
        delivery: &PublicationDelivery,
    ) -> std::result::Result<VerifiedRepository, PublishFailure> {
        if delivery.envelope.space != self.space {
            return Err(PublishFailure::InvalidReceipt);
        }
        let wire = PublicationWire::encode(delivery.clone())
            .map_err(|_| PublishFailure::InvalidReceipt)?;
        if let Some(proof) = self.readback(&wire).await? {
            return Ok(proof);
        }
        let body = codifix_atproto::publication::create_request(&wire, &self.repository)
            .map_err(|_| PublishFailure::InvalidReceipt)?;
        let url = self
            .endpoint
            .url("/xrpc/com.atproto.repo.createRecord")
            .map_err(|_| PublishFailure::InvalidReceipt)?;
        // Regardless of a success, timeout, conflict or lost reply, only exact
        // exact read-back AND signed repository inclusion permit success.
        let response = self
            .endpoint
            .post(url)
            .bearer_auth(&self.token)
            .json(&body)
            .send()
            .await;
        let rejected = response.as_ref().ok().and_then(|r| match r.status() {
            reqwest::StatusCode::UNAUTHORIZED => Some(self.authentication_failure()),
            reqwest::StatusCode::FORBIDDEN => Some(PublishFailure::AuthorizationDenied),
            _ => None,
        });
        if let Some(proof) = self.readback(&wire).await? {
            Ok(proof)
        } else {
            Err(rejected.unwrap_or(PublishFailure::Unavailable))
        }
    }

    /// Publish only through the fixed private repository binding. A successful
    /// createRecord HTTP response is insufficient: exact read-back and signed
    /// CAR inclusion are required, including after ambiguous network results.
    pub async fn publish_addressed_verified(
        &self,
        addressed: &AddressedExpertiseRecord,
    ) -> std::result::Result<VerifiedRepository, PublishFailure> {
        let snapshot = self
            .credential_snapshot()
            .map_err(|_| self.authentication_failure())?;
        if addressed.space != snapshot.space {
            return Err(PublishFailure::InvalidReceipt);
        }
        let wire = AddressedExpertiseWire::encode(addressed.clone())
            .map_err(|_| PublishFailure::InvalidReceipt)?;
        if let Some(proof) = snapshot.readback_addressed(&wire).await? {
            return Ok(proof);
        }
        let body =
            codifix_atproto::addressed_expertise::create_request(&wire, &snapshot.repository)
                .map_err(|_| PublishFailure::InvalidReceipt)?;
        let response = snapshot
            .endpoint
            .post(
                snapshot
                    .endpoint
                    .url("/xrpc/com.atproto.repo.createRecord")
                    .map_err(|_| PublishFailure::InvalidReceipt)?,
            )
            .bearer_auth(&snapshot.token)
            .json(&body)
            .send()
            .await;
        let rejected = response.as_ref().ok().and_then(|r| match r.status() {
            reqwest::StatusCode::UNAUTHORIZED => Some(snapshot.authentication_failure()),
            reqwest::StatusCode::FORBIDDEN => Some(PublishFailure::AuthorizationDenied),
            _ => None,
        });
        if let Some(proof) = snapshot.readback_addressed(&wire).await? {
            Ok(proof)
        } else {
            Err(rejected.unwrap_or(PublishFailure::Unavailable))
        }
    }

    /// Expire one exact addressed record. A missing getRecord response alone is
    /// insufficient: a newer signed repository snapshot must omit the key.
    /// deleteRecord uses swapRecord so cleanup cannot erase a changed value.
    pub async fn delete_addressed_verified(
        &self,
        addressed: &AddressedExpertiseRecord,
        prior: Option<&VerifiedRepository>,
    ) -> std::result::Result<VerifiedDeletion, PublishFailure> {
        let snapshot = self
            .credential_snapshot()
            .map_err(|_| self.authentication_failure())?;
        if addressed.space != snapshot.space
            || prior.is_some_and(|proof| {
                proof.repository != snapshot.repository || proof.signing_key != snapshot.signing_key
            })
        {
            return Err(PublishFailure::InvalidReceipt);
        }
        let wire = AddressedExpertiseWire::encode(addressed.clone())
            .map_err(|_| PublishFailure::InvalidReceipt)?;
        if prior.is_some_and(|proof| wire.cid().ok().as_deref() != Some(proof.record_cid.as_str()))
        {
            return Err(PublishFailure::InvalidReceipt);
        }
        if let Some(current) = snapshot.readback_addressed(&wire).await? {
            if prior.is_some_and(|proof| {
                current.record_cid != proof.record_cid || current.revision < proof.revision
            }) {
                return Err(PublishFailure::Conflict);
            }
            let body =
                codifix_atproto::addressed_expertise::delete_request(&wire, &snapshot.repository)
                    .map_err(|_| PublishFailure::InvalidReceipt)?;
            let response = snapshot
                .endpoint
                .post(
                    snapshot
                        .endpoint
                        .url("/xrpc/com.atproto.repo.deleteRecord")
                        .map_err(|_| PublishFailure::InvalidReceipt)?,
                )
                .bearer_auth(&snapshot.token)
                .json(&body)
                .send()
                .await;
            if let Ok(response) = &response {
                match response.status() {
                    reqwest::StatusCode::UNAUTHORIZED => {
                        return Err(snapshot.authentication_failure());
                    }
                    reqwest::StatusCode::FORBIDDEN => {
                        return Err(PublishFailure::AuthorizationDenied);
                    }
                    _ => {}
                }
            }
        }
        if snapshot.readback_addressed(&wire).await?.is_some() {
            return Err(PublishFailure::Unavailable);
        }
        snapshot
            .verify_addressed_absence(
                &wire,
                prior.map_or("2222222222222", |proof| proof.revision.as_str()),
            )
            .await
    }

    async fn readback_addressed(
        &self,
        wire: &AddressedExpertiseWire,
    ) -> std::result::Result<Option<VerifiedRepository>, PublishFailure> {
        let record = wire.record().map_err(|_| PublishFailure::InvalidReceipt)?;
        let response = self
            .endpoint
            .get(
                self.endpoint
                    .url("/xrpc/com.atproto.repo.getRecord")
                    .map_err(|_| PublishFailure::InvalidReceipt)?,
            )
            .bearer_auth(&self.token)
            .query(&[
                ("repo", self.repository.as_str()),
                ("collection", record.collection.as_ref()),
                ("rkey", record.rkey.as_ref()),
            ])
            .send()
            .await
            .map_err(|_| PublishFailure::Unavailable)?;
        if response.status().is_client_error() {
            let code = response.status();
            if code == reqwest::StatusCode::UNAUTHORIZED {
                return Err(self.authentication_failure());
            }
            if code == reqwest::StatusCode::FORBIDDEN {
                return Err(PublishFailure::AuthorizationDenied);
            }
            let body: Value = http::json(response, 4096)
                .await
                .map_err(|_| PublishFailure::InvalidReceipt)?;
            if (code.as_u16() == 400 || code.as_u16() == 404) && body["error"] == "RecordNotFound" {
                return Ok(None);
            }
            return Err(PublishFailure::InvalidReceipt);
        }
        if !response.status().is_success() {
            return Err(PublishFailure::Unavailable);
        }
        let body: Value = http::json(response, 128 * 1024)
            .await
            .map_err(|_| PublishFailure::InvalidReceipt)?;
        if !codifix_atproto::addressed_expertise::verify_readback(wire, &self.repository, &body)
            .map_err(|_| PublishFailure::InvalidReceipt)?
        {
            return Err(PublishFailure::Conflict);
        }
        Ok(Some(self.verify_addressed_repository(wire).await?))
    }

    async fn verify_addressed_repository(
        &self,
        wire: &AddressedExpertiseWire,
    ) -> std::result::Result<VerifiedRepository, PublishFailure> {
        let permit = self
            .verification_slot
            .clone()
            .acquire_owned()
            .await
            .map_err(|_| PublishFailure::Unavailable)?;
        let response = self
            .endpoint
            .get(
                self.endpoint
                    .url("/xrpc/com.atproto.sync.getRepo")
                    .map_err(|_| PublishFailure::InvalidReceipt)?,
            )
            .bearer_auth(&self.token)
            .query(&[("did", self.repository.as_str())])
            .send()
            .await
            .map_err(|_| PublishFailure::Unavailable)?;
        if !response.status().is_success() {
            return Err(match response.status() {
                reqwest::StatusCode::UNAUTHORIZED => self.authentication_failure(),
                reqwest::StatusCode::FORBIDDEN => PublishFailure::AuthorizationDenied,
                _ => PublishFailure::Unavailable,
            });
        }
        let bytes = http::bytes(response, MAX_CAR_BYTES)
            .await
            .map_err(|_| PublishFailure::InvalidReceipt)?;
        let verifier = self.verifier.clone();
        let wire = wire.clone();
        let now = SystemTime::now()
            .duration_since(UNIX_EPOCH)
            .map_err(|_| PublishFailure::InvalidReceipt)?
            .as_micros();
        let now = u64::try_from(now).map_err(|_| PublishFailure::InvalidReceipt)?;
        tokio::task::spawn_blocking(move || {
            let _permit = permit;
            verifier.verify_full_car_addressed(&bytes, &wire, now)
        })
        .await
        .map_err(|_| PublishFailure::InvalidReceipt)?
        .map_err(|error| {
            tracing::warn!(reason = %error, "PDS addressed repository verification failed");
            PublishFailure::InvalidReceipt
        })
    }

    async fn verify_addressed_absence(
        &self,
        wire: &AddressedExpertiseWire,
        prior_revision: &str,
    ) -> std::result::Result<VerifiedDeletion, PublishFailure> {
        let permit = self
            .verification_slot
            .clone()
            .acquire_owned()
            .await
            .map_err(|_| PublishFailure::Unavailable)?;
        let response = self
            .endpoint
            .get(
                self.endpoint
                    .url("/xrpc/com.atproto.sync.getRepo")
                    .map_err(|_| PublishFailure::InvalidReceipt)?,
            )
            .bearer_auth(&self.token)
            .query(&[("did", self.repository.as_str())])
            .send()
            .await
            .map_err(|_| PublishFailure::Unavailable)?;
        if !response.status().is_success() {
            return Err(match response.status() {
                reqwest::StatusCode::UNAUTHORIZED => self.authentication_failure(),
                reqwest::StatusCode::FORBIDDEN => PublishFailure::AuthorizationDenied,
                _ => PublishFailure::Unavailable,
            });
        }
        let bytes = http::bytes(response, MAX_CAR_BYTES)
            .await
            .map_err(|_| PublishFailure::InvalidReceipt)?;
        let verifier = self.verifier.clone();
        let wire = wire.clone();
        let prior_revision = prior_revision.to_owned();
        let now = SystemTime::now()
            .duration_since(UNIX_EPOCH)
            .map_err(|_| PublishFailure::InvalidReceipt)?
            .as_micros();
        let now = u64::try_from(now).map_err(|_| PublishFailure::InvalidReceipt)?;
        tokio::task::spawn_blocking(move || {
            let _permit = permit;
            verifier.verify_full_car_absent_addressed(&bytes, &wire, &prior_revision, now)
        })
        .await
        .map_err(|_| PublishFailure::InvalidReceipt)?
        .map_err(|error| {
            tracing::warn!(reason = %error, "PDS addressed deletion verification failed");
            PublishFailure::InvalidReceipt
        })
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
