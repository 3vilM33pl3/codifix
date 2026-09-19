// SPDX-License-Identifier: AGPL-3.0-or-later
//! Private, nonce-bound operational metadata. Never part of the PDS or timeline.
use crate::{GatewayClient, GatewayCredentialStatus, Result};
use chrono::{DateTime, Utc};
use mem_federation::publication::{InstanceId, SpaceId};
use serde::{Deserialize, Serialize};
use uuid::Uuid;

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct GatewayStatus {
    pub credentials: GatewayCredentialStatus,
    pub pending: Option<PublicationStatus>,
}
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct PublicationStatus {
    pub sequence: u32,
    pub attempts: u32,
    pub blocked: bool,
    pub leased: bool,
    pub due: bool,
    pub failure: Option<PublicationFailureCategory>,
}
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum PublicationFailureCategory {
    Authentication,
    Unavailable,
    ImmutableConflict,
    InvalidReceipt,
    AttemptsExhausted,
    Unknown,
}
impl From<mem_federation_store::PendingPublication> for PublicationStatus {
    fn from(p: mem_federation_store::PendingPublication) -> Self {
        let failure = p.last_error.as_deref().map(|e| match e {
            "unavailable" => PublicationFailureCategory::Unavailable,
            "immutable_conflict" => PublicationFailureCategory::ImmutableConflict,
            "invalid_receipt" => PublicationFailureCategory::InvalidReceipt,
            "attempts_exhausted" => PublicationFailureCategory::AttemptsExhausted,
            v if v.starts_with("authentication:") => PublicationFailureCategory::Authentication,
            _ => PublicationFailureCategory::Unknown,
        });
        Self {
            sequence: p.sequence,
            attempts: p.attempts,
            blocked: p.blocked,
            leased: p.leased,
            due: p.due,
            failure,
        }
    }
}
#[derive(Debug, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct GatewayStatusClaims {
    pub purpose: String,
    pub nonce: Uuid,
    pub audience: InstanceId,
    pub space: SpaceId,
    pub authority: InstanceId,
    pub key_generation: u32,
    pub issued_at: DateTime<Utc>,
    pub expires_at: DateTime<Utc>,
    pub status: GatewayStatus,
}
#[derive(Debug, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct SignedGatewayStatus {
    pub claims: GatewayStatusClaims,
    pub signature: Vec<u8>,
}
impl GatewayClient {
    /// Caller authorization and project scoping remain the local service's
    /// obligation. The gateway independently checks current Pull membership.
    pub async fn status(&self, instance: InstanceId) -> Result<GatewayStatus> {
        let nonce = Uuid::new_v4();
        let response = self
            .endpoint
            .client
            .get(self.endpoint.url("/v1/status")?)
            .bearer_auth(&self.token)
            .query(&[("nonce", nonce)])
            .send()
            .await
            .map_err(|_| crate::RuntimeError::Unavailable)?;
        crate::sync::check_status(response.status())?;
        let signed = crate::http::json(response, 16 * 1024).await?;
        self.trust
            .verify_status(signed, nonce, instance, self.space, self.authority)
    }
}
