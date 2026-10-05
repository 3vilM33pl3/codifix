// SPDX-License-Identifier: AGPL-3.0-or-later

use axum::{
    Router,
    routing::{any, delete, get, post},
};
use tower_http::{
    services::{ServeDir, ServeFile},
    trace::TraceLayer,
};

use super::{
    AppState, activate_embedding_backend, admin_shutdown, agents_snapshot, approve_loop_approval,
    approve_loop_memory_proposal, archive, archive_memory, auth_callback, auth_create_token,
    auth_grant_membership, auth_list_memberships, auth_list_principals, auth_list_tokens,
    auth_login, auth_logout, auth_me, auth_revoke_membership, auth_revoke_token,
    build_loop_context_pack, cancel_loop_run, capture_task, create_loop_memory_proposal,
    curate_memory, deactivate_embedding_backend, delete_memory, edit_loop_approval,
    edit_loop_memory_proposal, finish_agent_workspace, get_loop_definition, get_loop_global_state,
    get_loop_run, get_loop_run_context_pack, get_memory, get_memory_history, healthz,
    heartbeat_agent_workspace, ingest_activity, list_agent_workspaces, list_embedding_backends,
    list_loop_approvals, list_loop_definitions, list_loop_memory_proposals, list_loop_runs,
    llm_audit_status, memory_scores, project_activities, project_bundle_export,
    project_bundle_export_preview, project_bundle_import, project_bundle_import_preview,
    project_commit_detail, project_commits, project_graph, project_graph_extract,
    project_graph_status, project_memories, project_memory_graph, project_overview,
    project_replacement_policy, project_replacement_policy_update,
    project_replacement_proposal_approve, project_replacement_proposal_reject,
    project_replacement_proposals, project_resume, project_structure, project_up_to_speed,
    prune_embeddings, prune_history, query, query_global, read_skill, reembed, reindex,
    reject_loop_approval, reject_loop_memory_proposal, repair_skills, review_validation_run,
    route_loop_trigger, run_loop, runtime_status, session_bootstrap,
    set_embedding_creation_enabled, set_llm_audit_enabled, skills, start_agent_workspace,
    submit_loop_feedback, sync_commits, update_loop_global_state, update_loop_settings,
    validate_memory, validation_runs, verify_provenance, watcher_heartbeat, watcher_restart_local,
    watcher_unregister, web_unavailable, websocket,
};

/// The API specification, embedded at compile time so the running service
/// always describes exactly the surface it was built with. A contract test
/// keeps the spec's path inventory in sync with this router.
async fn openapi_spec() -> impl axum::response::IntoResponse {
    (
        [(axum::http::header::CONTENT_TYPE, "application/yaml")],
        include_str!("../../../docs/api/openapi.yaml"),
    )
}

