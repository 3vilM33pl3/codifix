// SPDX-License-Identifier: AGPL-3.0-or-later

use std::collections::{BTreeMap, BTreeSet};

use sha2::{Digest, Sha256};
use uuid::Uuid;

use crate::policy::{HISTORICAL_PROJECT_TABLE, HISTORICAL_TABLES, historical_project_table};
use crate::*;

fn lineage_table(table: &str) -> bool {
    matches!(
        table,
        "public.memory_entries"
            | "public.memory_canonical_state"
            | "public.memory_sources"
            | "public.memory_source_checks"
            | "public.memory_tags"
    )
}

fn source_lineages(inventory: &Inventory) -> BTreeMap<Uuid, BTreeSet<Uuid>> {
    let mut lineages = BTreeMap::<Uuid, BTreeSet<Uuid>>::new();
    for table in &inventory.tables {
        if matches!(
            table.table.as_str(),
            "public.memory_entries" | "public.memory_canonical_state"
        ) {
            for row in &table.rows {
                if let Some(id) = row.canonical_id {
                    let projects = lineages.entry(id).or_default();
                    projects.extend(row.source_project_id);
                }
            }
        }
    }
    lineages
}

/// A stable digest of the complete supplied evidence. Vector ordering is
/// normalized except FK column order, which carries SQL semantics.
pub fn inventory_digest(inventory: &Inventory) -> Result<String, serde_json::Error> {
    use serde::ser::{SerializeSeq, SerializeStruct};

    // Preserve format-v1 field order and normalized bytes exactly, while holding
    // only sorted references rather than a cloned inventory plus full JSON buffer.
    struct NormalizedTable<'a>(&'a TableInventory);
    struct NormalizedRows<'a>(&'a [RowInventory]);
    impl serde::Serialize for NormalizedTable<'_> {
        fn serialize<S: serde::Serializer>(&self, serializer: S) -> Result<S::Ok, S::Error> {
            let table = self.0;
            let mut columns: Vec<_> = table.columns.iter().collect();
            columns.sort_by(|a, b| a.name.cmp(&b.name));
            let mut foreign_keys: Vec<_> = table.foreign_keys.iter().collect();
            foreign_keys.sort_by(|a, b| a.name.cmp(&b.name));
            let mut state = serializer.serialize_struct("TableInventory", 5)?;
            state.serialize_field("table", &table.table)?;
            state.serialize_field("columns", &columns)?;
            state.serialize_field("foreign_keys", &foreign_keys)?;
            state.serialize_field("row_count", &table.row_count)?;
            state.serialize_field("rows", &NormalizedRows(&table.rows))?;
            state.end()
        }
    }
    impl serde::Serialize for NormalizedRows<'_> {
        fn serialize<S: serde::Serializer>(&self, serializer: S) -> Result<S::Ok, S::Error> {
            #[derive(serde::Serialize)]
            struct NormalizedRow<'a> {
                key: &'a str,
                row_sha256: &'a str,
                source_project_id: Option<Uuid>,
                canonical_id: Option<Uuid>,
                version: &'a Option<VersionIdentity>,
                references: Vec<&'a RowReference>,
            }
            let mut rows: Vec<_> = self.0.iter().collect();
            rows.sort_by(|a, b| a.key.cmp(&b.key));
            let mut sequence = serializer.serialize_seq(Some(rows.len()))?;
            for row in rows {
                let mut references: Vec<_> = row.references.iter().collect();
                references.sort_by(|a, b| a.constraint.cmp(&b.constraint));
                sequence.serialize_element(&NormalizedRow {
                    key: &row.key,
                    row_sha256: &row.row_sha256,
                    source_project_id: row.source_project_id,
                    canonical_id: row.canonical_id,
                    version: &row.version,
                    references,
                })?;
            }
            sequence.end()
        }
    }
    #[derive(serde::Serialize)]
    struct NormalizedInventory<'a> {
        format_version: u32,
        source_instance: Uuid,
        snapshot_id: Uuid,
        captured_at: &'a str,
        migration_version: u32,
        projects: Vec<&'a ProjectProvenance>,
        tables: Vec<NormalizedTable<'a>>,
    }
    struct HashWriter(Sha256);
    impl std::io::Write for HashWriter {
        fn write(&mut self, bytes: &[u8]) -> std::io::Result<usize> {
            self.0.update(bytes);
            Ok(bytes.len())
        }

        fn flush(&mut self) -> std::io::Result<()> {
            Ok(())
        }
    }
    let mut projects: Vec<_> = inventory.projects.iter().collect();
    projects.sort_by_key(|project| project.id);
    let mut tables: Vec<_> = inventory.tables.iter().map(NormalizedTable).collect();
    tables.sort_by(|a, b| a.0.table.cmp(&b.0.table));
    let normalized = NormalizedInventory {
        format_version: inventory.format_version,
        source_instance: inventory.source_instance,
        snapshot_id: inventory.snapshot_id,
        captured_at: &inventory.captured_at,
        migration_version: inventory.migration_version,
        projects,
        tables,
    };
    let mut writer = HashWriter(Sha256::new());
    serde_json::to_writer(&mut writer, &normalized)?;
    Ok(format!("{:x}", writer.0.finalize()))
}

