// SPDX-License-Identifier: AGPL-3.0-or-later
use super::*;
use axum::extract::Query;
use codifix_federation::{CanonicalJsonCodec, ContentDigest};
use codifix_federation_store::{ActionRequest, ActionResponse};
use serde_json::{Value, json};
use sqlx::types::Json as DbJson;
use std::collections::BTreeSet;

pub(crate) async fn discover(
    State(state): State<AppState>,
    Extension(principal): Extension<AuthenticatedPrincipal>,
    Path(slug): Path<String>,
) -> Result<Json<Value>, ApiError> {
    let space = state
        .federation
        .read()
        .expect("federation registry poisoned")
        .as_ref()
        .map(|r| r.space)
        .ok_or_else(|| ApiError::not_found("federation is not configured"))?;
    let (runtime, access) = binding(&state, &principal, &slug, space).await?;
    // Discovery does not grant Pull. It allows an administrator to bootstrap
    // local grants without disclosing endpoint addresses or service credentials.
    Ok(Json(
        json!({"space":space,"instance":runtime.instance,"authority":runtime.authority,
        "principal":access.principal,"permissions":Permission::ALL.into_iter().filter(|p|access.permissions.contains(*p)).collect::<Vec<_>>(),
        "read_only":access.read_only,"profile":"private-gateway-v1"}),
    ))
}

#[derive(Deserialize)]
#[serde(deny_unknown_fields)]
pub(crate) struct CatalogQuery {
    #[serde(default)]
    offset: usize,
}

pub(crate) async fn catalog(
    State(state): State<AppState>,
    Extension(principal): Extension<AuthenticatedPrincipal>,
    Path(path): Path<SpacePath>,
    Query(query): Query<CatalogQuery>,
) -> Result<Json<Value>, ApiError> {
    if query.offset > 1_000_000 {
        return Err(bad_request("catalog offset exceeds limit"));
    }
    let (runtime, access) = binding(&state, &principal, &path.slug, path.space).await?;
    require_pull(&state, &runtime, &access).await?;
    let known = runtime
        .sync
        .verified_snapshot()
        .await
        .map_err(runtime_error)?;
    let mut streams = BTreeSet::new();
    let mut members = BTreeSet::from([known.authority()]);
    let mut proposals = BTreeSet::new();
    for delivery in known.accepted() {
        match &delivery.envelope.record {
            PublicationRecord::MemoryVersion { stream, .. } => {
                streams.insert(*stream);
            }
            PublicationRecord::Membership { member, .. }
            | PublicationRecord::Revocation { member, .. } => {
                members.insert(*member);
            }
            PublicationRecord::Proposal(_) => {
                proposals.insert(delivery.envelope.key());
            }
            _ => {}
        }
    }
    let page = |items: Vec<Value>| -> Value {
        let next = (items.len() > query.offset + 32).then_some(query.offset + 32);
        json!({"items":items.into_iter().skip(query.offset).take(32).collect::<Vec<_>>(),"next_offset":next})
    };
    let streams = page(
        streams
            .into_iter()
            .map(|stream| {
                json!({"stream":stream,
        "shared_version":known.content_version(stream),"state":known.current_state(stream)})
            })
            .collect(),
    );
    let members = page(members.into_iter().map(|member| json!({"member":member,"capabilities":
        ([SpaceCapability::Pull,SpaceCapability::Publish,SpaceCapability::ManageMembership].into_iter()
            .filter(|c|known.has_capability(member,*c)).collect::<Vec<_>>())})).collect());
    let proposals = page(proposals.into_iter().map(|key| {
        let p = known.proposal(key).expect("accepted proposal");
        let resolution = match p.resolution { ProposalResolution::Pending=>"pending",ProposalResolution::Accepted{..}=>"accepted",ProposalResolution::Rejected{..}=>"rejected" };
        json!({"key":key,"target":p.proposal.target,"base_version":p.proposal.base_version,"author":p.author,"authored_at":p.authored_at,"resolution":resolution})
    }).collect());
    // Local records are limited to the caller unless they can administer this
    // exact project. All lists use fixed queries and bounded pages.
    let pool = state.pool()?;
    let admin = access.permissions.contains(Permission::AuthManage);
    let mut local = serde_json::Map::new();
    for (name, sql) in [
        (
            "grants",
            "SELECT to_jsonb(g) FROM (SELECT principal_id,capabilities,enabled FROM federation_local_grants WHERE space=$1 AND instance=$2 AND project_id=$3 AND (principal_id=$4 OR $5) ORDER BY principal_id LIMIT 33 OFFSET $6) g",
        ),
        (
            "subscriptions",
            "SELECT to_jsonb(g) FROM (SELECT canonical_id,principal_id,review_id,enabled,shared_version,state_revision FROM federation_source_subscriptions WHERE space=$1 AND instance=$2 AND project_id=$3 AND (principal_id=$4 OR $5) ORDER BY canonical_id LIMIT 33 OFFSET $6) g",
        ),
        (
            "reviews",
            "SELECT to_jsonb(g) FROM (SELECT id,canonical_id,source_id,reviewer,reviewed_at FROM federation_publication_reviews WHERE space=$1 AND instance=$2 AND project_id=$3 AND (reviewer=$4 OR $5) ORDER BY id LIMIT 33 OFFSET $6) g",
        ),
    ] {
        let mut rows = sqlx::query_scalar::<_, DbJson<Value>>(sql)
            .bind(runtime.space.as_uuid())
            .bind(runtime.instance.as_uuid())
            .bind(runtime.project)
            .bind(access.principal)
            .bind(admin)
            .bind(query.offset as i64)
            .fetch_all(&pool)
            .await
            .map_err(|_| unavailable())?;
        let next = (rows.len() > 32).then_some(query.offset + 32);
        rows.truncate(32);
        local.insert(
            name.into(),
            json!({"items":rows.into_iter().map(|v|v.0).collect::<Vec<_>>(),"next_offset":next}),
        );
    }
    Ok(Json(
        json!({"as_of_sequence":known.sequence(),"streams":streams,"members":members,"proposals":proposals,
        "local":local,"untrusted":true,"retained_copy":true,"offline_copies_may_remain":true}),
    ))
}

