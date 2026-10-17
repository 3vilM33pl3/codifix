// SPDX-License-Identifier: AGPL-3.0-or-later
//! Explicit origin-to-gateway project policy. No device gateway credential and
//! no complete repository/CAR proof is implied by a selective page.
use crate::{GatewayClient, GatewaySecurity, GatewayServer, ReceiptTrust, Result, RuntimeError};
use axum::{
    Json,
    extract::State,
    http::{HeaderMap, StatusCode},
};
use chrono::{DateTime, Duration, Utc};
use codifix_federation::{CanonicalJsonCodec, ContentDigest, subscription::*};
use codifix_federation_store::{ProjectBindingReceipt, ProjectEvent, ProjectPage, audit_digest};
use serde::{Deserialize, Serialize};
use std::sync::Arc;

#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct ProjectPageClaims {
    pub purpose: String,
    pub key_generation: u32,
    pub issued_at: DateTime<Utc>,
    pub expires_at: DateTime<Utc>,
    pub request_digest: ContentDigest,
    pub payload_digest: ContentDigest,
    pub page: ProjectPage,
}
#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct SignedProjectPage {
    pub claims: ProjectPageClaims,
    pub signature: Vec<u8>,
}
#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct ProjectBindingClaims {
    pub purpose: String,
    pub key_generation: u32,
    pub issued_at: DateTime<Utc>,
    pub expires_at: DateTime<Utc>,
    pub request: ProjectFeedRegistration,
    pub receipt: ProjectBindingReceipt,
}
#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct SignedProjectBinding {
    pub claims: ProjectBindingClaims,
    pub signature: Vec<u8>,
}

