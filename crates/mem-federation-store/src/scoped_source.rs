// SPDX-License-Identifier: AGPL-3.0-or-later
//! Current, project-bound source selection. Never a union of historical outbox
//! records. The owning service resolves and locks reviewer permissions first.
use crate::{Result, StoreError};
use mem_federation::{
    CanonicalJsonCodec, ContentDigest, SpaceCapability, publication::*,
    subscription::ProjectStreamReview,
};
use mem_record::{MemoryStatus, Permission, PermissionSet};
use serde_json::json;
use sqlx::{Postgres, Row, Transaction, types::Json};
use std::collections::BTreeMap;
use uuid::Uuid;

pub struct CurrentProjectSources {
    pub reviews: Vec<ProjectStreamReview>,
    pub records: BTreeMap<RecordKey, PublicationDelivery>,
    pub fingerprint: ContentDigest,
}
/// Enumerating is not authorization. A new/changed reviewer discovered by the
/// locked builder is rejected unless the service resolved that identity too.
pub async fn project_source_reviewers(
    tx: &mut Transaction<'_, Postgres>,
    project: Uuid,
    space: SpaceId,
    owner: InstanceId,
) -> Result<Vec<Uuid>> {
    Ok(sqlx::query_scalar("SELECT DISTINCT principal_id FROM federation_source_subscriptions WHERE project_id=$1 AND space=$2 AND instance=$3 ORDER BY principal_id LIMIT 33")
        .bind(project).bind(space.as_uuid()).bind(owner.as_uuid()).fetch_all(&mut **tx).await?)
}

