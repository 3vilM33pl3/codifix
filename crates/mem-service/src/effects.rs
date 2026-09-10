// SPDX-License-Identifier: AGPL-3.0-or-later

//! External side effects are not rolled back by a failed database acknowledgement.
use crate::{ApiError, AppState};
use mem_store::{
    effects::{self, EffectIntent, EffectKind, EffectReason, EffectStatus},
    events::MutationContext,
};
use sha2::{Digest, Sha256};
use sqlx::Acquire;
use std::{
    io::{Read, Write},
    path::Path,
};
use uuid::Uuid;

pub(crate) struct FileOutcome {
    pub(crate) applied: bool,
    pub(crate) acknowledged: bool,
    pub(crate) effect: Option<Uuid>,
}

pub(crate) fn actor(principal: &crate::auth::AuthenticatedPrincipal) -> MutationContext {
    MutationContext {
        actor_id: principal.id.to_string(),
        actor_name: principal.display_name.clone(),
        source: "configuration".into(),
        occurred_at: None,
    }
}
impl FileOutcome {
    pub(crate) fn ephemeral() -> Self {
        Self {
            applied: true,
            acknowledged: true,
            effect: None,
        }
    }
    pub(crate) fn require_confirmed(&self) -> Result<(), ApiError> {
        if self.applied && self.acknowledged {
            return Ok(());
        }
        Err(ApiError::service_unavailable(&format!(
            "File operation {} has an {} outcome; inspect/reconcile the journal before retrying. The file has not been rolled back.",
            self.effect.map(|id| id.to_string()).unwrap_or_default(),
            if self.applied {
                "applied but unconfirmed"
            } else {
                "uncertain"
            }
        )))
    }
}

pub(crate) fn fingerprint(bytes: &[u8]) -> String {
    format!("{:x}", Sha256::digest(bytes))
}

/// Handle for one external attempt. Dropping it leaves a recoverable pending
/// intent rather than inventing a failure or re-executing the effect.
pub(crate) struct Attempt {
    pool: sqlx::PgPool,
    id: Uuid,
}
impl Attempt {
    pub(crate) async fn begin(
        state: &AppState,
        project: Option<&str>,
        kind: EffectKind,
        resource: &str,
        input: &[u8],
        context: &MutationContext,
    ) -> anyhow::Result<Self> {
        let pool = state.pool().map_err(|_| {
            anyhow::anyhow!("external intent store unavailable; operation was not started")
        })?;
        let project_id = if let Some(project) = project {
            sqlx::query_scalar("SELECT id FROM projects WHERE slug=$1")
                .bind(project)
                .fetch_optional(&pool)
                .await?
        } else {
            None
        };
        let id = Uuid::new_v4();
        effects::begin(
            &pool,
            &EffectIntent {
                id,
                project_id,
                kind,
                host_id: state.config.cluster.service_id.clone(),
                resource: resource.into(),
                input_fingerprint: fingerprint(input),
                actor_id: context.actor_id.clone(),
                source: context.source.clone(),
            },
        )
        .await?;
        Ok(Self { pool, id })
    }
    pub(crate) async fn observe(
        &self,
        status: EffectStatus,
        reason: EffectReason,
    ) -> anyhow::Result<()> {
        effects::conclude(&self.pool,self.id,Uuid::new_v5(&self.id,b"observed"),status,reason).await
            .map_err(|error|anyhow::anyhow!("external operation {} outcome acknowledgement failed ({error}); the external effect was not rolled back",self.id))
    }
}

/// Caller supplies a pure edit, performed after taking the path's writer lock.
/// The dedicated connection closes on cancellation so no pooled session retains
/// an advisory lock. No transaction spans the external write.
pub(crate) async fn replace_file<F>(
    pool: &sqlx::PgPool,
    host: &str,
    project: Option<Uuid>,
    path: &Path,
    context: &MutationContext,
    edit: F,
) -> Result<FileOutcome, ApiError>
where
    F: FnOnce(&str) -> anyhow::Result<String>,
{
    let target = resolve_file_target(path)?;
    let path = target.as_path();
    let mut conn = pool.acquire().await.map_err(ApiError::sql)?;
    conn.close_on_drop();
    sqlx::query("SELECT pg_advisory_lock(hashtextextended($1,0))")
        .bind(format!("file:{}", path.display()))
        .execute(&mut *conn)
        .await
        .map_err(ApiError::sql)?;
    let existing = match bounded_read(path) {
        Ok(bytes) => String::from_utf8(bytes)
            .map_err(|_| ApiError::internal("configuration is not UTF-8"))?,
        Err(error) if error.kind() == std::io::ErrorKind::NotFound => String::new(),
        Err(_) => {
            return Err(ApiError::internal(
                "could not read configuration before recording change",
            ));
        }
    };
    let rendered = edit(&existing).map_err(ApiError::io)?;
    if rendered.len() > 4 * 1024 * 1024 {
        return Err(ApiError::internal("configuration exceeds file limit"));
    }
    let id = Uuid::new_v4();
    let intent = EffectIntent {
        id,
        project_id: project,
        kind: EffectKind::FileReplace,
        host_id: host.into(),
        resource: path.to_string_lossy().into_owned(),
        input_fingerprint: fingerprint(rendered.as_bytes()),
        actor_id: context.actor_id.clone(),
        source: context.source.clone(),
    };
    effects::begin(&mut *conn, &intent).await.map_err(|_| {
        ApiError::service_unavailable("could not record file-change intent; no file was written")
    })?;
    // A synchronous, bounded local-file critical section cannot outlive an
    // aborted async future and later overwrite a new lock holder's result.
    let written = atomic_replace(path, rendered.as_bytes(), id).is_ok();
    let applied = written
        || bounded_read(path).is_ok_and(|bytes| fingerprint(&bytes) == intent.input_fingerprint);
    let status = if written {
        EffectStatus::Confirmed
    } else {
        EffectStatus::Uncertain
    };
    let reason = if written {
        EffectReason::Observed
    } else {
        EffectReason::LocalIo
    };
    let acknowledged = match conn.begin().await {
        Ok(mut tx) => {
            match effects::conclude_tx(&mut tx, id, Uuid::new_v5(&id, b"write"), status, reason)
                .await
            {
                Ok(()) => tx.commit().await.is_ok(),
                Err(_) => false,
            }
        }
        Err(_) => false,
    };
    Ok(FileOutcome {
        applied,
        acknowledged: acknowledged && written,
        effect: Some(id),
    })
}

