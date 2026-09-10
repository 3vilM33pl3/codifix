// SPDX-License-Identifier: AGPL-3.0-or-later

//! At-least-once scheduling with fenced transactional outputs. Network/provider
//! calls MUST happen before `begin_result`, never while a database lock is held.

use crate::{Error, Result};
use mem_record::{
    JobAttempt, JobDetail, JobFailureCode, JobKind, JobList, JobRetryResponse, JobStatus,
    JobSummary,
};
use serde::{Deserialize, Serialize};
use serde_json::Value;
use sqlx::{PgPool, Postgres, Row, Transaction, postgres::PgRow};
use uuid::Uuid;

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(tag = "kind", content = "id", rename_all = "snake_case")]
pub enum JobActor {
    Principal(Uuid),
    /// Credential IDs are non-secret, and allow revocation to fence queued work.
    Credential {
        principal: Uuid,
        token: Option<Uuid>,
        session: Option<Uuid>,
    },
    System(String),
}

impl JobActor {
    fn validate(&self) -> Result<()> {
        match self {
            Self::Credential {
                principal,
                token,
                session,
            } if !principal.is_nil()
                && !(token.is_some() && session.is_some())
                && token.is_none_or(|id| !id.is_nil())
                && session.is_none_or(|id| !id.is_nil()) =>
            {
                Ok(())
            }
            Self::Principal(id) if !id.is_nil() => Ok(()),
            Self::System(name)
                if !name.is_empty()
                    && name.len() <= 80
                    && name.bytes().all(|b| b.is_ascii_alphanumeric() || b == b'-') =>
            {
                Ok(())
            }
            _ => Err(Error::Invalid("job actor")),
        }
    }
}

pub struct NewJob {
    pub id: Uuid,
    pub project_id: Uuid,
    pub operation_id: Uuid,
    pub kind: JobKind,
    pub work_key: String,
    pub actor: JobActor,
    /// Versioned, bounded worker contract containing only required inputs.
    pub input: Value,
    pub dependencies: Vec<Uuid>,
}

#[derive(Debug, Clone, Copy)]
pub struct JobPolicy {
    pub concurrent_projects: u32,
    pub lease_seconds: u32,
    pub max_attempts: u32,
    pub initial_delay_seconds: u32,
    pub max_delay_seconds: u32,
}

impl Default for JobPolicy {
    fn default() -> Self {
        Self {
            concurrent_projects: 2,
            lease_seconds: 120,
            max_attempts: 5,
            initial_delay_seconds: 10,
            max_delay_seconds: 600,
        }
    }
}

impl JobPolicy {
    pub fn validate(&self) -> Result<()> {
        if !(1..=16).contains(&self.concurrent_projects)
            || !(2..=3600).contains(&self.lease_seconds)
            || !(1..=20).contains(&self.max_attempts)
            || self.initial_delay_seconds == 0
            || self.initial_delay_seconds > self.max_delay_seconds
            || self.max_delay_seconds > 86400
        {
            return Err(Error::Invalid("job retry policy"));
        }
        Ok(())
    }

    pub fn delay(&self, attempt: u32) -> u32 {
        self.initial_delay_seconds
            .saturating_mul(1_u32 << attempt.saturating_sub(1).min(30))
            .min(self.max_delay_seconds)
    }
}

#[derive(Debug, Clone)]
pub struct JobLease {
    pub job: JobSummary,
    pub token: Uuid,
    pub actor: JobActor,
    pub input: Value,
}

#[derive(Debug, Clone, Copy)]
pub enum Completion {
    Succeeded,
    Skipped,
    Failure(JobFailureCode),
}

fn label<T: Serialize>(value: T) -> Result<String> {
    serde_json::to_value(value)?
        .as_str()
        .map(str::to_owned)
        .ok_or(Error::Invalid("enum label"))
}

