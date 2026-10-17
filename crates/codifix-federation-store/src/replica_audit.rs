// SPDX-License-Identifier: AGPL-3.0-or-later
use crate::{
    AuditReceipt, ReceiptKey, ReplicaStore, Result, StoreError, audit_digest, validate_key_schedule,
};
use codifix_federation::publication::PublicationDelivery;
use sqlx::{Postgres, Transaction, types::Json};

pub fn receipt_matches(
    receipt: &AuditReceipt,
    delivery: &PublicationDelivery,
    authority: codifix_federation::publication::InstanceId,
    keys: &[ReceiptKey],
) -> Result<()> {
    let key = keys
        .iter()
        .rev()
        .find(|k| k.first_sequence <= delivery.sequence.get())
        .ok_or(StoreError::Binding)?;
    let c = &receipt.claims;
    c.policy.validate()?;
    if c.purpose != "memory-publication-receipt-v1"
        || c.space != delivery.envelope.space
        || c.authority != authority
        || c.sequence != delivery.sequence.get()
        || c.key_generation != key.generation
        || c.delivery_digest != audit_digest(delivery)?
        || receipt.signature.len() != 64
        || serde_json::to_vec(&c.transport)?.len() > 4096
    {
        return Err(StoreError::Binding);
    }
    Ok(())
}
impl ReplicaStore {
    pub async fn audited_snapshot(
        &self,
    ) -> Result<(
        codifix_federation::publication::PublicationState,
        Vec<AuditReceipt>,
    )> {
        let mut tx = self.pool.begin().await?;
        let state = self.load(&mut tx, true).await?;
        let rows:Vec<Json<AuditReceipt>>=sqlx::query_scalar("SELECT receipt FROM federation_replica_receipts WHERE space=$1 AND receiver=$2 ORDER BY sequence")
            .bind(self.space.as_uuid()).bind(self.receiver.as_uuid()).fetch_all(&mut *tx).await?;
        tx.commit().await?;
        Ok((state, rows.into_iter().map(|j| j.0).collect()))
    }
    /// Explicit trusted key distribution. Never learn keys from remote pages.
    pub async fn enroll_trust(&self, keys: &[ReceiptKey]) -> Result<()> {
        validate_key_schedule(keys)?;
        let mut tx = self.pool.begin().await?;
        let cursor: i64 = sqlx::query_scalar(
            "SELECT cursor FROM federation_replicas WHERE space=$1 AND receiver=$2 FOR UPDATE",
        )
        .bind(self.space.as_uuid())
        .bind(self.receiver.as_uuid())
        .fetch_one(&mut *tx)
        .await?;
        let previous: Option<Json<Vec<ReceiptKey>>> = sqlx::query_scalar(
            "SELECT keys FROM federation_replica_trust WHERE space=$1 AND receiver=$2",
        )
        .bind(self.space.as_uuid())
        .bind(self.receiver.as_uuid())
        .fetch_optional(&mut *tx)
        .await?;
        if previous
            .as_ref()
            .is_some_and(|old| !keys.starts_with(&old.0))
        {
            return Err(StoreError::Binding);
        }
        let receipts:Vec<Json<AuditReceipt>>=sqlx::query_scalar("SELECT receipt FROM federation_replica_receipts WHERE space=$1 AND receiver=$2 ORDER BY sequence")
            .bind(self.space.as_uuid()).bind(self.receiver.as_uuid()).fetch_all(&mut *tx).await?;
        if receipts.len() as i64 != cursor {
            return Err(StoreError::Corrupt);
        }
        for receipt in receipts {
            let key = keys
                .iter()
                .rev()
                .find(|k| k.first_sequence <= receipt.claims.sequence)
                .ok_or(StoreError::Binding)?;
            if key.generation != receipt.claims.key_generation {
                return Err(StoreError::Binding);
            }
        }
        sqlx::query("INSERT INTO federation_replica_trust(space,receiver,keys) VALUES($1,$2,$3) ON CONFLICT(space,receiver) DO UPDATE SET keys=$3")
            .bind(self.space.as_uuid()).bind(self.receiver.as_uuid()).bind(Json(keys)).execute(&mut *tx).await?;
        tx.commit().await?;
        Ok(())
    }
    /// Trusted crypto edge calls this inside the same transaction as inbox and
    /// cursor. Domain/hash/chain checks remain independently enforced by storage.
    pub(crate) async fn persist_receipts(
        &self,
        tx: &mut Transaction<'_, Postgres>,
        batch: &[PublicationDelivery],
        receipts: Option<&[AuditReceipt]>,
    ) -> Result<()> {
        let keys: Option<Json<Vec<ReceiptKey>>> = sqlx::query_scalar(
            "SELECT keys FROM federation_replica_trust WHERE space=$1 AND receiver=$2",
        )
        .bind(self.space.as_uuid())
        .bind(self.receiver.as_uuid())
        .fetch_optional(&mut **tx)
        .await?;
        let (keys, receipts) = match (keys, receipts) {
            (Some(keys), Some(receipts)) if batch.len() == receipts.len() => (keys, receipts),
            (None, None) => return Ok(()),
            _ => return Err(StoreError::Binding),
        };
        for (delivery, receipt) in batch.iter().zip(receipts) {
            receipt_matches(receipt, delivery, self.authority, &keys)?;
            let sequence = i64::from(delivery.sequence.get());
            let previous = if sequence == 1 {
                None
            } else {
                let prior:Json<AuditReceipt>=sqlx::query_scalar("SELECT receipt FROM federation_replica_receipts WHERE space=$1 AND receiver=$2 AND sequence=$3")
                    .bind(self.space.as_uuid()).bind(self.receiver.as_uuid()).bind(sequence-1).fetch_one(&mut **tx).await?;
                Some(audit_digest(&prior.0)?)
            };
            if receipt.claims.previous != previous {
                return Err(StoreError::Binding);
            }
            let existing:Option<Json<AuditReceipt>>=sqlx::query_scalar("SELECT receipt FROM federation_replica_receipts WHERE space=$1 AND receiver=$2 AND sequence=$3")
                .bind(self.space.as_uuid()).bind(self.receiver.as_uuid()).bind(sequence).fetch_optional(&mut **tx).await?;
            if let Some(existing) = existing {
                if existing.0 != *receipt {
                    return Err(StoreError::Binding);
                }
            } else {
                sqlx::query("INSERT INTO federation_replica_receipts(space,receiver,sequence,receipt) VALUES($1,$2,$3,$4)")
                .bind(self.space.as_uuid()).bind(self.receiver.as_uuid()).bind(sequence).bind(Json(receipt)).execute(&mut **tx).await?;
            }
        }
        Ok(())
    }
    pub async fn receipts(&self) -> Result<Vec<AuditReceipt>> {
        let rows:Vec<Json<AuditReceipt>>=sqlx::query_scalar("SELECT receipt FROM federation_replica_receipts WHERE space=$1 AND receiver=$2 ORDER BY sequence")
            .bind(self.space.as_uuid()).bind(self.receiver.as_uuid()).fetch_all(&self.pool).await?;
        Ok(rows.into_iter().map(|j| j.0).collect())
    }
}