pub(crate) fn resolve_file_target(path: &Path) -> Result<std::path::PathBuf, ApiError> {
    let absolute = if path.is_absolute() {
        path.to_path_buf()
    } else {
        std::env::current_dir()
            .map_err(|_| ApiError::internal("cannot resolve configuration path"))?
            .join(path)
    };
    // Resolve aliases before choosing the cross-process writer lock. A missing
    // target may be created, but an existing symlink names its actual target.
    let target = std::fs::canonicalize(&absolute)
        .or_else(|_| {
            let parent = absolute
                .parent()
                .ok_or_else(|| std::io::Error::other("missing parent"))?;
            Ok::<_, std::io::Error>(
                std::fs::canonicalize(parent)?.join(
                    absolute
                        .file_name()
                        .ok_or_else(|| std::io::Error::other("missing filename"))?,
                ),
            )
        })
        .unwrap_or(absolute);
    if target.components().any(|c| {
        matches!(
            c,
            std::path::Component::ParentDir | std::path::Component::CurDir
        )
    }) || target.to_str().is_none()
    {
        return Err(ApiError::internal(
            "configuration path cannot be normalized",
        ));
    }
    Ok(target)
}

fn bounded_read(path: &Path) -> std::io::Result<Vec<u8>> {
    if !std::fs::metadata(path)?.is_file() {
        return Err(std::io::Error::other("configuration is not a regular file"));
    }
    let mut bytes = Vec::new();
    std::fs::File::open(path)?
        .take(4 * 1024 * 1024 + 1)
        .read_to_end(&mut bytes)?;
    if bytes.len() > 4 * 1024 * 1024 {
        return Err(std::io::Error::other("file exceeds limit"));
    }
    Ok(bytes)
}

fn atomic_replace(path: &Path, bytes: &[u8], id: Uuid) -> std::io::Result<()> {
    let parent = path
        .parent()
        .ok_or_else(|| std::io::Error::other("no parent directory"))?;
    std::fs::create_dir_all(parent)?;
    let temporary = parent.join(format!(".memory-effect-{id}.tmp"));
    let mut options = std::fs::OpenOptions::new();
    options.create_new(true).write(true);
    #[cfg(unix)]
    {
        use std::os::unix::fs::OpenOptionsExt;
        options.mode(0o600);
    }
    let mut file = options.open(&temporary)?;
    if let Ok(metadata) = std::fs::metadata(path) {
        file.set_permissions(metadata.permissions())?;
    }
    file.write_all(bytes)?;
    file.sync_all()?;
    std::fs::rename(&temporary, path)?;
    std::fs::File::open(parent)?.sync_all()?;
    Ok(())
}

/// Recovery never repeats irreversible effects. File bytes can be checked;
/// provider/restart outcomes without receipts remain explicitly uncertain.
pub(crate) async fn reconcile(host: &str, pool: &sqlx::PgPool) -> anyhow::Result<()> {
    mem_reinforce::validate::reconcile_interrupted(pool).await?;
    crate::repository::handlers::loops::run_persistence::reconcile_interrupted_runs(pool)
        .await
        .map_err(|e| anyhow::anyhow!(e.message))?;
    let mut pending = effects::pending(pool, host, 100).await?;
    if host != effects::SHARED_PROVIDER_OWNER {
        pending.extend(effects::pending(pool, effects::SHARED_PROVIDER_OWNER, 100).await?);
    }
    for intent in pending {
        let (status, reason) = if intent.kind == EffectKind::FileReplace {
            match bounded_read(Path::new(&intent.resource)) {
                Ok(bytes) if fingerprint(&bytes) == intent.input_fingerprint => {
                    (EffectStatus::Confirmed, EffectReason::Observed)
                }
                Ok(_) => (EffectStatus::Uncertain, EffectReason::Mismatch),
                Err(_) => (EffectStatus::Uncertain, EffectReason::LocalIo),
            }
        } else {
            (EffectStatus::Uncertain, EffectReason::Interrupted)
        };
        effects::conclude(
            pool,
            intent.id,
            Uuid::new_v5(&intent.id, b"reconcile"),
            status,
            reason,
        )
        .await?;
    }
    Ok(())
}

pub(crate) struct Recovery(tokio::task::JoinHandle<()>);
impl Drop for Recovery {
    fn drop(&mut self) {
        self.0.abort();
    }
}
pub(crate) fn start(state: AppState) -> Option<Recovery> {
    if !state.is_primary() || state.config.service.read_only {
        return None;
    }
    Some(Recovery(tokio::spawn(async move {
        let mut tick = tokio::time::interval(std::time::Duration::from_secs(30));
        loop {
            tick.tick().await;
            if let Ok(pool) = state.pool()
                && reconcile(&state.config.cluster.service_id, &pool)
                    .await
                    .is_err()
            {
                tracing::warn!("external-effect reconciliation unavailable; polling will retry");
            }
        }
    })))
}

#[cfg(test)]
mod tests;