fn decode<T: serde::de::DeserializeOwned>(row: &PgRow, column: &str) -> Result<T> {
    Ok(serde_json::from_value(Value::String(row.try_get(column)?))?)
}

fn summary(row: &PgRow) -> Result<JobSummary> {
    Ok(JobSummary {
        reason: row
            .try_get::<Option<String>, _>("reason")?
            .map(|s| serde_json::from_value(Value::String(s)))
            .transpose()?,
        id: row.try_get("id")?,
        project_id: row.try_get("project_id")?,
        operation_id: row.try_get("operation_id")?,
        kind: decode(row, "kind")?,
        status: decode(row, "status")?,
        generation: row.try_get("generation")?,
        attempts: row.try_get("attempts")?,
        created_at: row.try_get("created_at")?,
        available_at: row.try_get("available_at")?,
        lease_until: row.try_get("lease_until")?,
        last_failure: row
            .try_get::<Option<String>, _>("last_failure")?
            .map(|s| serde_json::from_value(Value::String(s)))
            .transpose()?,
    })
}

/// Dependencies must already exist in this project. New edges only point from
/// a newly inserted job, so the API cannot introduce cycles or mutate a recipe.
pub async fn enqueue(tx: &mut Transaction<'_, Postgres>, job: &NewJob) -> Result<Uuid> {
    job.actor.validate()?;
    if job.id.is_nil()
        || job.project_id.is_nil()
        || job.operation_id.is_nil()
        || job.work_key.is_empty()
        || job.work_key.len() > 256
        || job.dependencies.len() > 64
        || serde_json::to_vec(&job.input)?.len() > 16_384
    {
        return Err(Error::Invalid("job identity, work key or input"));
    }
    let kind = label(job.kind)?;
    let actor = serde_json::to_value(&job.actor)?;
    sqlx::query(
        "INSERT INTO background_project_slots(project_id) VALUES($1) ON CONFLICT DO NOTHING",
    )
    .bind(job.project_id)
    .execute(&mut **tx)
    .await?;
    let inserted: Option<Uuid> = sqlx::query_scalar("INSERT INTO background_jobs(id,project_id,operation_id,kind,work_key,actor,input) VALUES($1,$2,$3,$4,$5,$6,$7) ON CONFLICT(project_id,work_key) DO NOTHING RETURNING id")
        .bind(job.id).bind(job.project_id).bind(job.operation_id).bind(&kind).bind(&job.work_key).bind(&actor).bind(&job.input).fetch_optional(&mut **tx).await?;
    let mut deps = job.dependencies.clone();
    deps.sort_unstable();
    deps.dedup();
    if let Some(id) = inserted {
        for dep in deps {
            sqlx::query("INSERT INTO background_job_dependencies(project_id,job_id,depends_on) VALUES($1,$2,$3)")
                .bind(job.project_id).bind(id).bind(dep).execute(&mut **tx).await?;
        }
        return Ok(id);
    }
    let existing = sqlx::query("SELECT * FROM background_jobs WHERE project_id=$1 AND work_key=$2")
        .bind(job.project_id)
        .bind(&job.work_key)
        .fetch_one(&mut **tx)
        .await?;
    let id: Uuid = existing.try_get("id")?;
    let existing_deps: Vec<Uuid> = sqlx::query_scalar(
        "SELECT depends_on FROM background_job_dependencies WHERE job_id=$1 ORDER BY depends_on",
    )
    .bind(id)
    .fetch_all(&mut **tx)
    .await?;
    if existing.try_get::<Uuid, _>("operation_id")? != job.operation_id
        || existing.try_get::<String, _>("kind")? != kind
        || existing.try_get::<Value, _>("actor")? != actor
        || existing.try_get::<Value, _>("input")? != job.input
        || existing_deps != deps
    {
        return Err(Error::Conflict);
    }
    Ok(id)
}

