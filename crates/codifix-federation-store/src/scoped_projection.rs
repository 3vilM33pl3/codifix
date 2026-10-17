// SPDX-License-Identifier: AGPL-3.0-or-later
//! Separate read-only laptop projection. Callers establish gateway authenticity
//! before applying a page; storage independently checks scope, digests and CAS.
use crate::{ProjectEvent, ProjectPage, Result, StoreError, audit_digest, delivery_stream};
use chrono::{DateTime, Utc};
use codifix_federation::{
    AuthorIdentity, CanonicalJsonCodec, ContentDigest, RecordLocator, publication::*,
    subscription::*,
};
use serde::{Deserialize, Serialize};
use sqlx::{PgPool, Row, types::Json};
use uuid::Uuid;

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize, Default)]
#[serde(deny_unknown_fields)]
struct ReadOnlyProjection {
    records: Vec<PublicationDelivery>,
    reviews: Vec<ProjectStreamReview>,
}
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct ProjectSearchHit {
    pub scope: ProjectScope,
    pub stream: StreamId,
    pub author: AuthorIdentity,
    pub authored_at: DateTime<Utc>,
    pub locator: RecordLocator,
    pub digest: ContentDigest,
    pub shared_version: SharedVersion,
    pub source: SourceVersionRef,
    pub content: MemoryContent,
    pub state: StatePublication,
    pub evidence: Vec<ReviewedEvidence>,
    pub read_only: bool,
}
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct ProjectProjectionStatus {
    pub cursor: u64,
    /// Last committed verified page, not a claim of current network authority.
    pub observed_at: Option<DateTime<Utc>>,
}
#[derive(Clone)]
pub struct ProjectProjectionStore {
    pool: PgPool,
    subscription: Uuid,
    device: Uuid,
    scope: ProjectScope,
}
impl ProjectProjectionStore {
    /// Inspection/search never creates a subscription or changes its cursor.
    pub async fn open(
        pool: PgPool,
        subscription: Uuid,
        device: Uuid,
        scope: ProjectScope,
    ) -> Result<Self> {
        if subscription.is_nil() || device.is_nil() || !scope.valid() {
            return Err(StoreError::Binding);
        }
        let projection = Self {
            pool,
            subscription,
            device,
            scope,
        };
        projection.status().await?;
        Ok(projection)
    }
    /// The trusted onboarding operator supplies these identities, not an
    /// untrusted page. This cannot merge with local authoritative memories.
    pub async fn initialize(
        pool: PgPool,
        subscription: Uuid,
        device: Uuid,
        scope: ProjectScope,
    ) -> Result<Self> {
        if subscription.is_nil() || device.is_nil() || !scope.valid() {
            return Err(StoreError::Binding);
        }
        let mut tx = pool.begin().await?;
        sqlx::query("INSERT INTO onboarding_project_projections(subscription,device,scope,projection) VALUES($1,$2,$3,$4) ON CONFLICT DO NOTHING")
            .bind(subscription).bind(device).bind(Json(&scope)).bind(Json(ReadOnlyProjection::default())).execute(&mut *tx).await?;
        let row=sqlx::query("SELECT device,scope FROM onboarding_project_projections WHERE subscription=$1 FOR SHARE").bind(subscription).fetch_one(&mut *tx).await?;
        if row.try_get::<Uuid, _>("device")? != device
            || row.try_get::<Json<ProjectScope>, _>("scope")?.0 != scope
        {
            return Err(StoreError::Binding);
        }
        tx.commit().await?;
        Ok(Self {
            pool,
            subscription,
            device,
            scope,
        })
    }
    pub async fn cursor(&self) -> Result<u64> {
        let n: i64 = sqlx::query_scalar(
            "SELECT cursor FROM onboarding_project_projections WHERE subscription=$1",
        )
        .bind(self.subscription)
        .fetch_one(&self.pool)
        .await?;
        u64::try_from(n).map_err(|_| StoreError::Corrupt)
    }
    pub async fn status(&self) -> Result<ProjectProjectionStatus> {
        let row = sqlx::query("SELECT device,scope,cursor,observed_at FROM onboarding_project_projections WHERE subscription=$1")
            .bind(self.subscription).fetch_one(&self.pool).await?;
        if row.try_get::<Uuid, _>("device")? != self.device
            || row.try_get::<Json<ProjectScope>, _>("scope")?.0 != self.scope
        {
            return Err(StoreError::Binding);
        }
        Ok(ProjectProjectionStatus {
            cursor: u64::try_from(row.try_get::<i64, _>("cursor")?)
                .map_err(|_| StoreError::Corrupt)?,
            observed_at: row.try_get("observed_at")?,
        })
    }
    /// Exact replay is harmless, including lost acknowledgements after commit.
    /// Altered bytes under one nonce fail before any projection/cursor effect.
    /// Page, accepted projection and cursor commit together under one row lock.
    pub async fn apply_verified(&self, page: &ProjectPage) -> Result<u64> {
        if !page.request.valid()
            || page.request.device != self.device
            || page.request.scope != self.scope
            || page.next != page.request.after + page.entries.len() as u64
            || page.through < page.next
            || page.entries.len()
                != (page.through - page.request.after).min(u64::from(page.request.limit)) as usize
        {
            return Err(StoreError::Binding);
        }
        let digest = audit_digest(page)?;
        let mut tx = self.pool.begin().await?;
        let row=sqlx::query("SELECT device,scope,cursor,projection FROM onboarding_project_projections WHERE subscription=$1 FOR UPDATE").bind(self.subscription).fetch_one(&mut *tx).await?;
        if row.try_get::<Uuid, _>("device")? != self.device
            || row.try_get::<Json<ProjectScope>, _>("scope")?.0 != self.scope
        {
            return Err(StoreError::Binding);
        }
        let cursor =
            u64::try_from(row.try_get::<i64, _>("cursor")?).map_err(|_| StoreError::Corrupt)?;
        let prior: Option<Json<ContentDigest>> = sqlx::query_scalar(
            "SELECT page_digest FROM onboarding_project_pages WHERE subscription=$1 AND nonce=$2",
        )
        .bind(self.subscription)
        .bind(page.request.nonce)
        .fetch_optional(&mut *tx)
        .await?;
        if let Some(prior) = prior {
            if prior.0 != digest {
                return Err(StoreError::Binding);
            }
            tx.commit().await?;
            return Ok(cursor);
        }
        if cursor != page.request.after {
            return Err(StoreError::Resync);
        }
        let mut projection = row.try_get::<Json<ReadOnlyProjection>, _>("projection")?.0;
        for (index, entry) in page.entries.iter().enumerate() {
            if entry.sequence != cursor + index as u64 + 1
                || entry.event.stream().origin != self.scope.owner
            {
                return Err(StoreError::Binding);
            }
            match &entry.event {
                ProjectEvent::Record {
                    stream,
                    delivery,
                    receipt,
                } => {
                    if delivery.envelope.publisher != self.scope.owner
                        || delivery.envelope.space != self.scope.space
                        || delivery_stream(delivery)? != *stream
                        || !delivery.envelope.supported_schema()
                        || CanonicalJsonCodec::digest(&delivery.envelope)
                            .map_err(|_| StoreError::Binding)?
                            != delivery.digest
                        || receipt.claims.space != self.scope.space
                        || receipt.claims.authority != page.authority
                        || receipt.claims.sequence != delivery.sequence.get()
                        || receipt.claims.delivery_digest != audit_digest(delivery)?
                    {
                        return Err(StoreError::Binding);
                    }
                    if let Some(old) = projection
                        .records
                        .iter()
                        .find(|old| old.envelope.key() == delivery.envelope.key())
                    {
                        if old != delivery.as_ref() {
                            return Err(StoreError::Binding);
                        }
                    } else {
                        // One origin/shared-version or state revision cannot
                        // acquire two different records in this projection.
                        if projection
                            .records
                            .iter()
                            .any(|old| same_revision(old, delivery))
                        {
                            return Err(StoreError::Binding);
                        }
                        projection.records.push(delivery.as_ref().clone());
                    }
                }
                ProjectEvent::Review { review } => {
                    if review.review_id.is_nil()
                        || review.records.len() > 256
                        || review
                            .records
                            .iter()
                            .any(|key| key.publisher != self.scope.owner)
                    {
                        return Err(StoreError::Binding);
                    }
                    if let Some(old) = projection
                        .reviews
                        .iter_mut()
                        .find(|r| r.stream == review.stream)
                    {
                        *old = review.clone();
                    } else {
                        projection.reviews.push(review.clone());
                    }
                }
                ProjectEvent::Withheld { .. } => {} // No hidden bytes and no authority inferred.
            }
        }
        if serde_json::to_vec(&projection)?.len() > 16 * 1024 * 1024 {
            return Err(StoreError::Limits);
        }
        let next = i64::try_from(page.next).map_err(|_| StoreError::Limits)?;
        sqlx::query("UPDATE onboarding_project_projections SET cursor=$2,projection=$3,observed_at=clock_timestamp() WHERE subscription=$1")
            .bind(self.subscription).bind(next).bind(Json(projection)).execute(&mut *tx).await?;
        sqlx::query("INSERT INTO onboarding_project_pages(subscription,nonce,page_digest,through_cursor) VALUES($1,$2,$3,$4)")
            .bind(self.subscription).bind(page.request.nonce).bind(Json(digest)).bind(next).execute(&mut *tx).await?;
        tx.commit().await?;
        Ok(page.next)
    }
    /// Bounded lexical search, no embeddings/learning/model writeback. All
    /// identities and citations remain exact reviewed publication metadata.
    pub async fn search(&self, query: &str, limit: u32) -> Result<Vec<ProjectSearchHit>> {
        Ok(self.search_with_status(query, limit).await?.1)
    }
    /// The displayed freshness/cursor belongs to the same committed projection
    /// as these results, even if synchronization commits during inspection.
    pub async fn search_with_status(
        &self,
        query: &str,
        limit: u32,
    ) -> Result<(ProjectProjectionStatus, Vec<ProjectSearchHit>)> {
        if query.is_empty() || query.len() > 4096 || !(1..=50).contains(&limit) {
            return Err(StoreError::Limits);
        }
        let row = sqlx::query(
            "SELECT device,scope,cursor,observed_at,projection FROM onboarding_project_projections WHERE subscription=$1",
        )
        .bind(self.subscription)
        .fetch_one(&self.pool)
        .await?;
        if row.try_get::<Uuid, _>("device")? != self.device
            || row.try_get::<Json<ProjectScope>, _>("scope")?.0 != self.scope
        {
            return Err(StoreError::Binding);
        }
        let status = ProjectProjectionStatus {
            cursor: u64::try_from(row.try_get::<i64, _>("cursor")?)
                .map_err(|_| StoreError::Corrupt)?,
            observed_at: row.try_get("observed_at")?,
        };
        let projection = row.try_get::<Json<ReadOnlyProjection>, _>("projection")?.0;
        let tokens: Vec<_> = query
            .split_whitespace()
            .take(32)
            .map(str::to_lowercase)
            .collect();
        let mut hits = Vec::new();
        for review in projection.reviews.iter().filter(|r| r.enabled) {
            let latest = projection
                .records
                .iter()
                .filter(|d| delivery_stream(d).ok() == Some(review.stream))
                .filter_map(|d| match &d.envelope.record {
                    PublicationRecord::MemoryVersion { publication, .. } => Some((d, publication)),
                    _ => None,
                })
                .max_by_key(|(_, m)| m.version);
            let Some((record, memory)) = latest else {
                continue;
            };
            if !review.records.contains(&record.envelope.key()) {
                continue;
            }
            let state = projection
                .records
                .iter()
                .filter(|d| delivery_stream(d).ok() == Some(review.stream))
                .filter_map(|d| match &d.envelope.record {
                    PublicationRecord::CanonicalState { state, .. } => Some((d, state)),
                    _ => None,
                })
                .max_by_key(|(_, s)| s.revision);
            let Some((state_record, state)) = state else {
                continue;
            };
            if state.tombstone
                || state.status != codifix_record::MemoryStatus::Active
                || state.content_version != memory.version
                || !review.records.contains(&state_record.envelope.key())
            {
                continue;
            }
            let mut evidence = Vec::new();
            for key in &memory.evidence {
                if !review.records.contains(key) {
                    break;
                }
                if let Some(e) = projection.records.iter().find_map(|d| {
                    if d.envelope.key() != *key {
                        return None;
                    }
                    match &d.envelope.record {
                        PublicationRecord::Evidence(e) if e.snapshot().stream == review.stream => {
                            Some(e)
                        }
                        _ => None,
                    }
                }) {
                    evidence.push(e.clone());
                } else {
                    break;
                }
            }
            if evidence.len() != memory.evidence.len() {
                continue;
            }
            let text = format!(
                "{} {} {}",
                memory.content.text,
                memory.content.summary,
                memory.content.tags.join(" ")
            )
            .to_lowercase();
            let score = tokens
                .iter()
                .filter(|token| text.contains(token.as_str()))
                .count();
            if score == 0 {
                continue;
            }
            hits.push((
                score,
                ProjectSearchHit {
                    scope: self.scope.clone(),
                    stream: review.stream,
                    author: record.envelope.author.clone(),
                    authored_at: record.envelope.authored_at,
                    locator: record.locator.clone(),
                    digest: record.digest.clone(),
                    shared_version: memory.version,
                    source: memory.source.clone(),
                    content: memory.content.clone(),
                    state: state.clone(),
                    evidence,
                    read_only: true,
                },
            ));
        }
        hits.sort_by(|(a, ha), (b, hb)| b.cmp(a).then_with(|| ha.stream.cmp(&hb.stream)));
        Ok((
            status,
            hits.into_iter()
                .take(limit as usize)
                .map(|(_, h)| h)
                .collect(),
        ))
    }
}
fn same_revision(a: &PublicationDelivery, b: &PublicationDelivery) -> bool {
    match (&a.envelope.record, &b.envelope.record) {
        (
            PublicationRecord::MemoryVersion {
                stream: a,
                publication: ma,
            },
            PublicationRecord::MemoryVersion {
                stream: b,
                publication: mb,
            },
        ) => a == b && ma.version == mb.version,
        (
            PublicationRecord::CanonicalState {
                stream: a,
                state: sa,
            },
            PublicationRecord::CanonicalState {
                stream: b,
                state: sb,
            },
        ) => a == b && sa.revision == sb.revision,
        _ => false,
    }
}
