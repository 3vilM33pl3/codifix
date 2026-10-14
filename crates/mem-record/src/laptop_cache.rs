// SPDX-License-Identifier: AGPL-3.0-or-later
//! Local human inspection of an isolated, read-only approved project cache.
use chrono::{DateTime, Utc};
use serde::{Deserialize, Serialize};
use uuid::Uuid;

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq)]
pub struct LaptopCacheProject {
    pub subscription_id: Uuid,
    pub device_id: Uuid,
    pub company_id: Uuid,
    pub area: String,
    pub owner_instance_id: Uuid,
    pub project_id: Uuid,
    pub space_id: Uuid,
    pub cursor: u64,
    /// Last verified committed page, not current network authority.
    pub observed_at: Option<DateTime<Utc>>,
    pub initialized: bool,
    pub read_only: bool,
    /// Metadata-only local contribution counts; not remote acknowledgement of replication.
    #[serde(default, skip_serializing_if = "std::collections::BTreeMap::is_empty")]
    pub uploads: std::collections::BTreeMap<String, i64>,
}
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct LaptopCacheList {
    pub schema_version: u32,
    pub area: String,
    pub instance_id: Uuid,
    pub projects: Vec<LaptopCacheProject>,
}
#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct LaptopCacheSearchRequest {
    pub question: String,
    #[serde(default = "default_limit")]
    pub limit: u32,
}
fn default_limit() -> u32 {
    8
}
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct LaptopCacheEvidence {
    pub label: String,
    pub text: String,
    pub digest: String,
    pub reviewer: String,
    pub reviewed_at: DateTime<Utc>,
}
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct LaptopCacheHit {
    pub stream_id: Uuid,
    pub author: String,
    pub authored_at: DateTime<Utc>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub locator: Option<String>,
    /// SHA-256 of the accepted publication or curated version record.
    pub digest: String,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub shared_version: Option<u32>,
    #[serde(default)]
    pub origin: LaptopCacheOrigin,
    pub source_record_id: Uuid,
    pub source_version: u32,
    pub text: String,
    pub summary: String,
    pub evidence: Vec<LaptopCacheEvidence>,
    #[serde(default, skip_serializing_if = "Vec::is_empty")]
    pub unavailable_source_ids: Vec<Uuid>,
    pub read_only: bool,
}
#[derive(Debug, Clone, Copy, Default, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum LaptopCacheOrigin {
    #[default]
    SharedPublication,
    CuratedProject,
}
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct LaptopCacheSearchResponse {
    pub schema_version: u32,
    pub project: LaptopCacheProject,
    pub hits: Vec<LaptopCacheHit>,
}
