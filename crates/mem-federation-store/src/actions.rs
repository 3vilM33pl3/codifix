// SPDX-License-Identifier: AGPL-3.0-or-later
use crate::{PublicationIntent, Result, StoreError};
use mem_federation::{
    AuthorIdentity, CanonicalJsonCodec, ContentDigest, SpaceCapability, publication::*,
};
use mem_record::Permission;
use serde::{Deserialize, Serialize};
use sqlx::{PgPool, Postgres, Row, Transaction, types::Json};
use uuid::Uuid;

#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(tag = "kind", rename_all = "snake_case", deny_unknown_fields)]
pub enum FederationAction {
    Membership {
        member: InstanceId,
        capability: SpaceCapability,
        enabled: bool,
    },
    Revoke {
        member: InstanceId,
    },
    Propose {
        proposal: CorrectionProposal,
    },
    Resolve {
        proposal: RecordKey,
        approve: bool,
    },
    Pause {
        canonical: RecordId,
    },
}
#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct ActionRequest {
    pub request_id: Uuid,
    pub action: FederationAction,
}
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct ActionResponse {
    pub request_id: Uuid,
    pub submission: String,
    pub record: Option<RecordKey>,
}

/// Protect source/subscription serialization and reject queued or accepted-but-
/// unobserved work. No optimistic shared counter can overwrite accepted state.
pub(crate) async fn lock_quiet_stream(
    tx: &mut Transaction<'_, Postgres>,
    known: &PublicationState,
    project: Uuid,
    instance: InstanceId,
    canonical: RecordId,
) -> Result<()> {
    sqlx::query("SELECT canonical_id FROM memory_canonical_state WHERE project_id=$1 AND canonical_id=$2 FOR UPDATE")
        .bind(project).bind(canonical.as_uuid()).fetch_one(&mut **tx).await?;
    sqlx::query("SELECT canonical_id FROM federation_source_subscriptions WHERE space=$1 AND instance=$2 AND canonical_id=$3 FOR UPDATE")
        .bind(known.space().as_uuid()).bind(instance.as_uuid()).bind(canonical.as_uuid()).fetch_optional(&mut **tx).await?;
    let stream = StreamId {
        origin: instance,
        canonical,
    };
    // Only this stream's counters need a quiet boundary. An unrelated failed
    // membership/proposal must not freeze every source review in the space.
    // A stale/resolved proposal can never advance this monotonic stream again.
    let proposals: Vec<RecordKey> = known
        .accepted()
        .iter()
        .filter_map(|d| {
            let key = d.envelope.key();
            known
                .proposal(key)
                .filter(|p| {
                    p.proposal.target == stream
                        && p.proposal.base_version.get() == known.content_version(stream)
                        && p.resolution == ProposalResolution::Pending
                })
                .map(|_| key)
        })
        .collect();
    let pending: bool = sqlx::query_scalar("SELECT EXISTS(SELECT 1 FROM federation_source_changes WHERE space=$1 AND instance=$2 AND canonical_id=$3 AND NOT prepared) OR EXISTS(SELECT 1 FROM federation_source_outbox WHERE space=$1 AND publisher=$2 AND (acceptance IS NULL OR (acceptance->>'sequence')::bigint>$4) AND (envelope#>>'{record,payload,stream,canonical}'=$3::text OR (envelope#>>'{record,kind}'='resolve_proposal' AND envelope#>'{record,payload,proposal}' IN (SELECT jsonb_array_elements($5::jsonb)))))")
        .bind(known.space().as_uuid()).bind(instance.as_uuid()).bind(canonical.as_uuid()).bind(i64::from(known.sequence())).bind(Json(proposals)).fetch_one(&mut **tx).await?;
    if pending {
        return Err(StoreError::Binding);
    }
    Ok(())
}

