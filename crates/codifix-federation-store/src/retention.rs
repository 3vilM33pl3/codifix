// SPDX-License-Identifier: AGPL-3.0-or-later
//! Append-only operational decisions, not a legal policy or a purge engine.
use crate::{GatewayStore, Result, StoreError, WriterFence};
use codifix_federation::{SpaceCapability, publication::InstanceId};
use serde::{Deserialize, Serialize};

#[derive(Debug, Clone, Copy, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum RetentionAction {
    Hold,
    ReleaseHold,
    RequestErasure,
}
impl RetentionAction {
    fn name(self) -> &'static str {
        match self {
            Self::Hold => "hold",
            Self::ReleaseHold => "release_hold",
            Self::RequestErasure => "request_erasure",
        }
    }
}
#[derive(Debug, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct RetentionRequest {
    pub action: RetentionAction,
    pub reason: String,
}
#[derive(Debug, Serialize, Deserialize)]
pub struct RetentionStatus {
    pub decision: i64,
    pub hold: bool,
    /// Always false in this profile. Even a released hold cannot authorize
    /// physical purge of protocol history or recipients' previously held bytes.
    pub physical_erasure_authorized: bool,
}
impl GatewayStore {
    pub async fn retention_decision(
        &self,
        fence: WriterFence,
        actor: InstanceId,
        request: &RetentionRequest,
    ) -> Result<RetentionStatus> {
        if request.reason.trim().is_empty() || request.reason.len() > 1024 {
            return Err(StoreError::Limits);
        }
        let mut tx = self.pool.begin().await?;
        let authority = self.lock(&mut tx, fence).await?;
        let state = self.load(&mut tx, fence.space, authority).await?;
        if actor != authority || !state.has_capability(actor, SpaceCapability::ManageMembership) {
            return Err(StoreError::Binding);
        }
        let decision: i64 = sqlx::query_scalar(
            "SELECT COALESCE(max(decision),0)+1 FROM federation_retention_decisions WHERE space=$1",
        )
        .bind(fence.space.as_uuid())
        .fetch_one(&mut *tx)
        .await?;
        sqlx::query("INSERT INTO federation_retention_decisions(space,decision,actor,action,reason) VALUES($1,$2,$3,$4,$5)")
            .bind(fence.space.as_uuid()).bind(decision).bind(actor.as_uuid()).bind(request.action.name()).bind(&request.reason).execute(&mut *tx).await?;
        let hold:Option<bool>=sqlx::query_scalar("SELECT action='hold' FROM federation_retention_decisions WHERE space=$1 AND action IN ('hold','release_hold') ORDER BY decision DESC LIMIT 1")
            .bind(fence.space.as_uuid()).fetch_optional(&mut *tx).await?;
        tx.commit().await?;
        Ok(RetentionStatus {
            decision,
            hold: hold.unwrap_or(false),
            physical_erasure_authorized: false,
        })
    }
}
