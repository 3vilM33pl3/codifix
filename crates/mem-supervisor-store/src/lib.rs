// SPDX-License-Identifier: AGPL-3.0-or-later
//! Trusted in-process PostgreSQL persistence, not service authentication or an
//! executor. All clocks are read from PostgreSQL after acquiring relevant locks.
mod actions;
mod attempts;
mod contexts;
mod health;
mod inspection;
mod policy;
mod proposals;
pub mod questions;
pub mod sources;

use chrono::{DateTime, NaiveDate, Utc};
use mem_supervisor::{
    Area, StableId, UsdMicros, action::ActionState, budget::BudgetError, policy::PolicyError,
};
use serde::Serialize;
use sqlx::{PgConnection, PgPool, Row, postgres::PgRow};
use uuid::Uuid;

pub use actions::{ActionSubmission, ReadyTransition};
pub use attempts::{AttemptRecord, Reservation, SendPermit};
pub use contexts::{InstanceBinding, ModelWorkContext};
pub use policy::PolicyRecord;
pub use proposals::PrivateProposal;

pub type Result<T> = std::result::Result<T, StoreError>;
pub type EvidenceDigest = [u8; 32];

#[derive(Debug, thiserror::Error)]
pub enum StoreError {
    #[error("supervisor database operation failed")]
    Database(#[from] sqlx::Error),
    #[error(transparent)]
    Policy(#[from] PolicyError),
    #[error(transparent)]
    Budget(#[from] BudgetError),
    #[error("IDs must be non-nil UUIDs; leases and bounded limits must be valid")]
    InvalidInput,
    #[error("supervisor record does not exist")]
    NotFound,
    #[error("stable identity already has different immutable terms")]
    Conflict,
    #[error("action or attempt cannot make this transition")]
    Transition,
    #[error("action lease expired, was revoked, or has a stale fence")]
    Fenced,
    #[error("unsettled provider attempts require reconciliation")]
    Unsettled,
    #[error("stored supervisor contract is inconsistent")]
    Corrupt,
    #[error("private proposal storage operation failed")]
    PrivateStorage,
    #[error("private proposal exceeds allowed content or evidence bounds")]
    InvalidProposal,
    #[error("verified model work requires its matching durable private proposal")]
    UnpersistedProposal,
    #[error("missing or mismatched durable instance or model-work binding")]
    Binding,
    #[error("supervisor question queue is full; wait for pending questions to finish or expire")]
    QueueFull,
}

#[derive(Clone)]
pub struct SupervisorStore {
    pool: PgPool,
}

impl SupervisorStore {
    /// Migrations must be applied separately by the authorized migration path.
    pub fn new(pool: PgPool) -> Self {
        Self { pool }
    }

    /// Common deterministic-work preflight only. Model disclosures, including
    /// token counting, require check_model_work instead. Neither is user consent.
    pub async fn check_current(&self, claim: &LeaseClaim) -> Result<DateTime<Utc>> {
        let mut tx = self.pool.begin().await?;
        let policy = policy::lock_policy(&mut tx, claim.area).await?;
        let row = lock_action(&mut tx, claim.area, claim.action_id).await?;
        policy.validate(row.try_get("policy_revision")?)?;
        check_lease(&mut tx, &row, claim).await?;
        let now = db_now(&mut tx).await?;
        if now >= claim.expires_at {
            return Err(StoreError::Fenced);
        }
        tx.commit().await?;
        Ok(now)
    }

    pub async fn action(&self, area: Area, id: Uuid) -> Result<ActionRecord> {
        let row = sqlx::query("SELECT * FROM supervisor_actions WHERE area=$1 AND id=$2")
            .bind(area_name(area))
            .bind(id)
            .fetch_optional(&self.pool)
            .await?
            .ok_or(StoreError::NotFound)?;
        action_record(&row)
    }

    /// Bounded, metadata-only ordered journal; no provider/knowledge bodies.
    pub async fn journal(&self, area: Area, after: i64, limit: u32) -> Result<Vec<JournalEntry>> {
        if !(1..=500).contains(&limit) || after < 0 {
            return Err(StoreError::InvalidInput);
        }
        let rows = sqlx::query("SELECT sequence,action_id,attempt_id,policy_revision,event,evidence_digest,created_at FROM supervisor_journal WHERE area=$1 AND sequence>$2 ORDER BY sequence LIMIT $3")
            .bind(area_name(area)).bind(after).bind(i64::from(limit)).fetch_all(&self.pool).await?;
        rows.into_iter()
            .map(|r| {
                Ok(JournalEntry {
                    sequence: r.try_get("sequence")?,
                    action_id: r.try_get("action_id")?,
                    attempt_id: r.try_get("attempt_id")?,
                    policy_revision: r.try_get("policy_revision")?,
                    event: r.try_get("event")?,
                    evidence_digest: r.try_get("evidence_digest")?,
                    created_at: r.try_get("created_at")?,
                })
            })
            .collect()
    }
}

#[derive(Debug, Clone, Serialize)]
pub struct ActionRecord {
    pub id: Uuid,
    pub area: Area,
    pub source_event: Uuid,
    pub kind: mem_supervisor::action::ActionKind,
    pub policy_revision: Uuid,
    pub state: ActionState,
    pub fence: i64,
    pub lease_until: Option<DateTime<Utc>>,
}

/// Not deserializable or constructible outside this crate. Every use rechecks
/// persisted authority; cloning a claim grants no additional execution rights.
#[derive(Debug, Clone)]
pub struct LeaseClaim {
    area: Area,
    action_id: Uuid,
    worker: Uuid,
    fence: i64,
    expires_at: DateTime<Utc>,
}
impl LeaseClaim {
    pub fn area(&self) -> Area {
        self.area
    }
    pub fn action_id(&self) -> Uuid {
        self.action_id
    }
    pub fn fence(&self) -> i64 {
        self.fence
    }
    pub fn expires_at(&self) -> DateTime<Utc> {
        self.expires_at
    }
}

#[derive(Debug, Clone, Serialize)]
pub struct JournalEntry {
    pub sequence: i64,
    pub action_id: Option<Uuid>,
    pub attempt_id: Option<Uuid>,
    pub policy_revision: Uuid,
    pub event: String,
    pub evidence_digest: Option<Vec<u8>>,
    pub created_at: DateTime<Utc>,
}

fn area_name(area: Area) -> &'static str {
    match area {
        Area::Trust => "trust",
        Area::Homelab => "homelab",
        Area::Workshop => "workshop",
        Area::General => "general",
    }
}
fn parse_area(area: &str) -> Result<Area> {
    match area {
        "trust" => Ok(Area::Trust),
        "homelab" => Ok(Area::Homelab),
        "workshop" => Ok(Area::Workshop),
        "general" => Ok(Area::General),
        _ => Err(StoreError::Corrupt),
    }
}
fn opaque(id: &StableId) -> Result<Uuid> {
    let id = Uuid::parse_str(id.as_str()).map_err(|_| StoreError::InvalidInput)?;
    non_nil(id)?;
    Ok(id)
}
fn non_nil(id: Uuid) -> Result<()> {
    if id.is_nil() {
        Err(StoreError::InvalidInput)
    } else {
        Ok(())
    }
}
async fn db_now(conn: &mut PgConnection) -> Result<DateTime<Utc>> {
    Ok(sqlx::query_scalar("SELECT clock_timestamp()")
        .fetch_one(conn)
        .await?)
}
async fn current_day(conn: &mut PgConnection) -> Result<NaiveDate> {
    Ok(
        sqlx::query_scalar("SELECT (clock_timestamp() AT TIME ZONE 'UTC')::date")
            .fetch_one(conn)
            .await?,
    )
}
async fn lock_action(conn: &mut PgConnection, area: Area, id: Uuid) -> Result<PgRow> {
    sqlx::query("SELECT * FROM supervisor_actions WHERE area=$1 AND id=$2 FOR UPDATE")
        .bind(area_name(area))
        .bind(id)
        .fetch_optional(conn)
        .await?
        .ok_or(StoreError::NotFound)
}
async fn check_lease(conn: &mut PgConnection, row: &PgRow, claim: &LeaseClaim) -> Result<()> {
    if row.try_get::<Uuid, _>("id")? != claim.action_id
        || row.try_get::<String, _>("area")? != area_name(claim.area)
        || row.try_get::<String, _>("state")? != "running"
        || row.try_get::<i64, _>("fence")? != claim.fence
        || row.try_get::<Option<Uuid>, _>("worker")? != Some(claim.worker)
        || row.try_get::<Option<DateTime<Utc>>, _>("lease_until")? != Some(claim.expires_at)
        || db_now(conn).await? >= claim.expires_at
    {
        return Err(StoreError::Fenced);
    }
    Ok(())
}
async fn journal(
    conn: &mut PgConnection,
    area: Area,
    revision: Uuid,
    action: Option<Uuid>,
    attempt: Option<Uuid>,
    event: &str,
    evidence: Option<EvidenceDigest>,
) -> Result<()> {
    // Allocate identity only after a per-area exclusive lock. It is retained
    // through commit, so an area cursor cannot pass an uncommitted lower ID.
    sqlx::query("SELECT area FROM supervisor_journal_heads WHERE area=$1 FOR UPDATE")
        .bind(area_name(area))
        .fetch_one(&mut *conn)
        .await?;
    sqlx::query("INSERT INTO supervisor_journal(area,policy_revision,action_id,attempt_id,event,evidence_digest) VALUES($1,$2,$3,$4,$5,$6)")
        .bind(area_name(area)).bind(revision).bind(action).bind(attempt).bind(event).bind(evidence.map(|x| x.to_vec())).execute(conn).await?;
    Ok(())
}
fn action_record(row: &PgRow) -> Result<ActionRecord> {
    Ok(ActionRecord {
        id: row.try_get("id")?,
        area: parse_area(&row.try_get::<String, _>("area")?)?,
        source_event: row.try_get("source_event")?,
        kind: actions::parse_kind(&row.try_get::<String, _>("kind")?)?,
        policy_revision: row.try_get("policy_revision")?,
        state: actions::parse_state(&row.try_get::<String, _>("state")?)?,
        fence: row.try_get("fence")?,
        lease_until: row.try_get("lease_until")?,
    })
}