/// Check structural completeness against the supported table catalog. This
/// does not replace a trusted snapshot collector or independently counted DB.
pub fn verify_inventory(inventory: &Inventory) -> Vec<String> {
    inspect_inventory(inventory).blockers
}

/// Retention notices preserve unresolved historical metadata without treating
/// it as live ownership. Catalog FK and explicit logical references stay strict.
pub fn inspect_inventory(inventory: &Inventory) -> InventoryValidation {
    let mut errors = Vec::new();
    let mut notices = Vec::new();
    if inventory.format_version != FORMAT_VERSION {
        errors.push("unsupported inventory format version".into());
    }
    if !matches!(
        inventory.migration_version,
        SUPPORTED_MIGRATION | SUPERVISOR_MIGRATION | SUPERVISOR_PROPOSAL_MIGRATION
    ) {
        errors.push(format!(
            "unsupported migration version {}; expected {SUPPORTED_MIGRATION}, {SUPERVISOR_MIGRATION} or {SUPERVISOR_PROPOSAL_MIGRATION}",
            inventory.migration_version
        ));
    }
    if inventory.source_instance.is_nil()
        || inventory.snapshot_id.is_nil()
        || inventory.captured_at.trim().is_empty()
    {
        errors.push("source instance, snapshot identity and capture time are required".into());
    }
    let mut projects = BTreeMap::new();
    let mut slugs = BTreeSet::new();
    for project in &inventory.projects {
        if project.id.is_nil()
            || project.slug.trim().is_empty()
            || project.name.trim().is_empty()
            || project.created_at.trim().is_empty()
        {
            errors.push(format!("incomplete project provenance {}", project.id));
        }
        if projects.insert(project.id, project).is_some() || !slugs.insert(&project.slug) {
            errors.push(format!("duplicate project identity or slug {}", project.id));
        }
    }
    let mut tables = BTreeMap::new();
    let mut rows = BTreeMap::new();
    let mut historical_projects = BTreeSet::new();
    for table in &inventory.tables {
        if table.table == HISTORICAL_PROJECT_TABLE {
            for row in &table.rows {
                match Uuid::parse_str(&row.key) {
                    Ok(id) if !id.is_nil() => {
                        historical_projects.insert(id);
                    }
                    _ => errors.push(format!(
                        "invalid historical project identity {}:{}",
                        table.table, row.key
                    )),
                }
            }
        }
    }
    let mut versions = BTreeSet::new();
    let lineages = source_lineages(inventory);
    let mut state_ids = BTreeSet::new();
    for table in &inventory.tables {
        let strategy = table_strategy(&table.table);
        if tables.insert(table.table.as_str(), table).is_some() {
            errors.push(format!("duplicate table {}", table.table));
        }
        if strategy.is_none() {
            errors.push(format!("unclassified table {}", table.table));
        }
        if table.row_count != table.rows.len() as u64 {
            errors.push(format!("row count mismatch for {}", table.table));
        }
        let columns: BTreeSet<_> = table
            .columns
            .iter()
            .map(|column| column.name.as_str())
            .collect();
        if table.columns.is_empty()
            || columns.len() != table.columns.len()
            || table
                .columns
                .iter()
                .any(|column| column.name.is_empty() || column.data_type.is_empty())
        {
            errors.push(format!(
                "missing, duplicate or incomplete column inventory for {}",
                table.table
            ));
        }
        let mut constraints = BTreeSet::new();
        for foreign_key in &table.foreign_keys {
            if foreign_key.name.trim().is_empty()
                || foreign_key.name.starts_with("logical:")
                || !constraints.insert(foreign_key.name.as_str())
                || foreign_key.columns.is_empty()
                || foreign_key.columns.len() != foreign_key.target_columns.len()
                || foreign_key
                    .columns
                    .iter()
                    .any(|column| !columns.contains(column.as_str()))
            {
                errors.push(format!(
                    "invalid foreign key {}.{}",
                    table.table, foreign_key.name
                ));
            }
        }
        for row in &table.rows {
            let record = (table.table.as_str(), row.key.as_str());
            if row.key.trim().is_empty() || rows.insert(record, row).is_some() {
                errors.push(format!(
                    "missing or duplicate row key {}:{}",
                    table.table, row.key
                ));
            }
            if row.row_sha256.len() != 64
                || !row
                    .row_sha256
                    .bytes()
                    .all(|c| c.is_ascii_digit() || (b'a'..=b'f').contains(&c))
            {
                errors.push(format!(
                    "invalid complete-row SHA-256 {}:{}",
                    table.table, row.key
                ));
            }
            if let Some(project) = row.source_project_id {
                let known = if historical_project_table(&table.table).is_some() {
                    historical_projects.contains(&project)
                } else {
                    projects.contains_key(&project)
                };
                if !known {
                    if let Some(notice) = retained_project_notice(&table.table, &row.key, project) {
                        notices.push(notice);
                    } else {
                        errors.push(format!(
                            "unknown source project {project} for {}:{}",
                            table.table, row.key
                        ));
                    }
                }
            }
            if lineage_table(&table.table) {
                if row.canonical_id.is_none() || row.source_project_id.is_none() {
                    errors.push(format!(
                        "missing lineage/project metadata {}:{}",
                        table.table, row.key
                    ));
                }
                if row
                    .canonical_id
                    .is_some_and(|id| id.is_nil() || !lineages.contains_key(&id))
                {
                    errors.push(format!(
                        "orphaned canonical lineage {}:{}",
                        table.table, row.key
                    ));
                }
            } else if strategy == Some(TableStrategy::Owned) && row.canonical_id.is_some() {
                errors.push(format!(
                    "supporting record needs its own review {}:{}",
                    table.table, row.key
                ));
            }
            if table.table == "public.memory_entries" {
                if Uuid::parse_str(&row.key).is_err() {
                    errors.push(format!("memory version key must preserve UUID {}", row.key));
                }
                match &row.version {
                    Some(version) if version.version_no > 0 => {
                        if !versions.insert((row.canonical_id, version.version_no)) {
                            errors.push(format!(
                                "duplicate canonical version {}:{}",
                                table.table, row.key
                            ));
                        }
                    }
                    _ => errors.push(format!("missing or invalid memory version {}", row.key)),
                }
            } else if row.version.is_some() {
                errors.push(format!(
                    "version metadata on non-version row {}:{}",
                    table.table, row.key
                ));
            }
            if table.table == "public.memory_canonical_state"
                && (row.canonical_id.map(|id| id.to_string()) != Some(row.key.clone())
                    || !state_ids.insert(row.canonical_id))
            {
                errors.push(format!("invalid canonical state identity {}", row.key));
            }
            let mut references = BTreeSet::new();
            for reference in &row.references {
                if !references.insert(reference.constraint.as_str()) {
                    errors.push(format!(
                        "duplicate reference {}:{}:{}",
                        table.table, row.key, reference.constraint
                    ));
                }
                if !constraints.contains(reference.constraint.as_str())
                    && !reference.constraint.starts_with("logical:")
                {
                    errors.push(format!(
                        "unknown FK reference {}:{}:{}",
                        table.table, row.key, reference.constraint
                    ));
                }
            }
            for foreign_key in &table.foreign_keys {
                match row
                    .references
                    .iter()
                    .find(|reference| reference.constraint == foreign_key.name)
                {
                    None => errors.push(format!(
                        "missing FK accounting {}:{}:{}",
                        table.table, row.key, foreign_key.name
                    )),
                    Some(reference) => {
                        if let Some(target) = &reference.target {
                            if target.table != foreign_key.target_table {
                                errors.push(format!(
                                    "wrong FK target {}:{}:{}",
                                    table.table, row.key, foreign_key.name
                                ));
                            }
                        } else if !table.columns.iter().any(|column| {
                            foreign_key.columns.contains(&column.name) && column.nullable
                        }) {
                            errors.push(format!(
                                "null non-nullable FK {}:{}:{}",
                                table.table, row.key, foreign_key.name
                            ));
                        }
                    }
                }
            }
            for (constraint, target_table, nullable) in required_logical_refs(&table.table) {
                match row
                    .references
                    .iter()
                    .find(|reference| reference.constraint == *constraint)
                {
                    Some(reference)
                        if reference
                            .target
                            .as_ref()
                            .is_some_and(|target| target.table == *target_table)
                            || (*nullable && reference.target.is_none()) => {}
                    _ => errors.push(format!(
                        "missing/invalid logical reference {}:{}:{constraint}",
                        table.table, row.key
                    )),
                }
            }
        }
    }
    for (table, _) in KNOWN_TABLES {
        if !tables.contains_key(table) {
            errors.push(format!("missing table inventory {table}"));
        }
    }
    for (table, _) in SUPERVISOR_TABLES_0053 {
        if matches!(
            inventory.migration_version,
            SUPERVISOR_MIGRATION | SUPERVISOR_PROPOSAL_MIGRATION
        ) && !tables.contains_key(table)
        {
            errors.push(format!("missing migration 0053 table inventory {table}"));
        } else if inventory.migration_version == SUPPORTED_MIGRATION && tables.contains_key(table) {
            errors.push(format!(
                "migration 0053 table present at migration 0052 head: {table}"
            ));
        }
    }
    for (table, _) in SUPERVISOR_TABLES_0054 {
        if inventory.migration_version == SUPERVISOR_PROPOSAL_MIGRATION
            && !tables.contains_key(table)
        {
            errors.push(format!("missing migration 0054 table inventory {table}"));
        } else if matches!(
            inventory.migration_version,
            SUPPORTED_MIGRATION | SUPERVISOR_MIGRATION
        ) && tables.contains_key(table)
        {
            errors.push(format!(
                "migration 0054 table present before migration 0054 head: {table}"
            ));
        }
    }
    if HISTORICAL_TABLES
        .iter()
        .any(|table| tables.contains_key(table))
    {
        for table in HISTORICAL_TABLES {
            if !tables.contains_key(table) {
                errors.push(format!("missing historical table inventory {table}"));
            }
        }
    }
    for ((table, key), row) in &rows {
        for reference in &row.references {
            if let Some(target) = &reference.target {
                match rows.get(&(target.table.as_str(), target.key.as_str())) {
                    None => errors.push(format!(
                        "dangling reference {}:{}:{}",
                        table, key, reference.constraint
                    )),
                    Some(target_row)
                        if lineage_table(table)
                            && lineage_table(&target.table)
                            && !reference.constraint.starts_with("logical:")
                            && (row.canonical_id != target_row.canonical_id
                                || row.source_project_id != target_row.source_project_id) =>
                    {
                        errors.push(format!(
                            "inconsistent lineage metadata {}:{}:{}",
                            table, key, reference.constraint
                        ));
                    }
                    _ => {}
                }
            }
        }
        if *table == "public.memory_entries" && !state_ids.contains(&row.canonical_id) {
            errors.push(format!("memory version missing canonical state {}", key));
        }
    }
    for table in &inventory.tables {
        for fk in &table.foreign_keys {
            if !tables.get(fk.target_table.as_str()).is_some_and(|target| {
                fk.target_columns
                    .iter()
                    .all(|name| target.columns.iter().any(|column| column.name == *name))
            }) {
                errors.push(format!("unresolved FK schema {}:{}", table.table, fk.name));
            }
        }
    }
    for (id, project) in &projects {
        let project_key = id.to_string();
        if !rows.contains_key(&("public.projects", project_key.as_str())) {
            errors.push(format!(
                "project provenance missing source row {} ({})",
                id, project.slug
            ));
        }
    }
    if let Some(table) = tables.get("public.projects") {
        for row in &table.rows {
            if !Uuid::parse_str(&row.key)
                .ok()
                .is_some_and(|id| projects.contains_key(&id))
            {
                errors.push(format!("project source row missing provenance {}", row.key));
            }
        }
    }
    notices.sort_by(|a, b| a.record.cmp(&b.record));
    InventoryValidation {
        blockers: errors,
        notices,
    }
}

