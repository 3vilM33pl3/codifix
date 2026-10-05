// SPDX-License-Identifier: AGPL-3.0-or-later

use axum::{
    body::{Body, to_bytes},
    extract::{Request, State},
    http::{Method, StatusCode, header},
    middleware::Next,
    response::{IntoResponse, Response},
};
use mem_record::Permission;
use serde_json::Value;
use sqlx::Row;
use uuid::Uuid;

use super::{
    AuthenticatedPrincipal, CSRF_COOKIE_NAME, CookiePolicy, CredentialSource, cookie_value,
    hash_secret, resolve_request_principal,
};
use crate::{ApiError, AppState};

const MAX_AUTH_INSPECTION_BODY: usize = 64 * 1024 * 1024;

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub(crate) enum ProjectScope {
    Public,
    /// Valid credential only; the route narrows identity kind (e.g. device logout).
    Authenticated,
    /// Human OIDC browser session, with ordinary mutation Origin/CSRF checks.
    HumanSession,
    Unscoped,
    Global,
    PathProject,
    BodyProject,
    BodyMemoryResource,
    QueryProjectOrGlobal,
    MemoryResource,
    ValidationResource,
    LoopRunResource,
    LoopApprovalResource,
    LoopProposalResource,
    WorkspaceResource,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub(crate) struct RoutePolicy {
    pub(crate) permission: Permission,
    pub(crate) scope: ProjectScope,
    /// Allowed in read-only (student) mode even though the method mutates:
    /// queries, resume/up-to-speed briefings, and bundle exports are
    /// semantic reads carried over POST.
    pub(crate) semantic_read: bool,
}

/// The single authoritative route -> policy table. Registered together with
/// the axum routes by `PolicyRouter`; a path or method with no entry is
/// DENIED by the authorization guard (fail closed).
pub(crate) fn build_policy_table() -> matchit::Router<Vec<(Method, RoutePolicy)>> {
    let mut table = matchit::Router::new();
    let entries: &[(&str, &[(Method, RoutePolicy)])] = &[
        (
            "/v1/onboarding/cache",
            &[(
                Method::GET,
                RoutePolicy {
                    permission: Permission::MemoryRead,
                    scope: ProjectScope::Authenticated,
                    semantic_read: true,
                },
            )],
        ),
        (
            "/v1/onboarding/cache/{subscription_id}/search",
            &[(
                Method::POST,
                RoutePolicy {
                    permission: Permission::MemoryRead,
                    scope: ProjectScope::Authenticated,
                    semantic_read: true,
                },
            )],
        ),
        (
            "/v1/onboarding/delegated-grants",
            &[(
                Method::POST,
                RoutePolicy {
                    permission: Permission::AuthManage,
                    scope: ProjectScope::Authenticated,
                    semantic_read: false,
                },
            )],
        ),
        (
            "/v1/onboarding/delegated-grants/operations/{operation_id}",
            &[(
                Method::GET,
                RoutePolicy {
                    permission: Permission::AuthManage,
                    scope: ProjectScope::Authenticated,
                    semantic_read: false,
                },
            )],
        ),
        (
            "/v1/admin/onboarding/company/configuration",
            &[(
                Method::GET,
                RoutePolicy {
                    permission: Permission::SystemAdmin,
                    scope: ProjectScope::Global,
                    semantic_read: false,
                },
            )],
        ),
        (
            "/v1/onboarding/company/projects",
            &[(Method::GET,RoutePolicy {permission:Permission::MemoryRead,scope:ProjectScope::HumanSession,semantic_read:true})],
        ),
        (
            "/v1/admin/onboarding/company/operations",
            &[
                (
                    Method::GET,
                    RoutePolicy {
                        permission: Permission::SystemAdmin,
                        scope: ProjectScope::Global,
                        semantic_read: false,
                    },
                ),
                (
                    Method::POST,
                    RoutePolicy {
                        permission: Permission::SystemAdmin,
                        scope: ProjectScope::Global,
                        semantic_read: false,
                    },
                ),
            ],
        ),
        (
            "/v1/admin/onboarding/company/operations/{operation_id}",
            &[(
                Method::GET,
                RoutePolicy {
                    permission: Permission::SystemAdmin,
                    scope: ProjectScope::Global,
                    semantic_read: false,
                },
            )],
        ),
        (
            "/v1/admin/onboarding/company/operations/{operation_id}/reconcile",
            &[(
                Method::POST,
                RoutePolicy {
                    permission: Permission::SystemAdmin,
                    scope: ProjectScope::Global,
                    semantic_read: false,
                },
            )],
        ),
        (
            "/v1/onboarding/device/cancel",
            &[(
                Method::POST,
                RoutePolicy {
                    permission: Permission::MemoryRead,
                    scope: ProjectScope::Public,
                    semantic_read: false,
                },
            )],
        ),
        (
            "/v1/onboarding/device/start",
            &[(
                Method::POST,
                RoutePolicy {
                    permission: Permission::MemoryRead,
                    scope: ProjectScope::Public,
                    semantic_read: false,
                },
            )],
        ),
        (
            "/v1/onboarding/device/poll",
            &[(
                Method::POST,
                RoutePolicy {
                    permission: Permission::MemoryRead,
                    scope: ProjectScope::Public,
                    semantic_read: false,
                },
            )],
        ),
        (
            "/v1/onboarding/device/review",
            &[(
                Method::GET,
                RoutePolicy {
                    permission: Permission::MemoryRead,
                    scope: ProjectScope::HumanSession,
                    semantic_read: false,
                },
            )],
        ),
        (
            "/v1/onboarding/device/approve",
            &[(
                Method::POST,
                RoutePolicy {
                    permission: Permission::MemoryRead,
                    scope: ProjectScope::HumanSession,
                    semantic_read: false,
                },
            )],
        ),
        (
            "/v1/onboarding/device/logout",
            &[(
                Method::POST,
                RoutePolicy {
                    permission: Permission::MemoryRead,
                    scope: ProjectScope::Authenticated,
                    semantic_read: false,
                },
            )],
        ),
        (
            "/v1/onboarding/projects",
            &[(
                Method::GET,
                RoutePolicy {
                    permission: Permission::MemoryRead,
                    scope: ProjectScope::Authenticated,
                    semantic_read: false,
                },
            )],
        ),
        (
            "/v1/onboarding/company-delivery/catalog",
            &[(
                Method::POST,
                RoutePolicy {
                    permission: Permission::ScopedProjectDelivery,
                    scope: ProjectScope::Authenticated,
                    semantic_read: true,
                },
            )],
        ),
        (
            "/v1/onboarding/company-delivery/approve",
            &[(
                Method::POST,
                RoutePolicy {
                    permission: Permission::ScopedProjectDelivery,
                    scope: ProjectScope::Authenticated,
                    semantic_read: false,
                },
            )],
        ),
        (
            "/v1/onboarding/company-delivery/subscription",
            &[(
                Method::POST,
                RoutePolicy {
                    permission: Permission::ScopedProjectDelivery,
                    scope: ProjectScope::Authenticated,
                    semantic_read: true,
                },
            )],
        ),
        (
            "/v1/onboarding/company-delivery/snapshot",
            &[(
                Method::POST,
                RoutePolicy {
                    permission: Permission::ScopedProjectDelivery,
                    scope: ProjectScope::Authenticated,
                    semantic_read: false,
                },
            )],
        ),
        (
            "/v1/admin/onboarding/capabilities",
            &[(
                Method::GET,
                RoutePolicy {
                    permission: Permission::SystemAdmin,
                    scope: ProjectScope::Global,
                    semantic_read: false,
                },
            )],
        ),
        (
            "/v1/admin/onboarding/company/delegates",
            &[
                (
                    Method::GET,
                    RoutePolicy {
                        permission: Permission::SystemAdmin,
                        scope: ProjectScope::Global,
                        semantic_read: false,
                    },
                ),
                (
                    Method::POST,
                    RoutePolicy {
                        permission: Permission::SystemAdmin,
                        scope: ProjectScope::Global,
                        semantic_read: false,
                    },
                ),
            ],
        ),
        (
            "/v1/admin/onboarding/company/delivery-delegates",
            &[
                (
                    Method::GET,
                    RoutePolicy {
                        permission: Permission::SystemAdmin,
                        scope: ProjectScope::Global,
                        semantic_read: false,
                    },
                ),
                (
                    Method::POST,
                    RoutePolicy {
                        permission: Permission::SystemAdmin,
                        scope: ProjectScope::Global,
                        semantic_read: false,
                    },
                ),
            ],
        ),
        (
            "/v1/onboarding/projects/{project_id}/snapshot",
            &[(
                Method::POST,
                RoutePolicy {
                    permission: Permission::BundleExport,
                    scope: ProjectScope::Authenticated,
                    semantic_read: false,
                },
            )],
        ),
        (
            "/v1/onboarding/projects/{project_id}/subscription",
            &[(
                Method::GET,
                RoutePolicy {
                    permission: Permission::BundleExport,
                    scope: ProjectScope::Authenticated,
                    semantic_read: false,
                },
            )],
        ),
        (
            "/v1/admin/onboarding/idp/operations",
            &[(
                Method::GET,
                RoutePolicy {
                    permission: Permission::SystemAdmin,
                    scope: ProjectScope::Global,
                    semantic_read: false,
                },
            )],
        ),
        (
            "/v1/admin/onboarding/idp/operations/{id}",
            &[(
                Method::GET,
                RoutePolicy {
                    permission: Permission::SystemAdmin,
                    scope: ProjectScope::Global,
                    semantic_read: false,
                },
            )],
        ),
        (
            "/v1/admin/onboarding/users",
            &[
                (
                    Method::GET,
                    RoutePolicy {
                        permission: Permission::SystemAdmin,
                        scope: ProjectScope::Global,
                        semantic_read: false,
                    },
                ),
                (
                    Method::POST,
                    RoutePolicy {
                        permission: Permission::SystemAdmin,
                        scope: ProjectScope::Global,
                        semantic_read: false,
                    },
                ),
            ],
        ),
        (
            "/v1/admin/onboarding/users/invite",
            &[(
                Method::POST,
                RoutePolicy {
                    permission: Permission::SystemAdmin,
                    scope: ProjectScope::Global,
                    semantic_read: false,
                },
            )],
        ),
        (
            "/v1/admin/onboarding/users/{id}/disable",
            &[(
                Method::POST,
                RoutePolicy {
                    permission: Permission::SystemAdmin,
                    scope: ProjectScope::Global,
                    semantic_read: false,
                },
            )],
        ),
        (
            "/v1/admin/onboarding/users/{id}/access",
            &[(
                Method::GET,
                RoutePolicy {
                    permission: Permission::SystemAdmin,
                    scope: ProjectScope::Global,
                    semantic_read: false,
                },
            )],
        ),
        (
            "/v1/admin/onboarding/projects",
            &[(
                Method::GET,
                RoutePolicy {
                    permission: Permission::SystemAdmin,
                    scope: ProjectScope::Global,
                    semantic_read: false,
                },
            )],
        ),
        (
            "/v1/admin/onboarding/projects/{project_id}",
            &[(
                Method::POST,
                RoutePolicy {
                    permission: Permission::SystemAdmin,
                    scope: ProjectScope::Global,
                    semantic_read: false,
                },
            )],
        ),
        (
            "/v1/admin/onboarding/devices",
            &[(
                Method::GET,
                RoutePolicy {
                    permission: Permission::SystemAdmin,
                    scope: ProjectScope::Global,
                    semantic_read: false,
                },
            )],
        ),
        (
            "/v1/admin/onboarding/devices/{id}/revoke",
            &[(
                Method::POST,
                RoutePolicy {
                    permission: Permission::SystemAdmin,
                    scope: ProjectScope::Global,
                    semantic_read: false,
                },
            )],
        ),
        (
            "/v1/admin/onboarding/idp/users",
            &[(
                Method::POST,
                RoutePolicy {
                    permission: Permission::SystemAdmin,
                    scope: ProjectScope::Global,
                    semantic_read: false,
                },
            )],
        ),
        (
            "/v1/admin/onboarding/idp/users/{provider_id}/disable",
            &[(
                Method::POST,
                RoutePolicy {
                    permission: Permission::SystemAdmin,
                    scope: ProjectScope::Global,
                    semantic_read: false,
                },
            )],
        ),
        (
            "/v1/projects/{slug}/supervisor/questions/prepare",
            &[(
                Method::POST,
                RoutePolicy {
                    permission: Permission::LoopsRun,
                    scope: ProjectScope::PathProject,
                    semantic_read: false,
                },
            )],
        ),
        (
            "/v1/projects/{slug}/supervisor/questions",
            &[
                (
                    Method::GET,
                    RoutePolicy {
                        permission: Permission::MemoryRead,
                        scope: ProjectScope::PathProject,
                        semantic_read: false,
                    },
                ),
                (
                    Method::POST,
                    RoutePolicy {
                        permission: Permission::LoopsRun,
                        scope: ProjectScope::PathProject,
                        semantic_read: false,
                    },
                ),
            ],
        ),
        (
            "/v1/projects/{slug}/supervisor/questions/{id}",
            &[(
                Method::GET,
                RoutePolicy {
                    permission: Permission::MemoryRead,
                    scope: ProjectScope::PathProject,
                    semantic_read: false,
                },
            )],
        ),
        (
            "/v1/projects/{slug}/supervisor/proposals",
            &[(
                Method::GET,
                RoutePolicy {
                    permission: Permission::MemoryRead,
                    scope: ProjectScope::PathProject,
                    semantic_read: false,
                },
            )],
        ),
        (
            "/v1/projects/{slug}/supervisor/proposals/{id}",
            &[(
                Method::GET,
                RoutePolicy {
                    permission: Permission::MemoryRead,
                    scope: ProjectScope::PathProject,
                    semantic_read: false,
                },
            )],
        ),
        (
            "/v1/projects/{slug}/supervisor/proposals/{id}/review",
            &[(
                Method::POST,
                RoutePolicy {
                    permission: Permission::LoopsRun,
                    scope: ProjectScope::PathProject,
                    semantic_read: false,
                },
            )],
        ),
        (
            "/v1/supervisor/policy",
            &[(
                Method::POST,
                RoutePolicy {
                    permission: Permission::SystemAdmin,
                    scope: ProjectScope::Global,
                    semantic_read: false,
                },
            )],
        ),
        (
            "/v1/supervisor",
            &[(
                Method::GET,
                RoutePolicy {
                    permission: Permission::MemoryRead,
                    scope: ProjectScope::Global,
                    semantic_read: false,
                },
            )],
        ),
        (
            "/v1/projects/{slug}/jobs",
            &[(
                Method::GET,
                RoutePolicy {
                    permission: Permission::MemoryRead,
                    scope: ProjectScope::PathProject,
                    semantic_read: false,
                },
            )],
        ),
        (
            "/v1/projects/{slug}/jobs/{id}",
            &[(
                Method::GET,
                RoutePolicy {
                    permission: Permission::MemoryRead,
                    scope: ProjectScope::PathProject,
                    semantic_read: false,
                },
            )],
        ),
        (
            "/v1/projects/{slug}/jobs/{id}/retry",
            &[(
                Method::POST,
                RoutePolicy {
                    permission: Permission::LoopsRun,
                    scope: ProjectScope::PathProject,
                    semantic_read: false,
                },
            )],
        ),
        (
            "/healthz",
            &[(
                Method::GET,
                RoutePolicy {
                    permission: Permission::MemoryRead,
                    scope: ProjectScope::Public,
                    semantic_read: false,
                },
            )],
        ),
        (
            "/v1/openapi.yaml",
            &[(
                Method::GET,
                RoutePolicy {
                    permission: Permission::MemoryRead,
                    scope: ProjectScope::Public,
                    semantic_read: false,
                },
            )],
        ),
        (
            "/ws",
            &[(
                Method::GET,
                RoutePolicy {
                    permission: Permission::MemoryRead,
                    scope: ProjectScope::Unscoped,
                    semantic_read: false,
                },
            )],
        ),
        (
            "/v1/auth/session/bootstrap",
            &[(
                Method::POST,
                RoutePolicy {
                    permission: Permission::MemoryRead,
                    scope: ProjectScope::Public,
                    semantic_read: true,
                },
            )],
        ),
        (
            "/v1/auth/login",
            &[(
                Method::GET,
                RoutePolicy {
                    permission: Permission::MemoryRead,
                    scope: ProjectScope::Public,
                    semantic_read: false,
                },
            )],
        ),
        (
            "/v1/auth/callback",
            &[(
                Method::GET,
                RoutePolicy {
                    permission: Permission::MemoryRead,
                    scope: ProjectScope::Public,
                    semantic_read: false,
                },
            )],
        ),
        (
            "/v1/auth/me",
            &[(
                Method::GET,
                RoutePolicy {
                    permission: Permission::MemoryRead,
                    scope: ProjectScope::Unscoped,
                    semantic_read: false,
                },
            )],
        ),
        (
            "/v1/auth/logout",
            &[(
                Method::POST,
                RoutePolicy {
                    permission: Permission::MemoryRead,
                    scope: ProjectScope::Unscoped,
                    semantic_read: false,
                },
            )],
        ),
        (
            "/v1/auth/tokens",
            &[
                (
                    Method::GET,
                    RoutePolicy {
                        permission: Permission::AuthManage,
                        scope: ProjectScope::Global,
                        semantic_read: false,
                    },
                ),
                (
                    Method::POST,
                    RoutePolicy {
                        permission: Permission::AuthManage,
                        scope: ProjectScope::Global,
                        semantic_read: false,
                    },
                ),
            ],
        ),
        (
            "/v1/auth/tokens/{p}/revoke",
            &[(
                Method::POST,
                RoutePolicy {
                    permission: Permission::AuthManage,
                    scope: ProjectScope::Global,
                    semantic_read: false,
                },
            )],
        ),
        (
            "/v1/auth/principals",
            &[(
                Method::GET,
                RoutePolicy {
                    permission: Permission::AuthManage,
                    scope: ProjectScope::Global,
                    semantic_read: false,
                },
            )],
        ),
        (
            "/v1/auth/expertise-answerers",
            &[(
                Method::POST,
                RoutePolicy {
                    permission: Permission::SystemAdmin,
                    scope: ProjectScope::Global,
                    semantic_read: false,
                },
            )],
        ),
        (
            "/v1/auth/memberships",
            &[
                (
                    Method::GET,
                    RoutePolicy {
                        permission: Permission::AuthManage,
                        scope: ProjectScope::Global,
                        semantic_read: false,
                    },
                ),
                (
                    Method::POST,
                    RoutePolicy {
                        permission: Permission::AuthManage,
                        scope: ProjectScope::Global,
                        semantic_read: false,
                    },
                ),
            ],
        ),
        (
            "/v1/auth/memberships/{p}",
            &[(
                Method::DELETE,
                RoutePolicy {
                    permission: Permission::AuthManage,
                    scope: ProjectScope::Global,
                    semantic_read: false,
                },
            )],
        ),
        (
            "/v1/admin/shutdown",
            &[(
                Method::POST,
                RoutePolicy {
                    permission: Permission::SystemAdmin,
                    scope: ProjectScope::Global,
                    semantic_read: false,
                },
            )],
        ),
        (
            "/v1/runtime/status",
            &[(
                Method::GET,
                RoutePolicy {
                    permission: Permission::MemoryRead,
                    scope: ProjectScope::QueryProjectOrGlobal,
                    semantic_read: false,
                },
            )],
        ),
        (
            "/v1/skills",
            &[(
                Method::GET,
                RoutePolicy {
                    permission: Permission::MemoryRead,
                    scope: ProjectScope::Unscoped,
                    semantic_read: false,
                },
            )],
        ),
        (
            "/v1/skills/repair",
            &[(
                Method::POST,
                RoutePolicy {
                    permission: Permission::SystemAdmin,
                    scope: ProjectScope::Global,
                    semantic_read: false,
                },
            )],
        ),
        (
            "/v1/skills/{p}",
            &[(
                Method::GET,
                RoutePolicy {
                    permission: Permission::MemoryRead,
                    scope: ProjectScope::Unscoped,
                    semantic_read: false,
                },
            )],
        ),
        (
            "/v1/query",
            &[(
                Method::POST,
                RoutePolicy {
                    permission: Permission::MemoryRead,
                    scope: ProjectScope::BodyProject,
                    semantic_read: true,
                },
            )],
        ),
        (
            "/v1/query/global",
            &[(
                Method::POST,
                RoutePolicy {
                    permission: Permission::MemoryRead,
                    scope: ProjectScope::Unscoped,
                    semantic_read: true,
                },
            )],
        ),
        (
            "/v1/activity",
            &[(
                Method::POST,
                RoutePolicy {
                    permission: Permission::ActivityCapture,
                    scope: ProjectScope::BodyProject,
                    semantic_read: false,
                },
            )],
        ),
        (
            "/v1/commits/sync",
            &[(
                Method::POST,
                RoutePolicy {
                    permission: Permission::ActivityCapture,
                    scope: ProjectScope::BodyProject,
                    semantic_read: false,
                },
            )],
        ),
        (
            "/v1/capture/task",
            &[(
                Method::POST,
                RoutePolicy {
                    permission: Permission::ActivityCapture,
                    scope: ProjectScope::BodyProject,
                    semantic_read: false,
                },
            )],
        ),
        (
            "/v1/curate",
            &[(
                Method::POST,
                RoutePolicy {
                    permission: Permission::MemoryCurate,
                    scope: ProjectScope::BodyProject,
                    semantic_read: false,
                },
            )],
        ),
        (
            "/v1/provenance/verify",
            &[(
                Method::POST,
                RoutePolicy {
                    permission: Permission::MemoryCurate,
                    scope: ProjectScope::BodyProject,
                    semantic_read: false,
                },
            )],
        ),
        (
            "/v1/reindex",
            &[(
                Method::POST,
                RoutePolicy {
                    permission: Permission::MemoryCurate,
                    scope: ProjectScope::BodyProject,
                    semantic_read: false,
                },
            )],
        ),
        (
            "/v1/reembed",
            &[(
                Method::POST,
                RoutePolicy {
                    permission: Permission::MemoryCurate,
                    scope: ProjectScope::BodyProject,
                    semantic_read: false,
                },
            )],
        ),
        (
            "/v1/prune-embeddings",
            &[(
                Method::POST,
                RoutePolicy {
                    permission: Permission::MemoryCurate,
                    scope: ProjectScope::BodyProject,
                    semantic_read: false,
                },
            )],
        ),
        (
            "/v1/embeddings/backends",
            &[(
                Method::GET,
                RoutePolicy {
                    permission: Permission::MemoryRead,
                    scope: ProjectScope::Unscoped,
                    semantic_read: false,
                },
            )],
        ),
        (
            "/v1/embeddings/activate",
            &[(
                Method::POST,
                RoutePolicy {
                    permission: Permission::EmbeddingsManage,
                    scope: ProjectScope::Global,
                    semantic_read: false,
                },
            )],
        ),
        (
            "/v1/embeddings/deactivate",
            &[(
                Method::POST,
                RoutePolicy {
                    permission: Permission::EmbeddingsManage,
                    scope: ProjectScope::Global,
                    semantic_read: false,
                },
            )],
        ),
        (
            "/v1/embeddings/create-enabled",
            &[(
                Method::POST,
                RoutePolicy {
                    permission: Permission::EmbeddingsManage,
                    scope: ProjectScope::Global,
                    semantic_read: false,
                },
            )],
        ),
        (
            "/v1/config/llm-audit",
            &[
                (
                    Method::GET,
                    RoutePolicy {
                        permission: Permission::MemoryRead,
                        scope: ProjectScope::Unscoped,
                        semantic_read: false,
                    },
                ),
                (
                    Method::POST,
                    RoutePolicy {
                        permission: Permission::SystemAdmin,
                        scope: ProjectScope::Global,
                        semantic_read: false,
                    },
                ),
            ],
        ),
        (
            "/v1/loops",
            &[(
                Method::GET,
                RoutePolicy {
                    permission: Permission::MemoryRead,
                    scope: ProjectScope::Unscoped,
                    semantic_read: false,
                },
            )],
        ),
        (
            "/v1/loops/global-kill-switch",
            &[
                (
                    Method::GET,
                    RoutePolicy {
                        permission: Permission::MemoryRead,
                        scope: ProjectScope::Unscoped,
                        semantic_read: false,
                    },
                ),
                (
                    Method::POST,
                    RoutePolicy {
                        permission: Permission::LoopsConfigure,
                        scope: ProjectScope::Global,
                        semantic_read: false,
                    },
                ),
            ],
        ),
        (
            "/v1/loops/runs",
            &[(
                Method::GET,
                RoutePolicy {
                    permission: Permission::MemoryRead,
                    scope: ProjectScope::QueryProjectOrGlobal,
                    semantic_read: false,
                },
            )],
        ),
        (
            "/v1/loops/runs/{p}",
            &[(
                Method::GET,
                RoutePolicy {
                    permission: Permission::MemoryRead,
                    scope: ProjectScope::LoopRunResource,
                    semantic_read: false,
                },
            )],
        ),
        (
            "/v1/loops/runs/{p}/context-pack",
            &[(
                Method::GET,
                RoutePolicy {
                    permission: Permission::MemoryRead,
                    scope: ProjectScope::LoopRunResource,
                    semantic_read: false,
                },
            )],
        ),
        (
            "/v1/loops/runs/{p}/cancel",
            &[(
                Method::POST,
                RoutePolicy {
                    permission: Permission::LoopsRun,
                    scope: ProjectScope::LoopRunResource,
                    semantic_read: false,
                },
            )],
        ),
        (
            "/v1/loops/runs/{p}/feedback",
            &[(
                Method::POST,
                RoutePolicy {
                    permission: Permission::LoopsRun,
                    scope: ProjectScope::LoopRunResource,
                    semantic_read: false,
                },
            )],
        ),
        (
            "/v1/loops/approvals",
            &[(
                Method::GET,
                RoutePolicy {
                    permission: Permission::MemoryRead,
                    scope: ProjectScope::QueryProjectOrGlobal,
                    semantic_read: false,
                },
            )],
        ),
        (
            "/v1/loops/memory-proposals",
            &[
                (
                    Method::GET,
                    RoutePolicy {
                        permission: Permission::MemoryRead,
                        scope: ProjectScope::QueryProjectOrGlobal,
                        semantic_read: false,
                    },
                ),
                (
                    Method::POST,
                    RoutePolicy {
                        permission: Permission::LoopsRun,
                        scope: ProjectScope::QueryProjectOrGlobal,
                        semantic_read: false,
                    },
                ),
            ],
        ),
        (
            "/v1/loops/triggers/route",
            &[(
                Method::POST,
                RoutePolicy {
                    permission: Permission::LoopsRun,
                    scope: ProjectScope::BodyProject,
                    semantic_read: false,
                },
            )],
        ),
        (
            "/v1/loops/approvals/{p}/approve",
            &[(
                Method::POST,
                RoutePolicy {
                    permission: Permission::LoopsRun,
                    scope: ProjectScope::LoopApprovalResource,
                    semantic_read: false,
                },
            )],
        ),
        (
            "/v1/loops/approvals/{p}/reject",
            &[(
                Method::POST,
                RoutePolicy {
                    permission: Permission::LoopsRun,
                    scope: ProjectScope::LoopApprovalResource,
                    semantic_read: false,
                },
            )],
        ),
        (
            "/v1/loops/approvals/{p}/edit",
            &[(
                Method::POST,
                RoutePolicy {
                    permission: Permission::LoopsRun,
                    scope: ProjectScope::LoopApprovalResource,
                    semantic_read: false,
                },
            )],
        ),
        (
            "/v1/loops/memory-proposals/{p}/approve",
            &[(
                Method::POST,
                RoutePolicy {
                    permission: Permission::LoopsRun,
                    scope: ProjectScope::LoopProposalResource,
                    semantic_read: false,
                },
            )],
        ),
        (
            "/v1/loops/memory-proposals/{p}/reject",
            &[(
                Method::POST,
                RoutePolicy {
                    permission: Permission::LoopsRun,
                    scope: ProjectScope::LoopProposalResource,
                    semantic_read: false,
                },
            )],
        ),
        (
            "/v1/loops/memory-proposals/{p}/edit",
            &[(
                Method::POST,
                RoutePolicy {
                    permission: Permission::LoopsRun,
                    scope: ProjectScope::LoopProposalResource,
                    semantic_read: false,
                },
            )],
        ),
        (
            "/v1/loops/{p}",
            &[(
                Method::GET,
                RoutePolicy {
                    permission: Permission::MemoryRead,
                    scope: ProjectScope::QueryProjectOrGlobal,
                    semantic_read: false,
                },
            )],
        ),
        (
            "/v1/loops/{p}/settings",
            &[(
                Method::POST,
                RoutePolicy {
                    permission: Permission::LoopsConfigure,
                    scope: ProjectScope::BodyProject,
                    semantic_read: false,
                },
            )],
        ),
        (
            "/v1/loops/{p}/run",
            &[(
                Method::POST,
                RoutePolicy {
                    permission: Permission::LoopsRun,
                    scope: ProjectScope::BodyProject,
                    semantic_read: false,
                },
            )],
        ),
        (
            "/v1/loops/{p}/context-pack",
            &[(
                Method::GET,
                RoutePolicy {
                    permission: Permission::MemoryRead,
                    scope: ProjectScope::QueryProjectOrGlobal,
                    semantic_read: false,
                },
            )],
        ),
        (
            "/v1/memory/{p}",
            &[(
                Method::GET,
                RoutePolicy {
                    permission: Permission::MemoryRead,
                    scope: ProjectScope::MemoryResource,
                    semantic_read: false,
                },
            )],
        ),
        (
            "/v1/memory/{p}/validate",
            &[(
                Method::POST,
                RoutePolicy {
                    permission: Permission::MemoryCurate,
                    scope: ProjectScope::MemoryResource,
                    semantic_read: false,
                },
            )],
        ),
        (
            "/v1/validation-runs/{p}/review",
            &[(
                Method::POST,
                RoutePolicy {
                    permission: Permission::MemoryCurate,
                    scope: ProjectScope::ValidationResource,
                    semantic_read: false,
                },
            )],
        ),
        (
            "/v1/memory/{p}/archive",
            &[(
                Method::POST,
                RoutePolicy {
                    permission: Permission::MemoryCurate,
                    scope: ProjectScope::MemoryResource,
                    semantic_read: false,
                },
            )],
        ),
        (
            "/v1/memory/{p}/history",
            &[(
                Method::GET,
                RoutePolicy {
                    permission: Permission::MemoryRead,
                    scope: ProjectScope::MemoryResource,
                    semantic_read: false,
                },
            )],
        ),
        (
            "/v1/memory",
            &[(
                Method::DELETE,
                RoutePolicy {
                    permission: Permission::MemoryDelete,
                    scope: ProjectScope::BodyMemoryResource,
                    semantic_read: false,
                },
            )],
        ),
        (
            "/v1/prune-history",
            &[(
                Method::POST,
                RoutePolicy {
                    permission: Permission::MemoryCurate,
                    scope: ProjectScope::BodyProject,
                    semantic_read: false,
                },
            )],
        ),
        (
            "/v1/projects/{p}/memory-scores",
            &[(
                Method::GET,
                RoutePolicy {
                    permission: Permission::MemoryRead,
                    scope: ProjectScope::PathProject,
                    semantic_read: false,
                },
            )],
        ),
        (
            "/v1/projects/{p}/structure",
            &[(
                Method::GET,
                RoutePolicy {
                    permission: Permission::MemoryRead,
                    scope: ProjectScope::PathProject,
                    semantic_read: false,
                },
            )],
        ),
        (
            "/v1/projects/{p}/validation-runs",
            &[(
                Method::GET,
                RoutePolicy {
                    permission: Permission::MemoryRead,
                    scope: ProjectScope::PathProject,
                    semantic_read: false,
                },
            )],
        ),
        (
            "/v1/projects/{p}/commits",
            &[(
                Method::GET,
                RoutePolicy {
                    permission: Permission::MemoryRead,
                    scope: ProjectScope::PathProject,
                    semantic_read: false,
                },
            )],
        ),
        (
            "/v1/projects/{p}/commits/{p}",
            &[(
                Method::GET,
                RoutePolicy {
                    permission: Permission::MemoryRead,
                    scope: ProjectScope::PathProject,
                    semantic_read: false,
                },
            )],
        ),
        (
            "/v1/projects/{p}/bundle/export/preview",
            &[(
                Method::POST,
                RoutePolicy {
                    permission: Permission::BundleExport,
                    scope: ProjectScope::PathProject,
                    semantic_read: true,
                },
            )],
        ),
        (
            "/v1/projects/{p}/bundle/export",
            &[(
                Method::POST,
                RoutePolicy {
                    permission: Permission::BundleExport,
                    scope: ProjectScope::PathProject,
                    semantic_read: true,
                },
            )],
        ),
        (
            "/v1/projects/{p}/bundle/import/preview",
            &[(
                Method::POST,
                RoutePolicy {
                    permission: Permission::BundleImport,
                    scope: ProjectScope::PathProject,
                    semantic_read: false,
                },
            )],
        ),
        (
            "/v1/projects/{p}/bundle/import",
            &[(
                Method::POST,
                RoutePolicy {
                    permission: Permission::BundleImport,
                    scope: ProjectScope::PathProject,
                    semantic_read: false,
                },
            )],
        ),
        (
            "/v1/projects/{p}/replacement-proposals",
            &[(
                Method::GET,
                RoutePolicy {
                    permission: Permission::MemoryRead,
                    scope: ProjectScope::PathProject,
                    semantic_read: false,
                },
            )],
        ),
        (
            "/v1/projects/{p}/replacement-proposals/{p}/approve",
            &[(
                Method::POST,
                RoutePolicy {
                    permission: Permission::MemoryCurate,
                    scope: ProjectScope::PathProject,
                    semantic_read: false,
                },
            )],
        ),
        (
            "/v1/projects/{p}/replacement-proposals/{p}/reject",
            &[(
                Method::POST,
                RoutePolicy {
                    permission: Permission::MemoryCurate,
                    scope: ProjectScope::PathProject,
                    semantic_read: false,
                },
            )],
        ),
        (
            "/v1/projects/{p}/replacement-policy",
            &[
                (
                    Method::GET,
                    RoutePolicy {
                        permission: Permission::MemoryRead,
                        scope: ProjectScope::PathProject,
                        semantic_read: false,
                    },
                ),
                (
                    Method::POST,
                    RoutePolicy {
                        permission: Permission::MemoryCurate,
                        scope: ProjectScope::PathProject,
                        semantic_read: false,
                    },
                ),
                (
                    Method::PUT,
                    RoutePolicy {
                        permission: Permission::MemoryCurate,
                        scope: ProjectScope::PathProject,
                        semantic_read: false,
                    },
                ),
            ],
        ),
        (
            "/v1/projects/{p}/memories",
            &[(
                Method::GET,
                RoutePolicy {
                    permission: Permission::MemoryRead,
                    scope: ProjectScope::PathProject,
                    semantic_read: false,
                },
            )],
        ),
        (
            "/v1/projects/{p}/memory-graph",
            &[(
                Method::GET,
                RoutePolicy {
                    permission: Permission::MemoryRead,
                    scope: ProjectScope::PathProject,
                    semantic_read: false,
                },
            )],
        ),
        (
            "/v1/projects/{p}/overview",
            &[(
                Method::GET,
                RoutePolicy {
                    permission: Permission::MemoryRead,
                    scope: ProjectScope::PathProject,
                    semantic_read: false,
                },
            )],
        ),
        (
            "/v1/projects/{p}/graph/status",
            &[(
                Method::GET,
                RoutePolicy {
                    permission: Permission::MemoryRead,
                    scope: ProjectScope::PathProject,
                    semantic_read: false,
                },
            )],
        ),
        (
            "/v1/projects/{p}/graph/extract",
            &[(
                Method::POST,
                RoutePolicy {
                    permission: Permission::MemoryCurate,
                    scope: ProjectScope::PathProject,
                    semantic_read: false,
                },
            )],
        ),
        (
            "/v1/projects/{p}/graph",
            &[(
                Method::GET,
                RoutePolicy {
                    permission: Permission::MemoryRead,
                    scope: ProjectScope::PathProject,
                    semantic_read: false,
                },
            )],
        ),
        (
            "/v1/projects/{p}/resume",
            &[(
                Method::POST,
                RoutePolicy {
                    permission: Permission::MemoryRead,
                    scope: ProjectScope::PathProject,
                    semantic_read: true,
                },
            )],
        ),
        (
            "/v1/projects/{p}/activities",
            &[(
                Method::GET,
                RoutePolicy {
                    permission: Permission::MemoryRead,
                    scope: ProjectScope::PathProject,
                    semantic_read: false,
                },
            )],
        ),
        (
            "/v1/projects/{p}/up-to-speed",
            &[(
                Method::POST,
                RoutePolicy {
                    permission: Permission::MemoryRead,
                    scope: ProjectScope::PathProject,
                    semantic_read: true,
                },
            )],
        ),
        (
            "/v1/watchers/heartbeat",
            &[(
                Method::POST,
                RoutePolicy {
                    permission: Permission::ActivityCapture,
                    scope: ProjectScope::BodyProject,
                    semantic_read: false,
                },
            )],
        ),
        (
            "/v1/watchers/unregister",
            &[(
                Method::POST,
                RoutePolicy {
                    permission: Permission::ActivityCapture,
                    scope: ProjectScope::BodyProject,
                    semantic_read: false,
                },
            )],
        ),
        (
            "/v1/watchers/restart-local",
            &[(
                Method::POST,
                RoutePolicy {
                    permission: Permission::ActivityCapture,
                    scope: ProjectScope::BodyProject,
                    semantic_read: false,
                },
            )],
        ),
        (
            "/v1/archive",
            &[(
                Method::POST,
                RoutePolicy {
                    permission: Permission::MemoryCurate,
                    scope: ProjectScope::BodyProject,
                    semantic_read: false,
                },
            )],
        ),
        (
            "/v1/agents",
            &[(
                Method::GET,
                RoutePolicy {
                    permission: Permission::SystemAdmin,
                    scope: ProjectScope::Global,
                    semantic_read: false,
                },
            )],
        ),
        (
            "/v1/agents/workspaces",
            &[(
                Method::GET,
                RoutePolicy {
                    permission: Permission::SystemAdmin,
                    scope: ProjectScope::Global,
                    semantic_read: false,
                },
            )],
        ),
        (
            "/v1/agents/workspaces/start",
            &[(
                Method::POST,
                RoutePolicy {
                    permission: Permission::ActivityCapture,
                    scope: ProjectScope::WorkspaceResource,
                    semantic_read: false,
                },
            )],
        ),
        (
            "/v1/agents/workspaces/{p}/heartbeat",
            &[(
                Method::POST,
                RoutePolicy {
                    permission: Permission::ActivityCapture,
                    scope: ProjectScope::WorkspaceResource,
                    semantic_read: false,
                },
            )],
        ),
        (
            "/v1/agents/workspaces/{p}/finish",
            &[(
                Method::POST,
                RoutePolicy {
                    permission: Permission::ActivityCapture,
                    scope: ProjectScope::WorkspaceResource,
                    semantic_read: false,
                },
            )],
        ),
    ];
    for (path, policies) in entries {
        table
            .insert(path.to_string(), policies.to_vec())
            .expect("route policy table entries are unique");
    }
    for (path, method, permission, semantic_read) in [
        (
            "/v1/projects/{slug}/federation",
            Method::GET,
            Permission::MemoryRead,
            false,
        ),
        (
            "/v1/projects/{slug}/federation/{space}/catalog",
            Method::GET,
            Permission::MemoryRead,
            false,
        ),
        (
            "/v1/projects/{slug}/federation/{space}/proposals/{publisher}/{record}",
            Method::GET,
            Permission::MemoryRead,
            false,
        ),
        (
            "/v1/projects/{slug}/federation/{space}/review-preview",
            Method::POST,
            Permission::BundleExport,
            true,
        ),
        (
            "/v1/projects/{slug}/federation/{space}/questions/prepare",
            Method::POST,
            Permission::BundleExport,
            true,
        ),
        (
            "/v1/projects/{slug}/federation/{space}/collections/draft",
            Method::POST,
            Permission::MemoryCurate,
            true,
        ),
        (
            "/v1/projects/{slug}/federation/{space}/collections",
            Method::GET,
            Permission::MemoryCurate,
            true,
        ),
        (
            "/v1/projects/{slug}/federation/{space}/collections/{collection}",
            Method::GET,
            Permission::MemoryCurate,
            true,
        ),
        (
            "/v1/projects/{slug}/federation/{space}/collections/activate",
            Method::POST,
            Permission::MemoryCurate,
            false,
        ),
        (
            "/v1/projects/{slug}/federation/{space}/collections/withdraw",
            Method::POST,
            Permission::MemoryCurate,
            false,
        ),
        (
            "/v1/projects/{slug}/federation/{space}/capabilities",
            Method::GET,
            Permission::BundleExport,
            true,
        ),
        (
            "/v1/projects/{slug}/federation/{space}/capabilities/{publisher}/{record}",
            Method::GET,
            Permission::BundleExport,
            true,
        ),
        (
            "/v1/projects/{slug}/federation/{space}/questions/{id}",
            Method::GET,
            Permission::BundleExport,
            true,
        ),
        (
            "/v1/projects/{slug}/federation/{space}/questions/submit",
            Method::POST,
            Permission::BundleExport,
            false,
        ),
        (
            "/v1/projects/{slug}/federation/{space}/agent-questions/{id}",
            Method::GET,
            Permission::BundleExport,
            true,
        ),
        (
            "/v1/projects/{slug}/federation/{space}/actions",
            Method::POST,
            Permission::MemoryRead,
            false,
        ),
        (
            "/v1/projects/{slug}/federation/{space}/sync",
            Method::POST,
            Permission::MemoryRead,
            false,
        ),
        (
            "/v1/projects/{slug}/federation/{space}",
            Method::GET,
            Permission::MemoryRead,
            false,
        ),
        (
            "/v1/projects/{slug}/federation/{space}/grants",
            Method::POST,
            Permission::AuthManage,
            false,
        ),
        (
            "/v1/projects/{slug}/federation/{space}/memories/{canonical}/preview",
            Method::GET,
            Permission::BundleExport,
            false,
        ),
        (
            "/v1/projects/{slug}/federation/{space}/reviews",
            Method::POST,
            Permission::MemoryCurate,
            false,
        ),
        (
            "/v1/projects/{slug}/federation/{space}/context",
            Method::POST,
            Permission::MemoryRead,
            true,
        ),
    ] {
        table
            .insert(
                path.to_owned(),
                vec![(
                    method,
                    RoutePolicy {
                        permission,
                        scope: ProjectScope::PathProject,
                        semantic_read,
                    },
                )],
            )
            .expect("unique federation route policy");
    }
    let agent_path = "/v1/projects/{slug}/federation/{space}/agent-questions".to_owned();
    // matchit requires both methods under one path registration.
    let post_policy = RoutePolicy {
        permission: Permission::BundleExport,
        scope: ProjectScope::PathProject,
        semantic_read: false,
    };
    let get_policy = RoutePolicy {
        permission: Permission::BundleExport,
        scope: ProjectScope::PathProject,
        semantic_read: true,
    };
    table
        .insert(
            agent_path,
            vec![(Method::POST, post_policy), (Method::GET, get_policy)],
        )
        .expect("unique watched-agent route policy");
    table
        .insert(
            "/v1/projects/{slug}/federation/{space}/collections/{collection}/grants".to_owned(),
            vec![
                (
                    Method::GET,
                    RoutePolicy {
                        permission: Permission::AuthManage,
                        scope: ProjectScope::PathProject,
                        semantic_read: true,
                    },
                ),
                (
                    Method::POST,
                    RoutePolicy {
                        permission: Permission::AuthManage,
                        scope: ProjectScope::PathProject,
                        semantic_read: false,
                    },
                ),
            ],
        )
        .expect("unique collection grant route policy");
    table
}

fn policy_table() -> &'static matchit::Router<Vec<(Method, RoutePolicy)>> {
    static TABLE: std::sync::OnceLock<matchit::Router<Vec<(Method, RoutePolicy)>>> =
        std::sync::OnceLock::new();
    TABLE.get_or_init(build_policy_table)
}