#[derive(Deserialize)]
pub(crate) struct ProposalPath {
    slug: String,
    space: SpaceId,
    publisher: InstanceId,
    record: RecordId,
}
pub(crate) async fn proposal(
    State(state): State<AppState>,
    Extension(principal): Extension<AuthenticatedPrincipal>,
    Path(path): Path<ProposalPath>,
) -> Result<Json<Value>, ApiError> {
    let (runtime, access) = binding(&state, &principal, &path.slug, path.space).await?;
    require_pull(&state, &runtime, &access).await?;
    let known = runtime
        .sync
        .verified_snapshot()
        .await
        .map_err(runtime_error)?;
    let key = RecordKey {
        publisher: path.publisher,
        id: path.record,
    };
    let p = known
        .proposal(key)
        .ok_or_else(|| ApiError::not_found("shared proposal not found"))?;
    let delivery = known
        .accepted_record(key)
        .expect("stored proposal delivery");
    let resolution = match p.resolution {
        ProposalResolution::Pending => "pending",
        ProposalResolution::Accepted { .. } => "accepted",
        ProposalResolution::Rejected { .. } => "rejected",
    };
    Ok(Json(
        json!({"key":key,"proposal":p.proposal,"author":p.author,"authored_at":p.authored_at,
        "resolution":resolution,"locator":delivery.locator,"digest":delivery.digest,
        "current_shared_version":known.content_version(p.proposal.target),"untrusted":true,"retained_copy":true,
        "approval_pauses_local_subscription":true}),
    ))
}

#[derive(Deserialize)]
#[serde(deny_unknown_fields)]
pub(crate) struct ReviewDraftRequest {
    canonical: RecordId,
    evidence: Vec<SelectedEvidence>,
}
#[derive(Deserialize)]
#[serde(deny_unknown_fields)]
struct SelectedEvidence {
    public_source_label: String,
    selected_text: String,
    observed_at: chrono::DateTime<chrono::Utc>,
}