fn retained_project_notice(table: &str, key: &str, project: Uuid) -> Option<InventoryNotice> {
    let strategy = table_strategy(table)?;
    if !matches!(
        strategy,
        TableStrategy::HistoricalArchive | TableStrategy::RetireRuntime
    ) {
        return None;
    }
    Some(InventoryNotice {
        kind: InventoryNoticeKind::RetainedUnresolvedProjectReference,
        record: RecordRef {
            table: table.into(),
            key: key.into(),
        },
        source_project_id: project,
        project_table: historical_project_table(table)
            .unwrap_or("public.projects")
            .into(),
        mandatory_strategy: strategy,
    })
}

fn retained_project_notices(inventory: &Inventory) -> Vec<InventoryNotice> {
    let public: BTreeSet<_> = inventory
        .projects
        .iter()
        .map(|project| project.id)
        .collect();
    let historical: BTreeSet<_> = inventory
        .tables
        .iter()
        .filter(|table| table.table == HISTORICAL_PROJECT_TABLE)
        .flat_map(|table| &table.rows)
        .filter_map(|row| Uuid::parse_str(&row.key).ok().filter(|id| !id.is_nil()))
        .collect();
    let mut notices = Vec::new();
    for table in &inventory.tables {
        if !matches!(
            table_strategy(&table.table),
            Some(TableStrategy::HistoricalArchive | TableStrategy::RetireRuntime)
        ) {
            continue;
        }
        let projects = if historical_project_table(&table.table).is_some() {
            &historical
        } else {
            &public
        };
        for row in &table.rows {
            if let Some(project) = row.source_project_id
                && !projects.contains(&project)
                && let Some(notice) = retained_project_notice(&table.table, &row.key, project)
            {
                notices.push(notice);
            }
        }
    }
    notices.sort_by(|a, b| a.record.cmp(&b.record));
    notices
}

