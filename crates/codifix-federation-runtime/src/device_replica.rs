// SPDX-License-Identifier: AGPL-3.0-or-later
//! Short-lived gateway attestation of origin-authorized private device pages.
//! This is not an ATProto publication or proof of the complete repository.
use crate::{GatewaySecurity, GatewayServer, ReceiptTrust, Result, RuntimeError};
use axum::{
    Json,
    extract::State,
    http::{HeaderMap, StatusCode},
};
use chrono::{DateTime, Duration, Utc};
use codifix_federation::{
    CanonicalJsonCodec, ContentDigest, device_replica::*, publication::InstanceId,
};
use serde::{Deserialize, Serialize};
use std::sync::Arc;

#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct DeviceReplicaClaims {
    pub purpose: String,
    pub authority: InstanceId,
    pub key_generation: u32,
    pub issued_at: DateTime<Utc>,
    pub expires_at: DateTime<Utc>,
    pub payload_digest: ContentDigest,
    pub page: DeviceReplicaPage,
}
#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct SignedDeviceReplicaPage {
    pub claims: DeviceReplicaClaims,
    pub signature: Vec<u8>,
}
fn bytes(v: &impl Serialize) -> Result<Vec<u8>> {
    let bytes = CanonicalJsonCodec::encode(v).map_err(|_| RuntimeError::InvalidResponse)?;
    if bytes.len() > 4 * 1024 * 1024 {
        return Err(RuntimeError::InvalidResponse);
    }
    Ok(bytes)
}

impl GatewaySecurity {
    pub fn sign_device_replica_page(
        &self,
        authority: InstanceId,
        page: DeviceReplicaPage,
    ) -> Result<SignedDeviceReplicaPage> {
        if !page.valid() {
            return Err(RuntimeError::InvalidResponse);
        }
        let now = Utc::now();
        let claims = DeviceReplicaClaims {
            purpose: "codifix-private-device-replica-v1".into(),
            authority,
            key_generation: self.trust().keys().len() as u32,
            issued_at: now,
            expires_at: now + Duration::seconds(30),
            payload_digest: codifix_federation_store::audit_digest(&page)?,
            page,
        };
        let signature = self.sign_current(&bytes(&claims)?)?;
        Ok(SignedDeviceReplicaPage { claims, signature })
    }
}
impl ReceiptTrust {
    pub fn verify_device_replica_page(
        &self,
        signed: &SignedDeviceReplicaPage,
        expected: &DeviceReplicaRequest,
        authority: InstanceId,
    ) -> Result<()> {
        let c = &signed.claims;
        let now = Utc::now();
        if c.purpose != "codifix-private-device-replica-v1"
            || c.authority != authority
            || c.key_generation != self.keys().len() as u32
            || c.issued_at > now + Duration::seconds(1)
            || c.expires_at <= now
            || c.expires_at <= c.issued_at
            || c.expires_at > c.issued_at + Duration::seconds(30)
            || !c.page.valid()
            || &c.page.request != expected
            || c.payload_digest != codifix_federation_store::audit_digest(&c.page)?
        {
            return Err(RuntimeError::InvalidResponse);
        }
        self.verify_current_signature(c.key_generation, &bytes(c)?, &signed.signature)
    }
}
impl crate::GatewayClient {
    pub async fn attest_device_replica(
        &self,
        page: &DeviceReplicaPage,
    ) -> Result<SignedDeviceReplicaPage> {
        if !page.valid() || page.request.scope.space != self.space {
            return Err(RuntimeError::Configuration);
        }
        let response = self
            .endpoint
            .client
            .post(self.endpoint.url("/v1/projects/device-replica/attest")?)
            .bearer_auth(&self.token)
            .body(bytes(page)?)
            .header("Content-Type", "application/json")
            .send()
            .await
            .map_err(|_| RuntimeError::Unavailable)?;
        crate::sync::check_status(response.status())?;
        let signed: SignedDeviceReplicaPage = crate::http::json(response, 4 * 1024 * 1024).await?;
        self.trust
            .verify_device_replica_page(&signed, &page.request, self.authority)?;
        if &signed.claims.page != page {
            return Err(RuntimeError::InvalidResponse);
        }
        Ok(signed)
    }
}
impl crate::InstanceSync {
    pub async fn attest_device_replica(
        &self,
        page: &DeviceReplicaPage,
    ) -> Result<SignedDeviceReplicaPage> {
        self.require_active().await?;
        self.client.attest_device_replica(page).await
    }
    pub fn verify_device_replica(
        &self,
        signed: &SignedDeviceReplicaPage,
        request: &DeviceReplicaRequest,
    ) -> Result<()> {
        self.client
            .trust
            .verify_device_replica_page(signed, request, self.client.authority)
    }
}
pub(crate) async fn attest(
    State(server): State<Arc<GatewayServer>>,
    headers: HeaderMap,
    Json(page): Json<DeviceReplicaPage>,
) -> std::result::Result<Json<SignedDeviceReplicaPage>, StatusCode> {
    let actor = server.authenticate(&headers)?;
    let authority = server
        .store
        .authorize_device_replica_attestation(server.fence, actor, &page)
        .await
        .map_err(|_| StatusCode::FORBIDDEN)?;
    server
        .security
        .sign_device_replica_page(authority, page)
        .map(Json)
        .map_err(|_| StatusCode::SERVICE_UNAVAILABLE)
}

#[cfg(test)]
mod tests {
    use super::*;
    use codifix_federation::{publication::SpaceId, subscription::ProjectScope};
    use uuid::Uuid;
    #[test]
    fn private_page_attestation_binds_device_scope_cursor_purpose_and_expiry() {
        let security = GatewaySecurity::initial(&[73; 32], Default::default())
            .expect("valid attestation fixture");
        let authority = InstanceId::new(Uuid::new_v4()).expect("valid attestation fixture");
        let request = DeviceReplicaRequest {
            schema_version: 1,
            nonce: Uuid::new_v4(),
            device: Uuid::new_v4(),
            scope: ProjectScope {
                company: Uuid::new_v4(),
                area: "workshop".into(),
                owner: authority,
                space: SpaceId::new(Uuid::new_v4()).expect("valid attestation fixture"),
                project: Uuid::new_v4(),
            },
            catalog_revision: 1,
            after: 0,
            through: None,
            limit: 8,
        };
        let signed = security
            .sign_device_replica_page(
                authority,
                DeviceReplicaPage {
                    request: request.clone(),
                    through: 0,
                    next: 0,
                    entries: vec![],
                },
            )
            .expect("valid attestation fixture");
        let trust = security.trust();
        trust
            .verify_device_replica_page(&signed, &request, authority)
            .expect("valid attestation fixture");
        let mut another = request.clone();
        another.device = Uuid::new_v4();
        assert!(
            trust
                .verify_device_replica_page(&signed, &another, authority)
                .is_err()
        );
        another = request.clone();
        another.scope.project = Uuid::new_v4();
        assert!(
            trust
                .verify_device_replica_page(&signed, &another, authority)
                .is_err()
        );
        let mut altered = signed.clone();
        altered.claims.purpose = "codifix-project-page-v1".into();
        assert!(
            trust
                .verify_device_replica_page(&altered, &request, authority)
                .is_err()
        );
        altered = signed.clone();
        altered.claims.expires_at = Utc::now() - Duration::seconds(1);
        assert!(
            trust
                .verify_device_replica_page(&altered, &request, authority)
                .is_err()
        );
        altered = signed;
        altered.signature[0] ^= 1;
        assert!(
            trust
                .verify_device_replica_page(&altered, &request, authority)
                .is_err()
        );
    }
}
