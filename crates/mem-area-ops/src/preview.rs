// SPDX-License-Identifier: AGPL-3.0-or-later
//! Explicit, bounded preview selection. Files only; never grants disclosure,
//! authenticates review, imports rows, or invents a per-source approval.
use crate::{
    Failure, INVENTORY_LIMIT, Result, hash, json,
    private::{Directory, Input},
    valid_digest,
};
use mem_area::*;
use serde::{Deserialize, Serialize};
use std::{
    collections::{BTreeMap, BTreeSet},
    io::{BufWriter, Write},
    path::Path,
};
use uuid::Uuid;

const SELECTION_LIMIT: u64 = 16 * 1024 * 1024;
const MANIFEST: &str = "preview-manifest.json";
const RECEIPT: &str = "preview-receipt.json";

#[derive(Clone, Deserialize, Serialize)]
#[serde(deny_unknown_fields)]
pub struct Selection {
    pub format_version: u32,
    pub source_instance: Uuid,
    pub snapshot_id: Uuid,
    pub inventory_sha256: String,
    pub area: AreaId,
    pub project_id: Uuid,
    pub review: Review,
    pub lineages: Vec<SelectedLineage>,
    pub sources: Vec<SelectedSource>,
}
#[derive(Clone, Deserialize, Serialize)]
#[serde(deny_unknown_fields)]
pub struct Review {
    pub explicitly_approved: bool,
    pub operator: String,
    pub reviewed_at: String,
    pub authority_basis: String,
    pub source_evidence: String,
    pub project_evidence: String,
    pub content_evidence: String,
}
#[derive(Clone, Deserialize, Serialize)]
#[serde(deny_unknown_fields)]
pub struct SelectedLineage {
    pub canonical_id: Uuid,
    pub canonical_state_sha256: String,
    pub versions: Vec<SelectedVersion>,
}
#[derive(Clone, Deserialize, Serialize)]
#[serde(deny_unknown_fields)]
pub struct SelectedVersion {
    pub id: Uuid,
    pub version_no: u32,
    pub is_tombstone: bool,
    pub row_sha256: String,
    pub canonical_text_sha256: String,
    pub row_json: String,
}
#[derive(Clone, Deserialize, Serialize)]
#[serde(deny_unknown_fields)]
pub struct SelectedSource {
    pub source_id: Uuid,
    pub row_sha256: String,
    pub task_id: Uuid,
    pub task_row_sha256: String,
    pub decision_id: Uuid,
    pub approval_record_id: Uuid,
    pub explicitly_approved: bool,
}

#[derive(Deserialize)]
struct VersionPayload {
    id: Uuid,
    project_id: Uuid,
    canonical_id: Uuid,
    version_no: u32,
    is_tombstone: bool,
    canonical_text: String,
    #[serde(flatten)]
    remaining: BTreeMap<String, serde::de::IgnoredAny>,
}

fn table<'a>(inventory: &'a Inventory, name: &str) -> Result<&'a TableInventory> {
    inventory
        .tables
        .iter()
        .find(|table| table.table == name)
        .ok_or(Failure::Preview)
}
fn row(table: &TableInventory, key: Uuid) -> Result<&RowInventory> {
    let key = key.to_string();
    table
        .rows
        .iter()
        .find(|row| row.key == key)
        .ok_or(Failure::Preview)
}
fn reference<'a>(
    table: &TableInventory,
    row: &'a RowInventory,
    column: &str,
    target: &str,
) -> Result<(&'a str, &'a RecordRef)> {
    let foreign: Vec<_> = table
        .foreign_keys
        .iter()
        .filter(|fk| {
            fk.columns == [column] && fk.target_table == target && fk.target_columns == ["id"]
        })
        .collect();
    let [foreign] = foreign.as_slice() else {
        return Err(Failure::Preview);
    };
    let reference = row
        .references
        .iter()
        .find(|reference| reference.constraint == foreign.name)
        .ok_or(Failure::Preview)?;
    let target_ref = reference
        .target
        .as_ref()
        .filter(|record| record.table == target)
        .ok_or(Failure::Preview)?;
    Ok((&reference.constraint, target_ref))
}
fn bounded_text(value: &str) -> bool {
    !value.trim().is_empty() && value.len() <= 4096 && !value.contains('\0')
}
fn exclusion() -> Disposition {
    Disposition::ProtectedArchive {
        reason: "Outside bounded preview; original retained.".into(),
        review: ClassificationReview {
            classification: SourceClassification::Ambiguous,
            source_evidence: "Protected snapshot retained.".into(),
            project_evidence: "No reassignment.".into(),
            content_evidence: "Excluded; no completed review claimed.".into(),
        },
    }
}

