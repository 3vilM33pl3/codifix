// SPDX-License-Identifier: AGPL-3.0-or-later
//! Origin-local inspection, never permission to answer or disclose remotely.
use crate::{Result, StoreError, sharing::load_grant};
use mem_federation::{expertise::*, publication::*};
use mem_record::Permission;
use serde::Serialize;
use sqlx::{Postgres, Row, Transaction, types::Json};
use uuid::Uuid;

#[derive(Debug, Serialize)]
pub struct CollectionPage {
    pub collections: Vec<CapabilityAdvertisement>,
    pub next_after: Option<RecordId>,
}
#[derive(Debug, Serialize)]
pub struct CollectionDetail {
    pub snapshot: CollectionSnapshot,
    pub advertisement: CapabilityAdvertisement,
    pub reviewer: Uuid,
    pub request_id: Uuid,
}

async fn authorize(
    tx: &mut Transaction<'_, Postgres>,
    access: &LocalProjectAccess,
    space: SpaceId,
    owner: InstanceId,
) -> Result<()> {
    if access.read_only
        || [
            Permission::MemoryRead,
            Permission::MemoryCurate,
            Permission::BundleExport,
        ]
        .iter()
        .any(|p| !access.permissions.contains(*p))
    {
        return Err(StoreError::Binding);
    }
    load_grant(tx, access, space, owner).await?;
    Ok(())
}

/// Keyset pagination of local heads, including withdrawn collections. Active
/// reflects stored consent only, not current source eligibility or remote state.
pub async fn collection_page(
    tx: &mut Transaction<'_, Postgres>,
    access: &LocalProjectAccess,
    space: SpaceId,
    owner: InstanceId,
    after: Option<RecordId>,
    limit: u32,
) -> Result<CollectionPage> {
    if !(1..=32).contains(&limit) {
        return Err(StoreError::Limits);
    }
    authorize(tx, access, space, owner).await?;
    let rows = sqlx::query("SELECT r.advertisement FROM federation_expertise_collections c JOIN federation_expertise_reviews r USING(space,owner,project_id,collection,revision) WHERE c.space=$1 AND c.owner=$2 AND c.project_id=$3 AND ($4::uuid IS NULL OR c.collection>$4) ORDER BY c.collection LIMIT $5")
        .bind(space.as_uuid()).bind(owner.as_uuid()).bind(access.project)
        .bind(after.map(|id| id.as_uuid())).bind(i64::from(limit)+1).fetch_all(&mut **tx).await?;
    let more = rows.len() > limit as usize;
    let collections = rows
        .into_iter()
        .take(limit as usize)
        .map(|row| {
            row.try_get::<Json<CapabilityAdvertisement>, _>("advertisement")
                .map(|ad| ad.0)
        })
        .collect::<std::result::Result<Vec<_>, _>>()?;
    for ad in &collections {
        ad.validate()?;
        if ad.owner != owner {
            return Err(StoreError::Corrupt);
        }
    }
    let next_after = if more {
        collections.last().map(|ad| ad.collection)
    } else {
        None
    };
    Ok(CollectionPage {
        collections,
        next_after,
    })
}

/// Stored review only. Does not refresh versions, create consent or return an
/// activation request. A new revision always requires a new explicit preview.
pub async fn collection_detail(
    tx: &mut Transaction<'_, Postgres>,
    access: &LocalProjectAccess,
    space: SpaceId,
    owner: InstanceId,
    collection: RecordId,
) -> Result<Option<CollectionDetail>> {
    authorize(tx, access, space, owner).await?;
    let row = sqlx::query("SELECT r.snapshot,r.advertisement,r.principal_id,r.request_id FROM federation_expertise_collections c JOIN federation_expertise_reviews r USING(space,owner,project_id,collection,revision) WHERE c.space=$1 AND c.owner=$2 AND c.project_id=$3 AND c.collection=$4")
        .bind(space.as_uuid()).bind(owner.as_uuid()).bind(access.project).bind(collection.as_uuid()).fetch_optional(&mut **tx).await?;
    let Some(row) = row else {
        return Ok(None);
    };
    let snapshot = row.try_get::<Json<CollectionSnapshot>, _>("snapshot")?.0;
    let advertisement = row
        .try_get::<Json<CapabilityAdvertisement>, _>("advertisement")?
        .0;
    advertisement.validate()?;
    if snapshot.space != space
        || snapshot.owner != owner
        || snapshot.collection != collection
        || advertisement.owner != owner
        || advertisement.collection != collection
        || advertisement.revision != snapshot.revision
        || advertisement.collection_digest != snapshot.digest()?
    {
        return Err(StoreError::Corrupt);
    }
    Ok(Some(CollectionDetail {
        snapshot,
        advertisement,
        reviewer: row.try_get("principal_id")?,
        request_id: row.try_get("request_id")?,
    }))
}
