// SPDX-License-Identifier: AGPL-3.0-or-later
use mem_federation::{RecordLocator, publication::*};
use serde::{Deserialize, Serialize};
use sqlx::{PgPool, Postgres, Row, Transaction};

use crate::{DeliveryStatus, Result, StoreError};

/// No authority sequence is assigned locally. These bytes are frozen before
/// queueing; source provenance is not permission to expand reviewed evidence.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct PublicationIntent {
    pub locator: RecordLocator,
    pub envelope: PublicationEnvelope,
}

/// Compose with source mutation in ONE caller-owned transaction. `known_space`
/// is a trusted cached gateway state, not permission to bypass gateway recheck.
pub async fn queue_intent(
    tx: &mut Transaction<'_, Postgres>,
    known_space: &PublicationState,
    access: &LocalProjectAccess,
    grant: Option<&LocalSpaceGrant>,
    intent: &PublicationIntent,
) -> Result<()> {
    known_space
        .authorize_publication(access, grant, &intent.envelope)
        .map_err(StoreError::LocalPolicy)?;
    let e = &intent.envelope;
    if e.schema_version != PUBLICATION_SCHEMA_VERSION {
        return Err(StoreError::Binding);
    }
    // A MemoryVersion may select/redact source content, but cannot invent its
    // project, canonical identity, source record or source version attribution.
    if let PublicationRecord::MemoryVersion {
        stream,
        publication,
    } = &e.record
    {
        let exists: bool = sqlx::query_scalar("SELECT EXISTS (SELECT 1 FROM memory_entries WHERE id=$1 AND project_id=$2 AND canonical_id=$3 AND version_no=$4)")
            .bind(publication.source.record_id.as_uuid()).bind(access.project)
            .bind(stream.canonical.as_uuid()).bind(i64::from(publication.source.version.get()))
            .fetch_one(&mut **tx).await?;
        if !exists || stream.origin != e.publisher {
            return Err(StoreError::Binding);
        }
    }
    persist_intent(tx, access.project, access.principal, intent).await
}

pub(crate) async fn persist_intent(
    tx: &mut Transaction<'_, Postgres>,
    project: uuid::Uuid,
    principal: uuid::Uuid,
    intent: &PublicationIntent,
) -> Result<()> {
    let e = &intent.envelope;
    sqlx::query("INSERT INTO federation_source_outbox (space,publisher,record_id,project_id,locator,envelope,principal_id) VALUES ($1,$2,$3,$4,$5,$6,$7) ON CONFLICT DO NOTHING")
        .bind(e.space.as_uuid()).bind(e.publisher.as_uuid()).bind(e.record_id.as_uuid())
        .bind(project).bind(intent.locator.as_str()).bind(sqlx::types::Json(e)).bind(principal)
        .execute(&mut **tx).await?;
    let row = sqlx::query("SELECT project_id,principal_id,locator,envelope FROM federation_source_outbox WHERE space=$1 AND publisher=$2 AND record_id=$3")
        .bind(e.space.as_uuid()).bind(e.publisher.as_uuid()).bind(e.record_id.as_uuid())
        .fetch_optional(&mut **tx).await?.ok_or(StoreError::Binding)?;
    let stored: sqlx::types::Json<PublicationEnvelope> = row.try_get("envelope")?;
    if stored.0 != *e
        || row.try_get::<String, _>("locator")? != intent.locator.as_str()
        || row.try_get::<uuid::Uuid, _>("project_id")? != project
        || row.try_get::<Option<uuid::Uuid>, _>("principal_id")? != Some(principal)
    {
        return Err(StoreError::Binding);
    }
    Ok(())
}

/// Queue enumeration is scheduling, NOT application order. Missing causal
/// dependencies may require another pass; the gateway alone assigns order.
pub async fn pending_intents(
    pool: &PgPool,
    space: SpaceId,
    publisher: InstanceId,
    limit: u32,
) -> Result<Vec<PublicationIntent>> {
    if !(1..=1000).contains(&limit) {
        return Err(StoreError::Limits);
    }
    let rows = sqlx::query("SELECT locator,envelope FROM federation_source_outbox WHERE space=$1 AND publisher=$2 AND acceptance IS NULL ORDER BY queued_at,record_id LIMIT $3")
        .bind(space.as_uuid()).bind(publisher.as_uuid()).bind(i64::from(limit)).fetch_all(pool).await?;
    rows.into_iter()
        .map(|row| {
            let envelope: sqlx::types::Json<PublicationEnvelope> = row.try_get("envelope")?;
            Ok(PublicationIntent {
                locator: RecordLocator::new(row.try_get::<String, _>("locator")?)
                    .map_err(PublicationError::from)?,
                envelope: envelope.0,
            })
        })
        .collect()
}

/// The caller must verify gateway authenticity before persisting this receipt.
pub async fn record_acceptance(
    tx: &mut Transaction<'_, Postgres>,
    delivery: &PublicationDelivery,
) -> Result<()> {
    let e = &delivery.envelope;
    let expected =
        PublicationDelivery::new(delivery.sequence, delivery.locator.clone(), e.clone())?;
    if expected != *delivery {
        return Err(StoreError::Binding);
    }
    let row = sqlx::query("SELECT locator,envelope,acceptance FROM federation_source_outbox WHERE space=$1 AND publisher=$2 AND record_id=$3 FOR UPDATE")
        .bind(e.space.as_uuid()).bind(e.publisher.as_uuid()).bind(e.record_id.as_uuid()).fetch_one(&mut **tx).await?;
    let envelope: sqlx::types::Json<PublicationEnvelope> = row.try_get("envelope")?;
    let prior: Option<sqlx::types::Json<PublicationDelivery>> = row.try_get("acceptance")?;
    if envelope.0 != *e
        || row.try_get::<String, _>("locator")? != delivery.locator.as_str()
        || prior.is_some_and(|old| old.0 != *delivery)
    {
        return Err(StoreError::Binding);
    }
    sqlx::query("UPDATE federation_source_outbox SET acceptance=$4 WHERE space=$1 AND publisher=$2 AND record_id=$3")
        .bind(e.space.as_uuid()).bind(e.publisher.as_uuid()).bind(e.record_id.as_uuid()).bind(sqlx::types::Json(delivery))
        .execute(&mut **tx).await?;
    Ok(())
}

pub async fn intent_status(
    pool: &PgPool,
    space: SpaceId,
    key: RecordKey,
) -> Result<DeliveryStatus> {
    let receipt: Option<sqlx::types::Json<PublicationDelivery>> = sqlx::query_scalar("SELECT acceptance FROM federation_source_outbox WHERE space=$1 AND publisher=$2 AND record_id=$3")
        .bind(space.as_uuid()).bind(key.publisher.as_uuid()).bind(key.id.as_uuid()).fetch_one(pool).await?;
    Ok(
        receipt.map_or(DeliveryStatus::Queued, |d| DeliveryStatus::Accepted {
            sequence: d.sequence.get(),
        }),
    )
}
