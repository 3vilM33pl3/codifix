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
    Path((slug, proposal_id)): Path<(String, Uuid)>,
) -> Result<Json<ReplacementProposalResolutionResponse>, ApiError> {
    let response = approve_replacement_proposal(&state.pool()?, &slug, proposal_id)
        .await
        .map_err(ApiError::sql)?;
    if let Some(new_memory_id) = response.new_memory_id {
        notify_project_changed(
            &state,
            slug.clone(),
            Some(new_memory_id),
            ActivityKind::MemoryReplacement,
            format!(
                "Replaced memory \"{}\" with \"{}\" after review.",
                response.target_summary, response.candidate_summary
            ),
            Some(ActivityDetails::MemoryReplacement {
                old_memory_id: response.target_memory_id,
                old_summary: response.target_summary.clone(),
                new_memory_id,
                new_summary: response.candidate_summary.clone(),
                automatic: false,
                policy: response.policy,
            }),
        );
    }
    notify_project_refreshed(&state, slug.clone());
    Ok(Json(response))
}

pub(crate) async fn project_replacement_proposal_reject(
    State(state): State<AppState>,
    Path((slug, proposal_id)): Path<(String, Uuid)>,
) -> Result<Json<ReplacementProposalResolutionResponse>, ApiError> {
    let response = reject_replacement_proposal(&state.pool()?, &slug, proposal_id)
        .await
        .map_err(ApiError::sql)?;
    notify_project_refreshed(&state, slug.clone());
    Ok(Json(response))
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
    write_replacement_policy(FsPath::new(repo_root), request.replacement_policy)
        .map_err(ApiError::io)?;
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

pub(crate) fn write_replacement_policy(
    repo_root: &FsPath,
    policy: ReplacementPolicy,
) -> Result<()> {
    let path = repo_agent_settings_path(repo_root);
    let mut document = if path.exists() {
        std::fs::read_to_string(&path)
            .with_context(|| format!("read {}", path.display()))?
            .parse::<toml_edit::DocumentMut>()
            .with_context(|| format!("parse {}", path.display()))?
    } else {
        toml_edit::DocumentMut::new()
    };
    document["curation"]["replacement_policy"] = toml_edit::value(policy.to_string());
    if let Some(parent) = path.parent() {
        std::fs::create_dir_all(parent).with_context(|| format!("create {}", parent.display()))?;
    }
    std::fs::write(&path, document.to_string())
        .with_context(|| format!("write {}", path.display()))?;
    Ok(())
}
