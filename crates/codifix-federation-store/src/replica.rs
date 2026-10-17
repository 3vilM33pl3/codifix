// SPDX-License-Identifier: AGPL-3.0-or-later
use codifix_federation::publication::*;
use sqlx::{PgPool, Postgres, Row, Transaction, types::Json};

use crate::{DeliveryStatus, Result, StoreError, projection::snapshot};

#[derive(Clone)]
pub struct ReplicaStore {
    pub(crate) pool: PgPool,
    pub(crate) space: SpaceId,
    pub(crate) receiver: InstanceId,
    pub(crate) authority: InstanceId,
}

impl ReplicaStore {
    /// Explicit trusted configuration: it is never inferred from received data.
    pub async fn initialize(
        pool: PgPool,
        space: SpaceId,
        receiver: InstanceId,
        authority: InstanceId,
    ) -> Result<Self> {
        sqlx::query("INSERT INTO federation_replicas (space,receiver,authority,projection) VALUES ($1,$2,$3,$4) ON CONFLICT DO NOTHING")
            .bind(space.as_uuid()).bind(receiver.as_uuid()).bind(authority.as_uuid())
            .bind(snapshot(&PublicationState::new(space,authority))).execute(&pool).await?;
        let configured: uuid::Uuid = sqlx::query_scalar(
            "SELECT authority FROM federation_replicas WHERE space=$1 AND receiver=$2",
        )
        .bind(space.as_uuid())
        .bind(receiver.as_uuid())
        .fetch_one(&pool)
        .await?;
        if configured != authority.as_uuid() {
            return Err(StoreError::Binding);
        }
        Ok(Self {
            pool,
            space,
            receiver,
            authority,
        })
    }

    pub(crate) async fn load(
        &self,
        tx: &mut Transaction<'_, Postgres>,
        verify_projection: bool,
    ) -> Result<PublicationState> {
        let row = sqlx::query("SELECT authority,cursor,projection FROM federation_replicas WHERE space=$1 AND receiver=$2 FOR UPDATE")
            .bind(self.space.as_uuid()).bind(self.receiver.as_uuid()).fetch_one(&mut **tx).await?;
        if row.try_get::<uuid::Uuid, _>("authority")? != self.authority.as_uuid() {
            return Err(StoreError::Binding);
        }
        let records = sqlx::query("SELECT sequence,delivery FROM federation_inbox WHERE space=$1 AND receiver=$2 ORDER BY sequence")
            .bind(self.space.as_uuid()).bind(self.receiver.as_uuid()).fetch_all(&mut **tx).await?;
        let mut state = PublicationState::new(self.space, self.authority);
        for row in records {
            let record: Json<PublicationDelivery> = row.try_get("delivery")?;
            if row.try_get::<i64, _>("sequence")? != i64::from(record.sequence.get())
                || state.apply(record.envelope.publisher, record.0) != PublicationOutcome::Applied
            {
                return Err(StoreError::Corrupt);
            }
        }
        if row.try_get::<i64, _>("cursor")? != i64::from(state.sequence())
            || (verify_projection
                && row.try_get::<serde_json::Value, _>("projection")? != snapshot(&state))
        {
            return Err(StoreError::Corrupt);
        }
        Ok(state)
    }

    async fn persist(
        &self,
        tx: &mut Transaction<'_, Postgres>,
        state: &PublicationState,
        after: u32,
    ) -> Result<()> {
        for delivery in state.accepted().iter().filter(|d| d.sequence.get() > after) {
            sqlx::query("INSERT INTO federation_inbox (space,receiver,sequence,delivery) VALUES ($1,$2,$3,$4)")
                .bind(self.space.as_uuid()).bind(self.receiver.as_uuid()).bind(i64::from(delivery.sequence.get()))
                .bind(Json(delivery)).execute(&mut **tx).await?;
        }
        sqlx::query(
            "UPDATE federation_replicas SET cursor=$3,projection=$4 WHERE space=$1 AND receiver=$2",
        )
        .bind(self.space.as_uuid())
        .bind(self.receiver.as_uuid())
        .bind(i64::from(state.sequence()))
        .bind(snapshot(state))
        .execute(&mut **tx)
        .await?;
        Ok(())
    }