/// Policy for a concrete request path and method. `None` means the request
/// has no registered policy and MUST be denied.
pub(crate) fn policy_for(method: &Method, path: &str) -> Option<RoutePolicy> {
    let matched = policy_table().at(path).ok()?;
    matched
        .value
        .iter()
        .find(|(m, _)| m == method)
        .map(|(_, policy)| *policy)
}

#[axum::debug_middleware]
pub(crate) async fn authorization_guard(
    State(state): State<AppState>,
    mut request: Request,
    next: Next,
) -> Response {
    if !state.is_primary() {
        if request.uri().path() == "/ws" {
            return next.run(request).await;
        }
        if request.uri().path().starts_with("/v1/") {
            return match proxy_relay_request(&state, request).await {
                Ok(response) => response,
                Err(error) => error.into_response(),
            };
        }
    }

    let Some(policy) = policy_for(request.method(), request.uri().path()) else {
        // Fail closed: a route without an explicit policy entry is denied.
        return ApiError::forbidden("no authorization policy registered for this route")
            .into_response();
    };
    if policy.scope == ProjectScope::Public {
        let onboarding = request.uri().path().starts_with("/v1/onboarding/");
        let mut response = next.run(request).await;
        if onboarding {
            response.headers_mut().insert(
                header::CACHE_CONTROL,
                axum::http::HeaderValue::from_static("no-store"),
            );
        }
        return response;
    }

    let request_id = request
        .headers()
        .get("x-request-id")
        .and_then(|value| value.to_str().ok())
        .map(str::to_owned);
    let resolved = resolve_request_principal(&state, request.headers(), CookiePolicy::Allow).await;
    let resolved = if resolved.is_err()
        && request.method() == Method::POST
        && request.uri().path() == "/v1/onboarding/device/logout"
    {
        super::device::logout_proof(&state, request.headers()).await
    } else {
        resolved
    };
    let principal = match resolved {
        Ok(Some(principal)) => principal,
        Ok(None) => {
            crate::auth::record_denied_audit(
                &state,
                None,
                "denied",
                request.method().as_str(),
                request.uri().path(),
                "authentication required",
                request_id,
            );
            return ApiError::unauthorized("authentication required").into_response();
        }
        Err(error) => {
            let error = if matches!(
                request.uri().path(),
                "/v1/supervisor" | "/v1/supervisor/policy"
            ) {
                if error.status.is_server_error() {
                    ApiError::service_unavailable("supervisor authentication unavailable")
                } else {
                    ApiError::unauthorized("valid supervisor authentication is required")
                }
            } else {
                error
            };
            crate::auth::record_denied_audit(
                &state,
                None,
                "error",
                request.method().as_str(),
                request.uri().path(),
                &error.message,
                request_id,
            );
            return error.into_response();
        }
    };

    if principal.credential_source == CredentialSource::BrowserSession
        && is_mutating(request.method())
        && let Err(error) = validate_browser_mutation(&state, request.headers(), &principal)
    {
        return error.into_response();
    }

    let project = match resolve_request_project(&state, &mut request, policy.scope).await {
        Ok(project) => project,
        Err(error) => return error.into_response(),
    };
    let authorized = is_authorized(&principal, policy, project.as_deref());
    if !authorized {
        crate::auth::record_denied_audit(
            &state,
            Some(principal.id),
            "denied",
            request.method().as_str(),
            request.uri().path(),
            "missing required permission",
            request_id,
        );
        return ApiError::forbidden(
            "principal does not have the required permission for this resource",
        )
        .into_response();
    }

    request.extensions_mut().insert(principal);
    let onboarding = request.uri().path().starts_with("/v1/admin/onboarding/")
        || request.uri().path().starts_with("/v1/onboarding/");
    let mut response = next.run(request).await;
    if onboarding {
        response.headers_mut().insert(
            header::CACHE_CONTROL,
            axum::http::HeaderValue::from_static("no-store"),
        );
    }
    response
}

