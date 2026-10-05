// SPDX-License-Identifier: AGPL-3.0-or-later
//! Origin-authenticated project binding and project-only, commit-safe delivery.
//! The origin service, not a device or catalog row, supplies reviewed keys.
use crate::{AuditReceipt, GatewayStore, Result, StoreError, WriterFence, audit_digest};
use mem_federation::{ContentDigest, SpaceCapability, publication::*, subscription::*};
use serde::{Deserialize, Serialize};
use sqlx::{Postgres, Row, Transaction, types::Json};
use std::collections::BTreeSet;
use uuid::Uuid;

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct ProjectBindingReceipt {
    pub operation_id: Uuid,
    pub request_digest: ContentDigest,
    pub revision: u64,
    pub head: u64,
}
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(tag = "kind", rename_all = "snake_case", deny_unknown_fields)]
pub enum ProjectEvent {
    Record {
        stream: StreamId,
        delivery: Box<PublicationDelivery>,
        receipt: AuditReceipt,
    },
    Review {
        review: ProjectStreamReview,
    },
    /// A retained cursor position whose bytes no longer have disclosure consent.
    /// This is NOT a publication record and cannot revive a local projection.
    Withheld {
        stream: StreamId,
    },
}
impl ProjectEvent {
    pub fn stream(&self) -> StreamId {
        match self {
            Self::Record { stream, .. } | Self::Withheld { stream } => *stream,
            Self::Review { review } => review.stream,
        }
    }
}
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct ProjectEntry {
    pub sequence: u64,
    pub event: ProjectEvent,
}
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct ProjectPage {
    pub request: ProjectPageRequest,
    pub authority: InstanceId,
    pub through: u64,
    pub next: u64,
    pub entries: Vec<ProjectEntry>,
}

pub fn delivery_stream(delivery: &PublicationDelivery) -> Result<StreamId> {
    match &delivery.envelope.record {
        PublicationRecord::MemoryVersion { stream, .. }
        | PublicationRecord::CanonicalState { stream, .. } => Ok(*stream),
        PublicationRecord::Evidence(e) => Ok(e.snapshot().stream),
        _ => Err(StoreError::Binding),
    }
}
fn integer(value: u64) -> Result<i64> {
    i64::try_from(value).map_err(|_| StoreError::Limits)
}
fn natural(value: i64) -> Result<u64> {
    u64::try_from(value).map_err(|_| StoreError::Corrupt)
}
impl GatewayStore {
    async fn project_actor(
        &self,
        tx: &mut Transaction<'_, Postgres>,
        fence: WriterFence,
        actor: InstanceId,
    ) -> Result<InstanceId> {
        let authority = self.lock(tx, fence).await?;
        let state = self.load(tx, fence.space, authority).await?;
        if !state.has_capability(actor, SpaceCapability::Pull)
            || !state.has_capability(actor, SpaceCapability::Publish)
        {
            return Err(StoreError::Binding);
        }
        Ok(authority)
    }

