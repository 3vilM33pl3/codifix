// SPDX-License-Identifier: AGPL-3.0-or-later
//! Fresh owner-authorized metadata, not a device approval or content grant.
use crate::OwnerDeviceCatalogProject;
use chrono::{DateTime, Utc};
use serde::{Deserialize, Serialize};
use uuid::Uuid;

#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct CompanyDeviceCatalogResponse {
    pub company_id: Uuid,
    pub projects: Vec<OwnerDeviceCatalogProject>,
    pub observed_at: DateTime<Utc>,
}
