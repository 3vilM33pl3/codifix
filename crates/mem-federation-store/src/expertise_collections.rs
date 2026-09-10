// SPDX-License-Identifier: AGPL-3.0-or-later
//! Origin-only collection persistence. The caller resolves exact-project
//! access. No route, network endpoint or provider is selected by this module.
use crate::{
    EvidenceSelection, PublicationIntent, Result, StoreError,
    sharing::{load_grant, selection},
};
use chrono::{DateTime, Utc};
use mem_federation::{
    AuthorIdentity, CanonicalJsonCodec, ContentDigest, RecordLocator, SpaceCapability,
    expertise::*, publication::*,
};
use mem_record::Permission;
use serde::{Deserialize, Serialize};
use sqlx::{PgPool, Postgres, Row, Transaction, types::Json};
use uuid::Uuid;

#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct CollectionSelection {
    pub canonical: RecordId,
    pub evidence: Vec<EvidenceSelection>,
}
#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct CollectionBlueprint {
    pub collection: RecordId,
    pub name: String,
    pub description: String,
    pub topics: Vec<String>,
    pub modes: Vec<AnswerMode>,
    pub requesters: Vec<InstanceId>,
    pub model_destinations: Vec<String>,
    pub output_destinations: Vec<String>,
    pub entries: Vec<CollectionSelection>,
}
#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct CollectionDraft {
    pub request_id: Uuid,
    pub reviewer: Uuid,
    pub prepared_at: DateTime<Utc>,
    pub snapshot: CollectionSnapshot,
    pub advertisement: CapabilityAdvertisement,
    pub review_digest: ContentDigest,
}
#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct CollectionWithdrawal {
    pub request_id: Uuid,
    pub collection: RecordId,
    pub expected_revision: StateRevision,
    pub confirmed: bool,
}

pub async fn withdraw_collection(
    tx: &mut Transaction<'_, Postgres>,
    known: &PublicationState,
    access: &LocalProjectAccess,
    owner: InstanceId,
    request: &CollectionWithdrawal,
) -> Result<PublicationIntent> {
    permission(
        access,
        &[
            Permission::MemoryRead,
            Permission::MemoryCurate,
            Permission::BundleExport,
        ],
        true,
    )?;
    if !request.confirmed || request.request_id.is_nil() {
        return Err(StoreError::Binding);
    }
    let grant = load_grant(tx, access, known.space(), owner).await?;
    if !grant.capabilities.contains(&SpaceCapability::Publish)
        || !known.has_capability(owner, SpaceCapability::Publish)
    {
        return Err(StoreError::Binding);
    }
    collection_lock(tx, known.space(), owner, access.project, request.collection).await?;
    let digest = CanonicalJsonCodec::digest(request).map_err(PublicationError::from)?;
    let prior = sqlx::query("SELECT request_digest,intent FROM federation_expertise_reviews WHERE space=$1 AND owner=$2 AND project_id=$3 AND principal_id=$4 AND request_id=$5")
        .bind(known.space().as_uuid()).bind(owner.as_uuid()).bind(access.project).bind(access.principal).bind(request.request_id)
        .fetch_optional(&mut **tx).await?;
    if let Some(prior) = prior {
        if prior.try_get::<Json<ContentDigest>, _>("request_digest")?.0 != digest {
            return Err(StoreError::Binding);
        }
        return Ok(prior.try_get::<Json<PublicationIntent>, _>("intent")?.0);
    }
    let row = sqlx::query("SELECT r.snapshot,r.advertisement FROM federation_expertise_collections c JOIN federation_expertise_reviews r USING(space,owner,project_id,collection,revision) WHERE c.space=$1 AND c.owner=$2 AND c.project_id=$3 AND c.collection=$4 AND c.revision=$5")
        .bind(known.space().as_uuid()).bind(owner.as_uuid()).bind(access.project).bind(request.collection.as_uuid())
        .bind(i64::from(request.expected_revision.get())).fetch_optional(&mut **tx).await?.ok_or(StoreError::Expertise(ExpertiseReject::StaleCollection))?;
    let mut snapshot = row.try_get::<Json<CollectionSnapshot>, _>("snapshot")?.0;
    let mut advertisement = row
        .try_get::<Json<CapabilityAdvertisement>, _>("advertisement")?
        .0;
    snapshot.revision =
        next_revision(tx, known.space(), owner, access.project, request.collection).await?;
    advertisement.revision = snapshot.revision;
    advertisement.collection_digest = snapshot.digest()?;
    advertisement.active = false;
    let draft = CollectionDraft {
        request_id: request.request_id,
        reviewer: access.principal,
        prepared_at: Utc::now(),
        snapshot,
        advertisement,
        review_digest: digest,
    };
    let intent = draft_intent(&draft, access.principal)?;
    known
        .authorize_publication(access, Some(&grant), &intent.envelope)
        .map_err(StoreError::LocalPolicy)?;
    save_review(tx, access, &draft, &intent, "withdraw").await?;
    Ok(intent)
}
impl CollectionDraft {
    pub fn digest(&self) -> Result<ContentDigest> {
        Ok(CanonicalJsonCodec::digest(&(
            self.request_id,
            self.reviewer,
            self.prepared_at,
            &self.snapshot,
            &self.advertisement,
        ))
        .map_err(PublicationError::from)?)
    }
    fn validate(&self, principal: Uuid) -> Result<()> {
        self.advertisement.validate()?;
        if self.request_id.is_nil()
            || self.reviewer != principal
            || self.review_digest != self.digest()?
            || self.advertisement.owner != self.snapshot.owner
            || self.advertisement.collection != self.snapshot.collection
            || self.advertisement.revision != self.snapshot.revision
            || self.advertisement.collection_digest != self.snapshot.digest()?
        {
            return Err(StoreError::Binding);
        }
        Ok(())
    }
}

