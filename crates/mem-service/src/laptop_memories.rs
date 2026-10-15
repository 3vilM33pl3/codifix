// SPDX-License-Identifier: AGPL-3.0-or-later
//! Explicit local-human browsing; never changes the local-only repository reads.
use crate::{ApiError, AppState, AuthenticatedPrincipal};
use mem_config::LaptopCacheReplicationMode;
use mem_federation::{
    publication::{InstanceId, SpaceId},
    subscription::ProjectScope,
};
use mem_federation_store::DeviceProjectionStore;
use mem_record::*;
use std::net::SocketAddr;
use uuid::Uuid;

pub(crate) async fn list(
    state: &AppState,
    actor: &AuthenticatedPrincipal,
    peer: SocketAddr,
    slug: &str,
    status: Option<&str>,
    limit: i64,
    offset: i64,
) -> Result<ProjectMemoryBrowseResponse, ApiError> {
    crate::laptop_cache::authorize(state, actor, peer)?;
    let pool = state.pool()?;
    let project: Uuid = sqlx::query_scalar("SELECT id FROM projects WHERE slug=$1")
        .bind(slug)
        .fetch_optional(&pool)
        .await
        .map_err(ApiError::sql)?
        .ok_or_else(|| ApiError::not_found("selected local project not found"))?;
    let bindings: Vec<_> = state
        .config
        .laptop_cache
        .as_ref()
        .ok_or_else(|| ApiError::not_found("laptop cache unavailable"))?
        .subscriptions
        .iter()
        .filter(|b| {
            b.project_id == project
                && b.replication_mode == LaptopCacheReplicationMode::CuratedProjectV1
        })
        .collect();
    let mut local = crate::repository::fetch_project_memories(&pool, slug, status, 100_001, 0)
        .await
        .map_err(ApiError::sql)?;
    if local.total > 100_000 {
        return Err(ApiError::service_unavailable(
            "local memory browse limit exceeded",
        ));
    }
    if bindings.is_empty() {
        return page(local, limit, offset);
    }
    if bindings.len() != 1 {
        return Err(ApiError::forbidden(
            "exact selected project replica required",
        ));
    }
    let binding = bindings[0];
    let area = state
        .config
        .area
        .as_ref()
        .ok_or_else(|| ApiError::forbidden("laptop area missing"))?;
    let scope = ProjectScope {
        company: binding.company_id,
        area: area.name.clone(),
        owner: InstanceId::new(binding.owner_instance_id)
            .map_err(|_| ApiError::forbidden("owner missing"))?,
        project,
        space: SpaceId::new(binding.space_id).map_err(|_| ApiError::forbidden("space missing"))?,
    };
    let store = DeviceProjectionStore::open(
        pool.clone(),
        binding.subscription_id,
        binding.device_id,
        scope,
    )
    .await
    .map_err(|_| ApiError::service_unavailable("selected curated replica unavailable"))?;
    let remote = store
        .browse()
        .await
        .map_err(|_| ApiError::service_unavailable("selected curated replica unavailable"))?;
    let acknowledged = crate::laptop_query::acknowledged(&pool, binding, project).await?;
    local.items.retain(|row| !acknowledged.contains(&row.id));
    for hit in remote {
        if status.is_some_and(|s| {
            s != match hit.state.status {
                MemoryStatus::Active => "active",
                MemoryStatus::Archived => "archived",
            }
        }) {
            continue;
        }
        local.items.push(ProjectMemoryListItem {
            origin: Some(ProjectMemoryOrigin {
                subscription_id: binding.subscription_id,
                owner_instance_id: binding.owner_instance_id,
                project_id: project,
                author: hit.version.author.clone(),
                content_digest: format!("sha256:{}", hit.digest.as_hex()),
                read_only: true,
            }),
            id: hit.version.id,
            canonical_id: hit.version.canonical_id,
            version_no: hit.version.version as i32,
            is_tombstone: false,
            summary: hit.version.summary,
            preview: hit.version.text.chars().take(240).collect(),
            memory_type: hit.version.memory_type,
            status: hit.state.status,
            confidence: hit.state.confidence.as_confidence(),
            importance: hit.state.importance,
            updated_at: hit.version.updated_at.max(hit.state.updated_at),
            tag_count: hit.version.tags.len() as i64,
            tags: hit.version.tags,
            source_count: hit.version.unavailable_source_ids.len() as i64,
        });
    }
    local.items.sort_by(|a, b| {
        b.updated_at
            .cmp(&a.updated_at)
            .then_with(|| b.id.cmp(&a.id))
    });
    page(local, limit, offset)
}

fn page(
    mut local: ProjectMemoriesResponse,
    limit: i64,
    offset: i64,
) -> Result<ProjectMemoryBrowseResponse, ApiError> {
    for item in &mut local.items {
        item.summary = item.summary.chars().take(240).collect();
    }
    local.total = local.items.len() as i64;
    let browse_revision = mem_federation::CanonicalJsonCodec::digest(&local.items)
        .map_err(|_| ApiError::service_unavailable("invalid browsing snapshot"))?
        .as_hex()
        .to_owned();
    local.items = local
        .items
        .into_iter()
        .skip(offset as usize)
        .take(limit as usize)
        .collect();
    Ok(ProjectMemoryBrowseResponse {
        memories: local,
        browse_revision,
    })
}