    /// Only an authenticated, currently authorized origin may assert ownership.
    /// Gateway publication/receipt rows are selected AFTER this authority check.
    /// Local human/device checks are additional origin-service responsibilities.
    pub async fn register_project(
        &self,
        fence: WriterFence,
        actor: InstanceId,
        request: &ProjectFeedRegistration,
    ) -> Result<ProjectBindingReceipt> {
        if !request.scope.valid()
            || request.scope.owner != actor
            || request.scope.space != fence.space
            || request.operation_id.is_nil()
            || request.catalog_revision == 0
            || request.policy_revision == 0
            || request.reviews.len() > 32
            || request.reviews.iter().any(|r| {
                r.stream.origin != actor
                    || r.review_id.is_nil()
                    || r.records.len() > 256
                    || (r.enabled && r.records.is_empty())
                    || r.records.iter().any(|k| k.publisher != actor)
                    || r.records.iter().collect::<BTreeSet<_>>().len() != r.records.len()
            })
            || request
                .reviews
                .iter()
                .map(|r| r.stream)
                .collect::<BTreeSet<_>>()
                .len()
                != request.reviews.len()
        {
            return Err(StoreError::Binding);
        }
        let digest = audit_digest(request)?;
        let mut tx = self.pool.begin().await?;
        let authority = self.project_actor(&mut tx, fence, actor).await?;
        let prior = sqlx::query("SELECT request_digest,result FROM federation_project_operations WHERE space=$1 AND owner=$2 AND operation_id=$3")
            .bind(fence.space.as_uuid()).bind(actor.as_uuid()).bind(request.operation_id).fetch_optional(&mut *tx).await?;
        if let Some(row) = prior {
            if row.try_get::<Json<ContentDigest>, _>("request_digest")?.0 != digest {
                return Err(StoreError::Binding);
            }
            let receipt = row.try_get::<Json<ProjectBindingReceipt>, _>("result")?.0;
            tx.commit().await?;
            return Ok(receipt); // Historical outcome only; never reapply an old grant.
        }
        sqlx::query("INSERT INTO federation_project_feeds(space,owner,project,company,area,catalog_revision,policy_revision) VALUES($1,$2,$3,$4,$5,$6,$7) ON CONFLICT DO NOTHING")
            .bind(fence.space.as_uuid()).bind(actor.as_uuid()).bind(request.scope.project).bind(request.scope.company).bind(&request.scope.area)
            .bind(integer(request.catalog_revision)?).bind(integer(request.policy_revision)?).execute(&mut *tx).await?;
        let row = sqlx::query("SELECT * FROM federation_project_feeds WHERE space=$1 AND owner=$2 AND project=$3 FOR UPDATE")
            .bind(fence.space.as_uuid()).bind(actor.as_uuid()).bind(request.scope.project).fetch_one(&mut *tx).await?;
        if row.try_get::<Uuid, _>("company")? != request.scope.company
            || row.try_get::<String, _>("area")? != request.scope.area
            || natural(row.try_get("revision")?)? != request.expected_revision
            || natural(row.try_get("catalog_revision")?)? > request.catalog_revision
            || natural(row.try_get("policy_revision")?)? > request.policy_revision
        {
            return Err(StoreError::Binding);
        }
        let mut head = natural(row.try_get("head")?)?;
        for review in &request.reviews {
            let old = sqlx::query("SELECT project,review FROM federation_project_streams WHERE space=$1 AND owner=$2 AND canonical=$3 FOR UPDATE")
                .bind(fence.space.as_uuid()).bind(actor.as_uuid()).bind(review.stream.canonical.as_uuid()).fetch_optional(&mut *tx).await?;
            if let Some(old) = &old {
                if old.try_get::<Uuid, _>("project")? != request.scope.project {
                    return Err(StoreError::Binding);
                }
                if old.try_get::<Json<ProjectStreamReview>, _>("review")?.0 == *review {
                    continue;
                }
            }
            let records = self
                .review_records(&mut tx, fence, authority, review)
                .await?;
            // Validate every dependency before persisting this project policy.
            for (delivery, _) in &records {
                match &delivery.envelope.record {
                    PublicationRecord::MemoryVersion {publication,..} => {
                        for key in &publication.evidence {
                            if !records.iter().any(|(e,_)| e.envelope.key() == *key && matches!(e.envelope.record, PublicationRecord::Evidence(_))) {
                                return Err(StoreError::Binding);
                            }
                        }
                    }
                    PublicationRecord::CanonicalState {state,..}
                        if !records.iter().any(|(m,_)| matches!(&m.envelope.record, PublicationRecord::MemoryVersion{publication,..} if publication.version == state.content_version)) => {
                        return Err(StoreError::Binding);
                    }
                    _ => {}
                }
            }
            sqlx::query("INSERT INTO federation_project_streams(space,owner,canonical,project,review) VALUES($1,$2,$3,$4,$5) ON CONFLICT(space,owner,canonical) DO UPDATE SET review=$5")
                .bind(fence.space.as_uuid()).bind(actor.as_uuid()).bind(review.stream.canonical.as_uuid()).bind(request.scope.project).bind(Json(review)).execute(&mut *tx).await?;
            // Re-emitting exact approved history on a changed review makes a new
            // recipient/restored subscription independent of formerly withheld bytes.
            for (delivery, receipt) in records {
                append(
                    &mut tx,
                    &request.scope,
                    &mut head,
                    ProjectEvent::Record {
                        stream: review.stream,
                        delivery: Box::new(delivery),
                        receipt,
                    },
                )
                .await?;
            }
            append(
                &mut tx,
                &request.scope,
                &mut head,
                ProjectEvent::Review {
                    review: review.clone(),
                },
            )
            .await?;
        }
        let revision = request
            .expected_revision
            .checked_add(1)
            .ok_or(StoreError::Limits)?;
        sqlx::query("UPDATE federation_project_feeds SET revision=$4,head=$5,catalog_revision=$6,policy_revision=$7 WHERE space=$1 AND owner=$2 AND project=$3")
            .bind(fence.space.as_uuid()).bind(actor.as_uuid()).bind(request.scope.project).bind(integer(revision)?).bind(integer(head)?)
            .bind(integer(request.catalog_revision)?).bind(integer(request.policy_revision)?).execute(&mut *tx).await?;
        let receipt = ProjectBindingReceipt {
            operation_id: request.operation_id,
            request_digest: digest.clone(),
            revision,
            head,
        };
        sqlx::query("INSERT INTO federation_project_operations(space,owner,operation_id,request_digest,result) VALUES($1,$2,$3,$4,$5)")
            .bind(fence.space.as_uuid()).bind(actor.as_uuid()).bind(request.operation_id).bind(Json(digest)).bind(Json(&receipt)).execute(&mut *tx).await?;
        tx.commit().await?;
        Ok(receipt)
    }

