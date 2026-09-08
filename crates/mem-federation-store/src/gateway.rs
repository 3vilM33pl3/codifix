// SPDX-License-Identifier: AGPL-3.0-or-later
use mem_federation::{SpaceCapability, publication::*};
use sqlx::{PgPool, Postgres, Row, Transaction};
use uuid::Uuid;

use crate::{
    DeliveryStatus, PublicationIntent, Result, StoreError, projection::snapshot, replay_gateway,
};

#[derive(Debug, Clone, Copy)]
pub struct WriterFence {
    pub(crate) space: SpaceId,
    pub(crate) epoch: Uuid,
}

#[derive(Clone)]
pub struct GatewayStore {
    pub(crate) pool: PgPool,
}

#[derive(Debug)]
pub struct Acceptance {
    pub outcome: PublicationOutcome,
    pub delivery: Option<PublicationDelivery>,
}

impl GatewayStore {
    pub fn new(pool: PgPool) -> Self {
        Self { pool }
    }

    /// Explicit bootstrap only. Opening an existing space never changes its
    /// authority or steals the writer epoch. No HTTP authentication is implied.
    pub async fn initialize(&self, space: SpaceId, authority: InstanceId) -> Result<WriterFence> {
        let mut tx = self.pool.begin().await?;
        sqlx::query("INSERT INTO federation_gateway_spaces (space,authority,writer_epoch,projection) VALUES ($1,$2,$3,$4) ON CONFLICT DO NOTHING")
            .bind(space.as_uuid()).bind(authority.as_uuid()).bind(Uuid::new_v4())
            .bind(snapshot(&PublicationState::new(space,authority))).execute(&mut *tx).await?;
        let row = sqlx::query("SELECT authority,writer_epoch FROM federation_gateway_spaces WHERE space=$1 FOR UPDATE")
            .bind(space.as_uuid()).fetch_one(&mut *tx).await?;
        if row.try_get::<Uuid, _>("authority")? != authority.as_uuid() {
            return Err(StoreError::Binding);
        }
        let fence = WriterFence {
            space,
            epoch: row.try_get("writer_epoch")?,
        };
        tx.commit().await?;
        Ok(fence)
    }