pub(crate) async fn review_draft(
    State(state): State<AppState>,
    Extension(principal): Extension<AuthenticatedPrincipal>,
    Path(path): Path<SpacePath>,
    Json(request): Json<ReviewDraftRequest>,
) -> Result<Json<Value>, ApiError> {
    if request.evidence.len() > 16 {
        return Err(bad_request("select at most sixteen evidence snapshots"));
    }
    let (runtime, access) = binding(&state, &principal, &path.slug, path.space).await?;
    require_pull(&state, &runtime, &access).await?;
    let known = runtime
        .sync
        .verified_snapshot()
        .await
        .map_err(runtime_error)?;
    let stream = StreamId {
        origin: runtime.instance,
        canonical: request.canonical,
    };
    let expected = codifix_federation_store::preview_source(
        &state.pool()?,
        runtime.project,
        request.canonical,
    )
    .await
    .map_err(store_error)?;
    let evidence = request
        .evidence
        .into_iter()
        .map(|e| {
            let snapshot = EvidenceSnapshot {
                space: runtime.space,
                stream,
                public_source_label: e.public_source_label,
                selected_text: e.selected_text,
                observed_at: e.observed_at,
            };
            let bytes = CanonicalJsonCodec::encode(&snapshot)
                .map_err(|_| bad_request("invalid evidence"))?;
            if bytes.len() > 48 * 1024 {
                return Err(bad_request("evidence snapshot exceeds limit"));
            }
            Ok(EvidenceSelection {
                snapshot,
                snapshot_digest: ContentDigest::sha256_bytes(&bytes),
            })
        })
        .collect::<Result<Vec<_>, ApiError>>()?;
    let request = ShareReviewRequest {
        request_id: Uuid::new_v4(),
        canonical: request.canonical,
        expected,
        evidence,
        confirm_future_updates: false,
        expected_shared_version: known.content_version(stream),
    };
    Ok(Json(
        json!({"request":request,"current_shared_content":known.history(stream).last().map(|v|&v.publication.content),
        "warning":"Review exact content and evidence. Consent enables future local updates. Renewing a review can supersede an approved shared correction. Recipients and infrastructure administrators can read plaintext; delivered copies cannot be recalled."}),
    ))
}

pub(crate) async fn action(
    State(state): State<AppState>,
    Extension(principal): Extension<AuthenticatedPrincipal>,
    Path(path): Path<SpacePath>,
    Json(request): Json<ActionRequest>,
) -> Result<(StatusCode, Json<ActionResponse>), ApiError> {
    let (runtime, access) = binding(&state, &principal, &path.slug, path.space).await?;
    require_pull(&state, &runtime, &access).await?;
    let known = runtime
        .sync
        .verified_snapshot()
        .await
        .map_err(runtime_error)?;
    let response = codifix_federation_store::submit_action(
        &state.pool()?,
        &known,
        &access,
        runtime.instance,
        &request,
    )
    .await
    .map_err(store_error)?;
    Ok((StatusCode::ACCEPTED, Json(response)))
}

#[derive(Deserialize)]
#[serde(deny_unknown_fields)]
pub(crate) struct SyncRequest {
    operation: SyncOperation,
}
#[derive(Deserialize)]
#[serde(rename_all = "snake_case")]
enum SyncOperation {
    CatchUp,
    Resynchronize,
    Retry,
}
pub(crate) async fn synchronize(
    State(state): State<AppState>,
    Extension(principal): Extension<AuthenticatedPrincipal>,
    Path(path): Path<SpacePath>,
    Json(request): Json<SyncRequest>,
) -> Result<Json<Value>, ApiError> {
    let (runtime, access) = binding(&state, &principal, &path.slug, path.space).await?;
    require_pull(&state, &runtime, &access).await?;
    let cursor = match request.operation {
        SyncOperation::CatchUp => runtime.sync.catch_up().await.map_err(runtime_error)?,
        SyncOperation::Resynchronize => {
            runtime.sync.resynchronize().await.map_err(runtime_error)?
        }
        SyncOperation::Retry => {
            codifix_federation_store::retry_source_work(
                &state.pool()?,
                &access,
                runtime.space,
                runtime.instance,
            )
            .await
            .map_err(store_error)?;
            // A cycle returns the number of sent intents, not a retained
            // sequence. Read one verified, transaction-consistent snapshot
            // after the cycle without repeating any publication/pull effects.
            runtime.sync.cycle(&access).await.map_err(runtime_error)?;
            runtime
                .sync
                .verified_snapshot()
                .await
                .map_err(runtime_error)?
                .sequence()
        }
    };
    Ok(Json(
        json!({"retained_through":cursor,"scope":"this-instance","offline_copies_may_remain":true}),
    ))
}