/// Validate exact selection and produce a verified, preview-only manifest.
/// Review text is an operator assertion, never authenticated human consent.
pub fn reviewed_manifest(inventory: &Inventory, selection: &Selection) -> Result<Manifest> {
    if selection.format_version != 1
        || selection.source_instance != inventory.source_instance
        || selection.snapshot_id != inventory.snapshot_id
        || selection.project_id.is_nil()
        || selection.lineages.is_empty()
        || selection.lineages.len() > 64
        || selection.sources.len() > 4096
        || !selection.review.explicitly_approved
        || [
            &selection.review.operator,
            &selection.review.reviewed_at,
            &selection.review.authority_basis,
            &selection.review.source_evidence,
            &selection.review.project_evidence,
            &selection.review.content_evidence,
        ]
        .iter()
        .any(|value| !bounded_text(value))
    {
        return Err(Failure::Preview);
    }
    let mut manifest = draft_manifest(inventory).map_err(|_| Failure::Preview)?;
    if !valid_digest(&selection.inventory_sha256)
        || manifest.inventory_sha256 != selection.inventory_sha256
    {
        return Err(Failure::Binding);
    }
    let project = inventory
        .projects
        .iter()
        .find(|project| project.id == selection.project_id)
        .ok_or(Failure::Preview)?;
    let versions = table(inventory, "public.memory_entries")?;
    let states = table(inventory, "public.memory_canonical_state")?;
    let source_table = table(inventory, "public.memory_sources")?;
    let tasks = table(inventory, "public.tasks")?;
    let selected: BTreeSet<_> = selection
        .lineages
        .iter()
        .map(|lineage| lineage.canonical_id)
        .collect();
    if selected.len() != selection.lineages.len() || selected.contains(&Uuid::nil()) {
        return Err(Failure::Preview);
    }
    for lineage in &selection.lineages {
        let state = row(states, lineage.canonical_id)?;
        if state.row_sha256 != lineage.canonical_state_sha256
            || state.source_project_id != Some(project.id)
            || state.canonical_id != Some(lineage.canonical_id)
            || lineage.versions.is_empty()
            || lineage.versions.len() > 256
        {
            return Err(Failure::Preview);
        }
        let expected: BTreeMap<_, _> = versions
            .rows
            .iter()
            .filter(|row| row.canonical_id == Some(lineage.canonical_id))
            .map(|row| (row.key.as_str(), row))
            .collect();
        let supplied: BTreeSet<_> = lineage
            .versions
            .iter()
            .map(|version| version.id.to_string())
            .collect();
        if supplied.len() != lineage.versions.len()
            || !supplied
                .iter()
                .map(String::as_str)
                .eq(expected.keys().copied())
        {
            return Err(Failure::Preview);
        }
        for version in &lineage.versions {
            let original = expected
                .get(version.id.to_string().as_str())
                .ok_or(Failure::Preview)?;
            if version.id.is_nil()
                || version.row_json.len() > 8 * 1024 * 1024
                || original.source_project_id != Some(project.id)
                || original.version.as_ref()
                    != Some(&VersionIdentity {
                        version_no: version.version_no,
                        is_tombstone: version.is_tombstone,
                    })
                || original.row_sha256 != version.row_sha256
                || hash(version.row_json.as_bytes()) != version.row_sha256
                || !valid_digest(&version.canonical_text_sha256)
            {
                return Err(Failure::Preview);
            }
            let payload: VersionPayload =
                serde_json::from_str(&version.row_json).map_err(|_| Failure::Preview)?;
            let mut columns: BTreeSet<_> = payload.remaining.keys().map(String::as_str).collect();
            columns.extend([
                "id",
                "project_id",
                "canonical_id",
                "version_no",
                "is_tombstone",
                "canonical_text",
            ]);
            if columns
                != versions
                    .columns
                    .iter()
                    .map(|column| column.name.as_str())
                    .collect()
                || payload.id != version.id
                || payload.project_id != project.id
                || payload.canonical_id != lineage.canonical_id
                || payload.version_no != version.version_no
                || payload.is_tombstone != version.is_tombstone
                || hash(payload.canonical_text.as_bytes()) != version.canonical_text_sha256
            {
                return Err(Failure::Preview);
            }
        }
    }
    manifest.format_version = PROVENANCE_MANIFEST_VERSION;
    for lineage in &mut manifest.lineages {
        lineage.disposition = if selected.contains(&lineage.canonical_id) {
            if lineage.source_project_ids != [project.id] {
                return Err(Failure::Preview);
            }
            Disposition::Area {
                area: selection.area,
                review: ClassificationReview {
                    classification: SourceClassification::Knowledge,
                    source_evidence: selection.review.source_evidence.clone(),
                    project_evidence: selection.review.project_evidence.clone(),
                    content_evidence: selection.review.content_evidence.clone(),
                },
            }
        } else {
            exclusion()
        };
    }
    for record in &mut manifest.records {
        record.disposition = exclusion();
    }
    manifest.projects = vec![ProjectBinding {
        area: selection.area,
        source: project.clone(),
        reason: "Original project retained for exact reviewed preview lineages.".into(),
    }];
    let expected_sources: BTreeMap<_, _> = source_table
        .rows
        .iter()
        .filter(|row| row.canonical_id.is_some_and(|id| selected.contains(&id)))
        .map(|row| (row.key.as_str(), row))
        .collect();
    let supplied_sources: BTreeSet<_> = selection
        .sources
        .iter()
        .map(|source| source.source_id.to_string())
        .collect();
    if supplied_sources.len() != selection.sources.len()
        || !supplied_sources
            .iter()
            .map(String::as_str)
            .eq(expected_sources.keys().copied())
    {
        return Err(Failure::Preview);
    }
    for source in &selection.sources {
        let original = expected_sources
            .get(source.source_id.to_string().as_str())
            .ok_or(Failure::Preview)?;
        if !source.explicitly_approved
            || original.row_sha256 != source.row_sha256
            || original.source_project_id != Some(project.id)
        {
            return Err(Failure::Preview);
        }
        let (task_constraint, task_ref) =
            reference(source_table, original, "task_id", "public.tasks")?;
        if task_ref.key != source.task_id.to_string() {
            return Err(Failure::Preview);
        }
        let task = row(tasks, source.task_id)?;
        if task.row_sha256 != source.task_row_sha256 {
            return Err(Failure::Preview);
        }
        let (_, version_ref) = reference(
            source_table,
            original,
            "memory_entry_id",
            "public.memory_entries",
        )?;
        let version_id = Uuid::parse_str(&version_ref.key).map_err(|_| Failure::Preview)?;
        let version = row(versions, version_id)?;
        let mut projection = ProvenanceProjection {
            format_version: PROVENANCE_PROJECTION_VERSION,
            decision_id: source.decision_id,
            binding: ProvenanceBinding {
                source_instance: inventory.source_instance,
                snapshot_id: inventory.snapshot_id,
                inventory_sha256: manifest.inventory_sha256.clone(),
                destination_area: selection.area,
                canonical_id: original.canonical_id.ok_or(Failure::Preview)?,
                memory_version: HashedRecordRef {
                    record: version_ref.clone(),
                    row_sha256: version.row_sha256.clone(),
                },
                version: version.version.clone().ok_or(Failure::Preview)?,
                source: HashedRecordRef {
                    record: RecordRef {
                        table: source_table.table.clone(),
                        key: original.key.clone(),
                    },
                    row_sha256: original.row_sha256.clone(),
                },
                task_constraint: task_constraint.into(),
                archived_task: HashedRecordRef {
                    record: task_ref.clone(),
                    row_sha256: task.row_sha256.clone(),
                },
            },
            content: ProjectedSourceContent::ArchiveReferenceOnly {},
            operator_attestation: OperatorReviewAttestation {
                explicitly_approved: source.explicitly_approved,
                operator: selection.review.operator.clone(),
                approval_record_id: source.approval_record_id,
                reviewed_at: selection.review.reviewed_at.clone(),
                decision_sha256: String::new(),
                source_evidence: selection.review.source_evidence.clone(),
                destination_evidence: selection.review.project_evidence.clone(),
                content_evidence: format!(
                    "Archive reference only; no original context copied. Operator-supplied authority: {}",
                    selection.review.authority_basis
                ),
            },
        };
        projection.operator_attestation.decision_sha256 =
            projection.decision_digest().map_err(|_| Failure::Preview)?;
        manifest.provenance_projections.push(projection);
    }
    manifest
        .provenance_projections
        .sort_by(|left, right| left.binding.source.record.cmp(&right.binding.source.record));
    if !verify_manifest(inventory, &manifest).plan_valid {
        return Err(Failure::Preview);
    }
    Ok(manifest)
}

