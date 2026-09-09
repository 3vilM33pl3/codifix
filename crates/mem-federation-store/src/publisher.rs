// SPDX-License-Identifier: AGPL-3.0-or-later
use std::{future::Future, pin::Pin};

use mem_federation::publication::PublicationDelivery;
use sqlx::{Row, types::Json};
use uuid::Uuid;

use crate::{
    AuditReceipt, GatewayStore, ReceiptAuthority, ReceiptClaims, ReceiptKey, Result, SpacePolicy,
    StoreError, WriterFence, audit_digest,
};

#[derive(Debug, Clone, Copy)]
pub struct RetryPolicy {
    pub max_attempts: u32,
    pub lease_seconds: u32,
    pub backoff_seconds: u32,
}
impl Default for RetryPolicy {
    fn default() -> Self {
        Self {
            max_attempts: 8,
            lease_seconds: 60,
            backoff_seconds: 5,
        }
    }
}
impl RetryPolicy {
    fn validate(self) -> Result<Self> {
        if !(1..=100).contains(&self.max_attempts)
            || !(1..=3600).contains(&self.lease_seconds)
            || !(1..=3600).contains(&self.backoff_seconds)
        {
            return Err(StoreError::Limits);
        }
        Ok(self)
    }
}

#[derive(Debug)]
pub struct PublicationLease {
    delivery: PublicationDelivery,
    token: Uuid,
    fence: WriterFence,
    attempt: u32,
}
impl PublicationLease {
    pub fn delivery(&self) -> &PublicationDelivery {
        &self.delivery
    }
    pub fn attempt(&self) -> u32 {
        self.attempt
    }
}

#[derive(Debug)]
pub enum Claim {
    Ready(Box<PublicationLease>),
    Idle,
    Waiting,
    Blocked,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum PublishFailure {
    Unavailable,
    Conflict,
    InvalidReceipt,
}

/// This interface is trusted. Success means the exact immutable record exists
/// at this locator (create, or verified equal read-back after an uncertain write).
/// A PDS implementation must verify CID/bytes; "already exists" is NOT success.
/// No PDS client or credentials are supplied by this persistence crate.
pub trait ImmutablePublisher: Send + Sync {
    fn publish<'a>(
        &'a self,
        delivery: &'a PublicationDelivery,
    ) -> Pin<Box<dyn Future<Output = std::result::Result<(), PublishFailure>> + Send + 'a>>;
}

#[derive(Debug, PartialEq, Eq)]
pub enum WorkerOutcome {
    Published(u32),
    Retrying,
    Blocked,
    Idle,
    Waiting,
}

/// Trusted operational view; errors are fixed categories, never raw transport
/// responses that might contain credentials or private repository contents.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct PendingPublication {
    pub sequence: u32,
    pub attempts: u32,
    pub blocked: bool,
    pub leased: bool,
    pub due: bool,
    pub last_error: Option<String>,
}

impl GatewayStore {
    pub async fn pending_publication(
        &self,
        fence: WriterFence,
    ) -> Result<Option<PendingPublication>> {
        let mut tx = self.pool.begin().await?;
        self.lock(&mut tx, fence).await?;
        let row = sqlx::query("SELECT sequence,attempts,blocked,lease_until>clock_timestamp() AS leased,available_at<=clock_timestamp() AS due,last_error FROM federation_publish_outbox WHERE space=$1 AND NOT published ORDER BY sequence LIMIT 1")
            .bind(fence.space.as_uuid()).fetch_optional(&mut *tx).await?;
        let pending = row
            .map(|row| -> Result<_> {
                Ok(PendingPublication {
                    sequence: u32::try_from(row.try_get::<i64, _>("sequence")?)
                        .map_err(|_| StoreError::Corrupt)?,
                    attempts: u32::try_from(row.try_get::<i32, _>("attempts")?)
                        .map_err(|_| StoreError::Corrupt)?,
                    blocked: row.try_get("blocked")?,
                    leased: row.try_get::<Option<bool>, _>("leased")?.unwrap_or(false),
                    due: row.try_get("due")?,
                    last_error: row.try_get("last_error")?,
                })
            })
            .transpose()?;
        tx.commit().await?;
        Ok(pending)
    }

