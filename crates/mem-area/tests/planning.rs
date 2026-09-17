// SPDX-License-Identifier: AGPL-3.0-or-later

use std::collections::BTreeSet;

use mem_area::*;
use uuid::Uuid;

fn id(value: u128) -> Uuid {
    Uuid::from_u128(value)
}

fn row(
    key: String,
    project: Option<Uuid>,
    canonical: Option<Uuid>,
    version: Option<VersionIdentity>,
) -> RowInventory {
    RowInventory {
        key,
        row_sha256: "a".repeat(64),
        source_project_id: project,
        canonical_id: canonical,
        version,
        references: vec![],
    }
}

fn table_mut<'a>(inventory: &'a mut Inventory, name: &str) -> &'a mut TableInventory {
    inventory
        .tables
        .iter_mut()
        .find(|table| table.table == name)
        .expect("fixture table")
}

fn push(inventory: &mut Inventory, name: &str, row: RowInventory) {
    let table = table_mut(inventory, name);
    table.rows.push(row);
    table.row_count = table.rows.len() as u64;
}

fn fixture() -> Inventory {
    let project = ProjectProvenance {
        id: id(1),
        slug: "ca-2".into(),
        name: "Original project".into(),
        root_path: "/synthetic/ca-2".into(),
        created_at: "2026-09-17T08:00:00Z".into(),
    };
    let mut inventory = Inventory {
        format_version: FORMAT_VERSION,
        source_instance: id(2),
        snapshot_id: id(3),
        captured_at: "2026-09-17T08:00:00Z".into(),
        migration_version: SUPPORTED_MIGRATION,
        projects: vec![project],
        tables: KNOWN_TABLES
            .iter()
            .map(|(name, _)| TableInventory {
                table: (*name).into(),
                columns: vec![Column {
                    name: "id".into(),
                    data_type: "uuid".into(),
                    nullable: false,
                }],
                foreign_keys: vec![],
                row_count: 0,
                rows: vec![],
            })
            .collect(),
    };
    push(
        &mut inventory,
        "public.projects",
        row(id(1).to_string(), None, None, None),
    );
    for (key, version_no, is_tombstone) in [(10, 1, false), (11, 2, false), (12, 3, true)] {
        push(
            &mut inventory,
            "public.memory_entries",
            row(
                id(key).to_string(),
                Some(id(1)),
                Some(id(10)),
                Some(VersionIdentity {
                    version_no,
                    is_tombstone,
                }),
            ),
        );
    }
    push(
        &mut inventory,
        "public.memory_canonical_state",
        row(id(10).to_string(), Some(id(1)), Some(id(10)), None),
    );
    inventory
}

fn review(classification: SourceClassification) -> ClassificationReview {
    ClassificationReview {
        classification,
        source_evidence: "fixture source reviewed".into(),
        project_evidence: "original project preserved".into(),
        content_evidence: "all version content reviewed".into(),
    }
}

fn reviewed(inventory: &Inventory) -> Manifest {
    let mut manifest = draft_manifest(inventory).expect("serialize fixture");
    for lineage in &mut manifest.lineages {
        lineage.disposition = Disposition::Area {
            area: AreaId::General,
            review: review(SourceClassification::Knowledge),
        };
    }
    manifest.projects.push(ProjectBinding {
        area: AreaId::General,
        source: inventory.projects[0].clone(),
        reason: "Preserve original project subdivision".into(),
    });
    manifest
}

fn assert_blocked(inventory: &Inventory, manifest: &Manifest, needle: &str) {
    let report = verify_manifest(inventory, manifest);
    assert!(!report.plan_valid, "expected blocker {needle}");
    assert!(!report.import_supported);
    assert!(
        report.records.is_empty(),
        "invalid plans cannot expose partial executable actions"
    );
    assert!(
        report
            .blockers
            .iter()
            .any(|message| message.contains(needle)),
        "missing {needle}: {:?}",
        report.blockers
    );
}