fn bytes<T: Serialize>(v: &T) -> Result<Vec<u8>> {
    CanonicalJsonCodec::encode(v).map_err(|_| RuntimeError::InvalidResponse)
}
fn fresh(issued: DateTime<Utc>, expires: DateTime<Utc>) -> bool {
    let now = Utc::now();
    issued <= now + Duration::seconds(1)
        && expires > now
        && expires > issued
        && expires <= issued + Duration::seconds(30)
}
impl ReceiptTrust {
    pub fn verify_project_page(
        &self,
        signed: SignedProjectPage,
        expected: &ProjectPageRequest,
        authority: codifix_federation::publication::InstanceId,
    ) -> Result<ProjectPage> {
        let c = &signed.claims;
        if !expected.valid()
            || c.purpose != "memory-scoped-page-v1"
            || c.key_generation != self.keys().len() as u32
            || !fresh(c.issued_at, c.expires_at)
            || c.page.request != *expected
            || c.page.authority != authority
            || c.request_digest != audit_digest(expected)?
            || c.payload_digest != audit_digest(&c.page)?
            || c.page.through < expected.after
            || expected.through.is_some_and(|v| v != c.page.through)
            || c.page.entries.len()
                != (c.page.through - expected.after).min(u64::from(expected.limit)) as usize
            || c.page.next != expected.after + c.page.entries.len() as u64
        {
            return Err(RuntimeError::InvalidResponse);
        }
        self.verify_current_signature(c.key_generation, &bytes(c)?, &signed.signature)?;
        for (i, e) in c.page.entries.iter().enumerate() {
            if e.sequence != expected.after + i as u64 + 1
                || e.event.stream().origin != expected.scope.owner
            {
                return Err(RuntimeError::InvalidResponse);
            }
            if let ProjectEvent::Record {
                stream,
                delivery,
                receipt,
            } = &e.event
            {
                if delivery.envelope.space != expected.scope.space
                    || delivery.envelope.publisher != expected.scope.owner
                    || !delivery.envelope.supported_schema()
                    || codifix_federation_store::delivery_stream(delivery)? != *stream
                    || codifix_federation::publication::PublicationDelivery::new(
                        delivery.sequence,
                        delivery.locator.clone(),
                        delivery.envelope.clone(),
                    )
                    .map_err(|_| RuntimeError::InvalidResponse)?
                        != **delivery
                {
                    return Err(RuntimeError::InvalidResponse);
                }
                self.verify_receipt(authority, delivery, receipt)?;
            }
        }
        // Receipts are intentionally non-contiguous: verifying their individual
        // signatures is not verifying a complete publication/repository history.
        Ok(signed.claims.page)
    }
    pub fn verify_project_binding(
        &self,
        signed: SignedProjectBinding,
        expected: &ProjectFeedRegistration,
    ) -> Result<ProjectBindingReceipt> {
        let c = &signed.claims;
        if c.purpose != "memory-scoped-binding-v1"
            || c.request != *expected
            || c.key_generation != self.keys().len() as u32
            || !fresh(c.issued_at, c.expires_at)
            || c.receipt.operation_id != expected.operation_id
            || c.receipt.request_digest != audit_digest(expected)?
            || Some(c.receipt.revision) != expected.expected_revision.checked_add(1)
        {
            return Err(RuntimeError::InvalidResponse);
        }
        self.verify_current_signature(c.key_generation, &bytes(c)?, &signed.signature)?;
        Ok(signed.claims.receipt)
    }
}
impl GatewaySecurity {
    pub fn sign_project_page(&self, page: ProjectPage) -> Result<SignedProjectPage> {
        for entry in &page.entries {
            if let ProjectEvent::Record {
                delivery, receipt, ..
            } = &entry.event
            {
                self.trust()
                    .verify_receipt(page.authority, delivery, receipt)?;
            }
        }
        let now = Utc::now();
        let claims = ProjectPageClaims {
            purpose: "memory-scoped-page-v1".into(),
            key_generation: self.trust().keys().len() as u32,
            issued_at: now,
            expires_at: now + Duration::seconds(30),
            request_digest: audit_digest(&page.request)?,
            payload_digest: audit_digest(&page)?,
            page,
        };
        let signature = self.sign_current(&bytes(&claims)?)?;
        Ok(SignedProjectPage { claims, signature })
    }
    pub(crate) fn sign_project_binding(
        &self,
        request: ProjectFeedRegistration,
        receipt: ProjectBindingReceipt,
    ) -> Result<SignedProjectBinding> {
        let now = Utc::now();
        let claims = ProjectBindingClaims {
            purpose: "memory-scoped-binding-v1".into(),
            key_generation: self.trust().keys().len() as u32,
            issued_at: now,
            expires_at: now + Duration::seconds(30),
            request,
            receipt,
        };
        let signature = self.sign_current(&bytes(&claims)?)?;
        Ok(SignedProjectBinding { claims, signature })
    }
}
impl GatewayClient {
    pub async fn register_project(
        &self,
        request: &ProjectFeedRegistration,
    ) -> Result<ProjectBindingReceipt> {
        if request.scope.space != self.space {
            return Err(RuntimeError::Configuration);
        }
        let response = self
            .endpoint
            .client
            .post(self.endpoint.url("/v1/projects/register")?)
            .bearer_auth(&self.token)
            .json(request)
            .send()
            .await
            .map_err(|_| RuntimeError::Unavailable)?;
        crate::sync::check_status(response.status())?;
        let signed: SignedProjectBinding = crate::http::json(response, 512 * 1024).await?;
        self.trust.verify_project_binding(signed, request)
    }
    pub async fn project_page(&self, request: &ProjectPageRequest) -> Result<SignedProjectPage> {
        if !request.valid() || request.scope.space != self.space {
            return Err(RuntimeError::Configuration);
        }
        let response = self
            .endpoint
            .client
            .post(self.endpoint.url("/v1/projects/page")?)
            .bearer_auth(&self.token)
            .json(request)
            .send()
            .await
            .map_err(|_| RuntimeError::Unavailable)?;
        crate::sync::check_status(response.status())?;
        let signed: SignedProjectPage = crate::http::json(response, 4 * 1024 * 1024).await?;
        self.trust
            .verify_project_page(signed.clone(), request, self.authority)?;
        Ok(signed)
    }
}
pub(crate) async fn register(
    State(server): State<Arc<GatewayServer>>,
    headers: HeaderMap,
    Json(request): Json<ProjectFeedRegistration>,
) -> std::result::Result<Json<SignedProjectBinding>, StatusCode> {
    let actor = server.authenticate(&headers)?;
    let receipt = server
        .store
        .register_project(server.fence, actor, &request)
        .await
        .map_err(error)?;
    server
        .security
        .sign_project_binding(request, receipt)
        .map(Json)
        .map_err(|_| StatusCode::SERVICE_UNAVAILABLE)
}
pub(crate) async fn page(
    State(server): State<Arc<GatewayServer>>,
    headers: HeaderMap,
    Json(request): Json<ProjectPageRequest>,
) -> std::result::Result<Json<SignedProjectPage>, StatusCode> {
    let actor = server.authenticate(&headers)?;
    let page = server
        .store
        .project_page(server.fence, actor, &request)
        .await
        .map_err(error)?;
    server
        .security
        .sign_project_page(page)
        .map(Json)
        .map_err(|_| StatusCode::SERVICE_UNAVAILABLE)
}
fn error(e: codifix_federation_store::StoreError) -> StatusCode {
    match e {
        codifix_federation_store::StoreError::Binding => StatusCode::FORBIDDEN,
        codifix_federation_store::StoreError::Resync => StatusCode::CONFLICT,
        codifix_federation_store::StoreError::Limits => StatusCode::PAYLOAD_TOO_LARGE,
        _ => StatusCode::SERVICE_UNAVAILABLE,
    }
}