/// Pure authorization decision for a resolved (policy, project) pair.
/// Project-scoped policies whose project could not be resolved require a
/// GLOBAL grant — never "any grant anywhere", which would let a principal
/// scoped to one project act on resources it cannot even name.
fn is_authorized(
    principal: &AuthenticatedPrincipal,
    policy: RoutePolicy,
    project: Option<&str>,
) -> bool {
    match policy.scope {
        ProjectScope::Public => true,
        ProjectScope::Authenticated => {
            principal.credential_source != CredentialSource::AnonymousSingleUser
        }
        ProjectScope::HumanSession => {
            principal.kind == mem_record::AuthPrincipalKind::HumanOidc
                && principal.credential_source == CredentialSource::BrowserSession
        }
        ProjectScope::Global => principal.has_global(policy.permission),
        ProjectScope::Unscoped => principal.has_anywhere(policy.permission),
        _ => match project {
            Some(project) => principal.has_for_project(project, policy.permission),
            None => principal.has_global(policy.permission),
        },
    }
}

pub(crate) async fn proxy_relay_request(
    state: &AppState,
    request: Request,
) -> Result<Response, ApiError> {
    let peer = crate::selected_primary_peer(state).ok_or_else(|| {
        ApiError::service_unavailable("no primary memory service available on the local network")
    })?;
    let (parts, body) = request.into_parts();
    let path_and_query = parts
        .uri
        .path_and_query()
        .map(|value| value.as_str())
        .unwrap_or_else(|| parts.uri.path());
    let body = to_bytes(body, MAX_AUTH_INSPECTION_BODY)
        .await
        .map_err(|error| ApiError::io(error.into()))?;
    let mut headers = parts.headers;
    for name in [
        header::HOST,
        header::CONTENT_LENGTH,
        header::CONNECTION,
        header::TRANSFER_ENCODING,
        header::ACCEPT_ENCODING,
    ] {
        headers.remove(name);
    }
    let upstream = state
        .http_client
        .request(
            parts.method,
            format!("http://{}{}", peer.advertise_addr, path_and_query),
        )
        .headers(headers)
        .body(body)
        .send()
        .await
        .map_err(|error| ApiError::io(error.into()))?;
    let status = upstream.status();
    let upstream_headers = upstream.headers().clone();
    let mut response = Response::new(Body::from_stream(upstream.bytes_stream()));
    *response.status_mut() = status;
    for (name, value) in &upstream_headers {
        if !matches!(
            name.as_str(),
            "content-length" | "transfer-encoding" | "content-encoding" | "connection"
        ) {
            response.headers_mut().append(name, value.clone());
        }
    }
    Ok(response)
}