fn permission(access: &LocalProjectAccess, required: &[Permission], write: bool) -> Result<()> {
    if access.project.is_nil() || access.principal.is_nil() || (write && access.read_only) {
        return Err(StoreError::Binding);
    }
    for p in required {
        if !access.permissions.contains(*p) {
            return Err(StoreError::LocalPolicy(
                LocalPolicyReject::MissingPermission(*p),
            ));
        }
    }
    Ok(())
}
async fn collection_lock(
    tx: &mut Transaction<'_, Postgres>,
    space: SpaceId,
    owner: InstanceId,
    project: Uuid,
    collection: RecordId,
) -> Result<()> {
    sqlx::query("SELECT pg_advisory_xact_lock(hashtextextended($1,0))")
        .bind(format!(
            "expertise:{space:?}:{owner:?}:{project}:{collection:?}"
        ))
        .execute(&mut **tx)
        .await?;
    Ok(())
}
async fn next_revision(
    tx: &mut Transaction<'_, Postgres>,
    space: SpaceId,
    owner: InstanceId,
    project: Uuid,
    collection: RecordId,
) -> Result<StateRevision> {
    let revision: Option<i64> = sqlx::query_scalar("SELECT revision FROM federation_expertise_collections WHERE space=$1 AND owner=$2 AND project_id=$3 AND collection=$4")
        .bind(space.as_uuid()).bind(owner.as_uuid()).bind(project).bind(collection.as_uuid())
        .fetch_optional(&mut **tx).await?;
    StateRevision::new(u32::try_from(revision.unwrap_or(0) + 1).map_err(|_| StoreError::Limits)?)
        .map_err(Into::into)
}
/// Hold canonical-state locks through result commit. R1 migration 0044 makes
/// every version insertion, including tombstones, observe the same lock.
async fn entry(
    tx: &mut Transaction<'_, Postgres>,
    project: Uuid,
    canonical: RecordId,
) -> Result<CollectionEntry> {
    let active: Option<String> = sqlx::query_scalar("SELECT status FROM memory_canonical_state WHERE project_id=$1 AND canonical_id=$2 FOR SHARE")
        .bind(project).bind(canonical.as_uuid()).fetch_optional(&mut **tx).await?;
    if active.as_deref() != Some("active") {
        return Err(StoreError::Expertise(ExpertiseReject::StaleCollection));
    }
    let source = selection(tx, project, canonical).await?;
    let row = sqlx::query("SELECT m.created_at,COALESCE((SELECT NULLIF(s.writer_id,'') FROM memory_sources ms JOIN tasks t ON t.id=ms.task_id JOIN sessions s ON s.id=t.session_id WHERE ms.memory_entry_id=m.id ORDER BY ms.created_at,ms.id LIMIT 1),'unknown:local-author') AS author FROM memory_entries m WHERE m.id=$1 AND m.project_id=$2")
        .bind(source.source.record_id.as_uuid()).bind(project).fetch_one(&mut **tx).await?;
    Ok(CollectionEntry {
        canonical,
        source: source.source,
        content: source.content,
        author: AuthorIdentity::new(row.try_get::<String, _>("author")?)
            .map_err(PublicationError::from)?,
        authored_at: row.try_get("created_at")?,
        evidence: vec![],
    })
}
pub async fn preview_collection(
    pool: &PgPool,
    access: &LocalProjectAccess,
    space: SpaceId,
    owner: InstanceId,
    blueprint: &CollectionBlueprint,
) -> Result<CollectionDraft> {
    permission(
        access,
        &[
            Permission::MemoryRead,
            Permission::MemoryCurate,
            Permission::BundleExport,
        ],
        false,
    )?;
    if blueprint.entries.is_empty() || blueprint.entries.len() > MAX_COLLECTION_ENTRIES {
        return Err(StoreError::Limits);
    }
    let mut tx = pool.begin().await?;
    load_grant(&mut tx, access, space, owner).await?;
    collection_lock(&mut tx, space, owner, access.project, blueprint.collection).await?;
    let revision =
        next_revision(&mut tx, space, owner, access.project, blueprint.collection).await?;
    let now = Utc::now();
    let mut snapshot = CollectionSnapshot {
        space,
        owner,
        collection: blueprint.collection,
        revision,
        entries: vec![],
    };
    let mut selected = blueprint.entries.iter().collect::<Vec<_>>();
    selected.sort_by_key(|item| item.canonical);
    for item in selected {
        if item.evidence.len() > 16 {
            return Err(StoreError::Limits);
        }
        let mut frozen = entry(&mut tx, access.project, item.canonical).await?;
        for evidence in &item.evidence {
            frozen.evidence.push(ReviewedEvidence::new(
                evidence.snapshot.clone(),
                ReviewReceipt {
                    snapshot_digest: evidence.snapshot_digest.clone(),
                    reviewer: AuthorIdentity::new(access.principal.to_string())
                        .map_err(PublicationError::from)?,
                    reviewed_at: now,
                },
            )?);
        }
        snapshot.entries.push(frozen);
    }
    let advertisement = CapabilityAdvertisement {
        owner,
        collection: blueprint.collection,
        revision,
        collection_digest: snapshot.digest()?,
        name: blueprint.name.clone(),
        description: blueprint.description.clone(),
        topics: blueprint.topics.clone(),
        modes: blueprint.modes.clone(),
        requesters: blueprint.requesters.clone(),
        model_destinations: blueprint.model_destinations.clone(),
        output_destinations: blueprint.output_destinations.clone(),
        active: true,
        limits: Default::default(),
    };
    advertisement.validate()?;
    let mut draft = CollectionDraft {
        request_id: Uuid::new_v4(),
        reviewer: access.principal,
        prepared_at: now,
        snapshot,
        advertisement,
        review_digest: ContentDigest::sha256_bytes(b""),
    };
    draft.review_digest = draft.digest()?;
    tx.commit().await?;
    Ok(draft)
}

