// SPDX-License-Identifier: AGPL-3.0-or-later
//! Explicit standing sharing consent and transactional automatic source capture.
use crate::{PublicationIntent, Result, StoreError, source::persist_intent};
use chrono::{DateTime, Utc};
use mem_federation::{AuthorIdentity, RecordLocator, SpaceCapability, publication::*};
use mem_record::{MemoryStatus, Permission};
use serde::{Deserialize, Serialize};
use sqlx::{PgPool, Postgres, Row, Transaction, types::Json};
use uuid::Uuid;

/// The preview is compared again while holding the canonical-state row lock.
/// Evidence is separately reviewed and must not be derived from raw sources.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct SourceSelection {
    pub source: SourceVersionRef,
    pub content: MemoryContent,
}

pub async fn preview_source(
    pool: &PgPool,
    project: Uuid,
    canonical: RecordId,
) -> Result<SourceSelection> {
    let mut tx = pool.begin().await?;
    let selection = selection(&mut tx, project, canonical).await?;
    tx.commit().await?;
    Ok(selection)
}
async fn selection(
    tx: &mut Transaction<'_, Postgres>,
    project: Uuid,
    canonical: RecordId,
) -> Result<SourceSelection> {
    let row=sqlx::query("SELECT id,version_no,canonical_text,summary,memory_type,confidence,importance,is_tombstone FROM memory_entries WHERE project_id=$1 AND canonical_id=$2 ORDER BY version_no DESC LIMIT 1")
        .bind(project).bind(canonical.as_uuid()).fetch_one(&mut **tx).await?;
    if row.try_get::<bool, _>("is_tombstone")? {
        return Err(StoreError::Binding);
    }
    let id: Uuid = row.try_get("id")?;
    let tags =
        sqlx::query_scalar("SELECT tag FROM memory_tags WHERE memory_entry_id=$1 ORDER BY tag")
            .bind(id)
            .fetch_all(&mut **tx)
            .await?;
    Ok(SourceSelection {
        source: SourceVersionRef {
            record_id: RecordId::new(id)?,
            version: SourceVersion::new(
                u32::try_from(row.try_get::<i32, _>("version_no")?)
                    .map_err(|_| StoreError::Corrupt)?,
            )?,
        },
        content: MemoryContent {
            text: row.try_get("canonical_text")?,
            summary: row.try_get("summary")?,
            memory_type: serde_json::from_value(serde_json::Value::String(
                row.try_get::<String, _>("memory_type")?,
            ))?,
            confidence: ConfidencePpm::from_confidence(row.try_get("confidence")?)?,
            importance: row.try_get("importance")?,
            tags,
        },
    })
}

/// Called by the locally authorized administration path, not a remote role map.
pub async fn set_local_grant(
    tx: &mut Transaction<'_, Postgres>,
    actor: &LocalProjectAccess,
    grant: &LocalSpaceGrant,
    enabled: bool,
) -> Result<()> {
    if actor.read_only
        || actor.project != grant.project
        || !actor.permissions.contains(Permission::AuthManage)
        || !actor.permissions.contains(Permission::MemoryRead)
    {
        return Err(StoreError::Binding);
    }
    sqlx::query("INSERT INTO federation_local_grants(space,instance,project_id,principal_id,capabilities,enabled) VALUES($1,$2,$3,$4,$5,$6) ON CONFLICT(space,instance,project_id,principal_id) DO UPDATE SET capabilities=EXCLUDED.capabilities,enabled=EXCLUDED.enabled")
        .bind(grant.space.as_uuid()).bind(grant.instance.as_uuid()).bind(grant.project).bind(grant.principal).bind(Json(&grant.capabilities)).bind(enabled).execute(&mut **tx).await?;
    Ok(())
}
pub(crate) async fn load_grant(
    tx: &mut Transaction<'_, Postgres>,
    access: &LocalProjectAccess,
    space: SpaceId,
    instance: InstanceId,
) -> Result<LocalSpaceGrant> {
    let capabilities:Json<Vec<SpaceCapability>>=sqlx::query_scalar("SELECT g.capabilities FROM federation_local_grants g JOIN auth_principals p ON p.id=g.principal_id WHERE g.space=$1 AND g.instance=$2 AND g.project_id=$3 AND g.principal_id=$4 AND g.enabled AND p.disabled_at IS NULL FOR SHARE OF g,p")
        .bind(space.as_uuid()).bind(instance.as_uuid()).bind(access.project).bind(access.principal).fetch_optional(&mut **tx).await?.ok_or(StoreError::LocalPolicy(LocalPolicyReject::MissingLocalGrant))?;
    Ok(LocalSpaceGrant {
        space,
        instance,
        project: access.project,
        principal: access.principal,
        capabilities: capabilities.0,
    })
}

