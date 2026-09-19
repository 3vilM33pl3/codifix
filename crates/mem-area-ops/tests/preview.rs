// SPDX-License-Identifier: AGPL-3.0-or-later
//! Offline synthetic selections and the shipped file-only CLI. No DB fixture.
use mem_area::*;
use mem_area_ops::preview::*;
use serde_json::json;
use sha2::{Digest, Sha256};
use std::{
    fs::{self, OpenOptions},
    io::Write,
    os::unix::fs::{OpenOptionsExt, PermissionsExt, symlink},
    path::Path,
    process::Command,
};
use uuid::Uuid;

const PRIVATE: &str = "SYNTHETIC_PRIVATE_CANONICAL_TEXT";
fn id(value: u128) -> Uuid {
    Uuid::from_u128(value)
}
fn digest(value: &[u8]) -> String {
    format!("{:x}", Sha256::digest(value))
}
fn table<'a>(inventory: &'a mut Inventory, name: &str) -> &'a mut TableInventory {
    inventory
        .tables
        .iter_mut()
        .find(|table| table.table == name)
        .expect("fixture table")
}
fn col(name: &str) -> Column {
    Column {
        name: name.into(),
        data_type: "text".into(),
        nullable: false,
    }
}
fn row(key: u128, canonical: Option<u128>, version: Option<VersionIdentity>) -> RowInventory {
    RowInventory {
        key: id(key).to_string(),
        row_sha256: "a".repeat(64),
        source_project_id: canonical.map(|_| id(1)),
        canonical_id: canonical.map(id),
        version,
        references: vec![],
    }
}
fn fk(name: &str, column: &str, target: &str) -> ForeignKey {
    ForeignKey {
        name: name.into(),
        columns: vec![column.into()],
        target_table: target.into(),
        target_columns: vec!["id".into()],
    }
}
fn reference(name: &str, target: &str, key: u128) -> RowReference {
    RowReference {
        constraint: name.into(),
        target: Some(RecordRef {
            table: target.into(),
            key: id(key).to_string(),
        }),
    }
}
fn fixture() -> (Inventory, Selection) {
    let mut inventory = Inventory {
        format_version: 1,
        source_instance: id(2),
        snapshot_id: id(3),
        captured_at: "2026-09-19T00:00:00Z".into(),
        migration_version: 52,
        projects: vec![ProjectProvenance {
            id: id(1),
            slug: "synthetic-original".into(),
            name: "Original project".into(),
            root_path: "/synthetic".into(),
            created_at: "2026-09-19T00:00:00Z".into(),
        }],
        tables: KNOWN_TABLES
            .iter()
            .map(|(name, _)| TableInventory {
                table: (*name).into(),
                columns: vec![col("id")],
                foreign_keys: vec![],
                row_count: 0,
                rows: vec![],
            })
            .collect(),
    };
    table(&mut inventory, "public.projects")
        .rows
        .push(row(1, None, None));
    table(&mut inventory, "public.memory_entries").columns = [
        "id",
        "project_id",
        "canonical_id",
        "version_no",
        "is_tombstone",
        "canonical_text",
    ]
    .map(col)
    .to_vec();
    let mut versions = Vec::new();
    for (key, no, tombstone) in [(10, 1, false), (11, 2, false), (12, 3, true)] {
        let text = if tombstone { "" } else { PRIVATE };
        let payload = json!({"id":id(key),"project_id":id(1),"canonical_id":id(10),"version_no":no,"is_tombstone":tombstone,"canonical_text":text}).to_string();
        let mut original = row(
            key,
            Some(10),
            Some(VersionIdentity {
                version_no: no,
                is_tombstone: tombstone,
            }),
        );
        original.row_sha256 = digest(payload.as_bytes());
        versions.push(SelectedVersion {
            id: id(key),
            version_no: no,
            is_tombstone: tombstone,
            row_sha256: original.row_sha256.clone(),
            canonical_text_sha256: digest(text.as_bytes()),
            row_json: payload,
        });
        table(&mut inventory, "public.memory_entries")
            .rows
            .push(original);
    }
    table(&mut inventory, "public.memory_entries")
        .rows
        .push(row(
            20,
            Some(20),
            Some(VersionIdentity {
                version_no: 1,
                is_tombstone: false,
            }),
        ));
    table(&mut inventory, "public.memory_canonical_state").rows =
        vec![row(10, Some(10), None), row(20, Some(20), None)];
    table(&mut inventory, "public.sessions")
        .rows
        .push(row(40, None, None));
    let tasks = table(&mut inventory, "public.tasks");
    tasks.columns.push(col("session_id"));
    tasks
        .foreign_keys
        .push(fk("task_session", "session_id", "public.sessions"));
    let mut task = row(41, None, None);
    task.references
        .push(reference("task_session", "public.sessions", 40));
    tasks.rows.push(task);
    let captures = table(&mut inventory, "public.raw_captures");
    captures.columns.push(col("task_id"));
    captures
        .foreign_keys
        .push(fk("capture_task", "task_id", "public.tasks"));
    let mut capture = row(42, None, None);
    capture
        .references
        .push(reference("capture_task", "public.tasks", 41));
    captures.rows.push(capture);
    let sources = table(&mut inventory, "public.memory_sources");
    sources
        .columns
        .extend([col("memory_entry_id"), col("task_id")]);
    sources.foreign_keys.extend([
        fk("version_link", "memory_entry_id", "public.memory_entries"),
        fk("task_link", "task_id", "public.tasks"),
    ]);
    for (key, version) in [(50, 10), (51, 11)] {
        let mut source = row(key, Some(10), None);
        source.references = vec![
            reference("version_link", "public.memory_entries", version),
            reference("task_link", "public.tasks", 41),
            RowReference {
                constraint: "logical:target_memory_id".into(),
                target: None,
            },
        ];
        sources.rows.push(source);
    }
    let checks = table(&mut inventory, "public.memory_source_checks");
    checks.columns.push(col("source_id"));
    checks
        .foreign_keys
        .push(fk("source_link", "source_id", "public.memory_sources"));
    let mut check = row(60, Some(10), None);
    check
        .references
        .push(reference("source_link", "public.memory_sources", 50));
    checks.rows.push(check);
    table(&mut inventory, "public.auth_service_tokens")
        .rows
        .push(row(70, None, None));
    for table in &mut inventory.tables {
        table.row_count = table.rows.len() as u64;
    }
    assert!(verify_inventory(&inventory).is_empty());
    let selection = Selection {
        format_version: 1,
        source_instance: inventory.source_instance,
        snapshot_id: inventory.snapshot_id,
        inventory_sha256: inventory_digest(&inventory).expect("digest"),
        area: AreaId::Homelab,
        project_id: id(1),
        review: Review {
            explicitly_approved: true,
            operator: "Synthetic delegated operator, not a human identity claim".into(),
            reviewed_at: "2026-09-19T00:00:00Z".into(),
            authority_basis: "Synthetic test authority only; no remote or model permission".into(),
            source_evidence: "Synthetic complete-row hashes".into(),
            project_evidence: "Original synthetic project".into(),
            content_evidence: "Synthetic full history reviewed".into(),
        },
        lineages: vec![SelectedLineage {
            canonical_id: id(10),
            canonical_state_sha256: "a".repeat(64),
            versions,
        }],
        sources: (50..=51)
            .map(|key| SelectedSource {
                source_id: id(key),
                row_sha256: "a".repeat(64),
                task_id: id(41),
                task_row_sha256: "a".repeat(64),
                decision_id: id(key + 100),
                approval_record_id: id(key + 200),
                explicitly_approved: true,
            })
            .collect(),
    };
    (inventory, selection)
}

