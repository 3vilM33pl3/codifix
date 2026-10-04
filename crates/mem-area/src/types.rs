// SPDX-License-Identifier: AGPL-3.0-or-later

use serde::{Deserialize, Serialize};
use uuid::Uuid;

pub const FORMAT_VERSION: u32 = 1;
/// Opt-in manifest extension only. Inventories and generated drafts stay v1.
pub const PROVENANCE_MANIFEST_VERSION: u32 = 2;
pub const PROVENANCE_PROJECTION_VERSION: u32 = 1;
pub const MAX_REVIEWED_EXCERPT_BYTES: usize = 16 * 1024;
/// Retained baseline head; changing this would silently alter existing fixtures.
pub const SUPPORTED_MIGRATION: u32 = 52;
pub const SUPERVISOR_MIGRATION: u32 = 53;
pub const SUPERVISOR_PROPOSAL_MIGRATION: u32 = 54;
pub const SUPERVISOR_CONTEXT_MIGRATION: u32 = 55;
pub const AREA_IMPORT_MIGRATION: u32 = 56;
pub const SUPERVISOR_HEALTH_MIGRATION: u32 = 57;
pub const SUPERVISOR_USAGE_MIGRATION: u32 = 58;
pub const SUPERVISOR_REVIEW_MIGRATION: u32 = 59;
pub const SUPERVISOR_QUESTION_MIGRATION: u32 = 60;
pub const AREA_RECONCILIATION_MIGRATION: u32 = 61;
pub const ADDRESSED_EXPERTISE_MIGRATION: u32 = 62;
pub const AGENT_EXPERTISE_MIGRATION: u32 = 63;
pub const ADDRESSED_DELETION_MIGRATION: u32 = 64;

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
    /// Explicitly reviewed safe projections; never generated by the planner.
    /// Omission preserves the format-1 wire representation exactly.
    #[serde(default, skip_serializing_if = "Vec::is_empty")]
    pub provenance_projections: Vec<ProvenanceProjection>,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct HashedRecordRef {
    pub record: RecordRef,
    pub row_sha256: String,
}

/// Exact reviewed input/output boundary, including one original memory version.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct ProvenanceBinding {
    pub source_instance: Uuid,
    pub snapshot_id: Uuid,
    pub inventory_sha256: String,
    pub destination_area: AreaId,
    pub canonical_id: Uuid,
    pub memory_version: HashedRecordRef,
    pub version: VersionIdentity,
    pub source: HashedRecordRef,
    /// Only the declared public.memory_sources.task_id FK is supported.
    pub task_constraint: String,
    pub archived_task: HashedRecordRef,
}

/// Complete allowed content of the projection: no inherited file path, task
/// title, prompt, session, capture body or original excerpt is copied.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(tag = "kind", rename_all = "snake_case", deny_unknown_fields)]
pub enum ProjectedSourceContent {
    ArchiveReferenceOnly {},
    ReviewedExcerpt { text: String, text_sha256: String },
}

/// An externally supplied assertion, NOT authenticated human approval. The
/// offline verifier checks completeness and binding, not the operator's identity.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct OperatorReviewAttestation {
    pub explicitly_approved: bool,
    pub operator: String,
    pub approval_record_id: Uuid,
    pub reviewed_at: String,
    pub decision_sha256: String,
    pub source_evidence: String,
    pub destination_evidence: String,
    pub content_evidence: String,
}

/// Per-source decision. Its digest covers version, decision ID, exact binding
/// and representation, excluding the external review attestation itself.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct ProvenanceProjection {
    pub format_version: u32,
    pub decision_id: Uuid,
    pub binding: ProvenanceBinding,
    pub content: ProjectedSourceContent,
    pub operator_attestation: OperatorReviewAttestation,
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
    /// Original source row is archived; ONLY the separately reviewed safe
    /// projection may exist in this area. Never a full-row-copy instruction.
    ArchiveAndProjectProvenance(AreaId),
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
    /// Safe projection payloads, never partial output on any validation error.
    #[serde(default, skip_serializing_if = "Vec::is_empty")]
    pub provenance_projections: Vec<ProvenanceProjection>,
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
