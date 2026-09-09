// SPDX-License-Identifier: AGPL-3.0-or-later
use crate::{Endpoint, ReceiptTrust, Result, RuntimeError, SignedPage, http};
use mem_federation::publication::*;
use mem_federation_store::{
    Acceptance, PublicationIntent, PublishedPage, ReplicaStore, prepare_source_changes,
    record_acceptance,
};
use sqlx::{PgPool, Row, types::Json};

#[derive(Clone)]
pub struct GatewayClient {
    endpoint: Endpoint,
    token: String,
    pub(crate) space: SpaceId,
    pub(crate) authority: InstanceId,
    pub(crate) trust: ReceiptTrust,
}
impl GatewayClient {
    pub fn new(
        endpoint: Endpoint,
        token: String,
        space: SpaceId,
        authority: InstanceId,
        trust: ReceiptTrust,
    ) -> Result<Self> {
        if token.len() < 32 {
            return Err(RuntimeError::Configuration);
        }
        Ok(Self {
            endpoint,
            token,
            space,
            authority,
            trust,
        })
    }
    pub async fn accept(&self, intent: &PublicationIntent) -> Result<Acceptance> {
        if intent.envelope.space != self.space {
            return Err(RuntimeError::InvalidResponse);
        }
        let response = self
            .endpoint
            .client
            .post(self.endpoint.url("/v1/accept")?)
            .bearer_auth(&self.token)
            .json(intent)
            .send()
            .await
            .map_err(|_| RuntimeError::Unavailable)?;
        check_status(response.status())?;
        let accepted: Acceptance = http::json(response, 128 * 1024).await?;
        match (&accepted.outcome, &accepted.delivery) {
            (PublicationOutcome::Applied | PublicationOutcome::Duplicate, Some(delivery))
                if delivery.envelope == intent.envelope && delivery.locator == intent.locator =>
            {
                let expected = PublicationDelivery::new(
                    delivery.sequence,
                    delivery.locator.clone(),
                    delivery.envelope.clone(),
                )
                .map_err(|_| RuntimeError::InvalidResponse)?;
                if expected != *delivery {
                    return Err(RuntimeError::InvalidResponse);
                }
            }
            (PublicationOutcome::Rejected(_) | PublicationOutcome::Quarantined(_), None) => {}
            _ => return Err(RuntimeError::InvalidResponse),
        }
        Ok(accepted)
    }
    pub async fn pull(&self, after: u32, limit: u32) -> Result<PublishedPage> {
        if !(1..=32).contains(&limit) {
            return Err(RuntimeError::Configuration);
        }
        let nonce = uuid::Uuid::new_v4();
        let response = self
            .endpoint
            .client
            .get(self.endpoint.url("/v1/pull")?)
            .bearer_auth(&self.token)
            .query(&[("after", after), ("limit", limit)])
            .query(&[("nonce", nonce)])
            .send()
            .await
            .map_err(|_| RuntimeError::Unavailable)?;
        check_status(response.status())?;
        let signed: SignedPage = http::json(response, 3 * 1024 * 1024).await?;
        let page = self
            .trust
            .verify_page(signed, nonce, self.space, self.authority)?;
        if page.space != self.space
            || page.authority != self.authority
            || page.after != after
            || page.published_through < after
            || page.records.len() != ((page.published_through - after).min(limit) as usize)
        {
            return Err(RuntimeError::InvalidResponse);
        }
        for (i, record) in page.records.iter().enumerate() {
            if record.sequence.get() != after + 1 + i as u32 || record.envelope.space != self.space
            {
                return Err(RuntimeError::InvalidResponse);
            }
        }
        Ok(page)
    }
    /// Pages are bounded and accumulated before the atomic store swap. The
    /// initial high-water mark prevents chasing a continuously growing stream.
    /// Over 16 MiB fails closed and requires a larger reviewed deployment limit.
    pub async fn resynchronize(&self, replica: &ReplicaStore) -> Result<u32> {
        let mut history = Vec::new();
        let mut receipts = Vec::new();
        let mut bytes = 0;
        let mut after = 0;
        let mut target = None;
        loop {
            let limit = target.map_or(32, |head: u32| (head - after).min(32));
            if limit == 0 {
                break;
            }
            let page = self.pull(after, limit).await?;
            let head = *target.get_or_insert(page.published_through);
            if page.published_through < head {
                return Err(RuntimeError::InvalidResponse);
            }
            for receipt in page.receipts {
                bytes += serde_json::to_vec(&receipt)
                    .map_err(|_| RuntimeError::InvalidResponse)?
                    .len();
                receipts.push(receipt);
            }
            for record in page.records {
                bytes += serde_json::to_vec(&record)
                    .map_err(|_| RuntimeError::InvalidResponse)?
                    .len();
                if bytes > 16 * 1024 * 1024 {
                    return Err(RuntimeError::InvalidResponse);
                }
                after = record.sequence.get();
                history.push(record);
            }
            if after == head {
                break;
            }
        }
        self.trust
            .verify_history(self.authority, &history, &receipts)?;
        Ok(replica
            .resynchronize_with_receipts(self.authority, &history, &receipts)
            .await?)
    }
}
fn check_status(status: reqwest::StatusCode) -> Result<()> {
    if status == reqwest::StatusCode::UNAUTHORIZED || status == reqwest::StatusCode::FORBIDDEN {
        Err(RuntimeError::Unauthorized)
    } else if status.is_success() {
        Ok(())
    } else if status.is_server_error() {
        Err(RuntimeError::Unavailable)
    } else {
        Err(RuntimeError::InvalidResponse)
    }
}