pub async fn authorize_source_pull(
    pool: &PgPool,
    access: &LocalProjectAccess,
    space: SpaceId,
    instance: InstanceId,
) -> Result<()> {
    let mut tx = pool.begin().await?;
    let grant = load_grant(&mut tx, access, space, instance).await?;
    if !access.permissions.contains(Permission::MemoryRead)
        || !grant.capabilities.contains(&SpaceCapability::Pull)
    {
        return Err(StoreError::Binding);
    }
    tx.commit().await?;
    Ok(())
}

pub async fn authorize_source_intent(
    pool: &PgPool,
    known: &PublicationState,
    access: &LocalProjectAccess,
    instance: InstanceId,
    intent: &PublicationIntent,
) -> Result<()> {
    let mut tx = pool.begin().await?;
    let grant = load_grant(&mut tx, access, known.space(), instance).await?;
    known
        .authorize_publication(access, Some(&grant), &intent.envelope)
        .map_err(StoreError::LocalPolicy)?;
    tx.commit().await?;
    Ok(())
}

pub async fn retry_source_work(
    pool: &PgPool,
    access: &LocalProjectAccess,
    space: SpaceId,
    instance: InstanceId,
) -> Result<()> {
    if access.read_only
        || !access.permissions.contains(Permission::MemoryCurate)
        || !access.permissions.contains(Permission::BundleExport)
    {
        return Err(StoreError::Binding);
    }
    let mut tx = pool.begin().await?;
    load_grant(&mut tx, access, space, instance).await?;
    sqlx::query("UPDATE federation_source_outbox SET attempts=0,blocked=FALSE,last_error=NULL,available_at=clock_timestamp() WHERE space=$1 AND publisher=$2 AND project_id=$3 AND principal_id=$4 AND acceptance IS NULL")
        .bind(space.as_uuid()).bind(instance.as_uuid()).bind(access.project).bind(access.principal).execute(&mut *tx).await?;
    sqlx::query("UPDATE federation_replicas SET sync_failures=0,sync_error=NULL,retry_at=clock_timestamp() WHERE space=$1 AND receiver=$2")
        .bind(space.as_uuid()).bind(instance.as_uuid()).execute(&mut *tx).await?;
    tx.commit().await?;
    Ok(())
}