    /// Head-of-line publication deliberately prevents a later record bypassing
    /// failed membership/evidence. Different spaces can publish independently.
    pub async fn claim_publication(
        &self,
        fence: WriterFence,
        policy: RetryPolicy,
    ) -> Result<Claim> {
        let policy = policy.validate()?;
        let mut tx = self.pool.begin().await?;
        self.lock(&mut tx, fence).await?;
        let row = sqlx::query("SELECT o.sequence,o.attempts,o.blocked,o.available_at<=clock_timestamp() AS due, (o.lease_until IS NULL OR o.lease_until<=clock_timestamp()) AS expired,a.delivery FROM federation_publish_outbox o JOIN federation_accepted a USING(space,sequence) WHERE o.space=$1 AND NOT o.published ORDER BY o.sequence LIMIT 1 FOR UPDATE OF o")
            .bind(fence.space.as_uuid()).fetch_optional(&mut *tx).await?;
        let Some(row) = row else {
            tx.commit().await?;
            return Ok(Claim::Idle);
        };
        let attempts: i32 = row.try_get("attempts")?;
        let sequence: i64 = row.try_get("sequence")?;
        if row.try_get::<bool, _>("blocked")? {
            tx.commit().await?;
            return Ok(Claim::Blocked);
        }
        if !row.try_get::<bool, _>("due")? || !row.try_get::<bool, _>("expired")? {
            tx.commit().await?;
            return Ok(Claim::Waiting);
        }
        if i64::from(attempts) >= i64::from(policy.max_attempts) {
            sqlx::query("UPDATE federation_publish_outbox SET blocked=TRUE,last_error='attempts_exhausted',lease_token=NULL,lease_until=NULL WHERE space=$1 AND sequence=$2")
                .bind(fence.space.as_uuid()).bind(sequence).execute(&mut *tx).await?;
            tx.commit().await?;
            return Ok(Claim::Blocked);
        }
        let token = Uuid::new_v4();
        sqlx::query("UPDATE federation_publish_outbox SET attempts=attempts+1,lease_token=$3,lease_until=clock_timestamp()+make_interval(secs => $4) WHERE space=$1 AND sequence=$2")
            .bind(fence.space.as_uuid()).bind(sequence).bind(token).bind(f64::from(policy.lease_seconds))
            .execute(&mut *tx).await?;
        let delivery: Json<PublicationDelivery> = row.try_get("delivery")?;
        tx.commit().await?;
        Ok(Claim::Ready(Box::new(PublicationLease {
            delivery: delivery.0,
            token,
            fence,
            attempt: (attempts + 1) as u32,
        })))
    }

    /// Called only after immutable transport success. The lease CAS rejects late
    /// acknowledgements, including a successful old attempt after writer recovery.
    pub async fn acknowledge_publication(&self, lease: &PublicationLease) -> Result<()> {
        self.acknowledge_inner(lease, None).await
    }
    pub async fn acknowledge_with_receipt(
        &self,
        lease: &PublicationLease,
        transport: serde_json::Value,
        signer: &dyn ReceiptAuthority,
    ) -> Result<()> {
        self.acknowledge_inner(lease, Some((transport, signer)))
            .await
    }
    async fn acknowledge_inner(
        &self,
        lease: &PublicationLease,
        proof: Option<(serde_json::Value, &dyn ReceiptAuthority)>,
    ) -> Result<()> {
        let mut tx = self.pool.begin().await?;
        let authority = self.lock(&mut tx, lease.fence).await?;
        let security = sqlx::query("SELECT keys,policy FROM federation_security WHERE space=$1")
            .bind(lease.fence.space.as_uuid())
            .fetch_optional(&mut *tx)
            .await?;
        let receipt = match (security, proof) {
            (Some(row), Some((transport, signer))) => {
                if serde_json::to_vec(&transport)?.len() > 4096 {
                    return Err(StoreError::Limits);
                }
                let keys: Json<Vec<ReceiptKey>> = row.try_get("keys")?;
                let current = keys.last().ok_or(StoreError::Binding)?;
                if current.generation != signer.generation()
                    || current.public_key != signer.public_key()
                    || current.first_sequence > lease.delivery.sequence.get()
                {
                    return Err(StoreError::Binding);
                }
                let policy:Json<SpacePolicy>=sqlx::query_scalar("SELECT publication_policy FROM federation_accepted WHERE space=$1 AND sequence=$2")
                    .bind(lease.fence.space.as_uuid()).bind(i64::from(lease.delivery.sequence.get())).fetch_one(&mut *tx).await?;
                policy.validate()?;
                let previous = if lease.delivery.sequence.get() == 1 {
                    None
                } else {
                    let prior: Json<AuditReceipt> = sqlx::query_scalar(
                        "SELECT receipt FROM federation_audit WHERE space=$1 AND sequence=$2",
                    )
                    .bind(lease.fence.space.as_uuid())
                    .bind(i64::from(lease.delivery.sequence.get() - 1))
                    .fetch_one(&mut *tx)
                    .await?;
                    Some(audit_digest(&prior.0)?)
                };
                let claims = ReceiptClaims {
                    purpose: "memory-publication-receipt-v1".into(),
                    space: lease.fence.space,
                    authority,
                    sequence: lease.delivery.sequence.get(),
                    delivery_digest: audit_digest(&lease.delivery)?,
                    previous,
                    key_generation: signer.generation(),
                    policy: policy.0,
                    transport,
                };
                let receipt = signer.sign(claims.clone())?;
                if receipt.claims != claims || receipt.signature.len() != 64 {
                    return Err(StoreError::Binding);
                }
                Some(receipt)
            }
            (None, None) => None, // reference store tests, never an enrolled runtime
            _ => return Err(StoreError::Binding),
        };
        let count = sqlx::query("UPDATE federation_publish_outbox SET published=TRUE,lease_token=NULL,lease_until=NULL,last_error=NULL WHERE space=$1 AND sequence=$2 AND lease_token=$3 AND lease_until>clock_timestamp() AND NOT published")
            .bind(lease.fence.space.as_uuid()).bind(i64::from(lease.delivery.sequence.get())).bind(lease.token)
            .execute(&mut *tx).await?.rows_affected();
        if count != 1 {
            return Err(StoreError::LostLease);
        }
        let count = sqlx::query("UPDATE federation_gateway_spaces SET published_sequence=$2 WHERE space=$1 AND published_sequence=$2-1")
            .bind(lease.fence.space.as_uuid()).bind(i64::from(lease.delivery.sequence.get()))
            .execute(&mut *tx).await?.rows_affected();
        if count != 1 {
            return Err(StoreError::Corrupt);
        }
        if let Some(receipt) = receipt {
            sqlx::query("INSERT INTO federation_audit(space,sequence,receipt) VALUES($1,$2,$3)")
                .bind(lease.fence.space.as_uuid())
                .bind(i64::from(lease.delivery.sequence.get()))
                .bind(Json(receipt))
                .execute(&mut *tx)
                .await?;
        }
        tx.commit().await?;
        Ok(())
    }