    async fn review_records(
        &self,
        tx: &mut Transaction<'_, Postgres>,
        fence: WriterFence,
        authority: InstanceId,
        review: &ProjectStreamReview,
    ) -> Result<Vec<(PublicationDelivery, AuditReceipt)>> {
        if !review.enabled {
            return Ok(Vec::new());
        }
        let ids: Vec<_> = review.records.iter().map(|k| k.id.as_uuid()).collect();
        let rows = sqlx::query("SELECT a.delivery,r.receipt FROM federation_accepted a JOIN federation_audit r USING(space,sequence) JOIN federation_gateway_spaces g USING(space) WHERE a.space=$1 AND a.publisher=$2 AND a.record_id=ANY($3) AND a.sequence<=g.published_sequence ORDER BY a.sequence")
            .bind(fence.space.as_uuid()).bind(review.stream.origin.as_uuid()).bind(ids).fetch_all(&mut **tx).await?;
        if rows.len() != review.records.len() {
            return Err(StoreError::Binding);
        }
        let keys: Json<Vec<crate::ReceiptKey>> =
            sqlx::query_scalar("SELECT keys FROM federation_security WHERE space=$1")
                .bind(fence.space.as_uuid())
                .fetch_one(&mut **tx)
                .await?;
        let mut records = Vec::new();
        for row in rows {
            let delivery = row.try_get::<Json<PublicationDelivery>, _>("delivery")?.0;
            let receipt = row.try_get::<Json<AuditReceipt>, _>("receipt")?.0;
            if delivery_stream(&delivery)? != review.stream
                || delivery.envelope.publisher != review.stream.origin
                || delivery.envelope.space != fence.space
            {
                return Err(StoreError::Binding);
            }
            crate::receipt_matches(&receipt, &delivery, authority, &keys)?;
            records.push((delivery, receipt));
        }
        Ok(records)
    }