pub(crate) fn build_http_app(state: AppState) -> Router {
    let web_assets = state.web_root.clone();
    let mcp_config = state.config.mcp.clone();
    let mut app = Router::new()
        .route("/v1/onboarding/cache", get(crate::laptop_cache::list))
        .route(
            "/v1/onboarding/cache/{subscription_id}/search",
            post(crate::laptop_cache::search).layer(axum::extract::DefaultBodyLimit::max(8192)),
        )
        .route(
            "/v1/onboarding/device/cancel",
            post(crate::auth::device::cancel).layer(axum::extract::DefaultBodyLimit::max(4096)),
        )
        .route(
            "/v1/onboarding/device/start",
            post(crate::auth::device::start).layer(axum::extract::DefaultBodyLimit::max(4096)),
        )
        .route(
            "/v1/onboarding/device/poll",
            post(crate::auth::device::poll).layer(axum::extract::DefaultBodyLimit::max(4096)),
        )
        .route(
            "/v1/onboarding/device/review",
            get(crate::auth::device::review),
        )
        .route(
            "/v1/onboarding/company/device/review",
            get(crate::auth::company_devices::review),
        )
        .route(
            "/v1/onboarding/company/device/approve",
            post(crate::auth::company_devices::approve)
                .layer(axum::extract::DefaultBodyLimit::max(16384)),
        )
        .route(
            "/v1/onboarding/device/approve",
            post(crate::auth::device::approve).layer(axum::extract::DefaultBodyLimit::max(16384)),
        )
        .route(
            "/v1/onboarding/device/logout",
            post(crate::auth::device::logout),
        )
        .route("/v1/onboarding/projects", get(crate::auth::device::catalog))
        .route(
            "/v1/onboarding/devices/{device_id}/agents",
            get(crate::auth::company_agents::inspect)
                .post(crate::auth::company_agents::approve)
                .layer(axum::extract::DefaultBodyLimit::max(16384)),
        )
        .route(
            "/v1/onboarding/devices/{device_id}/agents/{agent_id}/revoke",
            post(crate::auth::company_agents::revoke),
        )
        .route(
            "/v1/onboarding/agents/{agent_id}/receipt",
            get(crate::auth::company_agents::child_receipt),
        )
        .route(
            "/v1/onboarding/areas/{area}/owners/{owner}/projects/{project_id}/snapshot",
            post(crate::auth::scoped_delivery::scoped_snapshot)
                .layer(axum::extract::DefaultBodyLimit::max(4096)),
        )
        .route(
            "/v1/onboarding/areas/{area}/owners/{owner}/projects/{project_id}/subscription",
            get(crate::auth::scoped_delivery::scoped_subscription),
        )
        .route(
            "/v1/onboarding/company-delivery/catalog",
            post(crate::auth::company_device_delivery::catalog)
                .layer(axum::extract::DefaultBodyLimit::max(4096)),
        )
        .route(
            "/v1/onboarding/company-delivery/approve",
            post(crate::auth::company_device_delivery::approve)
                .layer(axum::extract::DefaultBodyLimit::max(8192)),
        )
        .route(
            "/v1/onboarding/company-delivery/subscription",
            post(crate::auth::company_device_delivery::subscription)
                .layer(axum::extract::DefaultBodyLimit::max(4096)),
        )
        .route(
            "/v1/onboarding/company-delivery/snapshot",
            post(crate::auth::company_device_delivery::snapshot)
                .layer(axum::extract::DefaultBodyLimit::max(4096)),
        )
        .route(
            "/v1/onboarding/projects/{project_id}/snapshot",
            post(crate::auth::scoped_delivery::snapshot)
                .layer(axum::extract::DefaultBodyLimit::max(4096)),
        )
        .route(
            "/v1/onboarding/projects/{project_id}/subscription",
            get(crate::auth::scoped_delivery::subscription),
        )
        .route(
            "/v1/onboarding/delegated-grants",
            post(crate::auth::onboarding::company::delegated_grant)
                .layer(axum::extract::DefaultBodyLimit::max(8192)),
        )
        .route(
            "/v1/onboarding/delegated-grants/operations/{operation_id}",
            get(crate::auth::onboarding::company::delegated_status),
        )
        .route(
            "/v1/admin/onboarding/company/configuration",
            get(crate::auth::onboarding::company::company_configuration),
        )
        .route(
            "/v1/onboarding/company/projects",
            get(crate::auth::company_catalog::catalog),
        )
        .route(
            "/v1/admin/onboarding/company/delegates",
            get(crate::auth::company_delegate::list)
                .post(crate::auth::company_delegate::create)
                .layer(axum::extract::DefaultBodyLimit::max(32768)),
        )
        .route(
            "/v1/admin/onboarding/company/delivery-delegates",
            get(crate::auth::company_delivery_delegate::list)
                .post(crate::auth::company_delivery_delegate::create)
                .layer(axum::extract::DefaultBodyLimit::max(32768)),
        )
        .route(
            "/v1/admin/onboarding/company/operations",
            get(crate::auth::onboarding::company::company_operations)
                .post(crate::auth::onboarding::company::company_operation)
                .layer(axum::extract::DefaultBodyLimit::max(8192)),
        )
        .route(
            "/v1/admin/onboarding/company/operations/{operation_id}",
            get(crate::auth::onboarding::company::company_status),
        )
        .route(
            "/v1/admin/onboarding/company/operations/{operation_id}/reconcile",
            post(crate::auth::onboarding::company::company_reconcile),
        )
        .route(
            "/v1/admin/onboarding/capabilities",
            get(crate::auth::onboarding::capabilities),
        )
        .route(
            "/v1/admin/onboarding/users",
            get(crate::auth::onboarding::users).post(crate::auth::onboarding::register_user),
        )
        .route(
            "/v1/admin/onboarding/users/invite",
            post(crate::auth::onboarding::invite),
        )
        .route(
            "/v1/admin/onboarding/users/{id}/disable",
            post(crate::auth::onboarding::disable_user),
        )
        .route(
            "/v1/admin/onboarding/users/{id}/access",
            get(crate::auth::onboarding::access),
        )
        .route(
            "/v1/admin/onboarding/projects",
            get(crate::auth::onboarding::projects),
        )
        .route(
            "/v1/admin/onboarding/projects/{project_id}",
            post(crate::auth::onboarding::review_project),
        )
        .route(
            "/v1/admin/onboarding/devices",
            get(crate::auth::onboarding::devices),
        )
        .route(
            "/v1/admin/onboarding/devices/{id}/revoke",
            post(crate::auth::onboarding::revoke_device),
        )
        .route(
            "/v1/admin/onboarding/idp/users",
            post(crate::auth::onboarding::create_idp_user),
        )
        .route(
            "/v1/admin/onboarding/idp/operations",
            get(crate::auth::onboarding::operations),
        )
        .route(
            "/v1/admin/onboarding/idp/operations/{id}",
            get(crate::auth::onboarding::operation),
        )
        .route(
            "/v1/admin/onboarding/idp/users/{provider_id}/disable",
            post(crate::auth::onboarding::disable_idp_user),
        )
        .route("/v1/supervisor", get(crate::handlers::supervisor::inspect))
        .route(
            "/v1/projects/{slug}/supervisor/questions/prepare",
            post(crate::handlers::supervisor::questions::prepare),
        )
        .route(
            "/v1/projects/{slug}/supervisor/questions",
            get(crate::handlers::supervisor::questions::list)
                .post(crate::handlers::supervisor::questions::submit),
        )
        .route(
            "/v1/projects/{slug}/supervisor/questions/{id}",
            get(crate::handlers::supervisor::questions::status),
        )
        .route(
            "/v1/projects/{slug}/supervisor/proposals",
            get(crate::handlers::supervisor::proposals::list),
        )
        .route(
            "/v1/projects/{slug}/supervisor/proposals/{id}",
            get(crate::handlers::supervisor::proposals::detail),
        )
        .route(
            "/v1/projects/{slug}/supervisor/proposals/{id}/review",
            post(crate::handlers::supervisor::proposals::review),
        )
        .route(
            "/v1/supervisor/policy",
            post(crate::handlers::supervisor::control),
        )
        .route("/v1/projects/{slug}/jobs", get(crate::handlers::jobs::list))
        .route(
            "/v1/projects/{slug}/jobs/{id}",
            get(crate::handlers::jobs::detail),
        )
        .route(
            "/v1/projects/{slug}/jobs/{id}/retry",
            post(crate::handlers::jobs::retry),
        )
        .route("/healthz", get(healthz))
        .route("/v1/openapi.yaml", get(openapi_spec))
        .route("/ws", get(websocket))
        .route("/v1/auth/session/bootstrap", post(session_bootstrap))
        .route("/v1/auth/login", get(auth_login))
        .route("/v1/auth/callback", get(auth_callback))
        .route("/v1/auth/me", get(auth_me))
        .route("/v1/auth/logout", post(auth_logout))
        .route(
            "/v1/auth/tokens",
            get(auth_list_tokens).post(auth_create_token),
        )
        .route("/v1/auth/tokens/{selector}/revoke", post(auth_revoke_token))
        .route("/v1/auth/principals", get(auth_list_principals))
        .route(
            "/v1/auth/expertise-answerers",
            post(crate::auth::answerer::create),
        )
        .route(
            "/v1/auth/memberships",
            get(auth_list_memberships).post(auth_grant_membership),
        )
        .route("/v1/auth/memberships/{id}", delete(auth_revoke_membership))
        .route("/v1/admin/shutdown", post(admin_shutdown))
        .route("/v1/runtime/status", get(runtime_status))
        .route("/v1/skills", get(skills))
        .route("/v1/skills/repair", post(repair_skills))
        .route("/v1/skills/{skill_name}", get(read_skill))
        .route("/v1/query", post(query))
        .route("/v1/query/global", post(query_global))
        .route("/v1/activity", post(ingest_activity))
        .route("/v1/commits/sync", post(sync_commits))
        .route("/v1/capture/task", post(capture_task))
        .route("/v1/curate", post(curate_memory))
        .route("/v1/provenance/verify", post(verify_provenance))
        .route("/v1/reindex", post(reindex))
        .route("/v1/reembed", post(reembed))
        .route("/v1/prune-embeddings", post(prune_embeddings))
        .route("/v1/embeddings/backends", get(list_embedding_backends))
        .route("/v1/embeddings/activate", post(activate_embedding_backend))
        .route(
            "/v1/embeddings/deactivate",
            post(deactivate_embedding_backend),
        )
        .route(
            "/v1/embeddings/create-enabled",
            post(set_embedding_creation_enabled),
        )
        .route(
            "/v1/config/llm-audit",
            get(llm_audit_status).post(set_llm_audit_enabled),
        )
        .route("/v1/loops", get(list_loop_definitions))
        .route(
            "/v1/loops/global-kill-switch",
            get(get_loop_global_state).post(update_loop_global_state),
        )
        .route("/v1/loops/runs", get(list_loop_runs))
        .route("/v1/loops/runs/{run_id}", get(get_loop_run))
        .route(
            "/v1/loops/runs/{run_id}/context-pack",
            get(get_loop_run_context_pack),
        )
        .route("/v1/loops/runs/{run_id}/cancel", post(cancel_loop_run))
        .route(
            "/v1/loops/runs/{run_id}/feedback",
            post(submit_loop_feedback),
        )
        .route("/v1/loops/approvals", get(list_loop_approvals))
        .route(
            "/v1/loops/memory-proposals",
            get(list_loop_memory_proposals).post(create_loop_memory_proposal),
        )
        .route("/v1/loops/triggers/route", post(route_loop_trigger))
        .route(
            "/v1/loops/approvals/{approval_id}/approve",
            post(approve_loop_approval),
        )
        .route(
            "/v1/loops/approvals/{approval_id}/reject",
            post(reject_loop_approval),
        )
        .route(
            "/v1/loops/approvals/{approval_id}/edit",
            post(edit_loop_approval),
        )
        .route(
            "/v1/loops/memory-proposals/{proposal_id}/approve",
            post(approve_loop_memory_proposal),
        )
        .route(
            "/v1/loops/memory-proposals/{proposal_id}/reject",
            post(reject_loop_memory_proposal),
        )
        .route(
            "/v1/loops/memory-proposals/{proposal_id}/edit",
            post(edit_loop_memory_proposal),
        )
        .route("/v1/loops/{loop_id}", get(get_loop_definition))
        .route("/v1/loops/{loop_id}/settings", post(update_loop_settings))
        .route("/v1/loops/{loop_id}/run", post(run_loop))
        .route(
            "/v1/loops/{loop_id}/context-pack",
            get(build_loop_context_pack),
        )
        .route("/v1/memory/{id}", get(get_memory))
        .route("/v1/memory/{id}/validate", post(validate_memory))
        .route(
            "/v1/validation-runs/{id}/review",
            post(review_validation_run),
        )
        .route("/v1/memory/{id}/archive", post(archive_memory))
        .route("/v1/memory/{id}/history", get(get_memory_history))
        .route("/v1/memory", delete(delete_memory))
        .route("/v1/prune-history", post(prune_history))
        .route("/v1/projects/{project}/memory-scores", get(memory_scores))
        .route("/v1/projects/{project}/structure", get(project_structure))
        .route(
            "/v1/projects/{project}/validation-runs",
            get(validation_runs),
        )
        .route("/v1/projects/{slug}/commits", get(project_commits))
        .route(
            "/v1/projects/{slug}/commits/{hash}",
            get(project_commit_detail),
        )
        .route(
            "/v1/projects/{slug}/bundle/export/preview",
            post(project_bundle_export_preview),
        )
        .route(
            "/v1/projects/{slug}/bundle/export",
            post(project_bundle_export),
        )
        .route(
            "/v1/projects/{slug}/bundle/import/preview",
            post(project_bundle_import_preview),
        )
        .route(
            "/v1/projects/{slug}/bundle/import",
            post(project_bundle_import),
        )
        .route(
            "/v1/projects/{slug}/replacement-proposals",
            get(project_replacement_proposals),
        )
        .route(
            "/v1/projects/{slug}/replacement-proposals/{proposal_id}/approve",
            post(project_replacement_proposal_approve),
        )
        .route(
            "/v1/projects/{slug}/replacement-proposals/{proposal_id}/reject",
            post(project_replacement_proposal_reject),
        )
        .route(
            "/v1/projects/{slug}/replacement-policy",
            get(project_replacement_policy)
                .put(project_replacement_policy_update)
                .post(project_replacement_policy_update),
        )
        .route("/v1/projects/{slug}/memories", get(project_memories))
        .route(
            "/v1/projects/{slug}/federation",
            get(crate::federation::workflow::discover),
        )
        .route(
            "/v1/projects/{slug}/federation/{space}/catalog",
            get(crate::federation::workflow::catalog),
        )
        .route(
            "/v1/projects/{slug}/federation/{space}/proposals/{publisher}/{record}",
            get(crate::federation::workflow::proposal),
        )
        .route(
            "/v1/projects/{slug}/federation/{space}/review-preview",
            post(crate::federation::workflow::review_draft)
                .layer(axum::extract::DefaultBodyLimit::max(128 * 1024)),
        )
        .route(
            "/v1/projects/{slug}/federation/{space}/questions/prepare",
            post(crate::federation::workflow::expertise_prepare)
                .layer(axum::extract::DefaultBodyLimit::max(8192)),
        )
        .route(
            "/v1/projects/{slug}/federation/{space}/collections/draft",
            post(crate::federation::workflow::collection_draft)
                .layer(axum::extract::DefaultBodyLimit::max(2 * 1024 * 1024)),
        )
        .route(
            "/v1/projects/{slug}/federation/{space}/collections",
            get(crate::federation::workflow::collections),
        )
        .route(
            "/v1/projects/{slug}/federation/{space}/collections/{collection}",
            get(crate::federation::workflow::collection_detail),
        )
        .route(
            "/v1/projects/{slug}/federation/{space}/collections/{collection}/grants",
            get(crate::federation::workflow::collection_grant_state)
                .post(crate::federation::workflow::collection_grant)
                .layer(axum::extract::DefaultBodyLimit::max(4096)),
        )
        .route(
            "/v1/projects/{slug}/federation/{space}/collections/activate",
            post(crate::federation::workflow::collection_activate)
                .layer(axum::extract::DefaultBodyLimit::max(2 * 1024 * 1024)),
        )
        .route(
            "/v1/projects/{slug}/federation/{space}/collections/withdraw",
            post(crate::federation::workflow::collection_withdraw)
                .layer(axum::extract::DefaultBodyLimit::max(4096)),
        )
        .route(
            "/v1/projects/{slug}/federation/{space}/capabilities",
            get(crate::federation::workflow::expertise_capabilities),
        )
        .route(
            "/v1/projects/{slug}/federation/{space}/capabilities/{publisher}/{record}",
            get(crate::federation::workflow::expertise_capability),
        )
        .route(
            "/v1/projects/{slug}/federation/{space}/questions/{id}",
            get(crate::federation::workflow::expertise_result),
        )
        .route(
            "/v1/projects/{slug}/federation/{space}/questions/submit",
            post(crate::federation::workflow::expertise_submit)
                .layer(axum::extract::DefaultBodyLimit::max(32768)),
        )
        .route(
            "/v1/projects/{slug}/federation/{space}/agent-questions",
            post(crate::federation::workflow::expertise_agent_ask)
                .get(crate::federation::workflow::expertise_agent_exchanges)
                .layer(axum::extract::DefaultBodyLimit::max(8192)),
        )
        .route(
            "/v1/projects/{slug}/federation/{space}/agent-questions/{id}",
            get(crate::federation::workflow::expertise_agent_result),
        )
        .route(
            "/v1/projects/{slug}/federation/{space}/actions",
            post(crate::federation::workflow::action)
                .layer(axum::extract::DefaultBodyLimit::max(48 * 1024)),
        )
        .route(
            "/v1/projects/{slug}/federation/{space}/sync",
            post(crate::federation::workflow::synchronize)
                .layer(axum::extract::DefaultBodyLimit::max(4096)),
        )
        .route(
            "/v1/projects/{slug}/federation/{space}",
            get(crate::federation::workflow::status),
        )
        .route(
            "/v1/projects/{slug}/federation/{space}/grants",
            post(crate::federation::workflow::grant)
                .layer(axum::extract::DefaultBodyLimit::max(4096)),
        )
        .route(
            "/v1/projects/{slug}/federation/{space}/memories/{canonical}/preview",
            get(crate::federation::workflow::preview),
        )
        .route(
            "/v1/projects/{slug}/federation/{space}/reviews",
            post(crate::federation::workflow::review)
                .layer(axum::extract::DefaultBodyLimit::max(128 * 1024)),
        )
        .route(
            "/v1/projects/{slug}/federation/{space}/context",
            post(crate::federation::workflow::context)
                .layer(axum::extract::DefaultBodyLimit::max(4096)),
        )
        .route(
            "/v1/projects/{slug}/memory-graph",
            get(project_memory_graph),
        )
        .route("/v1/projects/{slug}/overview", get(project_overview))
        .route(
            "/v1/projects/{slug}/graph/status",
            get(project_graph_status),
        )
        .route(
            "/v1/projects/{slug}/graph/extract",
            post(project_graph_extract)
                .layer(axum::extract::DefaultBodyLimit::max(256 * 1024 * 1024)),
        )
        .route("/v1/projects/{slug}/graph", get(project_graph))
        .route("/v1/projects/{slug}/resume", post(project_resume))
        .route("/v1/projects/{slug}/activities", get(project_activities))
        .route("/v1/projects/{slug}/up-to-speed", post(project_up_to_speed))
        .route("/v1/watchers/heartbeat", post(watcher_heartbeat))
        .route("/v1/watchers/unregister", post(watcher_unregister))
        .route("/v1/watchers/restart-local", post(watcher_restart_local))
        .route("/v1/archive", post(archive))
        .route("/v1/agents", get(agents_snapshot))
        .route("/v1/agents/workspaces", get(list_agent_workspaces))
        .route("/v1/agents/workspaces/start", post(start_agent_workspace))
        .route(
            "/v1/agents/workspaces/{workspace_id}/heartbeat",
            post(heartbeat_agent_workspace),
        )
        .route(
            "/v1/agents/workspaces/{workspace_id}/finish",
            post(finish_agent_workspace),
        )
        .layer(axum::middleware::from_fn_with_state(
            state.clone(),
            crate::auth::read_only_guard,
        ))
        .layer(axum::middleware::from_fn_with_state(
            state.clone(),
            crate::auth::authorization_guard,
        ))
        .layer(axum::middleware::from_fn(
            crate::federation::workflow::private_response,
        ))
        .with_state(state.clone())
        .layer(TraceLayer::new_for_http());

    if mcp_config.enabled && mcp_config.http_enabled {
        app = app.merge(crate::mcp_http::build_mcp_http_router(state));
    }

    if let Some(root) = web_assets {
        let index = root.join("index.html");
        app.fallback_service(ServeDir::new(root).not_found_service(ServeFile::new(index)))
    } else {
        app.fallback(any(web_unavailable))
    }
}