/// Save a review and opt in an exact stream. Its future content/state mutations
/// are captured by deferred database hooks used by ALL existing source handlers.
/// Renewing review is the only way to expand the evidence reference set.
pub async fn review_and_share(
    tx: &mut Transaction<'_, Postgres>,
    known: &PublicationState,
    access: &LocalProjectAccess,
    instance: InstanceId,
    canonical: RecordId,
    expected: &SourceSelection,
    evidence: &[ReviewedEvidence],
) -> Result<Uuid> {
    let grant = load_grant(tx, access, known.space(), instance).await?;
    sqlx::query("SELECT canonical_id FROM memory_canonical_state WHERE project_id=$1 AND canonical_id=$2 FOR UPDATE")
        .bind(access.project).bind(canonical.as_uuid()).fetch_one(&mut **tx).await?;
    if selection(tx, access.project, canonical).await? != *expected {
        return Err(StoreError::Binding);
    }
    let stream = StreamId {
        origin: instance,
        canonical,
    };
    let reviewer =
        AuthorIdentity::new(access.principal.to_string()).map_err(PublicationError::from)?;
    let mut refs = Vec::new();
    let mut cause = None;
    for item in evidence {
        if item.snapshot().space != known.space()
            || item.snapshot().stream != stream
            || item.review().reviewer != reviewer
        {
            return Err(StoreError::Binding);
        }
        let intent = intent(
            known.space(),
            instance,
            reviewer.clone(),
            Utc::now(),
            cause,
            PublicationRecord::Evidence(item.clone()),
        )?;
        known
            .authorize_publication(access, Some(&grant), &intent.envelope)
            .map_err(StoreError::LocalPolicy)?;
        persist_intent(tx, access.project, access.principal, &intent).await?;
        cause = Some(intent.envelope.key());
        refs.push(intent.envelope.key());
    }
    let probe = intent(
        known.space(),
        instance,
        reviewer,
        Utc::now(),
        cause,
        PublicationRecord::MemoryVersion {
            stream,
            publication: MemoryPublication {
                version: SharedVersion::new(1)?,
                source: expected.source.clone(),
                content: expected.content.clone(),
                evidence: refs.clone(),
            },
        },
    )?;
    known
        .authorize_publication(access, Some(&grant), &probe.envelope)
        .map_err(StoreError::LocalPolicy)?;
    let review = Uuid::new_v4();
    sqlx::query("INSERT INTO federation_publication_reviews(id,space,instance,project_id,canonical_id,source_id,reviewer,evidence) VALUES($1,$2,$3,$4,$5,$6,$7,$8)")
        .bind(review).bind(known.space().as_uuid()).bind(instance.as_uuid()).bind(access.project).bind(canonical.as_uuid())
        .bind(expected.source.record_id.as_uuid()).bind(access.principal).bind(Json(evidence)).execute(&mut **tx).await?;
    let changed=sqlx::query("INSERT INTO federation_source_subscriptions(space,instance,canonical_id,project_id,principal_id,review_id,evidence,last_record) VALUES($1,$2,$3,$4,$5,$6,$7,$8) ON CONFLICT(space,instance,canonical_id) DO UPDATE SET review_id=EXCLUDED.review_id,evidence=EXCLUDED.evidence,enabled=TRUE,last_record=COALESCE(EXCLUDED.last_record,federation_source_subscriptions.last_record) WHERE federation_source_subscriptions.project_id=EXCLUDED.project_id AND federation_source_subscriptions.principal_id=EXCLUDED.principal_id")
        .bind(known.space().as_uuid()).bind(instance.as_uuid()).bind(canonical.as_uuid()).bind(access.project).bind(access.principal).bind(review).bind(Json(&refs)).bind(cause.map(Json)).execute(&mut **tx).await?.rows_affected();
    if changed != 1 {
        return Err(StoreError::Binding);
    }
    Ok(review)
}

#[derive(Deserialize)]
struct FrozenSource {
    source: SourceVersionRef,
    content: MemoryContent,
    status: MemoryStatus,
    confidence: ConfidencePpm,
    tombstone: bool,
    author: AuthorIdentity,
    authored_at: DateTime<Utc>,
    state_at: DateTime<Utc>,
    evidence: Vec<RecordKey>,
}
fn intent(
    space: SpaceId,
    publisher: InstanceId,
    author: AuthorIdentity,
    authored_at: DateTime<Utc>,
    causation: Option<RecordKey>,
    record: PublicationRecord,
) -> Result<PublicationIntent> {
    let record_id = RecordId::new(Uuid::new_v4())?;
    Ok(PublicationIntent {
        locator: RecordLocator::new(format!("publication:{}", record_id.as_uuid()))
            .map_err(PublicationError::from)?,
        envelope: PublicationEnvelope {
            schema_version: 2,
            space,
            publisher,
            record_id,
            author,
            authored_at,
            causation,
            record,
        },
    })
}