pub async fn current_project_sources(
    tx: &mut Transaction<'_, Postgres>,
    project: Uuid,
    space: SpaceId,
    owner: InstanceId,
    known: &PublicationState,
    reviewers: &BTreeMap<Uuid, PermissionSet>,
) -> Result<CurrentProjectSources> {
    if known.space() != space {
        return Err(StoreError::Binding);
    }
    // Grant -> canonical -> subscription/review -> exact outbox, with sorted
    // principals/canonicals/record IDs at each layer. No SQL locks cross HTTP.
    let mut grants = BTreeMap::new();
    for principal in reviewers.keys() {
        let grant: Option<Json<Vec<SpaceCapability>>>=sqlx::query_scalar("SELECT capabilities FROM federation_local_grants WHERE space=$1 AND instance=$2 AND project_id=$3 AND principal_id=$4 AND enabled FOR SHARE")
            .bind(space.as_uuid()).bind(owner.as_uuid()).bind(project).bind(principal).fetch_optional(&mut **tx).await?;
        grants.insert(*principal, grant.map(|v| v.0).unwrap_or_default());
    }
    let canonical_ids: Vec<Uuid>=sqlx::query_scalar("SELECT canonical_id FROM federation_source_subscriptions WHERE project_id=$1 AND space=$2 AND instance=$3 ORDER BY canonical_id LIMIT 33")
        .bind(project).bind(space.as_uuid()).bind(owner.as_uuid()).fetch_all(&mut **tx).await?;
    if canonical_ids.len() > 32 || reviewers.len() > 32 {
        return Err(StoreError::Limits);
    }
    for canonical in &canonical_ids {
        sqlx::query("SELECT canonical_id FROM memory_canonical_state WHERE project_id=$1 AND canonical_id=$2 FOR SHARE")
            .bind(project).bind(canonical).fetch_optional(&mut **tx).await?.ok_or(StoreError::Binding)?;
    }
    let subscriptions=sqlx::query("SELECT * FROM federation_source_subscriptions WHERE project_id=$1 AND space=$2 AND instance=$3 ORDER BY canonical_id FOR SHARE")
        .bind(project).bind(space.as_uuid()).bind(owner.as_uuid()).fetch_all(&mut **tx).await?;
    if subscriptions.len() != canonical_ids.len() {
        return Err(StoreError::Binding);
    }
    let mut reviews = Vec::new();
    let mut records = BTreeMap::new();
    let mut observed = Vec::new();
    for sub in subscriptions {
        let principal: Uuid = sub.try_get("principal_id")?;
        let permissions = reviewers.get(&principal).ok_or(StoreError::Binding)?;
        let canonical: Uuid = sub.try_get("canonical_id")?;
        let stream = StreamId {
            origin: owner,
            canonical: RecordId::new(canonical)?,
        };
        let review_id: Uuid = sub.try_get("review_id")?;
        let enabled: bool = sub.try_get("enabled")?;
        let caps = grants.get(&principal).ok_or(StoreError::Binding)?;
        let consent = enabled
            && [
                Permission::MemoryRead,
                Permission::MemoryCurate,
                Permission::BundleExport,
            ]
            .iter()
            .all(|p| permissions.contains(*p))
            && caps.contains(&SpaceCapability::Publish)
            && caps.contains(&SpaceCapability::Pull);
        let state=sqlx::query("SELECT status,confidence FROM memory_canonical_state WHERE project_id=$1 AND canonical_id=$2")
            .bind(project).bind(canonical).fetch_one(&mut **tx).await?;
        let latest=sqlx::query("SELECT id,version_no,is_tombstone FROM memory_entries WHERE project_id=$1 AND canonical_id=$2 ORDER BY version_no DESC LIMIT 1 FOR SHARE")
            .bind(project).bind(canonical).fetch_optional(&mut **tx).await?;
        let live = latest
            .as_ref()
            .is_some_and(|r| r.try_get::<bool, _>("is_tombstone").ok() == Some(false))
            && state.try_get::<String, _>("status")? == "active";
        let row = sqlx::query("SELECT * FROM federation_publication_reviews WHERE id=$1 FOR SHARE")
            .bind(review_id)
            .fetch_one(&mut **tx)
            .await?;
        if row.try_get::<Uuid, _>("project_id")? != project
            || row.try_get::<Uuid, _>("space")? != space.as_uuid()
            || row.try_get::<Uuid, _>("instance")? != owner.as_uuid()
            || row.try_get::<Uuid, _>("canonical_id")? != canonical
            || row.try_get::<Uuid, _>("reviewer")? != principal
        {
            return Err(StoreError::Binding);
        }
        // A standing review may follow later source versions, but its original
        // source must still be exactly this project's canonical lineage.
        let lineage:bool=sqlx::query_scalar("SELECT EXISTS(SELECT 1 FROM memory_entries WHERE id=$1 AND project_id=$2 AND canonical_id=$3)")
            .bind(row.try_get::<Uuid,_>("source_id")?).bind(project).bind(canonical).fetch_one(&mut **tx).await?;
        if !lineage {
            return Err(StoreError::Binding);
        }
        let evidence: Vec<RecordKey> = sub.try_get::<Json<Vec<RecordKey>>, _>("evidence")?.0;
        let reviewed: Vec<ReviewedEvidence> =
            row.try_get::<Json<Vec<ReviewedEvidence>>, _>("evidence")?.0;
        let selected = known.history(stream).last();
        let shared_state = known.current_state(stream);
        let mut eligible = false;
        let mut keys = Vec::new();
        if consent && live {
            if let (Some(version), Some(shared_state)) = (selected, shared_state) {
                let expected = crate::sharing::selection(tx, project, stream.canonical).await?;
                eligible = version.publication.source == expected.source
                    && version.publication.content == expected.content
                    && version.publication.evidence == evidence
                    && shared_state.status == MemoryStatus::Active
                    && !shared_state.tombstone
                    && shared_state.confidence
                        == ConfidencePpm::from_confidence(state.try_get("confidence")?)?
                    && shared_state.content_version == version.publication.version
                    && evidence.len() == reviewed.len();
                if eligible {
                    keys.extend(evidence.iter().copied());
                    keys.push(version.key);
                    let state_key = known
                        .state_history(stream)
                        .last()
                        .ok_or(StoreError::Binding)?
                        .0;
                    keys.push(state_key);
                    for (key, item) in evidence.iter().zip(&reviewed) {
                        if key.publisher != owner
                            || known.evidence(*key) != Some(item)
                            || item.snapshot().space != space
                            || item.snapshot().stream != stream
                            || item.review().reviewer.as_str() != principal.to_string()
                        {
                            return Err(StoreError::Binding);
                        }
                    }
                    keys.sort();
                    keys.dedup();
                    if keys.len() > 256 {
                        return Err(StoreError::Limits);
                    }
                    for key in &keys {
                        let delivery = known.accepted_record(*key).ok_or(StoreError::Binding)?;
                        if crate::delivery_stream(delivery)? != stream {
                            return Err(StoreError::Binding);
                        }
                        // Exact accepted source intent, not an arbitrary record
                        // sharing a locator or historical canonical UUID.
                        let outbox=sqlx::query("SELECT envelope,acceptance FROM federation_source_outbox WHERE space=$1 AND publisher=$2 AND record_id=$3 AND project_id=$4 AND principal_id=$5 FOR SHARE")
                            .bind(space.as_uuid()).bind(owner.as_uuid()).bind(key.id.as_uuid()).bind(project).bind(principal)
                            .fetch_optional(&mut **tx).await?.ok_or(StoreError::Binding)?;
                        let envelope: PublicationEnvelope = outbox
                            .try_get::<Json<PublicationEnvelope>, _>("envelope")?
                            .0;
                        let acceptance: Option<Json<PublicationDelivery>> =
                            outbox.try_get("acceptance")?;
                        if envelope != delivery.envelope
                            || acceptance.map(|a| a.0) != Some(delivery.clone())
                        {
                            return Err(StoreError::Binding);
                        }
                        records.insert(*key, delivery.clone());
                    }
                }
            }
        }
        observed.push(json!({"stream":stream,"principal":principal,"permissions":permissions.names(),"caps":caps,"review":review_id,
            "enabled":enabled,"eligible":eligible,"source":latest.map(|r|json!({"id":r.try_get::<Uuid,_>("id").ok(),"version":r.try_get::<i32,_>("version_no").ok(),"tombstone":r.try_get::<bool,_>("is_tombstone").ok()})),
            "state":state.try_get::<String,_>("status")?,"confidence":ConfidencePpm::from_confidence(state.try_get("confidence")?)?,"keys":keys}));
        if eligible {
            reviews.push(ProjectStreamReview {
                stream,
                review_id,
                records: keys,
                enabled: true,
            });
        }
    }
    if records.len() > 256 {
        return Err(StoreError::Limits);
    }
    let fingerprint = CanonicalJsonCodec::digest(
        &json!({"observed":observed,"reviews":reviews,"records":records.values().collect::<Vec<_>>()}),
    ).map_err(|_|StoreError::Binding)?;
    Ok(CurrentProjectSources {
        reviews,
        records,
        fingerprint,
    })
}