pub async fn submit_action(
    pool: &PgPool,
    known: &PublicationState,
    access: &LocalProjectAccess,
    instance: InstanceId,
    request: &ActionRequest,
) -> Result<ActionResponse> {
    if request.request_id.is_nil() {
        return Err(StoreError::Limits);
    }
    let bytes = CanonicalJsonCodec::encode(request).map_err(PublicationError::from)?;
    if bytes.len() > 48 * 1024 {
        return Err(StoreError::Limits);
    }
    let digest = ContentDigest::sha256_bytes(&bytes);
    let mut tx = pool.begin().await?;
    let grant = crate::sharing::load_grant(&mut tx, access, known.space(), instance).await?;
    let author =
        AuthorIdentity::new(access.principal.to_string()).map_err(PublicationError::from)?;
    let mut record = None;
    match &request.action {
        FederationAction::Membership {
            member,
            capability,
            enabled,
        } => {
            record = Some(if *enabled {
                PublicationRecord::Membership {
                    member: *member,
                    capability: *capability,
                }
            } else {
                PublicationRecord::Revocation {
                    member: *member,
                    capability: Some(*capability),
                }
            });
        }
        FederationAction::Revoke { member } => {
            record = Some(PublicationRecord::Revocation {
                member: *member,
                capability: None,
            })
        }
        FederationAction::Propose { proposal } => {
            if proposal.target.origin == instance {
                return Err(StoreError::Binding);
            }
            record = Some(PublicationRecord::Proposal(proposal.clone()));
        }
        FederationAction::Resolve { proposal, approve } => {
            let target = known
                .proposal(*proposal)
                .ok_or(StoreError::Binding)?
                .proposal
                .target;
            if target.origin != instance {
                return Err(StoreError::Binding);
            }
            let source = if *approve {
                Some(
                    known
                        .history(target)
                        .last()
                        .ok_or(StoreError::Binding)?
                        .publication
                        .source
                        .clone(),
                )
            } else {
                None
            };
            record = Some(PublicationRecord::ResolveProposal {
                proposal: *proposal,
                source,
            });
        }
        FederationAction::Pause { .. } => {
            if access.read_only
                || ![
                    Permission::MemoryRead,
                    Permission::MemoryCurate,
                    Permission::BundleExport,
                ]
                .iter()
                .all(|p| access.permissions.contains(*p))
                || !grant.capabilities.contains(&SpaceCapability::Publish)
            {
                return Err(StoreError::LocalPolicy(
                    LocalPolicyReject::MissingLocalCapability(SpaceCapability::Publish),
                ));
            }
        }
    }
    let intent: Option<PublicationIntent> = record
        .map(|record| {
            crate::sharing::intent(
                known.space(),
                instance,
                author,
                chrono::Utc::now(),
                match request.action {
                    FederationAction::Resolve { proposal, .. } => Some(proposal),
                    _ => None,
                },
                record,
            )
        })
        .transpose()?;
    if let Some(intent) = &intent {
        known
            .authorize_publication(access, Some(&grant), &intent.envelope)
            .map_err(StoreError::LocalPolicy)?;
    }
    let lock = format!(
        "action:{}:{}:{}:{}:{}",
        known.space().as_uuid(),
        instance.as_uuid(),
        access.project,
        access.principal,
        request.request_id
    );
    sqlx::query("SELECT pg_advisory_xact_lock(hashtextextended($1,0))")
        .bind(lock)
        .execute(&mut *tx)
        .await?;
    if let Some(row) = sqlx::query("SELECT request_digest,response FROM federation_action_requests WHERE space=$1 AND instance=$2 AND project_id=$3 AND principal_id=$4 AND request_id=$5")
        .bind(known.space().as_uuid()).bind(instance.as_uuid()).bind(access.project).bind(access.principal).bind(request.request_id).fetch_optional(&mut *tx).await? {
        if row.try_get::<Json<ContentDigest>,_>("request_digest")?.0 != digest { return Err(StoreError::Binding); }
        return Ok(row.try_get::<Json<ActionResponse>,_>("response")?.0);
    }
    if let Some(intent) = &intent {
        let mut candidate = known.clone();
        let delivery = PublicationDelivery::new(
            AuthoritySequence::new(known.sequence().checked_add(1).ok_or(StoreError::Limits)?)?,
            intent.locator.clone(),
            intent.envelope.clone(),
        )?;
        let outcome = candidate.apply(instance, delivery);
        if outcome != PublicationOutcome::Applied {
            return Err(StoreError::Outcome(outcome));
        }
    }
    match request.action {
        FederationAction::Pause { canonical } => {
            // Pause is allowed with queued work: it stops future capture, not the
            // already reviewed immutable backlog. Local grant revocation stops sending.
            sqlx::query("SELECT canonical_id FROM memory_canonical_state WHERE project_id=$1 AND canonical_id=$2 FOR UPDATE")
                .bind(access.project).bind(canonical.as_uuid()).fetch_one(&mut *tx).await?;
            let rows = sqlx::query("UPDATE federation_source_subscriptions SET enabled=FALSE WHERE space=$1 AND instance=$2 AND canonical_id=$3 AND project_id=$4 AND (principal_id=$5 OR $6)")
                .bind(known.space().as_uuid()).bind(instance.as_uuid()).bind(canonical.as_uuid()).bind(access.project).bind(access.principal).bind(access.permissions.contains(Permission::AuthManage))
                .execute(&mut *tx).await?.rows_affected();
            if rows != 1 {
                return Err(StoreError::Binding);
            }
        }
        FederationAction::Resolve {
            proposal,
            approve: true,
        } => {
            let target = known
                .proposal(proposal)
                .ok_or(StoreError::Binding)?
                .proposal
                .target;
            lock_quiet_stream(&mut tx, known, access.project, instance, target.canonical).await?;
            let rows = sqlx::query("UPDATE federation_source_subscriptions SET enabled=FALSE WHERE space=$1 AND instance=$2 AND canonical_id=$3 AND project_id=$4")
                .bind(known.space().as_uuid()).bind(instance.as_uuid()).bind(target.canonical.as_uuid()).bind(access.project).execute(&mut *tx).await?.rows_affected();
            if rows != 1 {
                return Err(StoreError::Binding);
            }
        }
        _ => {}
    }
    if let Some(intent) = &intent {
        crate::source::persist_intent(&mut tx, access.project, access.principal, intent).await?;
    }
    let response = ActionResponse {
        request_id: request.request_id,
        submission: if intent.is_some() { "queued" } else { "paused" }.into(),
        record: intent.map(|i| i.envelope.key()),
    };
    sqlx::query("INSERT INTO federation_action_requests(space,instance,project_id,principal_id,request_id,request_digest,response) VALUES($1,$2,$3,$4,$5,$6,$7)")
        .bind(known.space().as_uuid()).bind(instance.as_uuid()).bind(access.project).bind(access.principal).bind(request.request_id).bind(Json(digest)).bind(Json(&response)).execute(&mut *tx).await?;
    tx.commit().await?;
    Ok(response)
}
