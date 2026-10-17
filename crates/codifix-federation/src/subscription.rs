// SPDX-License-Identifier: AGPL-3.0-or-later
//! Project delivery scopes are policy assertions by the authenticated origin.
//! They do not alter publication schemas or prove a complete repository.
use crate::publication::{InstanceId, RecordKey, SpaceId, StreamId};
use serde::{Deserialize, Serialize};
use uuid::Uuid;

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct ProjectScope {
    pub company: Uuid,
    pub area: String,
    pub owner: InstanceId,
    pub project: Uuid,
    pub space: SpaceId,
}
impl ProjectScope {
    pub fn valid(&self) -> bool {
        !self.company.is_nil()
            && !self.project.is_nil()
            && matches!(self.area.as_str(), "workshop" | "homelab" | "general")
    }
}

/// Exact origin-reviewed publication keys, not local raw records or a request
/// to dereference arbitrary dependencies. An empty/disabled review withdraws
/// future disclosure. Past recipient copies cannot be recalled.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct ProjectStreamReview {
    pub stream: StreamId,
    pub review_id: Uuid,
    pub records: Vec<RecordKey>,
    pub enabled: bool,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct ProjectFeedRegistration {
    pub operation_id: Uuid,
    pub scope: ProjectScope,
    pub expected_revision: u64,
    pub catalog_revision: u64,
    pub policy_revision: u64,
    pub reviews: Vec<ProjectStreamReview>,
}

/// A company service must establish current human/device authority before
/// making this origin-authenticated request, then recheck it before delivery.
/// The device never receives the origin's gateway credential.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct ProjectPageRequest {
    pub nonce: Uuid,
    pub device: Uuid,
    pub scope: ProjectScope,
    pub catalog_revision: u64,
    pub policy_revision: u64,
    pub binding_revision: u64,
    pub after: u64,
    /// Freeze one high-water mark for a multi-page snapshot or delta pass.
    pub through: Option<u64>,
    pub limit: u32,
}
impl ProjectPageRequest {
    pub fn valid(&self) -> bool {
        self.scope.valid()
            && !self.nonce.is_nil()
            && !self.device.is_nil()
            && self.catalog_revision > 0
            && self.policy_revision > 0
            && self.binding_revision > 0
            && self.after <= i64::MAX as u64
            && self
                .through
                .is_none_or(|head| head >= self.after && head <= i64::MAX as u64)
            && (1..=32).contains(&self.limit)
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    #[test]
    fn trust_and_ambiguous_scope_are_not_offline_projects() {
        let mut scope = ProjectScope {
            company: Uuid::new_v4(),
            area: "general".into(),
            owner: InstanceId::new(Uuid::new_v4()).unwrap(),
            project: Uuid::new_v4(),
            space: SpaceId::new(Uuid::new_v4()).unwrap(),
        };
        assert!(scope.valid());
        scope.area = "trust".into();
        assert!(!scope.valid());
        scope.area = "General".into();
        assert!(!scope.valid());
        scope.area = "workshop".into();
        scope.company = Uuid::nil();
        assert!(!scope.valid());
    }
}
