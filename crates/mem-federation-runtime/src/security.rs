// SPDX-License-Identifier: AGPL-3.0-or-later
use crate::{Result, RuntimeError};
use mem_atproto_verify::{DetachedSigner, DetachedVerifier};
use mem_federation::{CanonicalJsonCodec, publication::*};
use mem_federation_store::{
    AuditReceipt, PublishedPage, ReceiptAuthority, ReceiptClaims, ReceiptKey, SpacePolicy,
    audit_digest, receipt_matches, validate_key_schedule,
};
use serde::{Deserialize, Serialize};
use std::sync::Arc;
use uuid::Uuid;

#[derive(Clone)]
pub struct ReceiptTrust {
    keys: Vec<ReceiptKey>,
    verifiers: Vec<DetachedVerifier>,
}
impl ReceiptTrust {
    pub fn new(keys: Vec<ReceiptKey>) -> Result<Self> {
        validate_key_schedule(&keys)?;
        let verifiers = keys
            .iter()
            .map(|k| {
                DetachedVerifier::new(k.public_key.clone()).map_err(|_| RuntimeError::Configuration)
            })
            .collect::<Result<_>>()?;
        Ok(Self { keys, verifiers })
    }
    pub fn keys(&self) -> &[ReceiptKey] {
        &self.keys
    }
    pub fn verify_receipt(
        &self,
        authority: InstanceId,
        delivery: &PublicationDelivery,
        receipt: &AuditReceipt,
    ) -> Result<()> {
        receipt_matches(receipt, delivery, authority, &self.keys)?;
        self.verifiers[(receipt.claims.key_generation - 1) as usize]
            .verify(&canonical(&receipt.claims)?, &receipt.signature)
            .map_err(|_| RuntimeError::InvalidResponse)
    }
    pub fn verify_history(
        &self,
        authority: InstanceId,
        records: &[PublicationDelivery],
        receipts: &[AuditReceipt],
    ) -> Result<()> {
        if records.len() != receipts.len() {
            return Err(RuntimeError::InvalidResponse);
        }
        let mut previous = None;
        for (record, receipt) in records.iter().zip(receipts) {
            self.verify_receipt(authority, record, receipt)?;
            if receipt.claims.previous != previous {
                return Err(RuntimeError::InvalidResponse);
            }
            previous = Some(audit_digest(receipt)?);
        }
        Ok(())
    }
    pub(crate) fn verify_page(
        &self,
        signed: SignedPage,
        nonce: Uuid,
        space: SpaceId,
        authority: InstanceId,
    ) -> Result<PublishedPage> {
        let c = &signed.claims;
        if c.purpose != "memory-pull-page-v1"
            || c.nonce != nonce
            || c.page.space != space
            || c.page.authority != authority
            || c.key_generation != self.keys.len() as u32
            || c.page.receipts.len() != c.page.records.len()
        {
            return Err(RuntimeError::InvalidResponse);
        }
        c.page.policy.validate()?;
        self.verifiers
            .last()
            .ok_or(RuntimeError::Configuration)?
            .verify(&canonical(c)?, &signed.signature)
            .map_err(|_| RuntimeError::InvalidResponse)?;
        for (record, receipt) in c.page.records.iter().zip(&c.page.receipts) {
            self.verify_receipt(authority, record, receipt)?;
        }
        for pair in c.page.receipts.windows(2) {
            if pair[1].claims.previous != Some(audit_digest(&pair[0])?) {
                return Err(RuntimeError::InvalidResponse);
            }
        }
        Ok(signed.claims.page)
    }
}
#[derive(Clone)]
pub struct GatewaySecurity {
    signer: Arc<DetachedSigner>,
    trust: ReceiptTrust,
    policy: SpacePolicy,
}
impl GatewaySecurity {
    /// Explicit fresh-space bootstrap helper; never used to learn a remote key.
    pub fn initial(secret: &[u8], policy: SpacePolicy) -> Result<Self> {
        let key = DetachedSigner::from_secret(secret)
            .map_err(|_| RuntimeError::Configuration)?
            .public_key();
        Self::new(
            secret,
            vec![ReceiptKey {
                generation: 1,
                first_sequence: 1,
                public_key: key,
            }],
            policy,
        )
    }
    pub fn new(secret: &[u8], keys: Vec<ReceiptKey>, policy: SpacePolicy) -> Result<Self> {
        policy.validate()?;
        let trust = ReceiptTrust::new(keys)?;
        let signer =
            DetachedSigner::from_secret(secret).map_err(|_| RuntimeError::Configuration)?;
        if trust
            .keys
            .last()
            .ok_or(RuntimeError::Configuration)?
            .public_key
            != signer.public_key()
        {
            return Err(RuntimeError::Configuration);
        }
        Ok(Self {
            signer: Arc::new(signer),
            trust,
            policy,
        })
    }
    pub fn trust(&self) -> ReceiptTrust {
        self.trust.clone()
    }
    pub fn policy(&self) -> &SpacePolicy {
        &self.policy
    }
    pub(crate) fn sign_page(&self, page: PublishedPage, nonce: Uuid) -> Result<SignedPage> {
        let claims = PageClaims {
            purpose: "memory-pull-page-v1".into(),
            nonce,
            key_generation: self.generation(),
            page,
        };
        let signature = self
            .signer
            .sign(&canonical(&claims)?)
            .map_err(|_| RuntimeError::InvalidResponse)?;
        Ok(SignedPage { claims, signature })
    }
}
impl ReceiptAuthority for GatewaySecurity {
    fn generation(&self) -> u32 {
        self.trust.keys.len() as u32
    }
    fn public_key(&self) -> String {
        self.signer.public_key()
    }
    fn sign(&self, claims: ReceiptClaims) -> mem_federation_store::Result<AuditReceipt> {
        let bytes = CanonicalJsonCodec::encode(&claims)
            .map_err(|_| mem_federation_store::StoreError::Binding)?;
        let signature = self
            .signer
            .sign(&bytes)
            .map_err(|_| mem_federation_store::StoreError::Binding)?;
        Ok(AuditReceipt { claims, signature })
    }
}
fn canonical<T: Serialize>(value: &T) -> Result<Vec<u8>> {
    CanonicalJsonCodec::encode(value).map_err(|_| RuntimeError::InvalidResponse)
}
#[derive(Debug, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct PageClaims {
    pub purpose: String,
    pub nonce: Uuid,
    pub key_generation: u32,
    pub page: PublishedPage,
}
#[derive(Debug, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct SignedPage {
    pub claims: PageClaims,
    pub signature: Vec<u8>,
}

