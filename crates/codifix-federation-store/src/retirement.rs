// SPDX-License-Identifier: AGPL-3.0-or-later
//! Durable release-specific fences; retained evidence is never rewritten.
use crate::{ReceiptKey, Result, StoreError};
use codifix_federation::publication::{InstanceId, SpaceId};
use serde::{Deserialize, Serialize};
use sqlx::{Executor, PgPool, Postgres, Row, types::Json};

#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct RetiredReplicaCheckpoint {
    pub receiver: InstanceId,
    pub authority: InstanceId,
    pub cursor: u32,
    pub history_sha256: String,
    pub keys: Vec<ReceiptKey>,
}
#[derive(Debug, Clone)]
pub struct RetiredSpace {
    pub local_instance: InstanceId,
    pub replicas: Vec<RetiredReplicaCheckpoint>,
}
/// Old binaries/processes must be stopped and drained before the operator writes
/// a marker. This is a durable restart/retry fence, not distributed cancellation.
pub async fn require_active<'e, E>(executor: E, space: SpaceId) -> Result<()>
where
    E: Executor<'e, Database = Postgres>,
{
    let retired: bool = sqlx::query_scalar(
        "SELECT EXISTS(SELECT 1 FROM codifix_retired_federation_spaces WHERE space=$1)",
    )
    .bind(space.as_uuid())
    .fetch_one(executor)
    .await?;
    if retired {
        return Err(StoreError::Retired);
    }
    Ok(())
}
pub async fn retired_space(pool: &PgPool, space: SpaceId) -> Result<Option<RetiredSpace>> {
    let row = sqlx::query(
        "SELECT local_instance,replicas FROM codifix_retired_federation_spaces WHERE space=$1",
    )
    .bind(space.as_uuid())
    .fetch_optional(pool)
    .await?;
    row.map(|r| {
        Ok(RetiredSpace {
            local_instance: InstanceId::new(r.try_get("local_instance")?)?,
            replicas: r
                .try_get::<Json<Vec<RetiredReplicaCheckpoint>>, _>("replicas")?
                .0,
        })
    })
    .transpose()
}
