// SPDX-License-Identifier: AGPL-3.0-or-later
//! Deterministic authenticated laptop search across private curated projections
//! and new local work. Remote evidence never enters a model prompt or learning.
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

pub(crate) async fn query(
    state: &AppState,
    actor: &AuthenticatedPrincipal,
    peer: SocketAddr,
    request: &QueryRequest,
) -> Result<QueryResponse, ApiError> {
    crate::laptop_cache::authorize(state, actor, peer)?;
    if request.history
        || request.include_stale
        || request.min_confidence.is_some()
        || !request.filters.types.is_empty()
        || !request.filters.tags.is_empty()
        || request.answer_mode == Some(QueryAnswerMode::Llm)
        || request
            .retrieval_mode
            .is_some_and(|m| m != QueryRetrievalMode::Lexical)
    {
        return Err(ApiError::validation(ValidationError::new(
            "curated laptop queries support current lexical deterministic knowledge; history, ranking filters, semantic/graph and model answers require another interface",
        )));
    }
    let pool = state.pool()?;
    let project: Uuid = sqlx::query_scalar("SELECT id FROM projects WHERE slug=$1")
        .bind(&request.project)
        .fetch_optional(&pool)
        .await
        .map_err(ApiError::sql)?
        .ok_or_else(|| ApiError::not_found("selected local project not found"))?;
    let config = state
        .config
        .laptop_cache
        .as_ref()
        .ok_or_else(|| ApiError::not_found("laptop cache unavailable"))?;
    let matches: Vec<_> = config
        .subscriptions
        .iter()
        .filter(|s| {
            s.project_id == project
                && s.replication_mode == LaptopCacheReplicationMode::CuratedProjectV1
        })
        .collect();
    if matches.len() != 1 {
        return Err(ApiError::forbidden(
            "exact selected project replica required",
        ));
    }
    let binding = matches[0];
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
    let projection = DeviceProjectionStore::open(
        pool.clone(),
        binding.subscription_id,
        binding.device_id,
        scope,
    )
    .await
    .map_err(|_| ApiError::forbidden("replica binding changed"))?;
    let remote = projection
        .search(&request.query, 50)
        .await
        .map_err(|_| ApiError::service_unavailable("selected curated replica unavailable"))?;
    let mut localrequest = request.clone();
    localrequest.top_k = 50;
    localrequest.retrieval_mode = Some(QueryRetrievalMode::Lexical);
    localrequest.answer_mode = Some(QueryAnswerMode::Deterministic);
    let mut response = mem_search::query_memory_with_configs(
        &pool,
        &localrequest,
        None,
        &state.config.provenance,
        &mem_search::ReinforcementRankParams::from(&state.config.reinforcement),
    )
    .await
    .map_err(ApiError::io)?;
    // Exact device/project matching below avoids assuming manifest revisions from
    // local configuration. Retained receipts are knowledge identity, not grants.
    let acknowledged: Vec<Uuid> = sqlx::query_scalar(
        r#"
        SELECT q.local_memory_id FROM (
          SELECT *,COALESCE(resolution_json,receipt) AS owner_result
          FROM laptop_curated_outbox
        ) q
        WHERE q.state='accepted' AND q.local_project_id=$2
          AND q.binding->>'device_id'=$3
          AND q.binding->'scope'->>'owner_instance_id'=$4
          AND (EXISTS (
            SELECT 1 FROM device_replica_owner_watermarks w
            WHERE w.subscription=$1
              AND w.canonical_id::text=q.owner_result->>'owner_canonical_id'
              AND (w.inactive OR w.version >= (q.owner_result->>'owner_version')::bigint)
          ) OR EXISTS (
            SELECT 1 FROM device_replica_records r WHERE r.subscription=$1 AND (
              (r.record_key='version:'||(q.owner_result->>'owner_memory_id')
                AND r.record->>'kind'='memory_version'
                AND r.record->'value'->>'id'=q.owner_result->>'owner_memory_id')
              OR (r.record_key='state:'||(q.owner_result->>'owner_canonical_id')
                AND r.record->>'kind'='canonical_state' AND (
                  r.record->'value'->>'status'<>'active'
                  OR EXISTS (
                    SELECT 1 FROM device_replica_records v
                    WHERE v.subscription=r.subscription
                      AND v.record_key='version:'||(r.record->'value'->>'current_version_id')
                      AND v.record->>'kind'='memory_version'
                      AND v.record->'value'->>'id'=r.record->'value'->>'current_version_id'
                      AND v.record->'value'->>'canonical_id'=q.owner_result->>'owner_canonical_id'
                  )
                )
              )
            )
          ))
    "#,
    )
    .bind(binding.subscription_id)
    .bind(project)
    .bind(binding.device_id.to_string())
    .bind(binding.owner_instance_id.to_string())
    .fetch_all(&pool)
    .await
    .map_err(ApiError::sql)?;
    response
        .results
        .retain(|r| !acknowledged.contains(&r.memory_id));
    let local_ids: Vec<Uuid> = response.results.iter().map(|r| r.memory_id).collect();
    let pending:Vec<(Uuid,String,Option<String>)>=sqlx::query_as("SELECT local_memory_id,state,blocked_reason FROM laptop_curated_outbox WHERE local_memory_id=ANY($1) AND local_project_id=$2 AND binding->>'device_id'=$3 AND binding->'scope'->>'owner_instance_id'=$4")
        .bind(&local_ids).bind(project).bind(binding.device_id.to_string()).bind(binding.owner_instance_id.to_string()).fetch_all(&pool).await.map_err(ApiError::sql)?;
    for result in &mut response.results {
        if let Some((_, status, reason)) = pending.iter().find(|(id, _, _)| *id == result.memory_id)
        {
            result.score_explanation.insert(
                0,
                format!(
                    "Local contribution: {status}{}; not an editable copy of owner history.",
                    reason
                        .as_ref()
                        .map(|r| format!(" ({r})"))
                        .unwrap_or_default()
                ),
            );
            result.needs_review |= matches!(status.as_str(), "proposed" | "conflict" | "blocked");
        }
    }
    for hit in remote {
        response.results.push(QueryResult {
            origin: Some(QueryResultOrigin {
                owner_instance_id: binding.owner_instance_id,
                project_id: project,
                author: hit.version.author.clone(),
                content_digest: format!("sha256:{}", hit.digest.as_hex()),
                read_only: true,
            }),
            memory_id: hit.version.id,
            canonical_id: Some(hit.version.canonical_id),
            version_no: Some(hit.version.version as i32),
            project: Some(request.project.clone()),
            project_name: None,
            repo_root: None,
            summary: hit.version.summary.clone(),
            memory_type: hit.version.memory_type,
            score: mem_record::text::lexical_overlap_ratio(
                &format!("{} {}", hit.version.text, hit.version.summary).to_lowercase(),
                &mem_record::text::extract_lexical_terms(&request.query),
            ),
            snippet: hit.version.text,
            match_kind: QueryMatchKind::Lexical,
            score_explanation: vec!["Approved read-only owner knowledge".into()],
            debug: QueryResultDebug {
                memory_confidence: hit.state.confidence.as_confidence(),
                ..Default::default()
            },
            tags: hit.version.tags,
            sources: vec![],
            graph_connections: vec![],
            needs_review: false,
        });
    }
    response.results.sort_by(|a, b| {
        b.score
            .total_cmp(&a.score)
            .then_with(|| a.memory_id.cmp(&b.memory_id))
    });
    response.results.truncate(request.top_k as usize);
    response.insufficient_evidence = response.results.is_empty();
    response.confidence = if response.insufficient_evidence {
        0.0
    } else {
        response
            .results
            .iter()
            .map(|r| r.debug.memory_confidence)
            .sum::<f32>()
            / response.results.len() as f32
    };
    response.answer = if response.insufficient_evidence {
        "Insufficient approved evidence.".into()
    } else {
        response
            .results
            .iter()
            .enumerate()
            .map(|(i, r)| format!("[{}] {}", i + 1, r.snippet))
            .collect::<Vec<_>>()
            .join("\n\n")
    };
    response.answer_citations = response
        .results
        .iter()
        .enumerate()
        .map(|(i, r)| QueryAnswerCitation {
            result_number: i + 1,
            memory_id: r.memory_id,
            canonical_id: r.canonical_id,
            version_no: r.version_no,
            project: r.project.clone(),
            project_name: r.project_name.clone(),
            repo_root: r.repo_root.clone(),
            memory_type: r.memory_type.clone(),
            summary: r.summary.clone(),
            snippet: r.snippet.clone(),
        })
        .collect();
    response.answer_generation = QueryAnswerGeneration {
        method: QueryAnswerMethod::Deterministic,
        cited_result_numbers: (1..=response.results.len()).collect(),
        evidence_count: response.results.len(),
        ..Default::default()
    };
    Ok(response)
}