/// Caller-owned transaction couples review, collection head, sanitized audit
/// and ordered publication intent. A draft is not consent until this call.
pub async fn activate_collection(
    tx: &mut Transaction<'_, Postgres>,
    known: &PublicationState,
    access: &LocalProjectAccess,
    owner: InstanceId,
    draft: &CollectionDraft,
    confirmed: bool,
) -> Result<PublicationIntent> {
    permission(
        access,
        &[
            Permission::MemoryRead,
            Permission::MemoryCurate,
            Permission::BundleExport,
        ],
        true,
    )?;
    draft.validate(access.principal)?;
    if !confirmed
        || !draft.advertisement.active
        || draft.snapshot.space != known.space()
        || draft.snapshot.owner != owner
    {
        return Err(StoreError::Binding);
    }
    let grant = load_grant(tx, access, known.space(), owner).await?;
    let intent = draft_intent(draft, access.principal)?;
    known
        .authorize_publication(access, Some(&grant), &intent.envelope)
        .map_err(StoreError::LocalPolicy)?;
    collection_lock(
        tx,
        known.space(),
        owner,
        access.project,
        draft.snapshot.collection,
    )
    .await?;
    if let Some(old) = previous_request(tx, access, draft).await? {
        return Ok(old);
    }
    if next_revision(
        tx,
        known.space(),
        owner,
        access.project,
        draft.snapshot.collection,
    )
    .await?
        != draft.snapshot.revision
    {
        return Err(StoreError::Expertise(ExpertiseReject::StaleCollection));
    }
    let mut entries = draft.snapshot.entries.iter().collect::<Vec<_>>();
    entries.sort_by_key(|e| e.canonical);
    for expected in entries {
        let mut current = entry(tx, access.project, expected.canonical).await?;
        current.evidence = expected.evidence.clone();
        if current != *expected
            || expected
                .evidence
                .iter()
                .any(|e| e.review().reviewer.as_str() != access.principal.to_string())
        {
            return Err(StoreError::Expertise(ExpertiseReject::StaleCollection));
        }
    }
    save_review(tx, access, draft, &intent, "activate").await?;
    Ok(intent)
}