#[test]
fn preserves_full_history_only_and_requires_no_raw_copy_or_implicit_grant() {
    let (inventory, selection) = fixture();
    let manifest = reviewed_manifest(&inventory, &selection).expect("reviewed fixture");
    let report = verify_manifest(&inventory, &manifest);
    assert!(report.plan_valid);
    assert_eq!(manifest.projects[0].source, inventory.projects[0]);
    assert_eq!(manifest.provenance_projections.len(), 2);
    assert_eq!(
        report
            .records
            .iter()
            .filter(|record| record.record.table == "public.memory_entries"
                && record.destination == Destination::Area(AreaId::Homelab))
            .count(),
        3
    );
    assert!(
        report
            .records
            .iter()
            .filter(|record| matches!(
                record.record.table.as_str(),
                "public.sessions"
                    | "public.tasks"
                    | "public.raw_captures"
                    | "public.memory_source_checks"
            ))
            .all(|record| record.destination == Destination::ProtectedArchive)
    );
    assert_eq!(
        report
            .records
            .iter()
            .find(|record| record.record.table == "public.auth_service_tokens")
            .expect("runtime row")
            .destination,
        Destination::ArchiveAndRetire
    );
    assert_eq!(
        report
            .records
            .iter()
            .find(|record| record.record.table == "public.memory_entries"
                && record.record.key == id(20).to_string())
            .expect("excluded lineage")
            .destination,
        Destination::ProtectedArchive
    );
    for (projection, source) in manifest
        .provenance_projections
        .iter()
        .zip(&selection.sources)
    {
        assert_eq!(
            projection.content,
            ProjectedSourceContent::ArchiveReferenceOnly {}
        );
        assert_eq!(projection.decision_id, source.decision_id);
        assert_eq!(
            projection.operator_attestation.approval_record_id,
            source.approval_record_id
        );
        assert_eq!(
            projection.operator_attestation.operator,
            selection.review.operator
        );
        assert_eq!(
            projection.binding.archived_task.row_sha256,
            source.task_row_sha256
        );
    }
    assert!(
        !serde_json::to_string(&manifest)
            .expect("manifest")
            .contains(PRIVATE)
    );
    assert_eq!(
        manifest,
        reviewed_manifest(&inventory, &selection).expect("repeat")
    );
}

