// SPDX-License-Identifier: AGPL-3.0-or-later

//! Ownership for foreground work which must not be automatically re-executed.
//! Domain phases and terminal journals use short, separately fenced transactions.
//! Session ownership requires a direct/session-pooled PostgreSQL connection,
//! not a transaction-pooling proxy. Dropping the owner closes its connection.
use crate::{Error, Result};
use sqlx::{Connection, PgConnection, PgPool, Postgres, Row, Transaction};
use uuid::Uuid;

#[derive(Clone, Copy)]
pub enum Kind {
    Loop,
    Validation,
}
impl Kind {
    fn label(self) -> &'static str {
        match self {
            Self::Loop => "loop",
            Self::Validation => "validation",
        }
    }
}

pub struct Execution {
    // Never return a session lock to the pool, including on cancellation.
    _connection: PgConnection,
    id: Uuid,
    pid: i32,
    key: i64,
}
impl Execution {
    /// Acquire before registering the domain run. UUIDs must be fresh, never a
    /// mechanism to restart a completed/interrupted operation.
    pub async fn acquire(pool: &PgPool, id: Uuid) -> Result<Self> {
        if id.is_nil() {
            return Err(Error::Invalid("execution identity"));
        }
        // Do not reserve a pool slot while the caller needs another for its
        // phase transactions: concurrent owners could otherwise exhaust it.
        let mut connection = PgConnection::connect_with(&pool.connect_options()).await?;
        let (pid, key): (i32, i64) =
            sqlx::query_as("SELECT pg_backend_pid(),hashtextextended($1,0)")
                .bind(format!("foreground:{id}"))
                .fetch_one(&mut connection)
                .await?;
        let acquired: bool = sqlx::query_scalar("SELECT pg_try_advisory_lock($1)")
            .bind(key)
            .fetch_one(&mut connection)
            .await?;
        if !acquired {
            return Err(Error::Conflict);
        }
        Ok(Self {
            _connection: connection,
            id,
            pid,
            key,
        })
    }

    /// Register in the SAME transaction as the domain run and start journal.
    pub async fn register(
        &self,
        tx: &mut Transaction<'_, Postgres>,
        kind: Kind,
        project: Option<Uuid>,
    ) -> Result<()> {
        sqlx::query("INSERT INTO foreground_executions(id,kind,project_id,backend_pid,lock_key) VALUES($1,$2,$3,$4,$5)")
            .bind(self.id).bind(kind.label()).bind(project).bind(self.pid).bind(self.key).execute(&mut **tx).await?;
        fence(tx, self.id).await
    }
}

/// Lock execution BEFORE run/canonical/proposal rows. Recovery cannot overtake
/// a phase after its ownership check. No provider calls while holding this.
pub async fn fence(tx: &mut Transaction<'_, Postgres>, id: Uuid) -> Result<()> {
    let row = sqlx::query(
        "SELECT backend_pid,lock_key,status FROM foreground_executions WHERE id=$1 FOR UPDATE",
    )
    .bind(id)
    .fetch_one(&mut **tx)
    .await?;
    if row.try_get::<String, _>("status")? != "running" {
        return Err(Error::StaleLease);
    }
    let alive:bool = sqlx::query_scalar("SELECT EXISTS(SELECT 1 FROM pg_locks WHERE locktype='advisory' AND pid=$1 AND database=(SELECT oid FROM pg_database WHERE datname=current_database()) AND classid::bigint=(($2::bigint >> 32) & 4294967295) AND objid::bigint=($2::bigint & 4294967295) AND objsubid=1 AND mode='ExclusiveLock' AND granted)")
        .bind(row.try_get::<i32,_>("backend_pid")?).bind(row.try_get::<i64,_>("lock_key")?).fetch_one(&mut **tx).await?;
    if !alive {
        return Err(Error::StaleLease);
    }
    Ok(())
}

/// Call after domain writes/journals, before the operation's transaction commit.
pub async fn finish_tx(tx: &mut Transaction<'_, Postgres>, id: Uuid) -> Result<()> {
    fence(tx, id).await?;
    sqlx::query("UPDATE foreground_executions SET status='finished',finished_at=clock_timestamp() WHERE id=$1")
        .bind(id).execute(&mut **tx).await?;
    Ok(())
}

pub async fn pending(pool: &PgPool, kind: Kind, limit: u32) -> Result<Vec<Uuid>> {
    if !(1..=100).contains(&limit) {
        return Err(Error::Invalid("execution scan limit"));
    }
    // Filter live owners before the limit so long-running work cannot starve
    // interrupted operations behind it in the recovery scan.
    Ok(sqlx::query_scalar("SELECT e.id FROM foreground_executions e WHERE e.status='running' AND e.kind=$1 AND NOT EXISTS(SELECT 1 FROM pg_locks l WHERE l.locktype='advisory' AND l.pid=e.backend_pid AND l.database=(SELECT oid FROM pg_database WHERE datname=current_database()) AND l.classid::bigint=((e.lock_key >> 32) & 4294967295) AND l.objid::bigint=(e.lock_key & 4294967295) AND l.objsubid=1 AND l.mode='ExclusiveLock' AND l.granted) ORDER BY e.created_at,e.id LIMIT $2")
        .bind(kind.label()).bind(i64::from(limit)).fetch_all(pool).await?)
}

pub struct Interrupted<'a> {
    tx: Transaction<'a, Postgres>,
    id: Uuid,
    pub project_id: Option<Uuid>,
}
impl<'a> Interrupted<'a> {
    pub fn transaction(&mut self) -> &mut Transaction<'a, Postgres> {
        &mut self.tx
    }
    /// Domain failure state, audit/event and this receipt commit together.
    pub async fn commit(mut self) -> Result<()> {
        sqlx::query("UPDATE foreground_executions SET status='interrupted',finished_at=clock_timestamp() WHERE id=$1")
            .bind(self.id).execute(&mut *self.tx).await?;
        self.tx.commit().await?;
        Ok(())
    }
}

/// Returns None for a live owner, a locked phase or a terminal operation.
/// Holding the acquired lock through commit prevents a new same-ID owner.
pub async fn recover<'a>(pool: &'a PgPool, id: Uuid) -> Result<Option<Interrupted<'a>>> {
    let mut tx = pool.begin().await?;
    let row = sqlx::query("SELECT lock_key,project_id FROM foreground_executions WHERE id=$1 AND status='running' FOR UPDATE SKIP LOCKED")
        .bind(id).fetch_optional(&mut *tx).await?;
    let Some(row) = row else {
        return Ok(None);
    };
    let free: bool = sqlx::query_scalar("SELECT pg_try_advisory_xact_lock($1)")
        .bind(row.try_get::<i64, _>("lock_key")?)
        .fetch_one(&mut *tx)
        .await?;
    if !free {
        return Ok(None);
    }
    Ok(Some(Interrupted {
        tx,
        id,
        project_id: row.try_get("project_id")?,
    }))
}