#[cfg(test)]
#[allow(clippy::unwrap_used)]
mod tests {
    use super::*;
    #[test]
    fn fresh_pages_cannot_be_replayed_rebound_or_modified() {
        let security = GatewaySecurity::initial(&[3; 32], SpacePolicy::default()).unwrap();
        let trust = security.trust();
        let nonce = Uuid::new_v4();
        let space = SpaceId::new(Uuid::new_v4()).unwrap();
        let authority = InstanceId::new(Uuid::new_v4()).unwrap();
        let page = PublishedPage {
            space,
            authority,
            after: 0,
            published_through: 0,
            records: vec![],
            receipts: vec![],
            policy: SpacePolicy::default(),
        };
        trust
            .verify_page(
                security.sign_page(page.clone(), nonce).unwrap(),
                nonce,
                space,
                authority,
            )
            .unwrap();
        assert!(
            trust
                .verify_page(
                    security.sign_page(page.clone(), nonce).unwrap(),
                    Uuid::new_v4(),
                    space,
                    authority
                )
                .is_err()
        );
        assert!(
            trust
                .verify_page(
                    security.sign_page(page.clone(), nonce).unwrap(),
                    nonce,
                    SpaceId::new(Uuid::new_v4()).unwrap(),
                    authority
                )
                .is_err()
        );
        assert!(
            trust
                .verify_page(
                    security.sign_page(page.clone(), nonce).unwrap(),
                    nonce,
                    space,
                    InstanceId::new(Uuid::new_v4()).unwrap()
                )
                .is_err()
        );
        let mut signed = security.sign_page(page.clone(), nonce).unwrap();
        signed
            .claims
            .page
            .policy
            .external_models
            .insert("evil".into());
        assert!(trust.verify_page(signed, nonce, space, authority).is_err());
        let mut signed = security.sign_page(page.clone(), nonce).unwrap();
        signed.signature.truncate(63);
        assert!(trust.verify_page(signed, nonce, space, authority).is_err());
        let mut signed = security.sign_page(page, nonce).unwrap();
        signed.claims.purpose = "another-context".into();
        assert!(trust.verify_page(signed, nonce, space, authority).is_err());
    }
    #[test]
    fn enrollment_is_explicit_bounded_and_never_resolves_identifiers() {
        for key in [
            "did:key:z1",
            "https://127.0.0.1/private",
            "http://169.254.169.254/",
        ] {
            assert!(
                ReceiptTrust::new(vec![ReceiptKey {
                    generation: 1,
                    first_sequence: 1,
                    public_key: key.into()
                }])
                .is_err()
            );
        }
        let mut keys = GatewaySecurity::initial(&[3; 32], SpacePolicy::default())
            .unwrap()
            .trust()
            .keys()
            .to_vec();
        assert!(GatewaySecurity::new(&[4; 32], keys.clone(), SpacePolicy::default()).is_err());
        keys.push(keys[0].clone());
        assert!(ReceiptTrust::new(keys).is_err());
        assert!(
            SpacePolicy {
                external_models: std::collections::BTreeSet::from(["https://untrusted".into()])
            }
            .validate()
            .is_err()
        );
    }
}
