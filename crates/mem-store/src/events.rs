// SPDX-License-Identifier: AGPL-3.0-or-later

use chrono::{DateTime, Utc};
use mem_record::{ActivityDetails, ActivityKind, TokenUsage};
use serde::{Deserialize, Serialize};
use sqlx::{Postgres, Row, Transaction};
use uuid::Uuid;

use crate::{Error, Result};

/// Trusted execution attribution, kept separate from client-reported author or
/// writer fields. Domain library wrappers use an explicit internal identity.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct MutationContext {
    pub actor_id: String,
    pub actor_name: String,
    pub source: String,
    #[serde(default)]
    pub occurred_at: Option<DateTime<Utc>>,
}

impl MutationContext {
    pub fn internal(source: &str) -> Self {
        Self {
            actor_id: format!("system:{source}"),
            actor_name: source.into(),
            source: source.into(),
            occurred_at: None,
        }
    }

    pub fn stamp(&self, event: &mut TimelineEvent) {
        event.actor_id = Some(self.actor_id.clone());
        event.actor_name = Some(self.actor_name.clone());
        event.source = Some(self.source.clone());
        if let Some(time) = self.occurred_at {
            event.recorded_at = time;
        }
    }
}

/// Explicit subjects survive deletion; never rediscover identity after commit.
/// Identity is supplied by trusted repository code, not an HTTP body.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct TimelineEvent {
    pub id: Uuid,
    pub project_id: Uuid,
    pub recorded_at: DateTime<Utc>,
    pub kind: ActivityKind,
    pub memory_id: Option<Uuid>,
    pub canonical_id: Option<Uuid>,
    pub version_no: Option<i32>,
    pub summary: String,
    pub details: Option<ActivityDetails>,
    pub actor_id: Option<String>,
    pub actor_name: Option<String>,
    pub source: Option<String>,
    pub operation_id: Option<String>,
    pub duration_ms: Option<u64>,
    pub provider: Option<String>,
    pub model: Option<String>,
    pub token_usage: Option<TokenUsage>,
}

impl TimelineEvent {
    pub fn new(project_id: Uuid, kind: ActivityKind, summary: String) -> Self {
        Self {
            id: Uuid::new_v4(),
            project_id,
            recorded_at: Utc::now(),
            kind,
            memory_id: None,
            canonical_id: None,
            version_no: None,
            summary,
            details: None,
            actor_id: None,
            actor_name: None,
            source: None,
            operation_id: None,
            duration_ms: None,
            provider: None,
            model: None,
            token_usage: None,
        }
    }
}

/// Append at the END of a mutation transaction, after domain locks/writes.
/// The database allocator holds its global position lock until commit. The
/// returned seq is provisional: callers must not publish it before commit.
pub async fn append(tx: &mut Transaction<'_, Postgres>, event: &TimelineEvent) -> Result<i64> {
    if event.id.is_nil() || event.project_id.is_nil() || event.summary.len() > 65_536 {
        return Err(Error::Invalid("event identity or summary"));
    }
    let kind = serde_json::to_value(&event.kind)?;
    let kind = kind.as_str().ok_or(Error::Invalid("activity kind"))?;
    let tokens = event.token_usage.as_ref();
    let seq = sqlx::query_scalar(
        "INSERT INTO project_timeline_events (id,project_id,recorded_at,kind,
         memory_id,canonical_id,version_no,summary,details_json,actor_id,actor_name,
         source,operation_id,duration_ms,provider,model,input_tokens,output_tokens,
         cache_read_tokens,cache_write_tokens,total_tokens)
         VALUES ($1,$2,$3,$4,$5,$6,$7,$8,$9,$10,$11,$12,$13,$14,$15,$16,$17,$18,$19,$20,$21)
         RETURNING seq",
    )
    .bind(event.id)
    .bind(event.project_id)
    .bind(event.recorded_at)
    .bind(kind)
    .bind(event.memory_id)
    .bind(event.canonical_id)
    .bind(event.version_no)
    .bind(&event.summary)
    .bind(event.details.as_ref().map(sqlx::types::Json))
    .bind(&event.actor_id)
    .bind(&event.actor_name)
    .bind(&event.source)
    .bind(&event.operation_id)
    .bind(checked(event.duration_ms)?)
    .bind(&event.provider)
    .bind(&event.model)
    .bind(checked(tokens.map(|t| t.input_tokens))?)
    .bind(checked(tokens.map(|t| t.output_tokens))?)
    .bind(checked(tokens.map(|t| t.cache_read_tokens))?)
    .bind(checked(tokens.map(|t| t.cache_write_tokens))?)
    .bind(checked(tokens.map(|t| t.total_tokens))?)
    .fetch_one(&mut **tx)
    .await?;
    Ok(seq)
}

fn checked(value: Option<u64>) -> Result<Option<i64>> {
    value
        .map(i64::try_from)
        .transpose()
        .map_err(|_| Error::Invalid("numeric metadata overflow"))
}

#[derive(Debug, Serialize)]
pub struct TimelinePage {
    pub events: Vec<serde_json::Value>,
    pub next_after: i64,
    pub has_more: bool,
    pub retention_gap: bool,
    pub deleted_through: Option<i64>,
}

/// Authorization belongs to the caller. A consistent read includes both the
/// deletion watermark and the page, so retention cannot silently race a cursor.
pub async fn page(
    pool: &sqlx::PgPool,
    project: Uuid,
    after: i64,
    limit: u32,
) -> Result<TimelinePage> {
    if project.is_nil() || after < 0 || !(1..=200).contains(&limit) {
        return Err(Error::Invalid("timeline cursor or limit"));
    }
    let mut tx = pool.begin().await?;
    sqlx::query("SET TRANSACTION ISOLATION LEVEL REPEATABLE READ READ ONLY")
        .execute(&mut *tx)
        .await?;
    let deleted_through: Option<i64> = sqlx::query_scalar(
        "SELECT deleted_through FROM timeline_retention_watermarks WHERE project_id=$1",
    )
    .bind(project)
    .fetch_optional(&mut *tx)
    .await?;
    let rows = sqlx::query("SELECT seq, to_jsonb(e) AS event FROM project_timeline_events e WHERE project_id=$1 AND seq>$2 ORDER BY seq LIMIT $3")
        .bind(project).bind(after).bind(i64::from(limit) + 1).fetch_all(&mut *tx).await?;
    let has_more = rows.len() > limit as usize;
    let mut events = Vec::new();
    let mut next_after = after;
    for row in rows.into_iter().take(limit as usize) {
        next_after = row.try_get("seq")?;
        events.push(row.try_get("event")?);
    }
    tx.commit().await?;
    Ok(TimelinePage {
        events,
        next_after,
        has_more,
        retention_gap: deleted_through.is_some_and(|seq| seq > after),
        deleted_through,
    })
}