#[test]
fn complete_review_preserves_every_version_tombstone_hash_and_project() {
    let inventory = fixture();
    assert_eq!(verify_inventory(&inventory), Vec::<String>::new());
    let report = verify_manifest(&inventory, &reviewed(&inventory));
    assert!(report.plan_valid, "{:?}", report.blockers);
    assert!(!report.import_supported);
    assert_eq!(report.records.len(), 5);
    let versions: Vec<_> = report
        .records
        .iter()
        .filter(|record| record.record.table == "public.memory_entries")
        .collect();
    assert_eq!(versions.len(), 3);
    for version in versions {
        assert_eq!(version.destination, Destination::Area(AreaId::General));
        assert_eq!(version.source_project_id, Some(id(1)));
        assert_eq!(version.canonical_id, Some(id(10)));
        assert_eq!(version.source_sha256, "a".repeat(64));
    }
}

#[test]
fn draft_does_not_infer_certificate_authority_from_ca_project_name() {
    let inventory = fixture();
    let manifest = draft_manifest(&inventory).expect("fixture");
    assert_eq!(manifest.lineages[0].disposition, Disposition::Unresolved);
    assert!(manifest.projects.is_empty());
    assert_blocked(&inventory, &manifest, "unresolved ownership");
}

#[test]
fn duplicate_lineage_owners_and_version_overrides_are_rejected() {
    let inventory = fixture();
    let mut manifest = reviewed(&inventory);
    let mut competing = manifest.lineages[0].clone();
    competing.disposition = Disposition::Area {
        area: AreaId::Trust,
        review: review(SourceClassification::Knowledge),
    };
    manifest.lineages.push(competing);
    assert_blocked(&inventory, &manifest, "duplicate canonical ownership");
    manifest.lineages.pop();
    manifest.records.push(RecordDecision {
        record: RecordRef {
            table: "public.memory_entries".into(),
            key: id(12).to_string(),
        },
        disposition: Disposition::Area {
            area: AreaId::Trust,
            review: review(SourceClassification::Knowledge),
        },
    });
    assert_blocked(
        &inventory,
        &manifest,
        "cannot override lineage/runtime policy",
    );
}

#[test]
fn protected_source_classes_cannot_become_retrievable_knowledge() {
    for class in [
        SourceClassification::Test,
        SourceClassification::Evaluation,
        SourceClassification::Ambiguous,
        SourceClassification::Mixed,
    ] {
        let inventory = fixture();
        let mut manifest = reviewed(&inventory);
        manifest.lineages[0].disposition = Disposition::Area {
            area: AreaId::General,
            review: review(class),
        };
        assert_blocked(&inventory, &manifest, "requires protected archive");
        manifest.lineages[0].disposition = Disposition::ProtectedArchive {
            reason: "operator-only non-retrievable archive".into(),
            review: review(class),
        };
        manifest.projects.clear();
        let report = verify_manifest(&inventory, &manifest);
        assert!(report.plan_valid, "{:?}", report.blockers);
        assert!(
            report
                .records
                .iter()
                .all(|row| row.destination == Destination::ProtectedArchive)
        );
    }
}

#[test]
fn source_row_hash_schema_and_project_changes_invalidate_review() {
    let inventory = fixture();
    let manifest = reviewed(&inventory);
    let mut changed = inventory.clone();
    table_mut(&mut changed, "public.memory_entries").rows[0].row_sha256 = "b".repeat(64);
    assert_blocked(&changed, &manifest, "inventory digest mismatch");
    let mut changed = inventory.clone();
    table_mut(&mut changed, "public.memory_entries")
        .columns
        .push(Column {
            name: "new_private_data".into(),
            data_type: "text".into(),
            nullable: true,
        });
    assert_blocked(&changed, &manifest, "inventory digest mismatch");
    let mut changed = inventory.clone();
    changed.projects[0].root_path = "/changed/source".into();
    assert_blocked(
        &changed,
        &manifest,
        "changed or unjustified project provenance",
    );
}

#[test]
fn incomplete_counts_keys_versions_tables_and_hashes_block_inventory() {
    let mut inventory = fixture();
    table_mut(&mut inventory, "public.memory_entries").row_count += 1;
    assert!(
        verify_inventory(&inventory)
            .iter()
            .any(|error| error.contains("row count mismatch"))
    );
    let mut inventory = fixture();
    let duplicate = table_mut(&mut inventory, "public.memory_entries").rows[0].clone();
    push(&mut inventory, "public.memory_entries", duplicate);
    let errors = verify_inventory(&inventory);
    assert!(
        errors
            .iter()
            .any(|error| error.contains("duplicate row key"))
    );
    assert!(
        errors
            .iter()
            .any(|error| error.contains("duplicate canonical version"))
    );
    let mut inventory = fixture();
    inventory
        .tables
        .retain(|table| table.table != "public.auth_service_tokens");
    assert!(
        verify_inventory(&inventory)
            .iter()
            .any(|error| error.contains("missing table inventory public.auth_service_tokens"))
    );
    let mut inventory = fixture();
    table_mut(&mut inventory, "public.memory_entries").rows[0].row_sha256 = "bad".into();
    assert!(
        verify_inventory(&inventory)
            .iter()
            .any(|error| error.contains("invalid complete-row SHA-256"))
    );
}