#[derive(Serialize)]
struct Receipt<'a> {
    format_version: u32,
    status: &'static str,
    source_instance: Uuid,
    snapshot_id: Uuid,
    area: AreaId,
    project_id: Uuid,
    inventory_sha256: &'a str,
    inventory_file_sha256: String,
    selection_file_sha256: String,
    manifest_sha256: String,
    lineages: usize,
    versions: usize,
    projections: usize,
    review: &'a Review,
    authenticated_human_approval: bool,
    full_partition_complete: bool,
    remote_or_model_consent_granted: bool,
}

/// File-only operator. All files are pinned private inputs or exclusive private
/// outputs. Failure may leave a partial; never overwrite/clean up unknown files.
pub fn write_preview(
    inventory_path: &Path,
    selection_path: &Path,
    expected_selection_sha256: &str,
    output: &Path,
) -> Result<()> {
    if !valid_digest(expected_selection_sha256) {
        return Err(Failure::Binding);
    }
    let directory = Directory::open(output)?;
    directory.ensure_absent(&[
        "preview-manifest.partial",
        MANIFEST,
        "preview-receipt.partial",
        RECEIPT,
    ])?;
    let mut inventory_input = Input::open(inventory_path, INVENTORY_LIMIT)?;
    let mut selection_input = Input::open(selection_path, SELECTION_LIMIT)?;
    let (selection, selection_file_sha256): (Selection, _) =
        json(&mut selection_input, SELECTION_LIMIT)?;
    if selection_file_sha256 != expected_selection_sha256 {
        return Err(Failure::Binding);
    }
    let (inventory, inventory_file_sha256): (Inventory, _) =
        json(&mut inventory_input, INVENTORY_LIMIT)?;
    let manifest = reviewed_manifest(&inventory, &selection)?;
    let receipt = Receipt {
        format_version: 1,
        status: "reviewed_preview_only_not_imported_or_activated",
        source_instance: selection.source_instance,
        snapshot_id: selection.snapshot_id,
        area: selection.area,
        project_id: selection.project_id,
        inventory_sha256: &manifest.inventory_sha256,
        inventory_file_sha256,
        selection_file_sha256,
        manifest_sha256: manifest_digest(&manifest).map_err(|_| Failure::Preview)?,
        lineages: selection.lineages.len(),
        versions: selection
            .lineages
            .iter()
            .map(|lineage| lineage.versions.len())
            .sum(),
        projections: manifest.provenance_projections.len(),
        review: &selection.review,
        authenticated_human_approval: false,
        full_partition_complete: false,
        remote_or_model_consent_granted: false,
    };
    inventory_input.unchanged()?;
    selection_input.unchanged()?;
    let mut writer = BufWriter::new(directory.create("preview-manifest.partial")?);
    serde_json::to_writer(&mut writer, &manifest).map_err(|_| Failure::Output)?;
    writer.flush().map_err(|_| Failure::Output)?;
    let file = writer.into_inner().map_err(|_| Failure::Output)?;
    directory.publish("preview-manifest.partial", MANIFEST, &file)?;
    let mut file = directory.create("preview-receipt.partial")?;
    serde_json::to_writer(&mut file, &receipt).map_err(|_| Failure::Output)?;
    directory.publish("preview-receipt.partial", RECEIPT, &file)?;
    Ok(())
}