fn is_mutating(method: &Method) -> bool {
    !matches!(*method, Method::GET | Method::HEAD | Method::OPTIONS)
}

fn validate_browser_mutation(
    state: &AppState,
    headers: &axum::http::HeaderMap,
    principal: &AuthenticatedPrincipal,
) -> Result<(), ApiError> {
    match state.config.auth.public_base_url.as_deref() {
        Some(public_base_url) => {
            let expected_origin = reqwest::Url::parse(public_base_url)
                .map_err(|_| ApiError::internal("auth.public_base_url is invalid"))?
                .origin()
                .ascii_serialization();
            let origin = headers
                .get(header::ORIGIN)
                .and_then(|value| value.to_str().ok())
                .ok_or_else(|| ApiError::forbidden("browser write is missing Origin"))?;
            if origin != expected_origin {
                return Err(ApiError::forbidden("browser write Origin is not allowed"));
            }
        }
        // Single-user installs have no public base URL; their sessions are
        // minted over loopback only, and the CSRF double-submit check below
        // remains unconditional.
        None if state.config.auth.mode == mem_record::AuthMode::SingleUser => {}
        None => {
            return Err(ApiError::forbidden(
                "auth.public_base_url is required for browser writes",
            ));
        }
    }
    let csrf_header = headers
        .get("x-csrf-token")
        .and_then(|value| value.to_str().ok())
        .ok_or_else(|| ApiError::forbidden("browser write is missing x-csrf-token"))?;
    let csrf_cookie = cookie_value(headers, CSRF_COOKIE_NAME)
        .ok_or_else(|| ApiError::forbidden("browser write is missing CSRF cookie"))?;
    if csrf_header != csrf_cookie
        || principal.session_csrf_hash.as_deref() != Some(hash_secret(csrf_header).as_slice())
    {
        return Err(ApiError::forbidden("invalid browser CSRF token"));
    }
    Ok(())
}