/// One instance's scheduler. Each iteration resolves current local permissions
/// outside this object; do not persist an old role/permission snapshot in it.
#[derive(Clone)]
pub struct InstanceSync {
    pub(crate) pool: PgPool,
    pub(crate) client: GatewayClient,
    pub(crate) replica: ReplicaStore,
    pub(crate) instance: InstanceId,
}
impl InstanceSync {
    pub async fn new(pool: PgPool, client: GatewayClient, instance: InstanceId) -> Result<Self> {
        let replica =
            ReplicaStore::initialize(pool.clone(), client.space, instance, client.authority)
                .await?;
        replica.enroll_trust(client.trust.keys()).await?;
        Ok(Self {
            pool,
            client,
            replica,
            instance,
        })
    }
    pub fn replica(&self) -> &ReplicaStore {
        &self.replica
    }
    pub async fn catch_up(&self) -> Result<u32> {
        let mut after = self.verified_snapshot().await?.sequence();
        // One bounded page per tick: unrelated local work stays responsive.
        let page = self.client.pull(after, 32).await?;
        after = self
            .replica
            .receive_with_receipts(self.client.authority, &page.records, &page.receipts)
            .await?;
        Ok(after)
    }
    pub async fn resynchronize(&self) -> Result<u32> {
        self.client.resynchronize(&self.replica).await
    }
    /// Crypto validation also runs on retained receipts after restart/offline
    /// access, not just on the original network response.
    pub async fn verified_snapshot(&self) -> Result<PublicationState> {
        let (state, receipts) = self.replica.audited_snapshot().await?;
        self.client
            .trust
            .verify_history(self.client.authority, state.accepted(), &receipts)?;
        Ok(state)
    }
    /// A service cannot construct LocalProjectAccess for a removed/disabled
    /// principal. Preserve that pause as durable status rather than only a log.
    pub async fn note_local_authorization_failure(&self) -> Result<()> {
        sqlx::query("UPDATE federation_replicas SET sync_failures=LEAST(sync_failures+1,10),sync_error='local_authorization',retry_at=clock_timestamp()+interval '30 seconds' WHERE space=$1 AND receiver=$2")
            .bind(self.client.space.as_uuid()).bind(self.instance.as_uuid()).execute(&self.pool).await.map_err(mem_federation_store::StoreError::from)?;
        Ok(())
    }
    pub async fn cycle(&self, access: &LocalProjectAccess) -> Result<u32> {
        let space = self.client.space.as_uuid();
        let instance = self.instance.as_uuid();
        let due:bool=sqlx::query_scalar("SELECT retry_at<=clock_timestamp() FROM federation_replicas WHERE space=$1 AND receiver=$2")
            .bind(space).bind(instance).fetch_one(&self.pool).await.map_err(mem_federation_store::StoreError::from)?;
        if !due {
            return Ok(0);
        }
        let result = self.connected_cycle(access).await;
        if result.is_ok() {
            sqlx::query("UPDATE federation_replicas SET sync_failures=0,sync_error=NULL,retry_at=clock_timestamp() WHERE space=$1 AND receiver=$2")
                .bind(space).bind(instance).execute(&self.pool).await.map_err(mem_federation_store::StoreError::from)?;
        } else {
            sqlx::query("UPDATE federation_replicas SET sync_failures=LEAST(sync_failures+1,10),sync_error=$3,retry_at=clock_timestamp()+make_interval(secs=>LEAST(300,power(2,LEAST(sync_failures+1,8)))::double precision) WHERE space=$1 AND receiver=$2")
                .bind(space).bind(instance).bind(if matches!(result,Err(RuntimeError::Unauthorized)){"unauthorized"}else{"sync_failed"})
                .execute(&self.pool).await.map_err(mem_federation_store::StoreError::from)?;
        }
        result
    }
    async fn connected_cycle(&self, access: &LocalProjectAccess) -> Result<u32> {
        mem_federation_store::authorize_source_pull(
            &self.pool,
            access,
            self.client.space,
            self.instance,
        )
        .await?;
        self.catch_up().await?;
        let sent = self.publish_pending(access).await?;
        self.catch_up().await?;
        Ok(sent)
    }

