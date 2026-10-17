// SPDX-License-Identifier: AGPL-3.0-or-later

use codifix_area::*;
use uuid::Uuid;

fn id(value: u128) -> Uuid {
    Uuid::from_u128(value)
}

fn row() -> RowInventory {
    RowInventory {
        key: id(50).to_string(),
        row_sha256: "a".repeat(64),
        source_project_id: Some(id(999)),
        canonical_id: None,
        version: None,
        references: vec![],
    }
}

fn fixture() -> Inventory {
    Inventory {
        format_version: FORMAT_VERSION,
        source_instance: id(1),
        snapshot_id: id(2),
        captured_at: "2026-09-17T09:00:00Z".into(),
        migration_version: SUPPORTED_MIGRATION,
        projects: vec![],
        tables: KNOWN_TABLES
            .iter()
            .map(|(name, strategy)| {
                let historical = matches!(
                    strategy,
                    TableStrategy::HistoricalArchive | TableStrategy::RetireRuntime
                );
                TableInventory {
                    table: (*name).into(),
                    columns: vec![Column {
                        name: "id".into(),
                        data_type: "uuid".into(),
                        nullable: false,
                    }],
                    foreign_keys: vec![],
                    row_count: u64::from(historical),
                    rows: if historical { vec![row()] } else { vec![] },
                }
            })
            .collect(),
    }
}

fn assert_blocked(inventory: &Inventory, manifest: &Manifest, reason: &str) {
    let report = verify_manifest(inventory, manifest);
    assert!(!report.plan_valid);
    assert!(report.records.is_empty());
    assert!(
        report
            .blockers
            .iter()
            .any(|blocker| blocker.contains(reason)),
        "{:?}",
        report.blockers
    );
}

#[test]
fn retention_notices_preserve_original_ids_and_mandatory_destinations() {
    let mut inventory = fixture();
    inventory
        .tables
        .iter_mut()
        .find(|table| table.table == "public.memory_score_audit")
        .expect("table")
        .rows[0]
        .source_project_id = Some(Uuid::nil());
    let validation = inspect_inventory(&inventory);
    assert!(validation.blockers.is_empty(), "{:?}", validation.blockers);
    let expected = KNOWN_TABLES
        .iter()
        .filter(|(_, strategy)| {
            matches!(
                strategy,
                TableStrategy::HistoricalArchive | TableStrategy::RetireRuntime
            )
        })
        .count();
    assert_eq!(validation.notices.len(), expected);
    assert!(
        validation
            .notices
            .iter()
            .any(|notice| notice.source_project_id.is_nil())
    );
    let manifest = draft_manifest(&inventory).expect("draft");
    assert_eq!(manifest.inventory_notices, validation.notices);
    let report = verify_manifest(&inventory, &manifest);
    assert!(report.plan_valid, "{:?}", report.blockers);
    assert_eq!(report.notices, validation.notices);
    for notice in &report.notices {
        let record = report
            .records
            .iter()
            .find(|record| record.record == notice.record)
            .expect("retained record");
        assert_eq!(record.source_project_id, Some(notice.source_project_id));
        assert_eq!(record.source_sha256, "a".repeat(64));
        assert_eq!(
            record.destination,
            if notice.mandatory_strategy == TableStrategy::HistoricalArchive {
                Destination::ProtectedArchive
            } else {
                Destination::ArchiveAndRetire
            }
        );
    }
    let mut missing = manifest.clone();
    missing.inventory_notices.pop();
    assert_blocked(&inventory, &missing, "inventory notices mismatch");
    let mut altered = manifest.clone();
    altered.inventory_notices[0].source_project_id = id(1);
    assert_blocked(&inventory, &altered, "inventory notices mismatch");
    let mut omitted = serde_json::to_value(manifest).expect("serialize");
    omitted
        .as_object_mut()
        .expect("object")
        .remove("inventory_notices");
    let omitted: Manifest = serde_json::from_value(omitted).expect("legacy default");
    assert_blocked(&inventory, &omitted, "inventory notices mismatch");
}