#[test]
fn incomplete_replayed_unapproved_unknown_or_mismatched_selections_fail_closed() {
    let (inventory, selection) = fixture();
    let mut cases = Vec::new();
    let mut bad = selection.clone();
    bad.review.explicitly_approved = false;
    cases.push(bad);
    let mut bad = selection.clone();
    bad.review.operator.clear();
    cases.push(bad);
    let mut bad = selection.clone();
    bad.review.authority_basis.clear();
    cases.push(bad);
    let mut bad = selection.clone();
    bad.format_version = 2;
    cases.push(bad);
    let mut bad = selection.clone();
    bad.project_id = id(99);
    cases.push(bad);
    let mut bad = selection.clone();
    bad.snapshot_id = id(99);
    cases.push(bad);
    let mut bad = selection.clone();
    bad.lineages.clear();
    cases.push(bad);
    let mut bad = selection.clone();
    bad.lineages.push(bad.lineages[0].clone());
    cases.push(bad);
    let mut bad = selection.clone();
    bad.lineages[0].canonical_state_sha256 = "b".repeat(64);
    cases.push(bad);
    let mut bad = selection.clone();
    bad.lineages[0].versions.pop();
    cases.push(bad);
    let mut bad = selection.clone();
    bad.lineages[0].versions[0].canonical_text_sha256 = "b".repeat(64);
    cases.push(bad);
    let mut bad = selection.clone();
    bad.lineages[0].versions[0].row_json.push(' ');
    cases.push(bad);
    let mut bad = selection.clone();
    bad.sources.pop();
    cases.push(bad);
    let mut bad = selection.clone();
    bad.sources[0].explicitly_approved = false;
    cases.push(bad);
    let mut bad = selection.clone();
    bad.sources[0].task_row_sha256 = "b".repeat(64);
    cases.push(bad);
    let mut bad = selection.clone();
    bad.sources[0].task_id = id(99);
    cases.push(bad);
    let mut bad = selection.clone();
    bad.sources[0].approval_record_id = bad.sources[1].approval_record_id;
    cases.push(bad);
    let mut bad = selection.clone();
    bad.sources[0].decision_id = bad.sources[1].decision_id;
    cases.push(bad);
    for bad in cases {
        let error = reviewed_manifest(&inventory, &bad).expect_err("rejected");
        assert!(!error.to_string().contains(PRIVATE));
    }
    let mut value = serde_json::to_value(&selection).expect("value");
    value["remote_consent"] = json!(true);
    assert!(serde_json::from_value::<Selection>(value).is_err());
    let mut value = serde_json::to_value(&selection).expect("value");
    value["review"]
        .as_object_mut()
        .expect("review")
        .remove("explicitly_approved");
    assert!(serde_json::from_value::<Selection>(value).is_err());
}

