// SPDX-License-Identifier: AGPL-3.0-or-later

use crate::prelude::*;
use crate::*;

pub(crate) async fn curate_memory(
    State(state): State<AppState>,
    axum::Extension(principal): axum::Extension<AuthenticatedPrincipal>,
    Json(request): Json<CurateRequest>,
) -> Result<Json<mem_record::CurateResponse>, ApiError> {
    request.validate().map_err(ApiError::validation)?;
    if request.dry_run {
        return preview_curate(&state.pool()?, &request)
            .await
            .map(Json)
            .map_err(ApiError::sql);
    }
    let actor = mem_store::jobs::JobActor::Credential {
        principal: principal.id,
        token: principal.token_id,
        session: principal.session_id,
    };
    let context = mem_store::events::MutationContext {
        actor_id: principal.id.to_string(),
        actor_name: principal.display_name,
        source: "service".into(),
        occurred_at: None,
    };
    let pool = state.pool()?;
    let mut tx = pool.begin().await.map_err(ApiError::sql)?;
    let mutation = mem_curate::curate_tx_with_actor(&mut tx, &request, &context, &actor)
        .await
        .map_err(ApiError::sql)?;
    tx.commit().await.map_err(ApiError::sql)?;
    for (event, seq) in mutation.events {
        crate::repository::events::broadcast_committed(&state, request.project.clone(), event, seq);
    }
    state.background.notify.notify_one();
    Ok(Json(mutation.response))
}

pub(crate) async fn project_replacement_proposals(
    State(state): State<AppState>,
    Path(slug): Path<String>,
) -> Result<Json<ReplacementProposalListResponse>, ApiError> {
    Ok(Json(
        list_replacement_proposals(&state.pool()?, &slug)
            .await
            .map_err(ApiError::sql)?,
    ))
}
pub(crate) async fn project_replacement_proposal_approve(
    State(state): State<AppState>,
    axum::Extension(principal): axum::Extension<AuthenticatedPrincipal>,
    Path((slug, proposal_id)): Path<(String, Uuid)>,
) -> Result<Json<ReplacementProposalResolutionResponse>, ApiError> {
    let context = mem_store::events::MutationContext {
        actor_id: principal.id.to_string(),
        actor_name: principal.display_name.clone(),
        source: "replacement-review".into(),
        occurred_at: None,
    };
    let actor = mem_store::jobs::JobActor::Credential {
        principal: principal.id,
        token: principal.token_id,
        session: principal.session_id,
    };
    let pool = state.pool()?;
    let mut tx = pool.begin().await.map_err(ApiError::sql)?;
    let mutation =
        mem_curate::approve_replacement_proposal_tx(&mut tx, &slug, proposal_id, &context, &actor)
            .await
            .map_err(ApiError::sql)?;
    tx.commit().await.map_err(ApiError::sql)?;
    crate::repository::events::broadcast_committed(&state, slug, mutation.event, mutation.sequence);
    state.background.notify.notify_one();
    Ok(Json(mutation.response))
}

pub(crate) async fn project_replacement_proposal_reject(
    State(state): State<AppState>,
    axum::Extension(principal): axum::Extension<AuthenticatedPrincipal>,
    Path((slug, proposal_id)): Path<(String, Uuid)>,
) -> Result<Json<ReplacementProposalResolutionResponse>, ApiError> {
    let context = mem_store::events::MutationContext {
        actor_id: principal.id.to_string(),
        actor_name: principal.display_name.clone(),
        source: "replacement-review".into(),
        occurred_at: None,
    };
    let actor = mem_store::jobs::JobActor::Credential {
        principal: principal.id,
        token: principal.token_id,
        session: principal.session_id,
    };
    let pool = state.pool()?;
    let mut tx = pool.begin().await.map_err(ApiError::sql)?;
    let mutation =
        mem_curate::reject_replacement_proposal_tx(&mut tx, &slug, proposal_id, &context, &actor)
            .await
            .map_err(ApiError::sql)?;
    tx.commit().await.map_err(ApiError::sql)?;
    crate::repository::events::broadcast_committed(&state, slug, mutation.event, mutation.sequence);
    state.background.notify.notify_one();
    Ok(Json(mutation.response))
}

#[derive(Debug, Deserialize)]
pub(crate) struct ReplacementPolicyQuery {
    repo_root: Option<String>,
}

