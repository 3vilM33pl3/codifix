// SPDX-License-Identifier: AGPL-3.0-or-later
//! Transport-neutral receipt storage. Cryptography belongs in a verified edge.
use crate::{GatewayStore, Result, StoreError, WriterFence};
use mem_federation::{CanonicalJsonCodec, ContentDigest, publication::*};
use serde::{Deserialize, Serialize};
use sqlx::{Row, types::Json};
use std::collections::BTreeSet;

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize, Default)]
#[serde(deny_unknown_fields)]
pub struct SpacePolicy {
    /// Explicit provider IDs, not URLs. Empty denies all external model use.
    pub external_models: BTreeSet<String>,
}
impl SpacePolicy {
    pub fn validate(&self) -> Result<()> {
        if self.external_models.len() > 16
            || self.external_models.iter().any(|id| {
                id.is_empty()
                    || id.len() > 64
                    || !id
                        .bytes()
                        .all(|c| c.is_ascii_alphanumeric() || b"_-".contains(&c))
            })
        {
            return Err(StoreError::Binding);
        }
        Ok(())
    }
}
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct ReceiptKey {
    pub generation: u32,
    pub first_sequence: u32,
    /// Opaque crypto adapter key encoding, not a resolvable URL or identity.
    pub public_key: String,
}
pub fn validate_key_schedule(keys: &[ReceiptKey]) -> Result<()> {
    if keys.is_empty() || keys.len() > 32 {
        return Err(StoreError::Binding);
    }
    for (i, key) in keys.iter().enumerate() {
        if key.generation != i as u32 + 1
            || key.first_sequence == 0
            || key.public_key.is_empty()
            || key.public_key.len() > 256
            || (i == 0 && key.first_sequence != 1)
            || (i > 0 && key.first_sequence <= keys[i - 1].first_sequence)
            || keys[..i].iter().any(|old| old.public_key == key.public_key)
        {
            return Err(StoreError::Binding);
        }
    }
    Ok(())
}
pub fn audit_digest<T: Serialize>(value: &T) -> Result<ContentDigest> {
    CanonicalJsonCodec::digest(value).map_err(|_| StoreError::Binding)
}
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct ReceiptClaims {
    pub purpose: String,
    pub space: SpaceId,
    pub authority: InstanceId,
    pub sequence: u32,
    pub delivery_digest: ContentDigest,
    pub previous: Option<ContentDigest>,
    pub key_generation: u32,
    pub policy: SpacePolicy,
    /// Bounded, opaque evidence from a trusted transport verifier. Not a CAR or
    /// repository history; e.g. commit/record digest and enrolled key attribution.
    pub transport: serde_json::Value,
}
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct AuditReceipt {
    pub claims: ReceiptClaims,
    pub signature: Vec<u8>,
}
pub trait ReceiptAuthority: Send + Sync {
    fn generation(&self) -> u32;
    fn public_key(&self) -> String;
    fn sign(&self, claims: ReceiptClaims) -> Result<AuditReceipt>;
}

