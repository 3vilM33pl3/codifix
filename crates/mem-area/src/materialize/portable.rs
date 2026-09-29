// SPDX-License-Identifier: AGPL-3.0-or-later
//! Operator-trusted, hash-pinned transfer of one already materialized area.
//! The expected file hash must arrive through the signed operator handoff, not
//! from the untrusted bundle itself. This is not source inventory verification.
use super::*;

pub const MAX_PORTABLE_BYTES: usize = 256 * 1024 * 1024;

#[derive(Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
struct Bundle<P> {
    format_version: u32,
    target_instance: Uuid,
    rows: usize,
    projections: usize,
    partition: P,
}

#[derive(Deserialize)]
#[serde(deny_unknown_fields)]
struct WirePartition {
    area: AreaId,
    source_instance: Uuid,
    snapshot_id: Uuid,
    source_migration_version: u32,
    inventory_sha256: String,
    manifest_sha256: String,
    partition_sha256: String,
    rows: Vec<WireRow>,
    projected_sources: Vec<WireProjection>,
}
#[derive(Deserialize)]
#[serde(deny_unknown_fields)]
struct WireRow {
    record: RecordRef,
    source_sha256: String,
    target_sha256: String,
    row_json: String,
    columns: Vec<Column>,
    source_project_id: Option<Uuid>,
    canonical_id: Option<Uuid>,
}
#[derive(Deserialize)]
#[serde(deny_unknown_fields)]
struct WireProjection {
    row: WireRow,
    binding: ProvenanceBinding,
    decision_id: Uuid,
    decision_sha256: String,
    content: ProjectedSourceContent,
}

fn digest(value: &str) -> bool {
    value.len() == 64
        && value
            .bytes()
            .all(|b| b.is_ascii_hexdigit() && !b.is_ascii_uppercase())
}
fn invalid<T>() -> Result<T, MaterializeError> {
    Err(MaterializeError::InvalidRow)
}
fn row(w: WireRow, projected: bool) -> Result<MaterializedRow, MaterializeError> {
    let allowed = if projected {
        w.record.table == "public.memory_sources"
    } else {
        matches!(
            w.record.table.as_str(),
            "public.projects"
                | "public.memory_entries"
                | "public.memory_canonical_state"
                | "public.memory_tags"
                | "public.memory_relations"
        )
    };
    if !allowed
        || w.row_json.len() > MAX_PAYLOAD_ENVELOPE_BYTES
        || !digest(&w.source_sha256)
        || !digest(&w.target_sha256)
        || format!("{:x}", Sha256::digest(w.row_json.as_bytes())) != w.target_sha256
        || (!projected && w.source_sha256 != w.target_sha256)
        || w.columns.is_empty()
        || w.columns.len() > 128
        || w.columns.iter().any(|c| {
            c.name.is_empty()
                || c.name.len() > 63
                || !c
                    .name
                    .bytes()
                    .all(|b| b.is_ascii_lowercase() || b.is_ascii_digit() || b == b'_')
        })
    {
        return invalid();
    }
    let table = TableInventory {
        table: w.record.table.clone(),
        columns: w.columns.clone(),
        foreign_keys: vec![],
        row_count: 0,
        rows: vec![],
    };
    let fields = validate_row(&w.row_json, &table)?;
    if w.columns.len() != fields.0.len()
        || key_from_columns(
            &fields,
            primary_columns(&w.record.table).ok_or(MaterializeError::InvalidRow)?,
            false,
        )?
        .as_ref()
            != Some(&w.record.key)
    {
        return invalid();
    }
    for (name, value) in [
        ("project_id", w.source_project_id),
        ("canonical_id", w.canonical_id),
    ] {
        if fields.0.contains_key(name) && raw_uuid(&fields, name) != value {
            return invalid();
        }
    }
    Ok(MaterializedRow {
        record: w.record,
        source_sha256: w.source_sha256,
        target_sha256: w.target_sha256,
        row_json: w.row_json,
        columns: w.columns.into(),
        source_project_id: w.source_project_id,
        canonical_id: w.canonical_id,
    })
}

/// Export only the selected area's already validated target rows. Supporting
/// raw content, other-area inventories and operational authority are excluded.
pub fn export(
    partition: &MaterializedPartition,
    target: Uuid,
) -> Result<Vec<u8>, MaterializeError> {
    if target.is_nil() || target == partition.source_instance {
        return invalid();
    }
    let bytes = serde_json::to_vec(&Bundle {
        format_version: 1,
        target_instance: target,
        rows: partition.rows.len(),
        projections: partition.projected_sources.len(),
        partition,
    })
    .map_err(|_| MaterializeError::InvalidEnvelope)?;
    let expected = format!("{:x}", Sha256::digest(&bytes));
    // The exporter shares the restricted portable-profile validator.
    import(&bytes, &expected, target, partition.area)?;
    Ok(bytes)
}