#[test]
fn unknown_tables_cannot_be_manually_whitelisted() {
    let mut inventory = fixture();
    let mut table = table_mut(&mut inventory, "public.auth_service_tokens").clone();
    table.table = "public.auth_new_secrets".into();
    inventory.tables.push(table);
    let mut manifest = reviewed(&inventory);
    let policy = manifest
        .table_policies
        .iter_mut()
        .find(|policy| policy.table == "public.auth_new_secrets")
        .expect("unknown table draft policy");
    assert!(policy.strategy.is_none());
    policy.strategy = Some(TableStrategy::HistoricalArchive);
    assert_blocked(
        &inventory,
        &manifest,
        "unclassified table public.auth_new_secrets",
    );
}

const HISTORICAL_SCHEMA: &str = "mab_full_20260516_150014";
const HISTORICAL_NAMES: &[&str] = &[
    "_sqlx_migrations",
    "curation_runs",
    "memory_chunks",
    "memory_entries",
    "memory_relations",
    "memory_sources",
    "memory_tags",
    "projects",
    "raw_captures",
    "sessions",
    "tasks",
];

fn historical_fixture() -> Inventory {
    let mut inventory = fixture();
    for name in HISTORICAL_NAMES {
        let key = if *name == "projects" {
            id(501)
        } else {
            id(502)
        };
        inventory.tables.push(TableInventory {
            table: format!("{HISTORICAL_SCHEMA}.{name}"),
            columns: vec![Column {
                name: "id".into(),
                data_type: "uuid".into(),
                nullable: false,
            }],
            foreign_keys: vec![],
            row_count: 1,
            rows: vec![row(
                key.to_string(),
                (*name != "projects").then_some(id(501)),
                None,
                None,
            )],
        });
    }
    inventory
}

#[test]
fn exact_historical_catalog_is_optional_and_archives_every_row() {
    assert!(
        verify_inventory(&fixture()).is_empty(),
        "historical schema is optional"
    );
    let inventory = historical_fixture();
    assert_eq!(verify_inventory(&inventory), Vec::<String>::new());
    let manifest = reviewed(&inventory);
    assert_eq!(
        manifest.lineages.len(),
        1,
        "historical rows create no public lineage decisions"
    );
    assert!(
        manifest.records.is_empty(),
        "historical tables need no area ownership reviews"
    );
    let report = verify_manifest(&inventory, &manifest);
    assert!(report.plan_valid, "{:?}", report.blockers);
    let archived: Vec<_> = report
        .records
        .iter()
        .filter(|record| record.record.table.starts_with(HISTORICAL_SCHEMA))
        .collect();
    assert_eq!(archived.len(), 11);
    assert!(
        archived
            .iter()
            .all(|record| record.destination == Destination::ProtectedArchive)
    );
    for name in HISTORICAL_NAMES {
        assert_eq!(
            table_strategy(&format!("{HISTORICAL_SCHEMA}.{name}")),
            Some(TableStrategy::HistoricalArchive)
        );
    }
}

#[test]
fn partial_historical_catalog_and_new_tables_or_schemas_stay_blocked() {
    let mut inventory = historical_fixture();
    inventory
        .tables
        .retain(|table| table.table != format!("{HISTORICAL_SCHEMA}.tasks"));
    assert_blocked(
        &inventory,
        &reviewed(&inventory),
        "missing historical table inventory mab_full_20260516_150014.tasks",
    );
    for unknown in [
        "mab_full_20260516_150014.new_table",
        "mab_full_20260516_150015.memory_entries",
        "mab_full_20260516_150014_extra.projects",
    ] {
        let mut inventory = historical_fixture();
        let mut table = inventory.tables.last().expect("table").clone();
        table.table = unknown.into();
        inventory.tables.push(table);
        assert_eq!(table_strategy(unknown), None);
        assert_blocked(
            &inventory,
            &reviewed(&inventory),
            &format!("unclassified table {unknown}"),
        );
    }
}

