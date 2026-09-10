// SPDX-License-Identifier: AGPL-3.0-or-later

//! External effects have a committed intent and separate observed outcomes.
//! A missing acknowledgement is uncertainty, not evidence of rollback. This
//! private journal contains bounded locators/hashes, never request/provider bodies.
use crate::{Error, Result};
use serde::{Deserialize, Serialize};
use sqlx::{Postgres, Row, Transaction};
use uuid::Uuid;

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum EffectKind {
    FileReplace,
    ProviderRequest,
    WatcherRestart,
    WorkspaceCreate,
    ExportPrepare,
}
impl EffectKind {
    fn label(self) -> &'static str {
        match self {
            Self::FileReplace => "file_replace",
            Self::ProviderRequest => "provider_request",
            Self::WatcherRestart => "watcher_restart",
            Self::WorkspaceCreate => "workspace_create",
            Self::ExportPrepare => "export_prepare",
        }
    }
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct EffectIntent {
    pub id: Uuid,
    pub project_id: Option<Uuid>,
    pub kind: EffectKind,
    pub host_id: String,
    /// Private local path or bounded operation identifier, not an execution payload.
    pub resource: String,
    /// For file replacement, SHA-256 of the intended resulting bytes.
    pub input_fingerprint: String,
    pub actor_id: String,
    pub source: String,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum EffectStatus {
    Confirmed,
    Failed,
    Uncertain,
}
impl EffectStatus {
    fn label(self) -> &'static str {
        match self {
            Self::Confirmed => "confirmed",
            Self::Failed => "failed",
            Self::Uncertain => "uncertain",
        }
    }
}
#[derive(Debug, Clone, Copy)]
pub enum EffectReason {
    Observed,
    LocalIo,
    RemoteIo,
    Interrupted,
    Mismatch,
    Denied,
}
impl EffectReason {
    fn label(self) -> &'static str {
        match self {
            Self::Observed => "observed",
            Self::LocalIo => "local_io",
            Self::RemoteIo => "remote_io",
            Self::Interrupted => "interrupted",
            Self::Mismatch => "mismatch",
            Self::Denied => "denied",
        }
    }
}

pub async fn begin<'e, E: sqlx::PgExecutor<'e>>(executor: E, intent: &EffectIntent) -> Result<()> {
    if intent.id.is_nil() || intent.project_id.is_some_and(|id| id.is_nil()) {
        return Err(Error::Invalid("effect identity"));
    }
    for (value, max) in [
        (&intent.host_id, 256),
        (&intent.resource, 4096),
        (&intent.actor_id, 256),
        (&intent.source, 80),
    ] {
        if value.is_empty() || value.len() > max || value.contains('\0') {
            return Err(Error::Invalid("effect locator"));
        }
    }
    if intent.input_fingerprint.len() != 64
        || !intent
            .input_fingerprint
            .bytes()
            .all(|b| b.is_ascii_digit() || (b'a'..=b'f').contains(&b))
    {
        return Err(Error::Invalid("effect fingerprint"));
    }
    // Retries may observe the same intent, never replace an existing recipe.
    let stored:bool=sqlx::query_scalar("INSERT INTO external_effects(id,project_id,kind,host_id,resource,input_fingerprint,actor_id,source)
        VALUES($1,$2,$3,$4,$5,$6,$7,$8) ON CONFLICT(id) DO UPDATE SET id=external_effects.id
        RETURNING project_id IS NOT DISTINCT FROM $2 AND kind=$3 AND host_id=$4 AND resource=$5 AND input_fingerprint=$6 AND actor_id=$7 AND source=$8")
        .bind(intent.id).bind(intent.project_id).bind(intent.kind.label()).bind(&intent.host_id).bind(&intent.resource)
        .bind(&intent.input_fingerprint).bind(&intent.actor_id).bind(&intent.source).fetch_one(executor).await?;
    if !stored {
        return Err(Error::Conflict);
    };
    Ok(())
}

pub async fn conclude_tx(
    tx: &mut Transaction<'_, Postgres>,
    id: Uuid,
    receipt: Uuid,
    status: EffectStatus,
    reason: EffectReason,
) -> Result<()> {
    let current: String =
        sqlx::query_scalar("SELECT status FROM external_effects WHERE id=$1 FOR UPDATE")
            .bind(id)
            .fetch_one(&mut **tx)
            .await?;
    let prior: Option<(Uuid, String, String)> =
        sqlx::query_as("SELECT effect_id,status,reason FROM external_effect_outcomes WHERE id=$1")
            .bind(receipt)
            .fetch_optional(&mut **tx)
            .await?;
    if let Some(prior) = prior {
        return if prior == (id, status.label().into(), reason.label().into()) {
            Ok(())
        } else {
            Err(Error::Conflict)
        };
    }
    if matches!(current.as_str(), "confirmed" | "failed") && current != status.label() {
        return Err(Error::Conflict);
    };
    sqlx::query(
        "INSERT INTO external_effect_outcomes(id,effect_id,status,reason) VALUES($1,$2,$3,$4)",
    )
    .bind(receipt)
    .bind(id)
    .bind(status.label())
    .bind(reason.label())
    .execute(&mut **tx)
    .await?;
    sqlx::query("UPDATE external_effects SET status=$2,updated_at=now() WHERE id=$1")
        .bind(id)
        .bind(status.label())
        .execute(&mut **tx)
        .await?;
    Ok(())
}

pub async fn conclude(
    pool: &sqlx::PgPool,
    id: Uuid,
    receipt: Uuid,
    status: EffectStatus,
    reason: EffectReason,
) -> Result<()> {
    let mut tx = pool.begin().await?;
    conclude_tx(&mut tx, id, receipt, status, reason).await?;
    tx.commit().await?;
    Ok(())
}

/// Only the owning host inspects locators. Recovery observes effects; it must
/// never blindly re-execute a provider call, restart, or repository mutation.
pub async fn pending(pool: &sqlx::PgPool, host: &str, limit: i64) -> Result<Vec<EffectIntent>> {
    let rows=sqlx::query("SELECT * FROM external_effects WHERE host_id=$1 AND status='pending' AND reconcile_after<=now() ORDER BY reconcile_after,id LIMIT $2")
        .bind(host).bind(limit.clamp(1,100)).fetch_all(pool).await?;
    rows.into_iter()
        .map(|row| {
            Ok(EffectIntent {
                id: row.try_get("id")?,
                project_id: row.try_get("project_id")?,
                kind: serde_json::from_value(serde_json::Value::String(row.try_get("kind")?))?,
                host_id: row.try_get("host_id")?,
                resource: row.try_get("resource")?,
                input_fingerprint: row.try_get("input_fingerprint")?,
                actor_id: row.try_get("actor_id")?,
                source: row.try_get("source")?,
            })
        })
        .collect()
}