/// Validate a single-area bundle against an independently trusted operator hash.
/// Hash agreement is an explicit delegation of the source preparation review;
/// it does not authenticate a sender or independently repeat source validation.
pub fn import(
    bytes: &[u8],
    expected: &str,
    target: Uuid,
    area: AreaId,
) -> Result<MaterializedPartition, MaterializeError> {
    if bytes.len() > MAX_PORTABLE_BYTES
        || !digest(expected)
        || format!("{:x}", Sha256::digest(bytes)) != expected
    {
        return Err(MaterializeError::HashMismatch);
    }
    let w: Bundle<WirePartition> =
        serde_json::from_slice(bytes).map_err(|_| MaterializeError::InvalidEnvelope)?;
    if w.format_version != 1
        || target.is_nil()
        || w.target_instance != target
        || w.partition.area != area
        || w.partition.source_instance == target
        || w.partition.source_instance.is_nil()
        || w.partition.snapshot_id.is_nil()
        || !(52..=61).contains(&w.partition.source_migration_version)
        || w.rows != w.partition.rows.len()
        || w.projections != w.partition.projected_sources.len()
        || w.rows + w.projections > 1_000_000
    {
        return invalid();
    }
    let w = w.partition;
    if [&w.inventory_sha256, &w.manifest_sha256, &w.partition_sha256]
        .iter()
        .any(|h| !digest(h))
    {
        return invalid();
    }
    let mut p = MaterializedPartition {
        area: w.area,
        source_instance: w.source_instance,
        snapshot_id: w.snapshot_id,
        source_migration_version: w.source_migration_version,
        inventory_sha256: w.inventory_sha256,
        manifest_sha256: w.manifest_sha256,
        partition_sha256: String::new(),
        rows: w
            .rows
            .into_iter()
            .map(|r| row(r, false))
            .collect::<Result<_, _>>()?,
        projected_sources: vec![],
    };
    for r in w.projected_sources {
        let row = row(r.row, true)?;
        let b = &r.binding;
        if b.source_instance != p.source_instance
            || b.snapshot_id != p.snapshot_id
            || b.inventory_sha256 != p.inventory_sha256
            || b.destination_area != p.area
            || b.source.record != row.record
            || b.source.row_sha256 != row.source_sha256
            || b.source.record.table != "public.memory_sources"
            || b.memory_version.record.table != "public.memory_entries"
            || b.archived_task.record.table != "public.tasks"
            || Uuid::parse_str(&b.archived_task.record.key).is_err()
            || b.task_constraint.is_empty()
            || b.task_constraint.len() > 63
            || !b
                .task_constraint
                .bytes()
                .all(|b| b.is_ascii_alphanumeric() || b == b'_')
            || b.canonical_id != row.canonical_id.ok_or(MaterializeError::InvalidRow)?
            || r.decision_id.is_nil()
            || !digest(&r.decision_sha256)
            || !digest(&b.memory_version.row_sha256)
            || !digest(&b.archived_task.row_sha256)
            || !matches!(r.content, ProjectedSourceContent::ArchiveReferenceOnly {})
        {
            return invalid();
        }
        let fields: RawRow =
            serde_json::from_str(&row.row_json).map_err(|_| MaterializeError::InvalidRow)?;
        if fields.0.len() != 14
            || fields.0.iter().any(|(k, v)| !match k.as_str() {
                "id" | "memory_entry_id" | "created_at" => true,
                "source_kind" => v.get() == "\"note\"",
                "task_id" | "file_path" | "git_commit" | "excerpt" | "symbol_name"
                | "symbol_kind" | "line_start" | "line_end" | "content_hash"
                | "target_memory_id" => v.get() == "null",
                _ => false,
            })
            || raw_uuid(&fields, "memory_entry_id")
                .map(|u| u.to_string())
                .as_ref()
                != Some(&b.memory_version.record.key)
        {
            return invalid();
        }
        p.projected_sources.push(MaterializedProjection {
            row,
            binding: r.binding,
            decision_id: r.decision_id,
            decision_sha256: r.decision_sha256,
            content: r.content,
        });
    }
    let mut entries = BTreeMap::new();
    let mut keys = BTreeSet::new();
    let mut projects = BTreeSet::new();
    let mut canonical = BTreeMap::new();
    let mut sequence = BTreeMap::<Uuid, BTreeSet<u32>>::new();
    for r in p
        .rows
        .iter()
        .chain(p.projected_sources.iter().map(|s| &s.row))
    {
        if !keys.insert(r.record.clone()) {
            return invalid();
        }
        let f: RawRow =
            serde_json::from_str(&r.row_json).map_err(|_| MaterializeError::InvalidRow)?;
        match r.record.table.as_str() {
            "public.projects" => {
                projects.insert(raw_uuid(&f, "id").ok_or(MaterializeError::InvalidRow)?);
            }
            "public.memory_entries" => {
                let id = raw_uuid(&f, "id").ok_or(MaterializeError::InvalidRow)?;
                let c = raw_uuid(&f, "canonical_id").ok_or(MaterializeError::InvalidRow)?;
                let project = raw_uuid(&f, "project_id").ok_or(MaterializeError::InvalidRow)?;
                let n: u32 = serde_json::from_str(
                    f.0.get("version_no")
                        .ok_or(MaterializeError::InvalidRow)?
                        .get(),
                )
                .map_err(|_| MaterializeError::InvalidRow)?;
                let tombstone: bool = serde_json::from_str(
                    f.0.get("is_tombstone")
                        .ok_or(MaterializeError::InvalidRow)?
                        .get(),
                )
                .map_err(|_| MaterializeError::InvalidRow)?;
                if n == 0
                    || id.is_nil()
                    || c.is_nil()
                    || project.is_nil()
                    || !sequence.entry(c).or_default().insert(n)
                {
                    return invalid();
                }
                entries.insert(id, (c, project, r.source_sha256.as_str(), n, tombstone));
            }
            "public.memory_canonical_state" => {
                canonical.insert(
                    raw_uuid(&f, "canonical_id").ok_or(MaterializeError::InvalidRow)?,
                    raw_uuid(&f, "project_id").ok_or(MaterializeError::InvalidRow)?,
                );
            }
            _ => {}
        }
    }
    for (id, ns) in sequence {
        if !canonical.contains_key(&id) || ns.iter().copied().ne(1..=ns.len() as u32) {
            return invalid();
        }
    }
    for r in &p.rows {
        let f: RawRow =
            serde_json::from_str(&r.row_json).map_err(|_| MaterializeError::InvalidRow)?;
        if r.source_project_id
            .is_some_and(|id| !projects.contains(&id))
        {
            return invalid();
        }
        match r.record.table.as_str() {
            "public.memory_entries" => {
                let (c, project, _, _, _) =
                    entries[&raw_uuid(&f, "id").ok_or(MaterializeError::InvalidRow)?];
                if canonical.get(&c) != Some(&project) {
                    return invalid();
                }
            }
            "public.memory_canonical_state" => {
                let id = raw_uuid(&f, "canonical_id").ok_or(MaterializeError::InvalidRow)?;
                if !entries.contains_key(&id) {
                    return invalid();
                }
            }
            "public.memory_tags" => {
                if !entries.contains_key(
                    &raw_uuid(&f, "memory_entry_id").ok_or(MaterializeError::InvalidRow)?,
                ) {
                    return invalid();
                }
            }
            "public.memory_relations" => {
                for k in ["src_canonical_id", "dst_canonical_id"] {
                    if !canonical
                        .contains_key(&raw_uuid(&f, k).ok_or(MaterializeError::InvalidRow)?)
                    {
                        return invalid();
                    }
                }
            }
            _ => {}
        }
    }
    for s in &p.projected_sources {
        let id = Uuid::parse_str(&s.binding.memory_version.record.key)
            .map_err(|_| MaterializeError::InvalidRow)?;
        let (canonical, project, hash, version, tombstone) =
            entries.get(&id).ok_or(MaterializeError::InvalidRow)?;
        if *canonical != s.binding.canonical_id
            || Some(*project) != s.row.source_project_id
            || *hash != s.binding.memory_version.row_sha256
            || *version != s.binding.version.version_no
            || *tombstone != s.binding.version.is_tombstone
        {
            return invalid();
        }
    }
    let calculated = digest_json(&p, b"mem-area-materialized-partition-v1\n")?;
    if calculated != w.partition_sha256 {
        return Err(MaterializeError::HashMismatch);
    }
    p.partition_sha256 = calculated;
    Ok(p)
}
