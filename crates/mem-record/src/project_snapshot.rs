// SPDX-License-Identifier: AGPL-3.0-or-later
//! A device may request only a bounded cursor, never source IDs or authority.
use serde::{Deserialize, Serialize};
use uuid::Uuid;

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct OnboardingProjectSnapshotRequest {
    pub nonce: Uuid,
    pub after: u64,
    pub through: Option<u64>,
    pub limit: u32,
}