    /// Identity/signatures and publisher attribution must have been verified by
    /// the gateway transport. The authority parameter is independent of payload.
    pub async fn receive(
        &self,
        verified_authority: InstanceId,
        batch: &[PublicationDelivery],
    ) -> Result<u32> {
        self.receive_inner(verified_authority, batch, None).await
    }
    pub async fn receive_with_receipts(
        &self,
        authority: InstanceId,
        batch: &[PublicationDelivery],
        receipts: &[crate::AuditReceipt],
    ) -> Result<u32> {
        self.receive_inner(authority, batch, Some(receipts)).await
    }
    async fn receive_inner(
        &self,
        verified_authority: InstanceId,
        batch: &[PublicationDelivery],
        receipts: Option<&[crate::AuditReceipt]>,
    ) -> Result<u32> {
        if verified_authority != self.authority {
            return Err(StoreError::Binding);
        }
        if batch.len() > 1000 {
            return Err(StoreError::Limits);
        }
        let mut tx = self.pool.begin().await?;
        let mut state = self.load(&mut tx, true).await?;
        let after = state.sequence();
        for delivery in batch {
            match state.apply(delivery.envelope.publisher, delivery.clone()) {
                PublicationOutcome::Applied | PublicationOutcome::Duplicate => {}
                outcome => return Err(StoreError::Outcome(outcome)),
            }
        }
        self.persist(&mut tx, &state, after).await?;
        self.persist_receipts(&mut tx, batch, receipts).await?;
        tx.commit().await?;
        Ok(state.sequence())
    }

    /// Reference full-log resync. Verify from genesis and require the same known
    /// prefix. A truncated snapshot or fork cannot erase known revocations/history.
    /// Large deployments will need authenticated paginated staging/checkpoints.
    pub async fn resynchronize(
        &self,
        verified_authority: InstanceId,
        history: &[PublicationDelivery],
    ) -> Result<u32> {
        self.resynchronize_inner(verified_authority, history, None)
            .await
    }
    pub async fn resynchronize_with_receipts(
        &self,
        authority: InstanceId,
        history: &[PublicationDelivery],
        receipts: &[crate::AuditReceipt],
    ) -> Result<u32> {
        self.resynchronize_inner(authority, history, Some(receipts))
            .await
    }
    async fn resynchronize_inner(
        &self,
        verified_authority: InstanceId,
        history: &[PublicationDelivery],
        receipts: Option<&[crate::AuditReceipt]>,
    ) -> Result<u32> {
        if verified_authority != self.authority {
            return Err(StoreError::Binding);
        }
        let mut tx = self.pool.begin().await?;
        // Rebuild a damaged derived projection, but never roll back the log or cursor.
        let old = self.load(&mut tx, false).await?;
        if !history.starts_with(old.accepted()) {
            return Err(StoreError::Resync);
        }
        let mut state = PublicationState::new(self.space, self.authority);
        for delivery in history {
            let outcome = state.apply(delivery.envelope.publisher, delivery.clone());
            if outcome != PublicationOutcome::Applied {
                return Err(StoreError::Outcome(outcome));
            }
        }
        // Reuse the verified prefix; append plus projection replacement is an
        // atomic swap without destructively clearing a good inbox first.
        self.persist(&mut tx, &state, old.sequence()).await?;
        self.persist_receipts(&mut tx, history, receipts).await?;
        tx.commit().await?;
        Ok(state.sequence())
    }

    /// Trusted local read of retained shared data. Offline-copy access and local
    /// authorization must be enforced by the application using this projection.
    pub async fn snapshot(&self) -> Result<PublicationState> {
        let mut tx = self.pool.begin().await?;
        let state = self.load(&mut tx, true).await?;
        tx.commit().await?;
        Ok(state)
    }

    pub async fn status(&self, key: RecordKey) -> Result<Option<DeliveryStatus>> {
        let state = self.snapshot().await?;
        Ok(state
            .accepted_record(key)
            .map(|delivery| DeliveryStatus::Replicated {
                sequence: delivery.sequence.get(),
                receiver: self.receiver,
            }))
    }
}