async fn resolve_request_project(
    state: &AppState,
    request: &mut Request,
    scope: ProjectScope,
) -> Result<Option<String>, ApiError> {
    match scope {
        ProjectScope::Public
        | ProjectScope::Authenticated
        | ProjectScope::HumanSession
        | ProjectScope::Unscoped
        | ProjectScope::Global => Ok(None),
        ProjectScope::PathProject => Ok(request
            .uri()
            .path()
            .split('/')
            .nth(3)
            .and_then(|value| urlencoding::decode(value).ok())
            .map(|value| value.into_owned())),
        ProjectScope::BodyProject => project_from_body(request).await,
        ProjectScope::BodyMemoryResource => {
            let memory_id = memory_id_from_body(request).await?;
            resource_project(state, memory_id, ResourceKind::Memory).await
        }
        ProjectScope::QueryProjectOrGlobal => Ok(query_value(request, "project")),
        ProjectScope::MemoryResource => {
            resource_project(state, resource_id(request), ResourceKind::Memory).await
        }
        ProjectScope::ValidationResource => {
            resource_project(state, resource_id(request), ResourceKind::Validation).await
        }
        ProjectScope::LoopRunResource => {
            resource_project(state, resource_id(request), ResourceKind::LoopRun).await
        }
        ProjectScope::LoopApprovalResource => {
            resource_project(state, resource_id(request), ResourceKind::LoopApproval).await
        }
        ProjectScope::LoopProposalResource => {
            resource_project(state, resource_id(request), ResourceKind::LoopProposal).await
        }
        ProjectScope::WorkspaceResource => {
            resource_project(state, resource_id(request), ResourceKind::Workspace).await
        }
    }
}