#[test]
fn historical_project_provenance_is_schema_local() {
    let mut inventory = historical_fixture();
    assert!(
        !inventory
            .projects
            .iter()
            .any(|project| project.id == id(501))
    );
    assert!(
        verify_inventory(&inventory).is_empty(),
        "historical-only UUID is valid locally"
    );
    table_mut(
        &mut inventory,
        &format!("{HISTORICAL_SCHEMA}.memory_entries"),
    )
    .rows[0]
        .source_project_id = Some(id(1));
    let validation = inspect_inventory(&inventory);
    assert!(validation.blockers.is_empty());
    assert_eq!(validation.notices.len(), 1);
    assert_eq!(validation.notices[0].source_project_id, id(1));
    assert_eq!(
        validation.notices[0].project_table,
        format!("{HISTORICAL_SCHEMA}.projects")
    );
    // A public UUID cannot substitute for an absent historical source project;
    // its original unresolved local reference remains visible in the archive.
    assert!(inventory.projects.iter().any(|project| project.id == id(1)));
    let mut invalid = historical_fixture();
    table_mut(&mut invalid, &format!("{HISTORICAL_SCHEMA}.projects")).rows[0].key =
        "not-a-uuid".into();
    assert_blocked(
        &invalid,
        &reviewed(&invalid),
        "invalid historical project identity",
    );
}

#[test]
fn historical_foreign_keys_remain_accounted_and_cannot_be_promoted() {
    let mut inventory = historical_fixture();
    let entries = table_mut(
        &mut inventory,
        &format!("{HISTORICAL_SCHEMA}.memory_entries"),
    );
    entries.columns.push(Column {
        name: "project_id".into(),
        data_type: "uuid".into(),
        nullable: false,
    });
    entries.foreign_keys.push(ForeignKey {
        name: "historical_project_fk".into(),
        columns: vec!["project_id".into()],
        target_table: format!("{HISTORICAL_SCHEMA}.projects"),
        target_columns: vec!["id".into()],
    });
    entries.rows[0].references.push(RowReference {
        constraint: "historical_project_fk".into(),
        target: Some(RecordRef {
            table: format!("{HISTORICAL_SCHEMA}.projects"),
            key: id(501).to_string(),
        }),
    });
    let manifest = reviewed(&inventory);
    assert!(verify_manifest(&inventory, &manifest).plan_valid);
    let mut changed = manifest.clone();
    changed
        .table_policies
        .iter_mut()
        .find(|policy| policy.table == format!("{HISTORICAL_SCHEMA}.memory_entries"))
        .expect("historical policy")
        .strategy = Some(TableStrategy::Owned);
    assert_blocked(&inventory, &changed, "invalid or unclassified table policy");
    let mut overridden = manifest.clone();
    overridden.records.push(RecordDecision {
        record: RecordRef {
            table: format!("{HISTORICAL_SCHEMA}.memory_entries"),
            key: id(502).to_string(),
        },
        disposition: Disposition::Area {
            area: AreaId::General,
            review: review(SourceClassification::Knowledge),
        },
    });
    assert_blocked(&inventory, &overridden, "extraneous record decision");
    table_mut(
        &mut inventory,
        &format!("{HISTORICAL_SCHEMA}.memory_entries"),
    )
    .rows[0]
        .references[0]
        .target
        .as_mut()
        .expect("target")
        .key = id(999).to_string();
    assert_blocked(&inventory, &reviewed(&inventory), "dangling reference");
}

