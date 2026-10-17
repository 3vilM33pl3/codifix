// SPDX-License-Identifier: AGPL-3.0-or-later
//! Real operator commands with protected synthetic files only; no DB needed.
use mem_area::{
    Column, Inventory, KNOWN_TABLES, SERIALIZATION_ID, SUPPORTED_MIGRATION, TableInventory,
};
use serde_json::{Value, json};
use sha2::{Digest, Sha256};
mod private_fixture;
use private_fixture::PrivateDir as TempDir;
use std::{
    fs,
    path::Path,
    process::{Command, Output},
};
use uuid::Uuid;

fn private_dir() -> TempDir {
    private_fixture::private_dir()
}
fn write(path: &Path, bytes: &[u8]) {
    private_fixture::write(path, bytes);
}
fn digest(bytes: &[u8]) -> String {
    format!("{:x}", Sha256::digest(bytes))
}
fn command() -> Command {
    Command::new(env!("CARGO_BIN_EXE_memory-area-import"))
}
fn failure(output: Output, reason: &str) {
    assert!(!output.status.success());
    assert!(output.stdout.is_empty());
    let error = String::from_utf8(output.stderr).expect("sanitized error");
    assert!(error.contains(reason), "{error}");
    assert!(!error.contains("PRIVATE_SYNTHETIC_SENTINEL"));
}
struct Fixture {
    dir: TempDir,
    inventory_sha256: String,
    manifest_sha256: String,
}
impl Fixture {
    fn new() -> Self {
        let dir = private_dir();
        let mut inventory = Inventory {
            format_version: 1,
            source_instance: Uuid::from_u128(1),
            snapshot_id: Uuid::from_u128(2),
            captured_at: "2026-09-20T00:00:00Z".into(),
            migration_version: SUPPORTED_MIGRATION,
            projects: vec![],
            tables: KNOWN_TABLES
                .iter()
                .map(|(table, _)| TableInventory {
                    table: (*table).into(),
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
        inventory.tables.sort_by(|a, b| a.table.cmp(&b.table));
        let manifest = mem_area::draft_manifest(&inventory).expect("manifest");
        assert!(mem_area::verify_manifest(&inventory, &manifest).plan_valid);
        let inventory_sha256 = mem_area::inventory_digest(&inventory).expect("inventory digest");
        let manifest_sha256 = mem_area::manifest_digest(&manifest).expect("manifest digest");
        let mut payload = vec![json!({"event":"header", "format_version":1,
            "source_instance":inventory.source_instance,"snapshot_id":inventory.snapshot_id,
            "inventory_sha256":inventory_sha256,"serialization_id":SERIALIZATION_ID})];
        for table in &inventory.tables {
            payload.push(json!({"event":"table_start","table":table.table,"row_count":0}));
            payload.push(json!({"event":"table_end","table":table.table,"row_count":0}));
        }
        payload
            .push(json!({"event":"complete","table_count":inventory.tables.len(),"row_count":0}));
        let payload = payload.iter().map(|v| format!("{v}\n")).collect::<String>();
        write(
            &dir.path().join("inventory.json"),
            &serde_json::to_vec(&inventory).expect("JSON"),
        );
        write(
            &dir.path().join("manifest.json"),
            &serde_json::to_vec(&manifest).expect("JSON"),
        );
        write(&dir.path().join("payload.ndjson"), payload.as_bytes());
        Self {
            dir,
            inventory_sha256,
            manifest_sha256,
        }
    }
    fn source(&self, cmd: &mut Command) {
        cmd.arg("--inventory")
            .arg(self.dir.path().join("inventory.json"))
            .arg("--manifest")
            .arg(self.dir.path().join("manifest.json"))
            .arg("--payload")
            .arg(self.dir.path().join("payload.ndjson"))
            .arg("--inventory-sha256")
            .arg(&self.inventory_sha256)
            .arg("--manifest-sha256")
            .arg(&self.manifest_sha256);
    }
    fn prepare(&self, output: &Path, mib: Option<&str>) -> Command {
        let mut cmd = command();
        cmd.arg("prepare");
        self.source(&mut cmd);
        cmd.arg("--output-dir").arg(output);
        if let Some(mib) = mib {
            cmd.args(["--max-retained-target-mib", mib]);
        }
        cmd
    }
    fn import(&self, output: &Path, receipt_sha: &str, mib: Option<&str>) -> Command {
        let mut cmd = command();
        cmd.arg("import");
        self.source(&mut cmd);
        cmd.arg("--prepared")
            .arg(output.join("prepared.json"))
            .args(["--prepared-sha256", receipt_sha])
            .arg("--target")
            .arg(self.dir.path().join("never-created-target.json"))
            .args([
                "--target-sha256",
                &"0".repeat(64),
                "--operation-id",
                &Uuid::from_u128(3).to_string(),
                "--execute",
                "--writers-stopped",
                "--archive-backed-up",
            ]);
        if let Some(mib) = mib {
            cmd.args(["--max-retained-target-mib", mib]);
        }
        cmd
    }
}

#[test]
fn opt_in_receipt_requires_the_same_budget_before_any_target_open() {
    let fixture = Fixture::new();
    for mib in [None, Some("512"), Some("1024")] {
        let output = private_dir();
        let result = fixture
            .prepare(output.path(), mib)
            .output()
            .expect("prepare");
        assert!(
            result.status.success(),
            "{}",
            String::from_utf8_lossy(&result.stderr)
        );
        let report: Value = serde_json::from_slice(&result.stdout).expect("report");
        let expected = mib.unwrap_or("256").parse::<u32>().expect("budget");
        assert_eq!(report["prepared"]["max_retained_target_mib"], expected);
        let bytes = fs::read(output.path().join("prepared.json")).expect("receipt");
        let receipt_sha = digest(&bytes);
        assert_eq!(report["prepared_sha256"], receipt_sha);
        let different = if mib.is_none() { Some("512") } else { None };
        failure(
            fixture
                .import(output.path(), &receipt_sha, different)
                .output()
                .expect("mismatch"),
            "digest does not match",
        );
        // Matching preparation passes full rematerialization, and only fails at
        // the deliberately nonexistent target. No database can be contacted.
        failure(
            fixture
                .import(output.path(), &receipt_sha, mib)
                .output()
                .expect("matched budget"),
            "private file boundary",
        );
    }
}

#[test]
fn old_receipts_mean_default_and_invalid_values_fail_before_file_access() {
    let fixture = Fixture::new();
    let output = private_dir();
    for invalid in [
        "0",
        "255",
        "1025",
        "4294967295",
        "PRIVATE_SYNTHETIC_SENTINEL",
    ] {
        failure(
            fixture
                .prepare(output.path(), Some(invalid))
                .output()
                .expect("invalid prepare"),
            "supplied values suppressed",
        );
        failure(
            fixture
                .import(output.path(), &"0".repeat(64), Some(invalid))
                .output()
                .expect("invalid import"),
            "supplied values suppressed",
        );
        assert_eq!(
            fs::read_dir(output.path()).expect("empty output").count(),
            0
        );
    }
    assert!(
        fixture
            .prepare(output.path(), None)
            .output()
            .expect("prepare")
            .status
            .success()
    );
    let mut receipt: Value =
        serde_json::from_slice(&fs::read(output.path().join("prepared.json")).expect("receipt"))
            .expect("JSON");
    receipt
        .as_object_mut()
        .expect("receipt object")
        .remove("max_retained_target_mib");
    let old = serde_json::to_vec(&receipt).expect("legacy receipt");
    fs::write(output.path().join("prepared.json"), &old).expect("legacy bytes");
    failure(
        fixture
            .import(output.path(), &digest(&old), None)
            .output()
            .expect("legacy import"),
        "private file boundary",
    );
    failure(
        fixture
            .import(output.path(), &digest(&old), Some("512"))
            .output()
            .expect("legacy mismatch"),
        "digest does not match",
    );
    receipt["max_retained_target_mib"] = json!(0);
    let invalid = serde_json::to_vec(&receipt).expect("invalid receipt");
    fs::write(output.path().join("prepared.json"), &invalid).expect("invalid bytes");
    failure(
        fixture
            .import(output.path(), &digest(&invalid), None)
            .output()
            .expect("invalid receipt"),
        "digest does not match",
    );
}

#[test]
fn materialization_errors_expose_only_static_reasons() {
    for reason in [
        mem_area::MaterializeError::TargetLimit,
        mem_area::MaterializeError::HashMismatch,
        mem_area::MaterializeError::InvalidEnvelope,
        mem_area::MaterializeError::ArchiveIo,
    ] {
        let error = mem_area_ops::Failure::from(reason).to_string();
        assert!(error.contains(&reason.to_string()));
        assert!(error.contains("incomplete artifacts must not be used"));
    }
}