async fn project_from_body(request: &mut Request) -> Result<Option<String>, ApiError> {
    body_string_field(request, "project").await
}

async fn memory_id_from_body(request: &mut Request) -> Result<Option<Uuid>, ApiError> {
    Ok(body_string_field(request, "memory_id")
        .await?
        .and_then(|value| Uuid::parse_str(&value).ok()))
}

async fn body_string_field(request: &mut Request, field: &str) -> Result<Option<String>, ApiError> {
    let body = std::mem::replace(request.body_mut(), Body::empty());
    let bytes = to_bytes(body, MAX_AUTH_INSPECTION_BODY)
        .await
        .map_err(|_| {
            ApiError::status_message(StatusCode::PAYLOAD_TOO_LARGE, "request body is too large")
        })?;
    let value = if bytes.is_empty() {
        None
    } else {
        serde_json::from_slice::<Value>(&bytes)
            .ok()
            .and_then(|value| value.get(field).and_then(Value::as_str).map(str::to_owned))
    };
    *request.body_mut() = Body::from(bytes);
    Ok(value)
}

fn query_value(request: &Request, name: &str) -> Option<String> {
    request.uri().query().and_then(|query| {
        query.split('&').find_map(|part| {
            let (key, value) = part.split_once('=').unwrap_or((part, ""));
            (key == name)
                .then(|| {
                    urlencoding::decode(value)
                        .ok()
                        .map(|value| value.into_owned())
                })
                .flatten()
        })
    })
}