    /// Explicit recovery handover. Rotate after fencing old process/credentials
    /// as appropriate; DB epochs alone cannot fence an in-flight PDS request.
    pub async fn rotate_writer(&self, old: WriterFence) -> Result<WriterFence> {
        let mut tx = self.pool.begin().await?;
        self.lock(&mut tx, old).await?;
        let epoch = Uuid::new_v4();
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

    pub(crate) async fn lock(
        &self,
        tx: &mut Transaction<'_, Postgres>,
        fence: WriterFence,
    ) -> Result<InstanceId> {
        let row = sqlx::query("SELECT authority,writer_epoch FROM federation_gateway_spaces WHERE space=$1 FOR UPDATE")
            .bind(fence.space.as_uuid()).fetch_one(&mut **tx).await?;
        if row.try_get::<Uuid, _>("writer_epoch")? != fence.epoch {
            return Err(StoreError::Fenced);
        }
        Ok(InstanceId::new(row.try_get("authority")?)?)
    }

    pub(crate) async fn load(
        &self,
        tx: &mut Transaction<'_, Postgres>,
        space: SpaceId,
        authority: InstanceId,
    ) -> Result<PublicationState> {
        let state = replay_gateway(&mut *tx, space, authority).await?;
        let row = sqlx::query(
            "SELECT accepted_sequence,projection FROM federation_gateway_spaces WHERE space=$1",
        )
        .bind(space.as_uuid())
        .fetch_one(&mut **tx)
        .await?;
        if row.try_get::<i64, _>("accepted_sequence")? != i64::from(state.sequence())
            || row.try_get::<serde_json::Value, _>("projection")? != snapshot(&state)
        {
            return Err(StoreError::Corrupt);
        }
        Ok(state)
    }

    pub async fn accept(
        &self,
        fence: WriterFence,
        authenticated: InstanceId,
        intent: &PublicationIntent,
    ) -> Result<Acceptance> {
        let mut tx = self.pool.begin().await?;
        let authority = self.lock(&mut tx, fence).await?;
        let mut state = self.load(&mut tx, fence.space, authority).await?;
        // Exact retry must use the ORIGINAL sequence, even after revocation.
        // The reducer checks authenticated publisher and every immutable binding.
        let prior = state.accepted().iter().find(|d| {
            d.envelope.key() == intent.envelope.key()
                || (d.envelope.publisher == intent.envelope.publisher
                    && d.locator == intent.locator)
        });
        let sequence = if let Some(prior) = prior {
            prior.sequence
        } else {
            AuthoritySequence::new(state.sequence().checked_add(1).ok_or(StoreError::Outcome(
                PublicationOutcome::Rejected(PublicationReject::CounterExhausted),
            ))?)?
        };
        let delivery =
            PublicationDelivery::new(sequence, intent.locator.clone(), intent.envelope.clone())?;
        let outcome = state.apply(authenticated, delivery.clone());
        if outcome == PublicationOutcome::Applied {
            let e = &delivery.envelope;
            sqlx::query("INSERT INTO federation_accepted (space,sequence,publisher,record_id,locator,delivery) VALUES ($1,$2,$3,$4,$5,$6)")
                .bind(fence.space.as_uuid()).bind(i64::from(sequence.get())).bind(e.publisher.as_uuid())
                .bind(e.record_id.as_uuid()).bind(delivery.locator.as_str()).bind(sqlx::types::Json(&delivery))
                .execute(&mut *tx).await?;
            sqlx::query("INSERT INTO federation_publish_outbox (space,sequence) VALUES ($1,$2)")
                .bind(fence.space.as_uuid())
                .bind(i64::from(sequence.get()))
                .execute(&mut *tx)
                .await?;
            sqlx::query("UPDATE federation_gateway_spaces SET accepted_sequence=$2,projection=$3 WHERE space=$1")
                .bind(fence.space.as_uuid()).bind(i64::from(sequence.get())).bind(snapshot(&state)).execute(&mut *tx).await?;
        }
        let accepted = matches!(
            outcome,
            PublicationOutcome::Applied | PublicationOutcome::Duplicate
        );
        tx.commit().await?;
        Ok(Acceptance {
            outcome,
            delivery: accepted.then_some(delivery),
        })
    }

    /// Only current gateway Pull membership matters here, not original authorship
    /// or a lagging receiver cache. Local permission intersection is additional.
    pub async fn pull(
        &self,
        space: SpaceId,
        authenticated: InstanceId,
        after: u32,
        limit: u32,
    ) -> Result<Vec<PublicationDelivery>> {
        if !(1..=1000).contains(&limit) {
            return Err(StoreError::Limits);
        }
        let mut tx = self.pool.begin().await?;
        let row = sqlx::query("SELECT authority,published_sequence FROM federation_gateway_spaces WHERE space=$1 FOR UPDATE")
            .bind(space.as_uuid()).fetch_one(&mut *tx).await?;
        let state = self
            .load(&mut tx, space, InstanceId::new(row.try_get("authority")?)?)
            .await?;
        if !state.has_capability(authenticated, SpaceCapability::Pull) {
            return Err(StoreError::Outcome(PublicationOutcome::Rejected(
                PublicationReject::Unauthorized,
            )));
        }
        let published: i64 = row.try_get("published_sequence")?;
        if i64::from(after) > published {
            return Err(StoreError::Resync);
        }
        let deliveries = state
            .accepted()
            .iter()
            .filter(|d| d.sequence.get() > after && i64::from(d.sequence.get()) <= published)
            .take(limit as usize)
            .cloned()
            .collect();
        tx.commit().await?;
        Ok(deliveries)
    }

    /// Trusted administrative observation, NOT a remotely authorized endpoint.
    pub async fn status(&self, space: SpaceId, key: RecordKey) -> Result<DeliveryStatus> {
        let row = sqlx::query("SELECT a.sequence,o.published FROM federation_accepted a JOIN federation_publish_outbox o USING(space,sequence) WHERE a.space=$1 AND a.publisher=$2 AND a.record_id=$3")
            .bind(space.as_uuid()).bind(key.publisher.as_uuid()).bind(key.id.as_uuid()).fetch_one(&self.pool).await?;
        let sequence =
            u32::try_from(row.try_get::<i64, _>("sequence")?).map_err(|_| StoreError::Corrupt)?;
        Ok(if row.try_get("published")? {
            DeliveryStatus::Published { sequence }
        } else {
            DeliveryStatus::Accepted { sequence }
        })
    }
}