#[test]
fn streamed_inventory_digest_preserves_format_one_bytes() {
    use sha2::{Digest, Sha256};
    let mut inventory = historical_fixture();
    inventory.projects[0].name = "Name with unicode £ and \"quoted\" text\n".into();
    inventory.tables.reverse();
    let entries = table_mut(&mut inventory, "public.memory_entries");
    entries.rows.reverse();
    entries.columns.push(Column {
        name: "a_first".into(),
        data_type: "text".into(),
        nullable: true,
    });
    entries.foreign_keys.push(ForeignKey {
        name: "z_last".into(),
        columns: vec!["id".into(), "a_first".into()],
        target_table: "public.projects".into(),
        target_columns: vec!["id".into(), "slug".into()],
    });
    entries.foreign_keys.push(ForeignKey {
        name: "a_first".into(),
        columns: vec!["id".into()],
        target_table: "public.projects".into(),
        target_columns: vec!["id".into()],
    });
    entries.rows[0].references = vec![
        RowReference {
            constraint: "z_last".into(),
            target: None,
        },
        RowReference {
            constraint: "a_first".into(),
            target: Some(RecordRef {
                table: "public.projects".into(),
                key: id(1).to_string(),
            }),
        },
    ];
    // Reference implementation is the original format-v1 normalization. Keep
    // this comparison so reducing allocations never invalidates reviewed hashes.
    let mut normalized = inventory.clone();
    normalized.projects.sort_by_key(|project| project.id);
    normalized.tables.sort_by(|a, b| a.table.cmp(&b.table));
    for table in &mut normalized.tables {
        table.columns.sort_by(|a, b| a.name.cmp(&b.name));
        table.foreign_keys.sort_by(|a, b| a.name.cmp(&b.name));
        table.rows.sort_by(|a, b| a.key.cmp(&b.key));
        for row in &mut table.rows {
            row.references
                .sort_by(|a, b| a.constraint.cmp(&b.constraint));
        }
    }
    let legacy = format!(
        "{:x}",
        Sha256::digest(serde_json::to_vec(&normalized).expect("legacy serialization"))
    );
    assert_eq!(
        inventory_digest(&inventory).expect("streamed digest"),
        legacy
    );
}

#[test]
fn tokens_jobs_and_federation_authority_stay_retired() {
    let mut inventory = fixture();
    for table in [
        "public.auth_service_tokens",
        "public.background_jobs",
        "public.federation_local_grants",
    ] {
        push(
            &mut inventory,
            table,
            row("synthetic-runtime".into(), None, None, None),
        );
    }
    let mut manifest = reviewed(&inventory);
    let report = verify_manifest(&inventory, &manifest);
    assert!(report.plan_valid, "{:?}", report.blockers);
    assert_eq!(
        report
            .records
            .iter()
            .filter(|row| row.destination == Destination::ArchiveAndRetire)
            .count(),
        3
    );
    manifest
        .table_policies
        .iter_mut()
        .find(|policy| policy.table == "public.background_jobs")
        .expect("policy")
        .strategy = Some(TableStrategy::Owned);
    assert_blocked(
        &inventory,
        &manifest,
        "invalid or unclassified table policy public.background_jobs",
    );
}

#[test]
fn cross_area_and_mixed_session_references_block_without_transformation() {
    let mut inventory = fixture();
    push(
        &mut inventory,
        "public.sessions",
        row(id(30).to_string(), Some(id(1)), None, None),
    );
    let mut task = row(id(31).to_string(), Some(id(1)), None, None);
    task.references.push(RowReference {
        constraint: "logical:session_id".into(),
        target: Some(RecordRef {
            table: "public.sessions".into(),
            key: id(30).to_string(),
        }),
    });
    push(&mut inventory, "public.tasks", task);
    let mut manifest = reviewed(&inventory);
    for decision in &mut manifest.records {
        decision.disposition = if decision.record.table == "public.sessions" {
            Disposition::ProtectedArchive {
                reason: "contains unrelated private material".into(),
                review: review(SourceClassification::Mixed),
            }
        } else {
            Disposition::Area {
                area: AreaId::General,
                review: review(SourceClassification::Knowledge),
            }
        };
    }
    assert_blocked(&inventory, &manifest, "cross-area/archive reference");
    for decision in &mut manifest.records {
        decision.disposition = Disposition::ProtectedArchive {
            reason: "retain mixed session and dependent task together".into(),
            review: review(SourceClassification::Mixed),
        };
    }
    assert!(verify_manifest(&inventory, &manifest).plan_valid);
}