impl GatewayStore {
    /// Administrator-supplied enrollment/rollover is atomic with writer fencing.
    /// Old readers and writers must both use this epoch. Existing unsigned
    /// published history requires explicit revalidation, never automatic adoption.
    pub async fn enroll_security(
        &self,
        old: WriterFence,
        keys: &[ReceiptKey],
        policy: &SpacePolicy,
    ) -> Result<WriterFence> {
        validate_key_schedule(keys)?;
        policy.validate()?;
        let mut tx = self.pool.begin().await?;
        self.lock(&mut tx, old).await?;
        let head: i64 = sqlx::query_scalar(
            "SELECT published_sequence FROM federation_gateway_spaces WHERE space=$1",
        )
        .bind(old.space.as_uuid())
        .fetch_one(&mut *tx)
        .await?;
        let count: i64 = sqlx::query_scalar("SELECT count(*) FROM federation_audit WHERE space=$1")
            .bind(old.space.as_uuid())
            .fetch_one(&mut *tx)
            .await?;
        if head != count {
            return Err(StoreError::Corrupt);
        }
        let previous: Option<Json<Vec<ReceiptKey>>> =
            sqlx::query_scalar("SELECT keys FROM federation_security WHERE space=$1")
                .bind(old.space.as_uuid())
                .fetch_optional(&mut *tx)
                .await?;
        match previous {
            Some(previous) if previous.0 == keys => {}
            Some(previous)
                if keys.starts_with(&previous.0)
                    && keys.len() == previous.len() + 1
                    && i64::from(keys[keys.len() - 1].first_sequence) == head + 1 => {}
            None if keys.len() == 1 && head == 0 => {}
            _ => return Err(StoreError::Binding),
        }
        sqlx::query("INSERT INTO federation_security(space,keys,policy) VALUES($1,$2,$3) ON CONFLICT(space) DO UPDATE SET keys=$2,policy=$3")
            .bind(old.space.as_uuid()).bind(Json(keys)).bind(Json(policy)).execute(&mut *tx).await?;
        let epoch = uuid::Uuid::new_v4();
        sqlx::query("UPDATE federation_gateway_spaces SET writer_epoch=$2 WHERE space=$1")
            .bind(old.space.as_uuid())
            .bind(epoch)
            .execute(&mut *tx)
            .await?;
        sqlx::query("UPDATE federation_publish_outbox SET lease_token=NULL,lease_until=NULL WHERE space=$1 AND NOT published")
            .bind(old.space.as_uuid()).execute(&mut *tx).await?;
        tx.commit().await?;
        Ok(WriterFence {
            space: old.space,
            epoch,
        })
    }

    /// Current membership, writer epoch, page and receipts share a transaction.
    /// No raw repository material is exposed. The runtime additionally signs a
    /// fresh nonce-bound response including this current policy and page head.
    pub async fn pull_audited(
        &self,
        fence: WriterFence,
        member: InstanceId,
        after: u32,
        limit: u32,
    ) -> Result<crate::PublishedPage> {
        if !(1..=32).contains(&limit) {
            return Err(StoreError::Limits);
        }
        let mut tx = self.pool.begin().await?;
        let authority = self.lock(&mut tx, fence).await?;
        let state = self.load(&mut tx, fence.space, authority).await?;
        if !state.has_capability(member, mem_federation::SpaceCapability::Pull) {
            return Err(StoreError::Outcome(PublicationOutcome::Rejected(
                PublicationReject::Unauthorized,
            )));
        }
        let head: i64 = sqlx::query_scalar(
            "SELECT published_sequence FROM federation_gateway_spaces WHERE space=$1",
        )
        .bind(fence.space.as_uuid())
        .fetch_one(&mut *tx)
        .await?;
        if i64::from(after) > head {
            return Err(StoreError::Resync);
        }
        let policy: Json<SpacePolicy> =
            sqlx::query_scalar("SELECT policy FROM federation_security WHERE space=$1")
                .bind(fence.space.as_uuid())
                .fetch_one(&mut *tx)
                .await?;
        let records: Vec<_> = state
            .accepted()
            .iter()
            .filter(|r| r.sequence.get() > after && i64::from(r.sequence.get()) <= head)
            .take(limit as usize)
            .cloned()
            .collect();
        let rows=sqlx::query("SELECT receipt FROM federation_audit WHERE space=$1 AND sequence>$2 ORDER BY sequence LIMIT $3")
            .bind(fence.space.as_uuid()).bind(i64::from(after)).bind(i64::from(limit)).fetch_all(&mut *tx).await?;
        let receipts: Vec<AuditReceipt> = rows
            .into_iter()
            .map(|r| r.try_get::<Json<AuditReceipt>, _>("receipt").map(|j| j.0))
            .collect::<std::result::Result<_, _>>()?;
        if receipts.len() != records.len() {
            return Err(StoreError::Corrupt);
        }
        tx.commit().await?;
        Ok(crate::PublishedPage {
            space: fence.space,
            authority,
            after,
            published_through: head as u32,
            records,
            receipts,
            policy: policy.0,
        })
    }
}