fn draft_intent(draft: &CollectionDraft, principal: Uuid) -> Result<PublicationIntent> {
    Ok(PublicationIntent {
        locator: RecordLocator::new(format!("expertise:{}", draft.request_id))
            .map_err(PublicationError::from)?,
        envelope: PublicationEnvelope {
            schema_version: EXPERTISE_SCHEMA_VERSION,
            space: draft.snapshot.space,
            publisher: draft.snapshot.owner,
            record_id: RecordId::new(draft.request_id)?,
            author: AuthorIdentity::new(principal.to_string()).map_err(PublicationError::from)?,
            authored_at: draft.prepared_at,
            causation: None,
            record: PublicationRecord::Expertise(draft.advertisement.clone()),
        },
    })
}
async fn previous_request(
    tx: &mut Transaction<'_, Postgres>,
    access: &LocalProjectAccess,
    draft: &CollectionDraft,
) -> Result<Option<PublicationIntent>> {
    let prior = sqlx::query("SELECT request_digest,intent FROM federation_expertise_reviews WHERE space=$1 AND owner=$2 AND project_id=$3 AND principal_id=$4 AND request_id=$5")
        .bind(draft.snapshot.space.as_uuid()).bind(draft.snapshot.owner.as_uuid()).bind(access.project).bind(access.principal)
        .bind(draft.request_id).fetch_optional(&mut **tx).await?;
    prior
        .map(|row| {
            if row.try_get::<Json<ContentDigest>, _>("request_digest")?.0 != draft.review_digest {
                return Err(StoreError::Binding);
            }
            Ok(row.try_get::<Json<PublicationIntent>, _>("intent")?.0)
        })
        .transpose()
}
async fn save_review(
    tx: &mut Transaction<'_, Postgres>,
    access: &LocalProjectAccess,
    draft: &CollectionDraft,
    intent: &PublicationIntent,
    action: &str,
) -> Result<()> {
    let s = &draft.snapshot;
    sqlx::query("INSERT INTO federation_expertise_reviews(space,owner,project_id,collection,revision,principal_id,request_id,request_digest,snapshot,advertisement,intent,active) VALUES($1,$2,$3,$4,$5,$6,$7,$8,$9,$10,$11,$12)")
        .bind(s.space.as_uuid()).bind(s.owner.as_uuid()).bind(access.project).bind(s.collection.as_uuid()).bind(i64::from(s.revision.get()))
        .bind(access.principal).bind(draft.request_id).bind(Json(&draft.review_digest)).bind(Json(s)).bind(Json(&draft.advertisement)).bind(Json(intent)).bind(draft.advertisement.active)
        .execute(&mut **tx).await?;
    sqlx::query("INSERT INTO federation_expertise_collections(space,owner,project_id,collection,revision) VALUES($1,$2,$3,$4,$5) ON CONFLICT(space,owner,project_id,collection) DO UPDATE SET revision=EXCLUDED.revision")
        .bind(s.space.as_uuid()).bind(s.owner.as_uuid()).bind(access.project).bind(s.collection.as_uuid()).bind(i64::from(s.revision.get())).execute(&mut **tx).await?;
    crate::source::persist_intent(tx, access.project, access.principal, intent).await?;
    collection_audit(
        tx,
        access,
        s.space,
        s.owner,
        s.collection,
        action,
        Some(s.revision),
        None,
    )
    .await
}
#[allow(clippy::too_many_arguments)]
async fn collection_audit(
    tx: &mut Transaction<'_, Postgres>,
    access: &LocalProjectAccess,
    space: SpaceId,
    owner: InstanceId,
    collection: RecordId,
    action: &str,
    revision: Option<StateRevision>,
    subject: Option<Uuid>,
) -> Result<()> {
    sqlx::query("INSERT INTO federation_expertise_collection_audit(id,space,owner,project_id,collection,actor,action,revision,subject) VALUES($1,$2,$3,$4,$5,$6,$7,$8,$9)")
        .bind(Uuid::new_v4()).bind(space.as_uuid()).bind(owner.as_uuid()).bind(access.project).bind(collection.as_uuid()).bind(access.principal)
        .bind(action).bind(revision.map(|v| i64::from(v.get()))).bind(subject).execute(&mut **tx).await?;
    Ok(())
}