#[test]
fn foreign_key_catalog_requires_every_row_reference_and_checks_targets() {
    let mut inventory = fixture();
    let entries = table_mut(&mut inventory, "public.memory_entries");
    entries.columns.push(Column {
        name: "project_id".into(),
        data_type: "uuid".into(),
        nullable: false,
    });
    entries.foreign_keys.push(ForeignKey {
        name: "memory_entries_project_id_fkey".into(),
        columns: vec!["project_id".into()],
        target_table: "public.projects".into(),
        target_columns: vec!["id".into()],
    });
    assert!(
        verify_inventory(&inventory)
            .iter()
            .any(|error| error.contains("missing FK accounting"))
    );
    for row in &mut table_mut(&mut inventory, "public.memory_entries").rows {
        row.references.push(RowReference {
            constraint: "memory_entries_project_id_fkey".into(),
            target: Some(RecordRef {
                table: "public.projects".into(),
                key: id(1).to_string(),
            }),
        });
    }
    assert!(verify_inventory(&inventory).is_empty());
    assert!(verify_manifest(&inventory, &reviewed(&inventory)).plan_valid);
    table_mut(&mut inventory, "public.memory_entries").rows[0].references[0].target = None;
    assert!(
        verify_inventory(&inventory)
            .iter()
            .any(|error| error.contains("null non-nullable FK"))
    );
    table_mut(&mut inventory, "public.memory_entries").rows[0].references[0].target =
        Some(RecordRef {
            table: "public.projects".into(),
            key: id(999).to_string(),
        });
    assert!(
        verify_inventory(&inventory)
            .iter()
            .any(|error| error.contains("dangling reference"))
    );
}

#[test]
fn row_order_does_not_change_digest_but_missing_history_does() {
    let mut inventory = fixture();
    let digest = inventory_digest(&inventory).expect("digest");
    inventory.tables.reverse();
    table_mut(&mut inventory, "public.memory_entries")
        .rows
        .reverse();
    assert_eq!(inventory_digest(&inventory).expect("digest"), digest);
    let manifest = reviewed(&inventory);
    let entries = table_mut(&mut inventory, "public.memory_entries");
    entries.rows.pop();
    entries.row_count -= 1;
    assert_blocked(&inventory, &manifest, "inventory digest mismatch");
}

#[test]
fn reviewed_schema_catalog_matches_every_current_repository_table() {
    let migrations = std::path::Path::new(env!("CARGO_MANIFEST_DIR")).join("../../migrations");
    let mut paths: Vec<_> = std::fs::read_dir(migrations)
        .expect("migrations directory")
        .map(|entry| entry.expect("migration entry").path())
        .collect();
    paths.sort();
    let mut active = BTreeSet::from(["public._sqlx_migrations".to_owned()]);
    for path in paths {
        if path.extension().is_none_or(|extension| extension != "sql") {
            continue;
        }
        for line in std::fs::read_to_string(path)
            .expect("migration source")
            .lines()
        {
            if let Some(rest) = line.strip_prefix("CREATE TABLE ") {
                let rest = rest.strip_prefix("IF NOT EXISTS ").unwrap_or(rest);
                let name = rest.split_whitespace().next().expect("created table name");
                active.insert(format!("public.{name}"));
            } else if let Some(rest) = line.strip_prefix("DROP TABLE ") {
                let name = rest.trim_end_matches(';');
                active.remove(&format!("public.{name}"));
            }
        }
    }
    let catalog: BTreeSet<_> = KNOWN_TABLES
        .iter()
        .chain(SUPERVISOR_TABLES_0053)
        .chain(SUPERVISOR_TABLES_0054)
        .map(|(name, _)| name.to_string())
        .collect();
    assert_eq!(
        catalog.len(),
        KNOWN_TABLES.len() + SUPERVISOR_TABLES_0053.len() + SUPERVISOR_TABLES_0054.len(),
        "duplicate table policy"
    );
    assert_eq!(
        catalog, active,
        "schema changes require explicit policy review"
    );
}