#[test]
fn source_with_null_task_or_memory_target_never_falls_back_to_copying_context() {
    for constraint in ["task_link", "logical:target_memory_id"] {
        let (mut inventory, mut selection) = fixture();
        let source = &mut table(&mut inventory, "public.memory_sources").rows[0];
        source
            .references
            .iter_mut()
            .find(|reference| reference.constraint == constraint)
            .expect("reference")
            .target = if constraint == "task_link" {
            None
        } else {
            Some(RecordRef {
                table: "public.memory_entries".into(),
                key: id(10).to_string(),
            })
        };
        selection.inventory_sha256 = inventory_digest(&inventory).expect("rebound digest");
        assert!(reviewed_manifest(&inventory, &selection).is_err());
    }
}

fn private_dir() -> tempfile::TempDir {
    tempfile::Builder::new()
        .permissions(fs::Permissions::from_mode(0o700))
        .tempdir()
        .expect("private fixture")
}
fn write(path: &Path, bytes: &[u8]) {
    let mut file = OpenOptions::new()
        .write(true)
        .create_new(true)
        .mode(0o600)
        .open(path)
        .expect("private input");
    file.write_all(bytes).expect("fixture bytes");
}
fn execute(input: &Path, output: &Path, selection_sha: &str) -> std::process::Output {
    Command::new(env!("CARGO_BIN_EXE_memory-area-import"))
        .args(["preview-manifest", "--inventory"])
        .arg(input.join("inventory.json"))
        .arg("--selection")
        .arg(input.join("selection.json"))
        .args(["--selection-sha256", selection_sha, "--output-dir"])
        .arg(output)
        .env("PGHOST", "invalid-network-sentinel")
        .output()
        .expect("offline operator")
}

#[test]
fn real_cli_writes_only_private_exclusive_outputs_and_suppresses_errors() {
    let (inventory, selection) = fixture();
    let input = private_dir();
    let output = private_dir();
    let bytes = serde_json::to_vec(&selection).expect("selection");
    write(
        &input.path().join("inventory.json"),
        &serde_json::to_vec(&inventory).expect("inventory"),
    );
    write(&input.path().join("selection.json"), &bytes);
    let result = execute(input.path(), output.path(), &digest(&bytes));
    assert!(
        result.status.success(),
        "{}",
        String::from_utf8_lossy(&result.stderr)
    );
    assert!(result.stdout.is_empty() && result.stderr.is_empty());
    for name in ["preview-manifest.json", "preview-receipt.json"] {
        assert_eq!(
            fs::metadata(output.path().join(name))
                .expect("metadata")
                .permissions()
                .mode()
                & 0o777,
            0o600
        );
    }
    let receipt: serde_json::Value = serde_json::from_slice(
        &fs::read(output.path().join("preview-receipt.json")).expect("receipt"),
    )
    .expect("receipt JSON");
    assert_eq!(receipt["versions"], 3);
    assert_eq!(receipt["selection_file_sha256"], digest(&bytes));
    for field in [
        "authenticated_human_approval",
        "full_partition_complete",
        "remote_or_model_consent_granted",
    ] {
        assert_eq!(receipt[field], false);
    }
    let original = fs::read(output.path().join("preview-manifest.json")).expect("manifest");
    let replay = execute(input.path(), output.path(), &digest(&bytes));
    assert!(!replay.status.success());
    assert!(replay.stdout.is_empty());
    assert_eq!(
        fs::read(output.path().join("preview-manifest.json")).expect("untouched"),
        original
    );
    let bad = private_dir();
    assert!(
        !execute(input.path(), bad.path(), &"0".repeat(64))
            .status
            .success()
    );
    assert_eq!(fs::read_dir(bad.path()).expect("directory").count(), 0);
    fs::set_permissions(
        input.path().join("selection.json"),
        fs::Permissions::from_mode(0o644),
    )
    .expect("mode");
    let failed = execute(input.path(), bad.path(), &digest(&bytes));
    assert!(!failed.status.success());
    assert!(failed.stdout.is_empty());
    assert!(!String::from_utf8_lossy(&failed.stderr).contains(PRIVATE));
    fs::set_permissions(
        input.path().join("selection.json"),
        fs::Permissions::from_mode(0o600),
    )
    .expect("mode");
    let alias = input.path().join("alias");
    symlink(output.path(), &alias).expect("symlink");
    assert!(
        !execute(input.path(), &alias, &digest(&bytes))
            .status
            .success()
    );
}