pub async fn set_collection_grant(
    tx: &mut Transaction<'_, Postgres>,
    access: &LocalProjectAccess,
    space: SpaceId,
    owner: InstanceId,
    collection: RecordId,
    principal: Uuid,
    enabled: bool,
) -> Result<()> {
    permission(
        access,
        &[Permission::MemoryRead, Permission::AuthManage],
        true,
    )?;
    load_grant(tx, access, space, owner).await?;
    let exists: bool = sqlx::query_scalar(
        "SELECT EXISTS(SELECT 1 FROM auth_principals WHERE id=$1 AND disabled_at IS NULL)",
    )
    .bind(principal)
    .fetch_one(&mut **tx)
    .await?;
    if !exists {
        return Err(StoreError::Binding);
    }
    sqlx::query("INSERT INTO federation_expertise_collection_grants(space,owner,project_id,collection,principal_id,enabled) VALUES($1,$2,$3,$4,$5,$6) ON CONFLICT(space,owner,project_id,collection,principal_id) DO UPDATE SET enabled=EXCLUDED.enabled")
        .bind(space.as_uuid()).bind(owner.as_uuid()).bind(access.project).bind(collection.as_uuid()).bind(principal).bind(enabled).execute(&mut **tx).await?;
    collection_audit(
        tx,
        access,
        space,
        owner,
        collection,
        if enabled { "grant" } else { "revoke_grant" },
        None,
        Some(principal),
    )
    .await
}