/// Claims take admission -> project slot -> job -> attempts/receipts -> events.
/// Other writes start at the project slot and never acquire admission. Across
/// processes, live projects share the configured cap and each has one stage.
pub async fn claim(pool: &PgPool, policy: JobPolicy) -> Result<Option<JobLease>> {
    policy.validate()?;
    let mut tx = pool.begin().await?;
    sqlx::query("SELECT pg_advisory_xact_lock(hashtextextended('background-admission',0))")
        .execute(&mut *tx)
        .await?;
    let active: i64 = sqlx::query_scalar(
        "SELECT count(*) FROM background_project_slots WHERE lease_until>clock_timestamp()",
    )
    .fetch_one(&mut *tx)
    .await?;
    if active >= i64::from(policy.concurrent_projects) {
        return Ok(None);
    }
    let project: Option<Uuid> = sqlx::query_scalar(
        "SELECT s.project_id FROM background_project_slots s
         WHERE (s.lease_until IS NULL OR s.lease_until<=clock_timestamp()) AND EXISTS (
           SELECT 1 FROM background_jobs j WHERE j.project_id=s.project_id
             AND ((j.status IN ('queued','retry_wait') AND j.available_at<=clock_timestamp())
               OR (j.status='running' AND j.lease_until<=clock_timestamp()))
             AND NOT EXISTS (SELECT 1 FROM background_job_dependencies d JOIN background_jobs p ON p.id=d.depends_on WHERE d.job_id=j.id AND p.status NOT IN ('succeeded','skipped')))
         ORDER BY (SELECT min(available_at) FROM background_jobs j WHERE j.project_id=s.project_id AND j.status IN ('queued','retry_wait','running')),s.project_id
         LIMIT 1 FOR UPDATE OF s SKIP LOCKED")
        .fetch_optional(&mut *tx).await?;
    let Some(project) = project else {
        return Ok(None);
    };
    let row = sqlx::query(
        "SELECT j.* FROM background_jobs j WHERE project_id=$1
         AND ((status IN ('queued','retry_wait') AND available_at<=clock_timestamp()) OR (status='running' AND lease_until<=clock_timestamp()))
         AND NOT EXISTS (SELECT 1 FROM background_job_dependencies d JOIN background_jobs p ON p.id=d.depends_on WHERE d.job_id=j.id AND p.status NOT IN ('succeeded','skipped'))
         ORDER BY created_at,id LIMIT 1 FOR UPDATE OF j")
        .bind(project).fetch_one(&mut *tx).await?;
    let old = summary(&row)?;
    if old.status == JobStatus::Running {
        sqlx::query("UPDATE background_job_attempts SET finished_at=clock_timestamp(),outcome='lease_lost',failure='lease_lost',uncertain=TRUE WHERE job_id=$1 AND finished_at IS NULL")
            .bind(old.id).execute(&mut *tx).await?;
    }
    if old.attempts >= policy.max_attempts as i32 {
        sqlx::query("UPDATE background_jobs SET status='failed',last_failure='attempts_exhausted',lease_token=NULL,lease_until=NULL WHERE id=$1")
            .bind(old.id).execute(&mut *tx).await?;
        clear_slot(&mut tx, project).await?;
        tx.commit().await?;
        return Ok(None);
    }
    let token = Uuid::new_v4();
    let row = sqlx::query("UPDATE background_jobs SET status='running',attempts=attempts+1,lease_token=$2,lease_until=clock_timestamp()+make_interval(secs=>$3) WHERE id=$1 RETURNING *")
        .bind(old.id).bind(token).bind(f64::from(policy.lease_seconds)).fetch_one(&mut *tx).await?;
    let job = summary(&row)?;
    sqlx::query("UPDATE background_project_slots SET job_id=$2,lease_token=$3,lease_until=$4 WHERE project_id=$1")
        .bind(project).bind(job.id).bind(token).bind(job.lease_until).execute(&mut *tx).await?;
    sqlx::query(
        "INSERT INTO background_job_attempts(job_id,generation,attempt,token) VALUES($1,$2,$3,$4)",
    )
    .bind(job.id)
    .bind(job.generation)
    .bind(job.attempts)
    .bind(token)
    .execute(&mut *tx)
    .await?;
    let actor = serde_json::from_value(row.try_get("actor")?)?;
    let input = row.try_get("input")?;
    tx.commit().await?;
    Ok(Some(JobLease {
        job,
        token,
        actor,
        input,
    }))
}

async fn clear_slot(tx: &mut Transaction<'_, Postgres>, project: Uuid) -> Result<()> {
    sqlx::query("UPDATE background_project_slots SET job_id=NULL,lease_token=NULL,lease_until=NULL WHERE project_id=$1")
        .bind(project).execute(&mut **tx).await?;
    Ok(())
}

/// Hold these locks through result writes and `finish_result`. Never write
/// derived results through a pool/another transaction after acquiring this.
pub struct JobResult<'a> {
    tx: Transaction<'a, Postgres>,
    lease: JobLease,
}

impl<'a> JobResult<'a> {
    /// Borrow the fenced transaction; callers cannot consume/commit it without
    /// passing the final lease check in `finish_result`.
    pub fn transaction(&mut self) -> &mut Transaction<'a, Postgres> {
        &mut self.tx
    }
}

