// SPDX-License-Identifier: AGPL-3.0-or-later
//! Trusted, in-process persistence APIs. Authentication, review UI, network
//! verification and automatic service scheduling are NOT provided by this crate.

mod audit;
mod expertise_collections;
mod expertise_delivery;
mod expertise_mailbox;
mod expertise_requests;
mod expertise_results;
pub use expertise_delivery::*;
pub use expertise_requests::*;
pub use expertise_results::*;
mod gateway;
pub use audit::*;
pub use expertise_collections::*;
pub use expertise_mailbox::*;
mod projection;
mod publisher;
mod replica;
mod retention;
pub use retention::*;
mod replica_audit;
pub use replica_audit::receipt_matches;
mod sharing;
mod workflow;
pub use workflow::*;
mod actions;
pub use actions::*;
mod source;
pub use sharing::*;

pub use gateway::*;
pub use publisher::*;
pub use replica::*;
pub use source::*;

use mem_federation::publication::*;
use serde::{Deserialize, Serialize};
use sqlx::{PgConnection, Row};

#[derive(Debug, thiserror::Error)]
pub enum StoreError {
    #[error(transparent)]
    Expertise(#[from] mem_federation::expertise::ExpertiseReject),
    #[error(transparent)]
    Database(#[from] sqlx::Error),
    #[error(transparent)]
    Json(#[from] serde_json::Error),
    #[error(transparent)]
    Publication(#[from] PublicationError),
    #[error("local policy rejected intent: {0:?}")]
    LocalPolicy(LocalPolicyReject),
    #[error("record not applied: {0:?}")]
    Outcome(PublicationOutcome),
    #[error("stale gateway writer epoch")]
    Fenced,
    #[error("expired or superseded publication attempt lease")]
    LostLease,
    #[error("identity, scope or immutable binding mismatch")]
    Binding,
    #[error("stored log, projection or cursor is inconsistent")]
    Corrupt,
    #[error("cursor ahead of published prefix or resync attempts rollback/fork")]
    Resync,
    #[error("invalid retry policy or batch limit")]
    Limits,
}
pub type Result<T> = std::result::Result<T, StoreError>;

/// These are independent observations. Replicated always names one receiver;
/// it is not a globally durable or all-members acknowledgement.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(tag = "state", rename_all = "snake_case")]
pub enum DeliveryStatus {
    Queued,
    Accepted { sequence: u32 },
    Published { sequence: u32 },
    Replicated { sequence: u32, receiver: InstanceId },
}

pub(crate) async fn replay_gateway(
    conn: &mut PgConnection,
    space: SpaceId,
    authority: InstanceId,
) -> Result<PublicationState> {
    let rows = sqlx::query(
        "SELECT sequence,publisher,record_id,locator,delivery FROM federation_accepted WHERE space=$1 ORDER BY sequence",
    )
    .bind(space.as_uuid())
    .fetch_all(conn)
    .await?;
    let mut state = PublicationState::new(space, authority);
    for row in rows {
        let delivery: sqlx::types::Json<PublicationDelivery> = row.try_get("delivery")?;
        if row.try_get::<i64, _>("sequence")? != i64::from(delivery.sequence.get())
            || row.try_get::<uuid::Uuid, _>("publisher")? != delivery.envelope.publisher.as_uuid()
            || row.try_get::<uuid::Uuid, _>("record_id")? != delivery.envelope.record_id.as_uuid()
            || row.try_get::<String, _>("locator")? != delivery.locator.as_str()
            || state.apply(delivery.envelope.publisher, delivery.0) != PublicationOutcome::Applied
        {
            return Err(StoreError::Corrupt);
        }
    }
    Ok(state)
}