#[derive(Debug, Clone, Copy)]
enum ResourceKind {
    Memory,
    Validation,
    LoopRun,
    LoopApproval,
    LoopProposal,
    Workspace,
}

async fn resource_project(
    state: &AppState,
    id: Option<Uuid>,
    kind: ResourceKind,
) -> Result<Option<String>, ApiError> {
    let Some(id) = id else {
        return Ok(None);
    };
    let pool = state.pool()?;
    let sql = match kind {
        ResourceKind::Memory => {
            "SELECT p.slug FROM memory_entries r JOIN projects p ON p.id = r.project_id WHERE r.id = $1 OR r.canonical_id = $1 LIMIT 1"
        }
        ResourceKind::Validation => {
            "SELECT p.slug FROM memory_validation_runs r JOIN projects p ON p.id = r.project_id WHERE r.id = $1"
        }
        ResourceKind::LoopRun => {
            "SELECT p.slug FROM loop_runs r JOIN projects p ON p.id = r.project_id WHERE r.id = $1"
        }
        ResourceKind::LoopApproval => {
            "SELECT p.slug FROM approval_requests r JOIN projects p ON p.id = r.project_id WHERE r.id = $1"
        }
        ResourceKind::LoopProposal => {
            "SELECT p.slug FROM memory_proposals r JOIN projects p ON p.id = r.project_id WHERE r.id = $1"
        }
        ResourceKind::Workspace => {
            "SELECT p.slug FROM agent_workspaces r JOIN projects p ON p.id = r.project_id WHERE r.id = $1"
        }
    };
    let row = sqlx::query(sql)
        .bind(id)
        .fetch_optional(&pool)
        .await
        .map_err(ApiError::sql)?;
    row.map(|row| row.try_get("slug").map_err(ApiError::sql))
        .transpose()
}

fn resource_id(request: &Request) -> Option<Uuid> {
    request
        .uri()
        .path()
        .split('/')
        .find_map(|part| Uuid::parse_str(part).ok())
}

/// Whether a request is allowed while the service runs in read-only mode:
/// plain reads always, mutating methods only when their policy entry marks
/// them as semantic reads.
pub(crate) fn read_only_request_allowed(method: &Method, path: &str) -> bool {
    if matches!(*method, Method::GET | Method::HEAD | Method::OPTIONS) {
        return true;
    }
    policy_for(method, path).is_some_and(|policy| policy.semantic_read)
}

pub(crate) async fn read_only_guard(
    State(state): State<AppState>,
    request: Request,
    next: Next,
) -> Response {
    if state.config.service.read_only
        && !read_only_request_allowed(request.method(), request.uri().path())
    {
        return ApiError::status_message(
            axum::http::StatusCode::FORBIDDEN,
            "this Memory Layer runs in read-only (student) mode; writes are disabled",
        )
        .into_response();
    }
    next.run(request).await
}

