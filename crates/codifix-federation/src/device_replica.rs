// SPDX-License-Identifier: AGPL-3.0-or-later
//! Private project replicas are not publications in the shared ATProto space.
//! This deliberately has no raw-source, session, path or arbitrary JSON field.
use crate::{
    CanonicalJsonCodec, ContentDigest, publication::ConfidencePpm, subscription::ProjectScope,
};
use chrono::{DateTime, Utc};
use codifix_record::{MemoryStatus, MemoryType};
use serde::{Deserialize, Serialize};
use uuid::Uuid;

pub const DEVICE_REPLICA_SCHEMA: u32 = 1;
pub const DEVICE_REPLICA_PAGE_LIMIT: u32 = 32;

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct DeviceMemoryVersion {
    pub id: Uuid,
    pub canonical_id: Uuid,
    pub version: u32,
    pub tombstone: bool,
    pub text: String,
    pub summary: String,
    pub memory_type: MemoryType,
    pub scope: String,
    pub author: String,
    pub authored_at: DateTime<Utc>,
    pub updated_at: DateTime<Utc>,
    pub tags: Vec<String>,
    /// References preserve provenance identity without transferring raw bytes.
    pub unavailable_source_ids: Vec<Uuid>,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct DeviceCanonicalState {
    pub canonical_id: Uuid,
    pub current_version_id: Uuid,
    pub status: MemoryStatus,
    pub confidence: ConfidencePpm,
    pub importance: i32,
    pub updated_at: DateTime<Utc>,
    pub archived_at: Option<DateTime<Utc>>,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct DeviceRelation {
    pub id: Uuid,
    pub source_canonical: Uuid,
    pub target_canonical: Uuid,
    pub relation_type: String,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(
    tag = "kind",
    content = "value",
    rename_all = "snake_case",
    deny_unknown_fields
)]
pub enum DeviceReplicaRecord {
    MemoryVersion(DeviceMemoryVersion),
    CanonicalState(DeviceCanonicalState),
    Relation(DeviceRelation),
}
impl DeviceReplicaRecord {
    pub fn key(&self) -> String {
        match self {
            Self::MemoryVersion(v) => format!("version:{}", v.id),
            Self::CanonicalState(v) => format!("state:{}", v.canonical_id),
            Self::Relation(v) => format!("relation:{}", v.id),
        }
    }
    pub fn valid(&self) -> bool {
        match self {
            Self::MemoryVersion(v) => {
                !v.id.is_nil()
                    && !v.canonical_id.is_nil()
                    && v.version > 0
                    && v.text.len() <= 262_144
                    && v.summary.len() <= 32_768
                    && !v.author.is_empty()
                    && v.author.len() <= 512
                    && !v.scope.is_empty()
                    && v.scope.len() <= 512
                    && v.tags.len() <= 256
                    && v.tags.iter().all(|t| t.len() <= 512)
                    && v.unavailable_source_ids.len() <= 4096
                    && v.unavailable_source_ids.iter().all(|id| !id.is_nil())
            }
            Self::CanonicalState(v) => !v.canonical_id.is_nil() && !v.current_version_id.is_nil(),
            Self::Relation(v) => {
                !v.id.is_nil()
                    && !v.source_canonical.is_nil()
                    && !v.target_canonical.is_nil()
                    && !v.relation_type.is_empty()
                    && v.relation_type.len() <= 128
            }
        }
    }
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct DeviceReplicaRequest {
    pub schema_version: u32,
    pub nonce: Uuid,
    pub device: Uuid,
    pub scope: ProjectScope,
    pub catalog_revision: u64,
    pub after: u64,
    pub through: Option<u64>,
    pub limit: u32,
}
impl DeviceReplicaRequest {
    pub fn valid(&self) -> bool {
        self.schema_version == DEVICE_REPLICA_SCHEMA
            && !self.nonce.is_nil()
            && !self.device.is_nil()
            && self.scope.valid()
            && self.catalog_revision > 0
            && self.after <= i64::MAX as u64
            && self
                .through
                .is_none_or(|v| v >= self.after && v <= i64::MAX as u64)
            && (1..=DEVICE_REPLICA_PAGE_LIMIT).contains(&self.limit)
    }
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct DeviceReplicaEntry {
    pub sequence: u64,
    pub key: String,
    /// None removes a retained projection, e.g. source retention or relation deletion.
    pub record: Option<DeviceReplicaRecord>,
    pub digest: ContentDigest,
}
impl DeviceReplicaEntry {
    pub fn valid(&self) -> bool {
        let key_valid = self.key.split_once(':').is_some_and(|(kind, id)| {
            matches!(kind, "version" | "state" | "relation")
                && Uuid::parse_str(id).is_ok_and(|v| !v.is_nil())
        });
        key_valid
            && self.sequence > 0
            && self
                .record
                .as_ref()
                .is_none_or(|v| v.valid() && v.key() == self.key)
            && CanonicalJsonCodec::digest(&self.record).is_ok_and(|d| d == self.digest)
    }
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct DeviceReplicaPage {
    pub request: DeviceReplicaRequest,
    pub through: u64,
    pub next: u64,
    pub entries: Vec<DeviceReplicaEntry>,
}
impl DeviceReplicaPage {
    pub fn valid(&self) -> bool {
        self.request.valid()
            && self.through >= self.request.after
            && self.request.through.is_none_or(|v| v == self.through)
            && self.next == self.request.after + self.entries.len() as u64
            && self.next <= self.through
            && self.entries.len()
                == (self.through - self.request.after).min(u64::from(self.request.limit)) as usize
            && self
                .entries
                .iter()
                .enumerate()
                .all(|(i, e)| e.valid() && e.sequence == self.request.after + i as u64 + 1)
    }
}