fn required_logical_refs(table: &str) -> &'static [(&'static str, &'static str, bool)] {
    match table {
        "public.memory_relations" => &[
            (
                "logical:src_canonical_id",
                "public.memory_canonical_state",
                false,
            ),
            (
                "logical:dst_canonical_id",
                "public.memory_canonical_state",
                false,
            ),
        ],
        "public.memory_sources" => &[("logical:target_memory_id", "public.memory_entries", true)],
        _ => &[],
    }
}

/// Never infer a classification from a project name. Every potentially moved
/// knowledge row starts unresolved; operator reviews make decisions explicit.
pub fn draft_manifest(inventory: &Inventory) -> Result<Manifest, serde_json::Error> {
    let table_policies = inventory.tables.iter().map(|table| {
        let strategy = table_strategy(&table.table);
        TablePolicy {
            table: table.table.clone(),
            strategy,
            reason: match strategy {
                Some(TableStrategy::Owned) => "Explicit content/source/project review required; preserve full source rows".into(),
                Some(TableStrategy::ProjectProvenance) => "Preserve original project catalog and IDs; bind each target area explicitly".into(),
                Some(TableStrategy::RebuildDerived) => "Retain source rows in protected archive; rebuild in target without model calls during migration".into(),
                Some(TableStrategy::HistoricalArchive) => "Retain complete historical evidence in operator-only non-retrievable archive".into(),
                Some(TableStrategy::RetireRuntime) => "Retain evidence in protected archive; never reactivate source runtime or authority".into(),
                None => "BLOCKED: table has no reviewed migration strategy".into(),
            },
        }
    }).collect();
    let lineages = source_lineages(inventory)
        .into_iter()
        .map(|(canonical_id, projects)| LineageDecision {
            canonical_id,
            source_project_ids: projects.into_iter().collect(),
            disposition: Disposition::Unresolved,
        })
        .collect();
    let mut records = Vec::new();
    for table in &inventory.tables {
        if table_strategy(&table.table) == Some(TableStrategy::Owned)
            && !lineage_table(&table.table)
        {
            records.extend(table.rows.iter().map(|row| RecordDecision {
                record: RecordRef {
                    table: table.table.clone(),
                    key: row.key.clone(),
                },
                disposition: Disposition::Unresolved,
            }));
        }
    }
    Ok(Manifest {
        format_version: FORMAT_VERSION,
        source_instance: inventory.source_instance,
        snapshot_id: inventory.snapshot_id,
        inventory_sha256: inventory_digest(inventory)?,
        inventory_notices: retained_project_notices(inventory),
        areas: AREAS.to_vec(),
        table_policies,
        lineages,
        records,
        projects: Vec::new(),
    })
}