#[cfg(test)]
mod tests {
    use std::collections::BTreeMap;

    use axum::http::Method;
    use mem_record::{AuthPrincipalKind, AuthRole, Permission, PermissionSet};
    use uuid::Uuid;

    use super::{
        AuthenticatedPrincipal, CredentialSource, ProjectScope, RoutePolicy, is_authorized,
        policy_for, read_only_request_allowed,
    };
    use crate::auth::ProjectRoleGrant;

    fn project_admin(project: &str) -> AuthenticatedPrincipal {
        let mut project_roles = BTreeMap::new();
        project_roles.insert(
            project.to_string(),
            ProjectRoleGrant {
                role: AuthRole::Admin,
                permissions: AuthRole::Admin.permissions(),
                source: "test".to_string(),
            },
        );
        AuthenticatedPrincipal {
            id: Uuid::new_v4(),
            kind: AuthPrincipalKind::HumanOidc,
            display_name: "Project Admin".to_string(),
            email: None,
            issuer: None,
            subject: None,
            groups: Vec::new(),
            global_role: None,
            global: PermissionSet::EMPTY,
            project_roles,
            credential_source: CredentialSource::BrowserSession,
            token_id: None,
            session_id: None,
            session_csrf_hash: None,
        }
    }

    #[test]
    fn delete_memory_resolves_the_owning_project() {
        assert_eq!(
            policy_for(&Method::DELETE, "/v1/memory")
                .expect("policy")
                .scope,
            ProjectScope::BodyMemoryResource
        );
    }

    #[test]
    fn project_scoped_admin_cannot_act_across_projects() {
        let principal = project_admin("project-a");
        let delete_policy = RoutePolicy {
            permission: Permission::MemoryDelete,
            scope: ProjectScope::BodyMemoryResource,
            semantic_read: false,
        };
        // Memory resolved to another project: denied.
        assert!(!is_authorized(&principal, delete_policy, Some("project-b")));
        // Memory in the granted project: allowed.
        assert!(is_authorized(&principal, delete_policy, Some("project-a")));
        // Unresolvable project (unknown memory id, or a body without a
        // project on a BodyProject route) requires a GLOBAL grant.
        assert!(!is_authorized(&principal, delete_policy, None));
        let prune_policy = RoutePolicy {
            permission: Permission::MemoryCurate,
            scope: ProjectScope::BodyProject,
            semantic_read: false,
        };
        assert!(!is_authorized(&principal, prune_policy, None));
        assert!(is_authorized(&principal, prune_policy, Some("project-a")));
    }

    #[test]
    fn global_admin_still_authorized_without_project() {
        let mut principal = project_admin("project-a");
        principal.global_role = Some(AuthRole::Admin);
        principal.global = AuthRole::Admin.permissions();
        let delete_policy = RoutePolicy {
            permission: Permission::MemoryDelete,
            scope: ProjectScope::BodyMemoryResource,
            semantic_read: false,
        };
        assert!(is_authorized(&principal, delete_policy, None));
        assert!(is_authorized(&principal, delete_policy, Some("project-b")));
    }

    #[test]
    fn route_roles_cover_role_boundaries() {
        assert_eq!(
            policy_for(&Method::POST, "/v1/capture/task")
                .expect("policy")
                .permission,
            Permission::ActivityCapture
        );
        assert_eq!(
            policy_for(&Method::POST, "/v1/curate")
                .expect("policy")
                .permission,
            Permission::MemoryCurate
        );
        assert_eq!(
            policy_for(&Method::DELETE, "/v1/memory")
                .expect("policy")
                .permission,
            Permission::MemoryDelete
        );
        assert_eq!(
            policy_for(&Method::GET, "/v1/projects/demo/overview")
                .expect("policy")
                .scope,
            ProjectScope::PathProject
        );
        assert_eq!(
            policy_for(&Method::GET, "/healthz").expect("policy").scope,
            ProjectScope::Public
        );
        assert_eq!(
            policy_for(&Method::GET, "/v1/loops/context_pack_refresh/context-pack")
                .expect("policy")
                .scope,
            ProjectScope::QueryProjectOrGlobal
        );
        assert_eq!(
            policy_for(&Method::POST, "/v1/auth/tokens")
                .expect("policy")
                .permission,
            Permission::AuthManage
        );
    }

    #[test]
    fn reads_and_query_paths_are_allowed() {
        assert!(read_only_request_allowed(&Method::GET, "/v1/loops"));
        assert!(read_only_request_allowed(
            &Method::GET,
            "/v1/projects/demo/structure"
        ));
        assert!(read_only_request_allowed(&Method::POST, "/v1/query"));
        assert!(read_only_request_allowed(&Method::POST, "/v1/query/global"));
        assert!(read_only_request_allowed(
            &Method::POST,
            "/v1/projects/classroom/resume"
        ));
        assert!(read_only_request_allowed(
            &Method::POST,
            "/v1/projects/classroom/up-to-speed"
        ));
        assert!(read_only_request_allowed(
            &Method::POST,
            "/v1/projects/classroom/bundle/export"
        ));
        assert!(read_only_request_allowed(
            &Method::POST,
            "/v1/projects/classroom/bundle/export/preview"
        ));
    }

    #[test]
    fn mutating_endpoints_are_blocked() {
        assert!(!read_only_request_allowed(&Method::POST, "/v1/curate"));
        assert!(!read_only_request_allowed(
            &Method::POST,
            "/v1/capture/task"
        ));
        assert!(!read_only_request_allowed(
            &Method::POST,
            "/v1/projects/classroom/bundle/import"
        ));
        assert!(!read_only_request_allowed(
            &Method::POST,
            "/v1/projects/classroom/bundle/import/preview"
        ));
        assert!(!read_only_request_allowed(&Method::POST, "/v1/archive"));
        assert!(!read_only_request_allowed(
            &Method::POST,
            "/v1/loops/memory_consolidation/run"
        ));
        assert!(!read_only_request_allowed(&Method::DELETE, "/v1/memory"));
        assert!(!read_only_request_allowed(
            &Method::PUT,
            "/v1/projects/classroom/replacement-policy"
        ));
        assert!(!read_only_request_allowed(
            &Method::POST,
            "/v1/admin/shutdown"
        ));
    }

    #[test]
    fn unregistered_routes_are_denied() {
        assert!(policy_for(&Method::GET, "/v1/not-a-route").is_none());
        assert!(policy_for(&Method::DELETE, "/v1/query").is_none());
        assert!(!read_only_request_allowed(&Method::POST, "/v1/not-a-route"));
    }

    /// Every route registered in routes.rs must have a policy entry for each of
    /// its methods. A missing entry is fail-closed at runtime, but this test
    /// makes the drift loud at build time instead of at first request.
    #[test]
    fn every_registered_route_has_a_policy() {
        let source = include_str!("../routes.rs");
        let mut missing = Vec::new();
        let mut index = 0;
        while let Some(offset) = source[index..].find(".route(") {
            let start = index + offset + ".route(".len();
            let rest = &source[start..];
            let Some(open_quote) = rest.find('"') else {
                break;
            };
            let after = &rest[open_quote + 1..];
            let Some(close_quote) = after.find('"') else {
                break;
            };
            let path = &after[..close_quote];
            // capture handler expression up to the matching close paren
            let mut depth = 1;
            let mut end = start;
            let bytes = source.as_bytes();
            while depth > 0 && end < source.len() {
                match bytes[end] {
                    b'(' => depth += 1,
                    b')' => depth -= 1,
                    _ => {}
                }
                end += 1;
            }
            let handlers = &source[start..end - 1];
            for (needle, method) in [
                ("get(", Method::GET),
                ("post(", Method::POST),
                ("put(", Method::PUT),
                ("delete(", Method::DELETE),
                ("patch(", Method::PATCH),
            ] {
                if handlers.contains(needle) && policy_for(&method, &sample_path(path)).is_none() {
                    missing.push(format!("{method} {path}"));
                }
            }
            index = end;
        }
        assert!(
            missing.is_empty(),
            "routes without a policy entry (add them to build_policy_table): {missing:?}"
        );
    }

    fn sample_path(template: &str) -> String {
        template
            .split('/')
            .map(|segment| {
                if segment.starts_with('{') {
                    "0be04b5e-0000-4000-8000-000000000000"
                } else {
                    segment
                }
            })
            .collect::<Vec<_>>()
            .join("/")
    }
}