#[test]
fn migration_0053_is_exact_version_gated_and_cannot_reactivate_supervisor_state() {
    let baseline = fixture();
    assert_eq!(SUPPORTED_MIGRATION, 52);
    assert_eq!(KNOWN_TABLES.len(), 93);
    assert!(verify_inventory(&baseline).is_empty());
    let baseline_digest = inventory_digest(&baseline).expect("baseline digest");
    let baseline_manifest = serde_json::to_vec(&reviewed(&baseline)).expect("baseline manifest");
    assert_eq!(SUPERVISOR_TABLES_0053.len(), 10);
    let mut inventory = baseline.clone();
    for (table, _) in SUPERVISOR_TABLES_0053 {
        inventory.tables.push(TableInventory {
            table: (*table).into(),
            columns: vec![Column {
                name: "id".into(),
                data_type: "uuid".into(),
                nullable: false,
            }],
            foreign_keys: vec![],
            row_count: 1,
            rows: vec![row(id(99).to_string(), None, None, None)],
        });
    }
    let mixed = verify_inventory(&inventory);
    assert_eq!(
        mixed
            .iter()
            .filter(
                |error| error.starts_with("migration 0053 table present at migration 0052 head:")
            )
            .count(),
        10
    );
    inventory.migration_version = SUPERVISOR_MIGRATION;
    assert!(verify_inventory(&inventory).is_empty());
    let manifest = reviewed(&inventory);
    let report = verify_manifest(&inventory, &manifest);
    assert!(report.plan_valid, "{:?}", report.blockers);
    for (table, strategy) in SUPERVISOR_TABLES_0053 {
        let planned = report
            .records
            .iter()
            .find(|record| record.record.table == *table)
            .expect("retained supervisor record");
        assert_eq!(
            planned.destination,
            match strategy {
                TableStrategy::HistoricalArchive => Destination::ProtectedArchive,
                TableStrategy::RetireRuntime => Destination::ArchiveAndRetire,
                _ => panic!("supervisor tables cannot be active knowledge"),
            }
        );
    }
    for (missing, _) in SUPERVISOR_TABLES_0053 {
        let mut incomplete = inventory.clone();
        incomplete.tables.retain(|table| table.table != *missing);
        assert!(
            verify_inventory(&incomplete)
                .iter()
                .any(|error| error == &format!("missing migration 0053 table inventory {missing}"))
        );
    }
    let mut unknown = inventory.clone();
    let mut table = unknown.tables.last().expect("table").clone();
    table.table = "public.supervisor_future_authority".into();
    unknown.tables.push(table);
    assert!(
        verify_inventory(&unknown)
            .iter()
            .any(|error| error.contains("unclassified table public.supervisor_future_authority"))
    );
    for unsupported in [51, 55, 999] {
        let mut future = inventory.clone();
        future.migration_version = unsupported;
        assert!(
            verify_inventory(&future)
                .iter()
                .any(|error| error.starts_with("unsupported migration version"))
        );
    }
    inventory.tables.retain(|table| {
        !SUPERVISOR_TABLES_0053
            .iter()
            .any(|(name, _)| *name == table.table)
    });
    inventory.migration_version = SUPPORTED_MIGRATION;
    assert_eq!(inventory, baseline);
    assert_eq!(
        inventory_digest(&inventory).expect("same digest"),
        baseline_digest
    );
    assert_eq!(
        serde_json::to_vec(&reviewed(&inventory)).expect("same manifest"),
        baseline_manifest
    );
}

#[test]
fn migration_0054_requires_exact_additive_catalog_and_archives_private_proposals() {
    assert_eq!(SUPERVISOR_TABLES_0054.len(), 1);
    assert_eq!(SUPERVISOR_PROPOSAL_MIGRATION, 54);
    let baseline = fixture();
    let mut inventory = baseline.clone();
    for (table, _) in SUPERVISOR_TABLES_0053.iter().chain(SUPERVISOR_TABLES_0054) {
        inventory.tables.push(TableInventory {
            table: (*table).into(),
            columns: vec![Column {
                name: "id".into(),
                data_type: "uuid".into(),
                nullable: false,
            }],
            foreign_keys: vec![],
            row_count: 1,
            rows: vec![row(id(990).to_string(), None, None, None)],
        });
    }
    for old_head in [SUPPORTED_MIGRATION, SUPERVISOR_MIGRATION] {
        inventory.migration_version = old_head;
        assert!(
            verify_inventory(&inventory)
                .iter()
                .any(|error| error.contains("migration 0054 table present before"))
        );
    }
    inventory.migration_version = SUPERVISOR_PROPOSAL_MIGRATION;
    assert!(verify_inventory(&inventory).is_empty());
    let manifest = reviewed(&inventory);
    let report = verify_manifest(&inventory, &manifest);
    assert!(report.plan_valid, "{:?}", report.blockers);
    assert_eq!(
        report
            .records
            .iter()
            .find(|record| record.record.table == "public.supervisor_proposals")
            .expect("synthetic fixture")
            .destination,
        Destination::ProtectedArchive
    );
    for (name, _) in SUPERVISOR_TABLES_0053.iter().chain(SUPERVISOR_TABLES_0054) {
        let mut missing = inventory.clone();
        missing.tables.retain(|table| table.table != *name);
        assert!(
            verify_inventory(&missing)
                .iter()
                .any(|error| error.starts_with("missing migration"))
        );
    }
    let mut forbidden = manifest.clone();
    forbidden
        .table_policies
        .iter_mut()
        .find(|policy| policy.table == "public.supervisor_proposals")
        .expect("synthetic fixture")
        .strategy = Some(TableStrategy::Owned);
    assert_blocked(
        &inventory,
        &forbidden,
        "invalid or unclassified table policy",
    );
    forbidden = manifest;
    forbidden.records.push(RecordDecision {
        record: RecordRef {
            table: "public.supervisor_proposals".into(),
            key: id(990).to_string(),
        },
        disposition: Disposition::Area {
            area: AreaId::Workshop,
            review: review(SourceClassification::Knowledge),
        },
    });
    assert_blocked(
        &inventory,
        &forbidden,
        "cannot override lineage/runtime policy",
    );
    let mut unknown = inventory.clone();
    let mut extra = unknown.tables.last().expect("synthetic fixture").clone();
    extra.table = "public.supervisor_proposals_future".into();
    unknown.tables.push(extra);
    assert!(
        verify_inventory(&unknown)
            .iter()
            .any(|error| error.contains("unclassified table public.supervisor_proposals_future"))
    );
    assert!(verify_inventory(&baseline).is_empty());
}