pub async fn begin_result<'a>(pool: &'a PgPool, lease: &JobLease) -> Result<JobResult<'a>> {
    let mut tx = pool.begin().await?;
    let slot: Option<Uuid> = sqlx::query_scalar("SELECT job_id FROM background_project_slots WHERE project_id=$1 AND job_id=$2 AND lease_token=$3 AND lease_until>clock_timestamp() FOR UPDATE")
        .bind(lease.job.project_id).bind(lease.job.id).bind(lease.token).fetch_optional(&mut *tx).await?;
    if slot.is_none() {
        return Err(Error::StaleLease);
    }
    let current: Option<Uuid> = sqlx::query_scalar("SELECT id FROM background_jobs WHERE id=$1 AND status='running' AND lease_token=$2 AND lease_until>clock_timestamp() FOR UPDATE")
        .bind(lease.job.id).bind(lease.token).fetch_optional(&mut *tx).await?;
    if current.is_none() {
        return Err(Error::StaleLease);
    }
    Ok(JobResult {
        tx,
        lease: lease.clone(),
    })
}

pub async fn renew(pool: &PgPool, lease: &JobLease, policy: JobPolicy) -> Result<()> {
    policy.validate()?;
    let JobResult { mut tx, .. } = begin_result(pool, lease).await?;
    let renewed=sqlx::query("UPDATE background_jobs SET lease_until=clock_timestamp()+make_interval(secs=>$2) WHERE id=$1 AND lease_token=$3 AND lease_until>clock_timestamp()")
        .bind(lease.job.id).bind(f64::from(policy.lease_seconds)).bind(lease.token).execute(&mut *tx).await?.rows_affected();
    if renewed != 1 {
        return Err(Error::StaleLease);
    }
    sqlx::query("UPDATE background_project_slots SET lease_until=(SELECT lease_until FROM background_jobs WHERE id=$2) WHERE project_id=$1")
        .bind(lease.job.project_id).bind(lease.job.id).execute(&mut *tx).await?;
    tx.commit().await?;
    Ok(())
}