#[cfg(test)]
#[allow(clippy::unwrap_used)]
mod tests {
    use super::*;
    use codifix_federation::{AuthorIdentity, RecordLocator, publication::*};
    use codifix_federation_store::{ProjectEntry, ReceiptAuthority, ReceiptClaims, SpacePolicy};
    use uuid::Uuid;
    fn identity() -> InstanceId {
        InstanceId::new(Uuid::new_v4()).unwrap()
    }
    fn page() -> ProjectPage {
        let owner = identity();
        let scope = ProjectScope {
            company: Uuid::new_v4(),
            area: "general".into(),
            owner,
            project: Uuid::new_v4(),
            space: SpaceId::new(Uuid::new_v4()).unwrap(),
        };
        ProjectPage {
            request: ProjectPageRequest {
                nonce: Uuid::new_v4(),
                device: Uuid::new_v4(),
                scope,
                catalog_revision: 1,
                policy_revision: 2,
                binding_revision: 3,
                after: 0,
                through: None,
                limit: 32,
            },
            authority: identity(),
            through: 1,
            next: 1,
            entries: vec![ProjectEntry {
                sequence: 1,
                event: ProjectEvent::Review {
                    review: ProjectStreamReview {
                        stream: StreamId {
                            origin: owner,
                            canonical: RecordId::new(Uuid::new_v4()).unwrap(),
                        },
                        review_id: Uuid::new_v4(),
                        records: vec![],
                        enabled: false,
                    },
                },
            }],
        }
    }
    #[test]
    fn scoped_signature_binds_device_scope_nonce_policy_cursor_and_exact_payload() {
        let security = GatewaySecurity::initial(&[17; 32], SpacePolicy::default()).unwrap();
        let trust = security.trust();
        let page = page();
        let signed = security.sign_project_page(page.clone()).unwrap();
        assert_eq!(
            trust
                .verify_project_page(signed.clone(), &page.request, page.authority)
                .unwrap(),
            page
        );
        for change in [
            (|r: &mut ProjectPageRequest| r.device = Uuid::new_v4()) as fn(&mut ProjectPageRequest),
            |r| r.nonce = Uuid::new_v4(),
            |r| r.scope.project = Uuid::new_v4(),
            |r| r.scope.company = Uuid::new_v4(),
            |r| r.scope.owner = identity(),
            |r| r.scope.area = "trust".into(),
            |r| r.catalog_revision += 1,
            |r| r.policy_revision += 1,
            |r| r.binding_revision += 1,
            |r| r.after = 1,
            |r| r.through = Some(2),
        ] {
            let mut request = page.request.clone();
            change(&mut request);
            assert!(
                trust
                    .verify_project_page(signed.clone(), &request, page.authority)
                    .is_err()
            );
        }
        for change in [
            (|c: &mut ProjectPageClaims| c.purpose = "memory-pull-page-v1".into())
                as fn(&mut ProjectPageClaims),
            |c| c.expires_at = c.issued_at + Duration::seconds(31),
            |c| c.expires_at = Utc::now() - Duration::seconds(1),
            |c| c.issued_at = Utc::now() + Duration::seconds(10),
            |c| c.page.entries[0].sequence = 2,
            |c| c.key_generation = 0,
        ] {
            let mut bad = signed.clone();
            change(&mut bad.claims);
            bad.signature = security.sign_current(&bytes(&bad.claims).unwrap()).unwrap();
            assert!(
                trust
                    .verify_project_page(bad, &page.request, page.authority)
                    .is_err()
            );
        }
        let mut altered = signed.clone();
        altered.claims.page.next = 99;
        assert!(
            trust
                .verify_project_page(altered, &page.request, page.authority)
                .is_err()
        );
        assert!(
            trust
                .verify_project_page(signed, &page.request, identity())
                .is_err()
        );
    }
    #[test]
    fn individual_publication_receipts_are_required_without_claiming_a_complete_chain() {
        let security = GatewaySecurity::initial(&[17; 32], SpacePolicy::default()).unwrap();
        let mut page = page();
        let stream = page.entries[0].event.stream();
        let delivery = PublicationDelivery::new(
            AuthoritySequence::new(73).unwrap(),
            RecordLocator::new("approved-evidence-locator").unwrap(),
            PublicationEnvelope {
                schema_version: 2,
                space: page.request.scope.space,
                publisher: stream.origin,
                record_id: RecordId::new(Uuid::new_v4()).unwrap(),
                author: AuthorIdentity::new("original-author").unwrap(),
                authored_at: Utc::now(),
                causation: None,
                record: PublicationRecord::CanonicalState {
                    stream,
                    state: StatePublication {
                        revision: StateRevision::new(1).unwrap(),
                        content_version: SharedVersion::new(1).unwrap(),
                        status: codifix_record::MemoryStatus::Active,
                        confidence: ConfidencePpm::try_from(800_000).unwrap(),
                        tombstone: true,
                    },
                },
            },
        )
        .unwrap();
        let receipt = security
            .sign(ReceiptClaims {
                purpose: "memory-publication-receipt-v1".into(),
                space: page.request.scope.space,
                authority: page.authority,
                sequence: 73,
                delivery_digest: audit_digest(&delivery).unwrap(),
                previous: None,
                key_generation: 1,
                policy: SpacePolicy::default(),
                transport: serde_json::json!({"fixture_verified":true}),
            })
            .unwrap();
        page.entries[0].event = ProjectEvent::Record {
            stream,
            delivery: Box::new(delivery),
            receipt,
        };
        let signed = security.sign_project_page(page.clone()).unwrap();
        security
            .trust()
            .verify_project_page(signed, &page.request, page.authority)
            .unwrap();
        if let ProjectEvent::Record { receipt, .. } = &mut page.entries[0].event {
            receipt.signature[0] ^= 1;
        }
        assert!(security.sign_project_page(page).is_err());
    }
}