#[test]
fn serde_rejects_unknown_fields_and_stable_area_ids_round_trip() {
    for (area, wire) in [
        (AreaId::Trust, "trust"),
        (AreaId::Homelab, "homelab"),
        (AreaId::Workshop, "workshop"),
        (AreaId::General, "general"),
    ] {
        assert_eq!(
            serde_json::to_string(&area).expect("serialize"),
            format!("\"{wire}\"")
        );
    }
    let mut value = serde_json::to_value(fixture()).expect("fixture JSON");
    value["ignored_private_table"] = serde_json::json!({});
    assert!(serde_json::from_value::<Inventory>(value).is_err());
}

#[test]
fn cli_only_emits_reviewable_offline_plans_and_reports_blockers_with_exit_two() {
    let temp = std::env::temp_dir().join(format!("mem-area-test-{}", Uuid::new_v4()));
    std::fs::create_dir(&temp).expect("temporary fixture directory");
    let inventory = fixture();
    let inventory_path = temp.join("inventory.json");
    let manifest_path = temp.join("manifest.json");
    std::fs::write(
        &inventory_path,
        serde_json::to_vec(&inventory).expect("inventory JSON"),
    )
    .expect("write synthetic fixture");
    let output = std::process::Command::new(env!("CARGO_BIN_EXE_memory-area"))
        .args(["inventory", "--input"])
        .arg(&inventory_path)
        .output()
        .expect("run inventory");
    assert!(output.status.success());
    let report: serde_json::Value =
        serde_json::from_slice(&output.stdout).expect("inventory output");
    assert_eq!(report["inventory_valid"], true);
    assert_eq!(report["import_supported"], false);
    let output = std::process::Command::new(env!("CARGO_BIN_EXE_memory-area"))
        .args(["plan", "--inventory"])
        .arg(&inventory_path)
        .output()
        .expect("run plan");
    assert!(output.status.success());
    let draft: Manifest = serde_json::from_slice(&output.stdout).expect("manifest output");
    assert_eq!(draft.lineages[0].disposition, Disposition::Unresolved);
    std::fs::write(&manifest_path, &output.stdout).expect("write draft");
    let output = std::process::Command::new(env!("CARGO_BIN_EXE_memory-area"))
        .args(["verify", "--inventory"])
        .arg(&inventory_path)
        .arg("--manifest")
        .arg(&manifest_path)
        .output()
        .expect("run verify");
    assert_eq!(output.status.code(), Some(2));
    let report: serde_json::Value =
        serde_json::from_slice(&output.stdout).expect("verification output");
    assert_eq!(report["plan_valid"], false);
    assert_eq!(report["import_supported"], false);
    assert_eq!(report["records"], serde_json::json!([]));
    std::fs::remove_dir_all(&temp).expect("remove only synthetic fixture directory");
}