/// Atomically commit fenced results and outcome; failed checks DROP/ROLL BACK
/// the supplied transaction, including any derived writes already performed.
pub async fn finish_result(
    result: JobResult<'_>,
    completion: Completion,
    policy: JobPolicy,
) -> Result<()> {
    let JobResult { mut tx, lease } = result;
    policy.validate()?;
    let (status, failure) = match completion {
        Completion::Succeeded => ("succeeded", None),
        Completion::Skipped => ("skipped", None),
        Completion::Failure(code) => {
            let status = match code {
                JobFailureCode::Transient | JobFailureCode::RateLimited
                    if lease.job.attempts < policy.max_attempts as i32 =>
                {
                    "retry_wait"
                }
                JobFailureCode::Transient
                | JobFailureCode::RateLimited
                | JobFailureCode::AttemptsExhausted => "failed",
                _ => "blocked",
            };
            (status, Some(label(code)?))
        }
    };
    let affected = sqlx::query("UPDATE background_jobs SET status=$3,last_failure=$4,available_at=clock_timestamp()+make_interval(secs=>$5),lease_token=NULL,lease_until=NULL WHERE id=$1 AND lease_token=$2 AND status='running' AND lease_until>clock_timestamp()")
        .bind(lease.job.id).bind(lease.token).bind(status).bind(&failure).bind(f64::from(policy.delay(lease.job.attempts as u32)))
        .execute(&mut *tx).await?.rows_affected();
    if affected != 1 {
        return Err(Error::StaleLease);
    }
    sqlx::query("UPDATE background_job_attempts SET finished_at=clock_timestamp(),outcome=$2,failure=$3 WHERE token=$1 AND finished_at IS NULL")
        .bind(lease.token).bind(status).bind(failure).execute(&mut *tx).await?;
    clear_slot(&mut tx, lease.job.project_id).await?;
    tx.commit().await?;
    Ok(())
}

/// Persist reusable successful output without acknowledging the stage. The
/// lease must still be live at commit, just as for the final result transaction.
pub async fn checkpoint_result(result: JobResult<'_>) -> Result<()> {
    let JobResult { mut tx, lease } = result;
    let alive: bool = sqlx::query_scalar("SELECT EXISTS(SELECT 1 FROM background_jobs WHERE id=$1 AND lease_token=$2 AND status='running' AND lease_until>clock_timestamp())")
        .bind(lease.job.id).bind(lease.token).fetch_one(&mut *tx).await?;
    if !alive {
        return Err(Error::StaleLease);
    }
    tx.commit().await?;
    Ok(())
}

/// Must be called only after current authorization/policy checks. Receipt binds
/// the retry actor as well as the target; retries cannot lend another identity.
pub async fn retry(
    pool: &PgPool,
    project: Uuid,
    id: Uuid,
    request: Uuid,
    actor: &JobActor,
) -> Result<JobRetryResponse> {
    let mut tx = pool.begin().await?;
    let response = retry_tx(&mut tx, project, id, request, actor).await?;
    tx.commit().await?;
    Ok(response)
}

/// Caller holds authorization locks through retry commit.
pub async fn retry_tx(
    tx: &mut Transaction<'_, Postgres>,
    project: Uuid,
    id: Uuid,
    request: Uuid,
    actor: &JobActor,
) -> Result<JobRetryResponse> {
    actor.validate()?;
    if request.is_nil() {
        return Err(Error::Invalid("retry request id"));
    }
    sqlx::query("SELECT project_id FROM background_project_slots WHERE project_id=$1 FOR UPDATE")
        .bind(project)
        .fetch_one(&mut **tx)
        .await?;
    let actor_value = serde_json::to_value(actor)?;
    let receipt = sqlx::query(
        "SELECT * FROM background_job_retry_receipts WHERE project_id=$1 AND request_id=$2",
    )
    .bind(project)
    .bind(request)
    .fetch_optional(&mut **tx)
    .await?;
    if let Some(receipt) = receipt {
        if receipt.try_get::<Uuid, _>("job_id")? != id
            || receipt.try_get::<Value, _>("actor")? != actor_value
        {
            return Err(Error::Conflict);
        }
        return Ok(JobRetryResponse {
            request_id: request,
            job_id: id,
            generation: receipt.try_get("generation")?,
        });
    }
    let generation: Option<i32> = sqlx::query_scalar("UPDATE background_jobs SET status='queued',generation=generation+1,attempts=0,last_failure=NULL,reason=NULL,available_at=clock_timestamp() WHERE project_id=$1 AND id=$2 AND status IN ('failed','blocked') RETURNING generation")
        .bind(project).bind(id).fetch_optional(&mut **tx).await?;
    let Some(generation) = generation else {
        return Err(Error::Conflict);
    };
    sqlx::query("INSERT INTO background_job_retry_receipts(project_id,request_id,job_id,generation,actor) VALUES($1,$2,$3,$4,$5)")
        .bind(project).bind(request).bind(id).bind(generation).bind(actor_value).execute(&mut **tx).await?;
    Ok(JobRetryResponse {
        request_id: request,
        job_id: id,
        generation,
    })
}