pub(crate) async fn project_replacement_policy(
    State(state): State<AppState>,
    Path(slug): Path<String>,
    Query(params): Query<ReplacementPolicyQuery>,
) -> Result<Json<ReplacementPolicyResponse>, ApiError> {
    let repo_root = resolve_project_repo_root(&state, &slug, params.repo_root.as_deref());
    let replacement_policy = repo_root
        .as_deref()
        .and_then(|root| load_repo_replacement_policy(FsPath::new(root)).ok())
        .unwrap_or_default();
    Ok(Json(ReplacementPolicyResponse {
        project: slug,
        writable: repo_root.is_some(),
        repo_root,
        replacement_policy,
    }))
}

pub(crate) async fn project_replacement_policy_update(
    State(state): State<AppState>,
    axum::Extension(principal): axum::Extension<AuthenticatedPrincipal>,
    Path(slug): Path<String>,
    Json(request): Json<ReplacementPolicyRequest>,
) -> Result<Json<ReplacementPolicyResponse>, ApiError> {
    request.validate().map_err(ApiError::validation)?;
    let repo_root = request
        .repo_root
        .as_deref()
        .map(str::trim)
        .filter(|value| !value.is_empty())
        .ok_or_else(|| ApiError::validation(ValidationError::new("repo_root must be non-empty")))?;
    let pool = state.pool()?;
    let (project, trusted_root): (Uuid, String) =
        sqlx::query_as("SELECT id,root_path FROM projects WHERE slug=$1")
            .bind(&slug)
            .fetch_one(&pool)
            .await
            .map_err(ApiError::sql)?;
    let policy_path = project_policy_path(FsPath::new(&trusted_root), FsPath::new(repo_root))?;
    crate::effects::replace_file(
        &pool,
        &state.config.cluster.service_id,
        Some(project),
        &policy_path,
        &crate::effects::actor(&principal),
        |existing| render_replacement_policy(existing, request.replacement_policy),
    )
    .await?
    .require_confirmed()?;
    notify_project_refreshed(&state, slug.clone());
    Ok(Json(ReplacementPolicyResponse {
        project: slug,
        repo_root: Some(repo_root.to_string()),
        replacement_policy: request.replacement_policy,
        writable: true,
    }))
}

pub(crate) fn resolve_project_repo_root(
    state: &AppState,
    project: &str,
    requested: Option<&str>,
) -> Option<String> {
    if let Some(repo_root) = requested.map(str::trim).filter(|value| !value.is_empty()) {
        return Some(repo_root.to_string());
    }
    if let Some(repo_root) = state
        .config
        .automation
        .repo_root
        .as_deref()
        .map(str::trim)
        .filter(|value| !value.is_empty())
    {
        return Some(repo_root.to_string());
    }

    let mut repo_roots = state
        .watchers
        .lock()
        .expect("watcher registry mutex poisoned")
        .values()
        .filter(|watcher| watcher.project == project)
        .map(|watcher| watcher.repo_root.clone())
        .collect::<Vec<_>>();
    repo_roots.sort();
    repo_roots.dedup();
    if repo_roots.len() == 1 {
        repo_roots.pop()
    } else {
        None
    }
}

pub(crate) fn render_replacement_policy(
    existing: &str,
    policy: ReplacementPolicy,
) -> Result<String> {
    let mut document = if !existing.is_empty() {
        existing
            .parse::<toml_edit::DocumentMut>()
            .context("parse replacement policy")?
    } else {
        toml_edit::DocumentMut::new()
    };
    document["curation"]["replacement_policy"] = toml_edit::value(policy.to_string());
    Ok(document.to_string())
}

pub(crate) fn project_policy_path(
    trusted: &FsPath,
    requested: &FsPath,
) -> Result<PathBuf, ApiError> {
    let trusted = std::fs::canonicalize(trusted)
        .map_err(|_| ApiError::forbidden("project has no accessible registered repository root"))?;
    let requested = std::fs::canonicalize(requested)
        .map_err(|_| ApiError::forbidden("requested repository root is not accessible"))?;
    if requested != trusted {
        return Err(ApiError::forbidden(
            "repository root does not belong to the authorized project",
        ));
    }
    let path = crate::effects::resolve_file_target(&repo_agent_settings_path(&trusted))?;
    if !path.starts_with(&trusted) {
        return Err(ApiError::forbidden(
            "project policy path resolves outside its registered root",
        ));
    }
    Ok(path)
}
