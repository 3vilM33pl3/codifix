// SPDX-License-Identifier: AGPL-3.0-or-later
//! Explicit read-only observation grants. No companion tokens or provider settings.
use serde::{Deserialize, Serialize};
use uuid::Uuid;

#[derive(Clone, Debug, Default, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct AgencyObservationConfig {
    #[serde(default)]
    pub enabled: bool,
    pub area: String,
    pub instance_id: Uuid,
    /// Explicit peer labels eligible for projection; no automatic discovery.
    #[serde(default)]
    pub peer_ids: Vec<String>,
    #[serde(default)]
    pub exchange_projects: Vec<AgencyExchangeProject>,
}
#[derive(Clone, Debug, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct AgencyExchangeProject {
    pub project: String,
    pub principal_ids: Vec<Uuid>,
}