pub async fn list(
    pool: &PgPool,
    project: Uuid,
    after: Option<Uuid>,
    status: Option<JobStatus>,
    limit: u32,
) -> Result<JobList> {
    if !(1..=100).contains(&limit) {
        return Err(Error::Invalid("job page limit"));
    }
    let rows = sqlx::query("SELECT * FROM background_jobs WHERE project_id=$1 AND ($2::uuid IS NULL OR id>$2) AND ($3::text IS NULL OR status=$3) ORDER BY id LIMIT $4")
        .bind(project).bind(after).bind(status.map(label).transpose()?).bind(i64::from(limit)+1).fetch_all(pool).await?;
    let more = rows.len() > limit as usize;
    let jobs = rows
        .iter()
        .take(limit as usize)
        .map(summary)
        .collect::<Result<Vec<_>>>()?;
    let next_after = if more {
        jobs.last().map(|j| j.id)
    } else {
        None
    };
    Ok(JobList { jobs, next_after })
}

pub async fn detail(pool: &PgPool, project: Uuid, id: Uuid) -> Result<JobDetail> {
    let mut tx = pool.begin().await?;
    sqlx::query("SET TRANSACTION ISOLATION LEVEL REPEATABLE READ READ ONLY")
        .execute(&mut *tx)
        .await?;
    let row = sqlx::query("SELECT * FROM background_jobs WHERE project_id=$1 AND id=$2")
        .bind(project)
        .bind(id)
        .fetch_one(&mut *tx)
        .await?;
    let job = summary(&row)?;
    let dependencies = sqlx::query_scalar(
        "SELECT depends_on FROM background_job_dependencies WHERE job_id=$1 ORDER BY depends_on",
    )
    .bind(id)
    .fetch_all(&mut *tx)
    .await?;
    let stage_rows=sqlx::query("SELECT * FROM background_jobs WHERE project_id=$1 AND operation_id=$2 ORDER BY created_at,id LIMIT 201")
        .bind(project).bind(job.operation_id).fetch_all(&mut *tx).await?;
    let stages_truncated = stage_rows.len() > 200;
    let stages = stage_rows
        .iter()
        .take(200)
        .map(summary)
        .collect::<Result<Vec<_>>>()?;
    let rows = sqlx::query("SELECT * FROM background_job_attempts WHERE job_id=$1 ORDER BY generation DESC,attempt DESC LIMIT 201")
        .bind(id).fetch_all(&mut *tx).await?;
    let history_truncated = rows.len() > 200;
    let history = rows
        .into_iter()
        .take(200)
        .map(|r| {
            Ok(JobAttempt {
                generation: r.try_get("generation")?,
                attempt: r.try_get("attempt")?,
                started_at: r.try_get("started_at")?,
                finished_at: r.try_get("finished_at")?,
                outcome: r.try_get("outcome")?,
                failure: r
                    .try_get::<Option<String>, _>("failure")?
                    .map(|s| serde_json::from_value(Value::String(s)))
                    .transpose()?,
                uncertain: r.try_get("uncertain")?,
            })
        })
        .collect::<Result<Vec<_>>>()?;
    tx.commit().await?;
    Ok(JobDetail {
        job,
        stages,
        stages_truncated,
        dependencies,
        history,
        history_truncated,
    })
}