/// Expand frozen intent into neutral envelopes. Retries cannot assign another
/// shared version: counters, causal head, envelopes and prepared flag commit once.
/// `access` must be freshly resolved by the service for the subscription owner.
pub async fn prepare_source_changes(
    pool: &PgPool,
    known: &PublicationState,
    access: &LocalProjectAccess,
    instance: InstanceId,
    limit: u32,
) -> Result<u32> {
    if !(1..=1000).contains(&limit) {
        return Err(StoreError::Limits);
    }
    let mut tx = pool.begin().await?;
    let grant = load_grant(&mut tx, access, known.space(), instance).await?;
    let subscriptions=sqlx::query("SELECT * FROM federation_source_subscriptions WHERE space=$1 AND instance=$2 AND project_id=$3 AND principal_id=$4 AND enabled ORDER BY canonical_id FOR UPDATE")
        .bind(known.space().as_uuid()).bind(instance.as_uuid()).bind(access.project).bind(access.principal).fetch_all(&mut *tx).await?;
    let mut prepared = 0;
    for sub in subscriptions {
        let canonical = RecordId::new(sub.try_get("canonical_id")?)?;
        let mut shared: u32 = u32::try_from(sub.try_get::<i64, _>("shared_version")?)
            .map_err(|_| StoreError::Corrupt)?;
        let mut revision: u32 = u32::try_from(sub.try_get::<i64, _>("state_revision")?)
            .map_err(|_| StoreError::Corrupt)?;
        let mut prior: Option<serde_json::Value> = sub.try_get("last_content")?;
        let mut prior_state: Option<serde_json::Value> = sub.try_get("last_state")?;
        let mut cause: Option<Json<RecordKey>> = sub.try_get("last_record")?;
        let changes=sqlx::query("SELECT id,snapshot FROM federation_source_changes WHERE space=$1 AND instance=$2 AND canonical_id=$3 AND NOT prepared ORDER BY capture_order LIMIT $4 FOR UPDATE")
            .bind(known.space().as_uuid()).bind(instance.as_uuid()).bind(canonical.as_uuid()).bind(i64::from(limit-prepared)).fetch_all(&mut *tx).await?;
        for change in changes {
            let frozen: Json<FrozenSource> = change.try_get("snapshot")?;
            let content = serde_json::json!({"source":frozen.source,"content":frozen.content,"evidence":frozen.evidence});
            let stream = StreamId {
                origin: instance,
                canonical,
            };
            if prior.as_ref() != Some(&content) {
                shared = shared.checked_add(1).ok_or(StoreError::Limits)?;
                let next = intent(
                    known.space(),
                    instance,
                    frozen.author.clone(),
                    frozen.authored_at,
                    cause.as_ref().map(|c| c.0),
                    PublicationRecord::MemoryVersion {
                        stream,
                        publication: MemoryPublication {
                            version: SharedVersion::new(shared)?,
                            source: frozen.source.clone(),
                            content: frozen.content.clone(),
                            evidence: frozen.evidence.clone(),
                        },
                    },
                )?;
                known
                    .authorize_publication(access, Some(&grant), &next.envelope)
                    .map_err(StoreError::LocalPolicy)?;
                persist_intent(&mut tx, access.project, access.principal, &next).await?;
                cause = Some(Json(next.envelope.key()));
                prior = Some(content);
            }
            let state = StatePublication {
                revision: StateRevision::new(revision.checked_add(1).ok_or(StoreError::Limits)?)?,
                content_version: SharedVersion::new(shared)?,
                status: frozen.status.clone(),
                confidence: frozen.confidence,
                tombstone: frozen.tombstone,
            };
            let comparison = serde_json::json!({"content_version":shared,"status":state.status,"confidence":state.confidence,"tombstone":state.tombstone});
            if prior_state.as_ref() != Some(&comparison) {
                revision = state.revision.get();
                let author = AuthorIdentity::new(format!("instance:{}", instance.as_uuid()))
                    .map_err(PublicationError::from)?;
                let next = intent(
                    known.space(),
                    instance,
                    author,
                    frozen.state_at,
                    cause.as_ref().map(|c| c.0),
                    PublicationRecord::CanonicalState { stream, state },
                )?;
                known
                    .authorize_publication(access, Some(&grant), &next.envelope)
                    .map_err(StoreError::LocalPolicy)?;
                persist_intent(&mut tx, access.project, access.principal, &next).await?;
                cause = Some(Json(next.envelope.key()));
                prior_state = Some(comparison);
            }
            sqlx::query("UPDATE federation_source_changes SET prepared=TRUE WHERE id=$1")
                .bind(change.try_get::<Uuid, _>("id")?)
                .execute(&mut *tx)
                .await?;
            prepared += 1;
        }
        sqlx::query("UPDATE federation_source_subscriptions SET shared_version=$4,state_revision=$5,last_content=$6,last_state=$7,last_record=$8 WHERE space=$1 AND instance=$2 AND canonical_id=$3")
            .bind(known.space().as_uuid()).bind(instance.as_uuid()).bind(canonical.as_uuid()).bind(i64::from(shared)).bind(i64::from(revision)).bind(prior).bind(prior_state).bind(cause).execute(&mut *tx).await?;
        if prepared >= limit {
            break;
        }
    }
    tx.commit().await?;
    Ok(prepared)
}