fn reviewed_destination(
    disposition: &Disposition,
    label: &str,
    errors: &mut Vec<String>,
) -> Option<Destination> {
    let (review, destination) = match disposition {
        Disposition::Unresolved => {
            errors.push(format!("unresolved ownership {label}"));
            return None;
        }
        Disposition::Area { area, review } => {
            if review.classification != SourceClassification::Knowledge {
                errors.push(format!(
                    "test/evaluation/ambiguous/mixed source requires protected archive: {label}"
                ));
            }
            (review, Destination::Area(*area))
        }
        Disposition::ProtectedArchive { reason, review } => {
            if reason.trim().is_empty() {
                errors.push(format!("archive reason required {label}"));
            }
            (review, Destination::ProtectedArchive)
        }
    };
    if review.source_evidence.trim().is_empty()
        || review.project_evidence.trim().is_empty()
        || review.content_evidence.trim().is_empty()
    {
        errors.push(format!(
            "source/project/content review evidence required {label}"
        ));
    }
    Some(destination)
}

/// Validate a reviewed partition against its pinned source inventory. A valid
/// plan is only an offline planning result, never permission to import or run.
pub fn verify_manifest(inventory: &Inventory, manifest: &Manifest) -> Verification {
    let validation = inspect_inventory(inventory);
    let mut errors = validation.blockers;
    if manifest.inventory_notices != validation.notices {
        errors.push("inventory notices mismatch; original historical project references must remain visible".into());
    }
    if manifest.format_version != FORMAT_VERSION
        || manifest.source_instance != inventory.source_instance
        || manifest.snapshot_id != inventory.snapshot_id
    {
        errors.push("manifest format/source/snapshot mismatch".into());
    }
    if inventory_digest(inventory).ok().as_ref() != Some(&manifest.inventory_sha256) {
        errors.push("inventory digest mismatch; source/schema/row/provenance changed".into());
    }
    if manifest.areas.len() != AREAS.len()
        || manifest.areas.iter().copied().collect::<BTreeSet<_>>() != AREAS.into_iter().collect()
    {
        errors.push("manifest must declare each of the four stable area IDs exactly once".into());
    }
    let mut policies = BTreeMap::new();
    let source_tables: BTreeSet<_> = inventory
        .tables
        .iter()
        .map(|table| table.table.as_str())
        .collect();
    for policy in &manifest.table_policies {
        if policies.insert(policy.table.as_str(), policy).is_some() {
            errors.push(format!("duplicate table policy {}", policy.table));
        }
        if !source_tables.contains(policy.table.as_str())
            || policy.strategy.is_none()
            || policy.strategy != table_strategy(&policy.table)
            || policy.reason.trim().is_empty()
        {
            errors.push(format!(
                "invalid or unclassified table policy {}",
                policy.table
            ));
        }
    }
    for table in &source_tables {
        if !policies.contains_key(table) {
            errors.push(format!("missing table policy {table}"));
        }
    }
    let source_lineages = source_lineages(inventory);
    let mut owners = BTreeMap::new();
    for lineage in &manifest.lineages {
        let destination = reviewed_destination(
            &lineage.disposition,
            &lineage.canonical_id.to_string(),
            &mut errors,
        );
        if owners.insert(lineage.canonical_id, destination).is_some() {
            errors.push(format!(
                "duplicate canonical ownership {}",
                lineage.canonical_id
            ));
        }
        let projects: BTreeSet<_> = lineage.source_project_ids.iter().copied().collect();
        if source_lineages.get(&lineage.canonical_id) != Some(&projects)
            || projects.len() != lineage.source_project_ids.len()
        {
            errors.push(format!(
                "lineage project provenance mismatch {}",
                lineage.canonical_id
            ));
        }
    }
    for id in source_lineages.keys() {
        if !owners.contains_key(id) {
            errors.push(format!("missing canonical ownership {id}"));
        }
    }
    let mut decisions = BTreeMap::new();
    for decision in &manifest.records {
        let destination = reviewed_destination(
            &decision.disposition,
            &format!("{}:{}", decision.record.table, decision.record.key),
            &mut errors,
        );
        if decisions.insert(&decision.record, destination).is_some() {
            errors.push(format!(
                "duplicate record decision {}:{}",
                decision.record.table, decision.record.key
            ));
        }
    }
    let mut bindings = BTreeMap::new();
    for binding in &manifest.projects {
        if binding.reason.trim().is_empty() || !inventory.projects.contains(&binding.source) {
            errors.push(format!(
                "changed or unjustified project provenance {}",
                binding.source.id
            ));
        }
        if bindings
            .insert((binding.area, binding.source.id), binding)
            .is_some()
        {
            errors.push(format!("duplicate project binding {}", binding.source.id));
        }
    }
    let mut actions = BTreeMap::new();
    let mut consumed_decisions = BTreeSet::new();
    let mut required_bindings = BTreeSet::new();
    for table in &inventory.tables {
        let strategy = table_strategy(&table.table);
        for row in &table.rows {
            let record = RecordRef {
                table: table.table.clone(),
                key: row.key.clone(),
            };
            let destination = match strategy {
                Some(TableStrategy::Owned) if lineage_table(&table.table) => row
                    .canonical_id
                    .and_then(|id| owners.get(&id))
                    .cloned()
                    .flatten(),
                Some(TableStrategy::Owned) => {
                    consumed_decisions.insert(record.clone());
                    match decisions.get(&record) {
                        Some(destination) => destination.clone(),
                        None => {
                            errors.push(format!(
                                "missing record decision {}:{}",
                                record.table, record.key
                            ));
                            None
                        }
                    }
                }
                Some(TableStrategy::ProjectProvenance | TableStrategy::HistoricalArchive) => {
                    Some(Destination::ProtectedArchive)
                }
                Some(TableStrategy::RebuildDerived) => Some(Destination::ArchiveAndRebuild),
                Some(TableStrategy::RetireRuntime) => Some(Destination::ArchiveAndRetire),
                None => None,
            };
            if let Some(destination) = destination {
                if let Destination::Area(area) = destination {
                    match row.source_project_id {
                        Some(project) => {
                            required_bindings.insert((area, project));
                        }
                        None => errors.push(format!(
                            "area record has no project provenance {}:{}",
                            record.table, record.key
                        )),
                    }
                }
                actions.insert(
                    record.clone(),
                    PlannedRecord {
                        record,
                        source_sha256: row.row_sha256.clone(),
                        source_project_id: row.source_project_id,
                        canonical_id: row.canonical_id,
                        destination,
                    },
                );
            }
        }
    }
    for record in decisions.keys() {
        if !consumed_decisions.contains(*record) {
            errors.push(format!(
                "extraneous record decision (cannot override lineage/runtime policy) {}:{}",
                record.table, record.key
            ));
        }
    }
    for binding in &required_bindings {
        if !bindings.contains_key(binding) {
            errors.push(format!(
                "missing area project binding {:?}:{}",
                binding.0, binding.1
            ));
        }
    }
    for binding in bindings.keys() {
        if !required_bindings.contains(binding) {
            errors.push(format!(
                "unused project binding {:?}:{}",
                binding.0, binding.1
            ));
        }
    }
    for table in &inventory.tables {
        for row in &table.rows {
            let record = RecordRef {
                table: table.table.clone(),
                key: row.key.clone(),
            };
            if let Some(PlannedRecord {
                destination: Destination::Area(area),
                ..
            }) = actions.get(&record)
            {
                for reference in &row.references {
                    if let Some(target) = &reference.target {
                        let target_action = actions.get(target);
                        let project_binding = target.table == "public.projects"
                            && Uuid::parse_str(&target.key)
                                .ok()
                                .is_some_and(|id| bindings.contains_key(&(*area, id)));
                        if !project_binding
                            && !target_action.is_some_and(|action| {
                                action.destination == Destination::Area(*area)
                            })
                        {
                            errors.push(format!("cross-area/archive reference requires reviewed transformation {}:{}:{}", table.table, row.key, reference.constraint));
                        }
                    }
                }
            }
        }
    }
    let plan_valid = errors.is_empty();
    Verification {
        plan_valid,
        import_supported: false,
        blockers: errors,
        notices: validation.notices,
        records: if plan_valid {
            actions.into_values().collect()
        } else {
            Vec::new()
        },
    }
}
