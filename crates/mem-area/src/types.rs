// SPDX-License-Identifier: AGPL-3.0-or-later

use serde::{Deserialize, Serialize};
use uuid::Uuid;

pub const FORMAT_VERSION: u32 = 1;
/// Retained baseline head; changing this would silently alter existing fixtures.
pub const SUPPORTED_MIGRATION: u32 = 52;
pub const SUPERVISOR_MIGRATION: u32 = 53;

/// Stable wire identities. Display names must not become routing identifiers.
#[derive(Debug, Clone, Copy, PartialEq, Eq, PartialOrd, Ord, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum AreaId {
    Trust,
    Homelab,
    Workshop,
    General,
}

pub const AREAS: [AreaId; 4] = [
    AreaId::Trust,
    AreaId::Homelab,
    AreaId::Workshop,
    AreaId::General,
];

/// Must describe one consistent source snapshot, including empty tables.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct Inventory {
    pub format_version: u32,
    pub source_instance: Uuid,
    pub snapshot_id: Uuid,
    pub captured_at: String,
    pub migration_version: u32,
    pub projects: Vec<ProjectProvenance>,
    pub tables: Vec<TableInventory>,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct ProjectProvenance {
    pub id: Uuid,
    pub slug: String,
    pub name: String,
    pub root_path: String,
    pub created_at: String,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct TableInventory {
    /// Schema-qualified catalog name, for example `public.memory_entries`.
    pub table: String,
    pub columns: Vec<Column>,
    pub foreign_keys: Vec<ForeignKey>,
    pub row_count: u64,
    pub rows: Vec<RowInventory>,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct Column {
    pub name: String,
    /// PostgreSQL format_type output, including typmod where applicable.
    pub data_type: String,
    pub nullable: bool,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct ForeignKey {
    pub name: String,
    pub columns: Vec<String>,
    pub target_table: String,
    pub target_columns: Vec<String>,
}

#[derive(Debug, Clone, PartialEq, Eq, PartialOrd, Ord, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct RecordRef {
    pub table: String,
    /// Stable lossless primary/unique key serialization; never a row offset.
    pub key: String,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct RowInventory {
    pub key: String,
    /// SHA-256 of the complete original row, including IDs, authorship, times,
    /// tombstones, provenance, and any columns the planner does not interpret.
    pub row_sha256: String,
    pub source_project_id: Option<Uuid>,
    /// Required for memory_entries/state/tags/sources/checks. A supporting row
    /// with several owners must remain unowned and receive its own review.
    pub canonical_id: Option<Uuid>,
    /// Present only for memory_entries; identifies every historical version.
    pub version: Option<VersionIdentity>,
    /// Exactly one entry per catalog FK, including a null target for SQL NULL.
    /// Additional logical references may use a `logical:` constraint name.
    pub references: Vec<RowReference>,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct VersionIdentity {
    pub version_no: u32,
    pub is_tombstone: bool,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct RowReference {
    pub constraint: String,
    pub target: Option<RecordRef>,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum TableStrategy {
    /// Requires lineage ownership or an individual reviewed row decision.
    Owned,
    /// Kept in protected archive; area-specific projects retain original IDs.
    ProjectProvenance,
    /// Preserve source evidence in archive and reconstruct target derived rows.
    RebuildDerived,
    /// Preserve complete rows in protected archive, never expose via retrieval.
    HistoricalArchive,
    /// Preserve evidence in archive; never import live jobs, tokens or grants.
    RetireRuntime,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct Manifest {
    pub format_version: u32,
    pub source_instance: Uuid,
    pub snapshot_id: Uuid,
    /// Binds all source row hashes, schema, counts and provenance to this plan.
    pub inventory_sha256: String,
    /// Retained unresolved historical references, copied exactly from inventory
    /// validation. These are evidence, never permission to restore authority.
    #[serde(default)]
    pub inventory_notices: Vec<InventoryNotice>,
    pub areas: Vec<AreaId>,
    pub table_policies: Vec<TablePolicy>,
    pub lineages: Vec<LineageDecision>,
    pub records: Vec<RecordDecision>,
    pub projects: Vec<ProjectBinding>,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct TablePolicy {
    pub table: String,
    /// None keeps unrecognized source tables explicitly blocked in a draft.
    pub strategy: Option<TableStrategy>,
    pub reason: String,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct LineageDecision {
    pub canonical_id: Uuid,
    pub source_project_ids: Vec<Uuid>,
    pub disposition: Disposition,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct RecordDecision {
    pub record: RecordRef,
    pub disposition: Disposition,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct ProjectBinding {
    pub area: AreaId,
    /// Original identity is the target identity in this format. Merging,
    /// renaming and project-ID reassignment require a future explicit format.
    pub source: ProjectProvenance,
    pub reason: String,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(tag = "kind", rename_all = "snake_case", deny_unknown_fields)]
pub enum Disposition {
    Unresolved,
    Area {
        area: AreaId,
        review: ClassificationReview,
    },
    ProtectedArchive {
        reason: String,
        review: ClassificationReview,
    },
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct ClassificationReview {
    pub classification: SourceClassification,
    pub source_evidence: String,
    pub project_evidence: String,
    pub content_evidence: String,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum SourceClassification {
    Knowledge,
    Test,
    Evaluation,
    Ambiguous,
    Mixed,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum Destination {
    Area(AreaId),
    ProtectedArchive,
    /// Original row is preserved in archive; target is rebuilt separately.
    ArchiveAndRebuild,
    /// Original row is preserved in archive with no activation authority.
    ArchiveAndRetire,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct PlannedRecord {
    pub record: RecordRef,
    pub source_sha256: String,
    pub source_project_id: Option<Uuid>,
    pub canonical_id: Option<Uuid>,
    pub destination: Destination,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct Verification {
    /// Only metadata/decision validation; not backup/restore/import proof.
    pub plan_valid: bool,
    pub import_supported: bool,
    pub blockers: Vec<String>,
    pub notices: Vec<InventoryNotice>,
    pub records: Vec<PlannedRecord>,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct InventoryValidation {
    pub blockers: Vec<String>,
    pub notices: Vec<InventoryNotice>,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct InventoryNotice {
    pub kind: InventoryNoticeKind,
    pub record: RecordRef,
    /// Original value, including a nil UUID if present in historical source.
    pub source_project_id: Uuid,
    /// Exact schema-local project catalog in which the value was absent.
    pub project_table: String,
    /// Only historical_archive or retire_runtime can produce this notice.
    pub mandatory_strategy: TableStrategy,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum InventoryNoticeKind {
    RetainedUnresolvedProjectReference,
}