    pub async fn fail_publication(
        &self,
        lease: &PublicationLease,
        policy: RetryPolicy,
        failure: PublishFailure,
    ) -> Result<bool> {
        let policy = policy.validate()?;
        let blocked =
            failure != PublishFailure::Unavailable || lease.attempt >= policy.max_attempts;
        let error = match failure {
            PublishFailure::Unavailable => "unavailable",
            PublishFailure::Conflict => "immutable_conflict",
            PublishFailure::InvalidReceipt => "invalid_receipt",
        };
        let backoff = policy
            .backoff_seconds
            .saturating_mul(1u32 << lease.attempt.saturating_sub(1).min(10))
            .min(3600);
        let mut tx = self.pool.begin().await?;
        self.lock(&mut tx, lease.fence).await?;
        let count = sqlx::query("UPDATE federation_publish_outbox SET blocked=$4,last_error=$5,lease_token=NULL,lease_until=NULL,available_at=clock_timestamp()+make_interval(secs => $6) WHERE space=$1 AND sequence=$2 AND lease_token=$3 AND lease_until>clock_timestamp() AND NOT published")
            .bind(lease.fence.space.as_uuid()).bind(i64::from(lease.delivery.sequence.get())).bind(lease.token)
            .bind(blocked).bind(error).bind(f64::from(backoff)).execute(&mut *tx).await?.rows_affected();
        if count != 1 {
            return Err(StoreError::LostLease);
        }
        tx.commit().await?;
        Ok(blocked)
    }

    /// Explicit operator retry of a blocked head. Never silently discard or skip.
    pub async fn retry_blocked(&self, fence: WriterFence) -> Result<()> {
        let mut tx = self.pool.begin().await?;
        self.lock(&mut tx, fence).await?;
        sqlx::query("UPDATE federation_publish_outbox SET blocked=FALSE,attempts=0,available_at=clock_timestamp(),last_error=NULL WHERE space=$1 AND blocked AND NOT published")
            .bind(fence.space.as_uuid()).execute(&mut *tx).await?;
        tx.commit().await?;
        Ok(())
    }

    /// One scheduling step, without holding a transaction across network I/O.
    pub async fn publish_one(
        &self,
        fence: WriterFence,
        policy: RetryPolicy,
        transport: &impl ImmutablePublisher,
    ) -> Result<WorkerOutcome> {
        let lease = match self.claim_publication(fence, policy).await? {
            Claim::Ready(lease) => lease,
            Claim::Idle => return Ok(WorkerOutcome::Idle),
            Claim::Waiting => return Ok(WorkerOutcome::Waiting),
            Claim::Blocked => return Ok(WorkerOutcome::Blocked),
        };
        match transport.publish(lease.delivery()).await {
            Ok(()) => {
                self.acknowledge_publication(&lease).await?;
                Ok(WorkerOutcome::Published(lease.delivery.sequence.get()))
            }
            Err(failure) => {
                if self.fail_publication(&lease, policy, failure).await? {
                    Ok(WorkerOutcome::Blocked)
                } else {
                    Ok(WorkerOutcome::Retrying)
                }
            }
        }
    }
}