    /// Process one locally authorized review owner's queue without assigning
    /// the background worker's identity or permissions to that owner. Failures
    /// here do not change the shared pull scheduler's retry state.
    pub async fn publish_pending(&self, access: &LocalProjectAccess) -> Result<u32> {
        mem_federation_store::authorize_source_pull(
            &self.pool,
            access,
            self.client.space,
            self.instance,
        )
        .await?;
        // Capture is already durable while disconnected. Prepare can continue
        // from cached membership; the gateway rechecks current authority later.
        let known = self.verified_snapshot().await?;
        prepare_source_changes(&self.pool, &known, access, self.instance, 32).await?;
        let rows=sqlx::query("SELECT locator,envelope FROM federation_source_outbox WHERE space=$1 AND publisher=$2 AND project_id=$3 AND principal_id=$4 AND acceptance IS NULL AND NOT blocked AND available_at<=clock_timestamp() ORDER BY queued_at,record_id LIMIT 32")
            .bind(self.client.space.as_uuid()).bind(self.instance.as_uuid()).bind(access.project).bind(access.principal)
            .fetch_all(&self.pool).await.map_err(mem_federation_store::StoreError::from)?;
        let mut sent = 0;
        for row in rows {
            let envelope: Json<PublicationEnvelope> = row
                .try_get("envelope")
                .map_err(mem_federation_store::StoreError::from)?;
            let intent = PublicationIntent {
                locator: mem_federation::RecordLocator::new(
                    row.try_get::<String, _>("locator")
                        .map_err(mem_federation_store::StoreError::from)?,
                )
                .map_err(|_| RuntimeError::InvalidResponse)?,
                envelope: envelope.0,
            };
            mem_federation_store::authorize_source_intent(
                &self.pool,
                &known,
                access,
                self.instance,
                &intent,
            )
            .await?;
            let reply = self.client.accept(&intent).await;
            if let Ok(Acceptance {
                delivery: Some(delivery),
                ..
            }) = &reply
            {
                let mut tx = self
                    .pool
                    .begin()
                    .await
                    .map_err(mem_federation_store::StoreError::from)?;
                record_acceptance(&mut tx, delivery).await?;
                tx.commit()
                    .await
                    .map_err(mem_federation_store::StoreError::from)?;
                sent += 1;
            } else {
                let quarantined = matches!(
                    reply,
                    Ok(Acceptance {
                        outcome: PublicationOutcome::Quarantined(_),
                        ..
                    })
                );
                let permanent = matches!(
                    reply,
                    Ok(Acceptance {
                        outcome: PublicationOutcome::Rejected(_),
                        ..
                    }) | Err(RuntimeError::Unauthorized | RuntimeError::InvalidResponse)
                );
                sqlx::query("UPDATE federation_source_outbox SET attempts=attempts+$4,blocked=$5 OR attempts+$4>=8,last_error=$6,available_at=clock_timestamp()+make_interval(secs=>LEAST(300,power(2,LEAST(attempts+1,8)))::double precision) WHERE space=$1 AND publisher=$2 AND record_id=$3 AND acceptance IS NULL")
                    .bind(self.client.space.as_uuid()).bind(self.instance.as_uuid()).bind(intent.envelope.record_id.as_uuid()).bind(if quarantined {0i32}else{1})
                    .bind(permanent).bind(if quarantined {"quarantined"}else if permanent {"rejected"}else{"unavailable"})
                    .execute(&self.pool).await.map_err(mem_federation_store::StoreError::from)?;
                reply?;
            }
        }
        Ok(sent)
    }
}