/// Called in a result transaction as well as before processing. Do not commit
/// this transaction and then claim its permission/source locks still fence a
/// later write. Remote authority must be checked separately at the gateway.
pub async fn eligible_collection(
    tx: &mut Transaction<'_, Postgres>,
    access: &LocalProjectAccess,
    space: SpaceId,
    owner: InstanceId,
    collection: RecordId,
    revision: StateRevision,
) -> Result<CollectionSnapshot> {
    permission(
        access,
        &[Permission::MemoryRead, Permission::BundleExport],
        true,
    )?;
    let grant = load_grant(tx, access, space, owner).await?;
    if !grant
        .capabilities
        .contains(&SpaceCapability::AnswerExpertise)
    {
        return Err(StoreError::Binding);
    }
    let enabled: Option<bool> = sqlx::query_scalar("SELECT enabled FROM federation_expertise_collection_grants WHERE space=$1 AND owner=$2 AND project_id=$3 AND collection=$4 AND principal_id=$5 FOR SHARE")
        .bind(space.as_uuid()).bind(owner.as_uuid()).bind(access.project).bind(collection.as_uuid()).bind(access.principal).fetch_optional(&mut **tx).await?;
    if enabled != Some(true) {
        return Err(StoreError::Binding);
    }
    let row = sqlx::query("SELECT r.snapshot,r.advertisement,r.active,r.principal_id FROM federation_expertise_collections c JOIN federation_expertise_reviews r USING(space,owner,project_id,collection,revision) WHERE c.space=$1 AND c.owner=$2 AND c.project_id=$3 AND c.collection=$4 AND c.revision=$5 FOR SHARE OF c")
        .bind(space.as_uuid()).bind(owner.as_uuid()).bind(access.project).bind(collection.as_uuid()).bind(i64::from(revision.get())).fetch_optional(&mut **tx).await?.ok_or(StoreError::Expertise(ExpertiseReject::StaleCollection))?;
    if !row.try_get::<bool, _>("active")? {
        return Err(StoreError::Binding);
    }
    // A disabled original reviewer cannot leave standing sharing consent alive.
    let reviewer_active: Option<bool> =
        sqlx::query_scalar("SELECT disabled_at IS NULL FROM auth_principals WHERE id=$1 FOR SHARE")
            .bind(row.try_get::<Uuid, _>("principal_id")?)
            .fetch_optional(&mut **tx)
            .await?;
    if reviewer_active != Some(true) {
        return Err(StoreError::Binding);
    }
    let mut snapshot = row.try_get::<Json<CollectionSnapshot>, _>("snapshot")?.0;
    let advertisement = row
        .try_get::<Json<CapabilityAdvertisement>, _>("advertisement")?
        .0;
    if snapshot.digest()? != advertisement.collection_digest
        || snapshot.space != space
        || snapshot.owner != owner
        || snapshot.collection != collection
        || snapshot.revision != revision
    {
        return Err(StoreError::Corrupt);
    }
    snapshot.entries.sort_by_key(|e| e.canonical);
    let mut eligible = Vec::new();
    for frozen in snapshot.entries {
        match entry(tx, access.project, frozen.canonical).await {
            Ok(mut current) => {
                current.evidence = frozen.evidence.clone();
                if current == frozen {
                    eligible.push(frozen);
                }
            }
            Err(
                StoreError::Binding
                | StoreError::Expertise(ExpertiseReject::StaleCollection)
                | StoreError::Database(sqlx::Error::RowNotFound),
            ) => {}
            Err(other) => return Err(other),
        }
    }
    snapshot.entries = eligible;
    // This is an eligible subset with the original collection identity, not a
    // new reviewed revision. Never recompute an advertised digest from it.
    Ok(snapshot)
}

/// Origin consent and exact-project service authority are checked independently
/// from the signed gateway lease. The caller keeps this transaction open while
/// accepting a derived result; source-version/review locks fence concurrent edits.
pub async fn eligible_question_collection(
    tx: &mut Transaction<'_, Postgres>,
    access: &LocalProjectAccess,
    owner: InstanceId,
    question: &ExpertiseQuestion,
) -> Result<(CollectionSnapshot, CapabilityAdvertisement)> {
    if question.capability.publisher != owner {
        return Err(StoreError::Binding);
    }
    let snapshot = eligible_collection(
        tx,
        access,
        question.space,
        owner,
        question.capability.id,
        question.revision,
    )
    .await?;
    let ad: Json<CapabilityAdvertisement> = sqlx::query_scalar("SELECT advertisement FROM federation_expertise_reviews WHERE space=$1 AND owner=$2 AND project_id=$3 AND collection=$4 AND revision=$5")
        .bind(question.space.as_uuid()).bind(owner.as_uuid()).bind(access.project).bind(question.capability.id.as_uuid())
        .bind(i64::from(question.revision.get())).fetch_one(&mut **tx).await?;
    let now = sqlx::query_scalar("SELECT clock_timestamp()")
        .fetch_one(&mut **tx)
        .await?;
    ad.authorize_question(question, now)?;
    Ok((snapshot, ad.0))
}