    pub async fn project_page(
        &self,
        fence: WriterFence,
        actor: InstanceId,
        request: &ProjectPageRequest,
    ) -> Result<ProjectPage> {
        if !request.valid() || request.scope.owner != actor || request.scope.space != fence.space {
            return Err(StoreError::Binding);
        }
        let mut tx = self.pool.begin().await?;
        let authority = self.project_actor(&mut tx, fence, actor).await?;
        let row=sqlx::query("SELECT * FROM federation_project_feeds WHERE space=$1 AND owner=$2 AND project=$3 FOR SHARE")
            .bind(fence.space.as_uuid()).bind(actor.as_uuid()).bind(request.scope.project).fetch_optional(&mut *tx).await?.ok_or(StoreError::Binding)?;
        if row.try_get::<Uuid, _>("company")? != request.scope.company
            || row.try_get::<String, _>("area")? != request.scope.area
            || natural(row.try_get("catalog_revision")?)? != request.catalog_revision
            || natural(row.try_get("policy_revision")?)? != request.policy_revision
            || natural(row.try_get("revision")?)? != request.binding_revision
        {
            return Err(StoreError::Resync);
        }
        let head = natural(row.try_get("head")?)?;
        let through = request.through.unwrap_or(head);
        if request.after > through
            || through > head
            || request.after < natural(row.try_get("retained_after")?)?
        {
            return Err(StoreError::Resync);
        }
        // Selection occurs in SQL within exactly one authenticated project. No
        // broad space page/CAR is fetched and filtered on the recipient.
        let rows=sqlx::query("SELECT sequence,event FROM federation_project_events WHERE space=$1 AND owner=$2 AND project=$3 AND sequence>$4 AND sequence<=$5 ORDER BY sequence LIMIT $6")
            .bind(fence.space.as_uuid()).bind(actor.as_uuid()).bind(request.scope.project).bind(integer(request.after)?).bind(integer(through)?)
            .bind(i64::from(request.limit)).fetch_all(&mut *tx).await?;
        let mut entries = Vec::new();
        for row in rows {
            let sequence = natural(row.try_get("sequence")?)?;
            if sequence != request.after + entries.len() as u64 + 1 {
                return Err(StoreError::Resync);
            }
            let event = row.try_get::<Json<ProjectEvent>, _>("event")?.0;
            let stream = event.stream();
            let current:Json<ProjectStreamReview>=sqlx::query_scalar("SELECT review FROM federation_project_streams WHERE space=$1 AND owner=$2 AND project=$3 AND canonical=$4")
                .bind(fence.space.as_uuid()).bind(actor.as_uuid()).bind(request.scope.project).bind(stream.canonical.as_uuid()).fetch_one(&mut *tx).await?;
            let event = match event {
                ProjectEvent::Record {
                    delivery, receipt, ..
                } if current.enabled && current.records.contains(&delivery.envelope.key()) => {
                    ProjectEvent::Record {
                        stream,
                        delivery,
                        receipt,
                    }
                }
                ProjectEvent::Review { .. } => ProjectEvent::Review { review: current.0 },
                _ => ProjectEvent::Withheld { stream },
            };
            entries.push(ProjectEntry { sequence, event });
        }
        if entries.len() != (through - request.after).min(u64::from(request.limit)) as usize {
            return Err(StoreError::Resync);
        }
        let page = ProjectPage {
            request: request.clone(),
            authority,
            through,
            next: request.after + entries.len() as u64,
            entries,
        };
        if serde_json::to_vec(&page)?.len() > 3 * 1024 * 1024 {
            return Err(StoreError::Limits);
        }
        tx.commit().await?;
        Ok(page)
    }
}
async fn append(
    tx: &mut Transaction<'_, Postgres>,
    scope: &ProjectScope,
    head: &mut u64,
    event: ProjectEvent,
) -> Result<()> {
    *head = head.checked_add(1).ok_or(StoreError::Limits)?;
    sqlx::query("INSERT INTO federation_project_events(space,owner,project,sequence,event) VALUES($1,$2,$3,$4,$5)")
        .bind(scope.space.as_uuid()).bind(scope.owner.as_uuid()).bind(scope.project).bind(integer(*head)?).bind(Json(event)).execute(&mut **tx).await?;
    Ok(())
}