#[test]
fn unknown_projects_still_block_owned_derived_provenance_and_unknown_tables() {
    for name in [
        "public.memory_entries",
        "public.memory_chunks",
        "public.projects",
        "public.unknown_table",
    ] {
        let mut inventory = fixture();
        if name == "public.unknown_table" {
            let mut table = inventory.tables[0].clone();
            table.table = name.into();
            inventory.tables.push(table);
        }
        let table = inventory
            .tables
            .iter_mut()
            .find(|table| table.table == name)
            .expect("table");
        table.rows = vec![row()];
        table.row_count = 1;
        let validation = inspect_inventory(&inventory);
        assert!(
            validation
                .blockers
                .iter()
                .any(|blocker| blocker.starts_with("unknown source project")
                    && blocker.contains(name)),
            "{name}"
        );
        assert!(
            !validation
                .notices
                .iter()
                .any(|notice| notice.record.table == name),
            "{name} cannot downgrade active references"
        );
    }
}

#[test]
fn archived_notices_do_not_downgrade_declared_fk_or_logical_targets() {
    for logical in [false, true] {
        let mut inventory = fixture();
        let table = inventory
            .tables
            .iter_mut()
            .find(|table| table.table == "public.memory_access_events")
            .expect("table");
        if logical {
            table.rows[0].canonical_id = Some(id(888));
            table.rows[0].references.push(RowReference {
                constraint: "logical:active_canonical".into(),
                target: Some(RecordRef {
                    table: "public.memory_canonical_state".into(),
                    key: id(888).to_string(),
                }),
            });
        } else {
            table.columns.push(Column {
                name: "project_id".into(),
                data_type: "uuid".into(),
                nullable: false,
            });
            table.foreign_keys.push(ForeignKey {
                name: "declared_project_fk".into(),
                columns: vec!["project_id".into()],
                target_table: "public.projects".into(),
                target_columns: vec!["id".into()],
            });
            table.rows[0].references.push(RowReference {
                constraint: "declared_project_fk".into(),
                target: Some(RecordRef {
                    table: "public.projects".into(),
                    key: id(999).to_string(),
                }),
            });
        }
        let validation = inspect_inventory(&inventory);
        assert!(
            validation
                .notices
                .iter()
                .any(|notice| notice.record.table == "public.memory_access_events")
        );
        assert!(
            validation
                .blockers
                .iter()
                .any(|blocker| blocker.starts_with("dangling reference"))
        );
        assert_blocked(
            &inventory,
            &draft_manifest(&inventory).expect("draft"),
            "dangling reference",
        );
    }
}

#[test]
fn cli_inventory_draft_and_verification_expose_all_retention_notices() {
    let temporary = std::env::temp_dir().join(format!("mem-area-notices-{}", Uuid::new_v4()));
    std::fs::create_dir(&temporary).expect("synthetic directory");
    let inventory = fixture();
    let input = temporary.join("inventory.json");
    let manifest = temporary.join("manifest.json");
    std::fs::write(&input, serde_json::to_vec(&inventory).expect("serialize"))
        .expect("write fixture");
    std::fs::write(
        &manifest,
        serde_json::to_vec(&draft_manifest(&inventory).expect("draft")).expect("serialize"),
    )
    .expect("write manifest");
    let expected = serde_json::to_value(inspect_inventory(&inventory).notices).expect("notices");
    for (command, argument, field) in [
        ("inventory", "--input", "notices"),
        ("plan", "--inventory", "inventory_notices"),
        ("verify", "--inventory", "notices"),
    ] {
        let mut process = std::process::Command::new(env!("CARGO_BIN_EXE_codifix-area"));
        process.args([command, argument]).arg(&input);
        if command == "verify" {
            process.arg("--manifest").arg(&manifest);
        }
        let output = process.output().expect("run CLI");
        assert!(output.status.success(), "{command}");
        let report: serde_json::Value = serde_json::from_slice(&output.stdout).expect("report");
        assert_eq!(
            report[field], expected,
            "{command} must retain every original ID and notice"
        );
    }
    std::fs::remove_file(input).expect("remove synthetic inventory");
    std::fs::remove_file(manifest).expect("remove synthetic manifest");
    std::fs::remove_dir(temporary).expect("remove empty directory");
}
