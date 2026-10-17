// SPDX-License-Identifier: AGPL-3.0-or-later
//! Exercise the shipped command, never an existing installation.
#[allow(dead_code)]
#[path = "../../codifix-area-store/tests/support/mod.rs"]
mod support;

use serde_json::{Value, json};
use sha2::{Digest, Sha256};
use sqlx::ConnectOptions;
mod private_fixture;
use private_fixture::PrivateDir as TempDir;
use std::{
    fs::{self, OpenOptions},
    io::Write,
    path::Path,
    process::{Command, Output},
};

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
    let mut cmd = Command::new(env!("CARGO_BIN_EXE_codifix-area-import"));
    for (key, _) in std::env::vars_os() {
        if key.to_string_lossy().starts_with("PG") {
            cmd.env_remove(key);
        }
    }
    cmd
}
fn success(output: Output) -> Value {
    assert!(
        output.status.success(),
        "{}",
        String::from_utf8_lossy(&output.stderr)
    );
    assert!(output.stderr.is_empty());
    assert!(!String::from_utf8_lossy(&output.stdout).contains(support::PRIVATE));
    serde_json::from_slice(&output.stdout).expect("synthetic operator fixture")
}
fn failure(output: Output, expected: &str) {
    assert!(!output.status.success());
    assert!(output.stdout.is_empty());
    let text = String::from_utf8(output.stderr).expect("synthetic operator fixture");
    assert!(text.contains(expected), "{text}");
    assert!(!text.contains(support::PRIVATE));
}
fn source(cmd: &mut Command, dir: &Path, digests: &Value) {
    cmd.arg("--inventory")
        .arg(dir.join("inventory.json"))
        .arg("--manifest")
        .arg(dir.join("manifest.json"))
        .arg("--payload")
        .arg(dir.join("payload.ndjson"))
        .arg("--inventory-sha256")
        .arg(
            digests["inventory_sha256"]
                .as_str()
                .expect("synthetic operator fixture"),
        )
        .arg("--manifest-sha256")
        .arg(
            digests["manifest_sha256"]
                .as_str()
                .expect("synthetic operator fixture"),
        );
}
fn import_command(
    dir: &Path,
    prepared: &Path,
    digests: &Value,
    receipt: &Value,
    target: &str,
) -> Command {
    let mut cmd = command();
    cmd.arg("import");
    source(&mut cmd, dir, digests);
    cmd.arg("--prepared")
        .arg(prepared.join("prepared.json"))
        .arg("--prepared-sha256")
        .arg(
            receipt["prepared_sha256"]
                .as_str()
                .expect("synthetic operator fixture"),
        )
        .arg("--target")
        .arg(dir.join("target.json"))
        .arg("--target-sha256")
        .arg(target)
        .arg("--operation-id")
        .arg(support::OPERATION.to_string());
    cmd
}

#[tokio::test]
async fn actual_command_prepares_imports_and_reconciles_only_an_inactive_target() {
    let Some(original) = support::database().await else {
        return;
    };
    let Some(target) = support::database().await else {
        return;
    };
    support::seed(&original.pool).await;
    let mut fixture = support::capture(&original.pool).await;
    fixture.project_provenance();
    for record in &mut fixture.manifest.records {
        if record.record.table == "public.project_commits" {
            record.disposition = codifix_area::Disposition::ProtectedArchive {
                reason: "Portable transfer excludes original git context".into(),
                review: codifix_area::ClassificationReview {
                    classification: codifix_area::SourceClassification::Knowledge,
                    source_evidence: "Synthetic git context remains archived".into(),
                    project_evidence: "Synthetic original project retained".into(),
                    content_evidence: "No git bodies authorized for transfer".into(),
                },
            };
        }
    }
    let private = private_dir();
    let artifacts = private_dir();
    let dir = private.path();
    write(
        &dir.join("inventory.json"),
        &serde_json::to_vec_pretty(&fixture.inventory).expect("synthetic operator fixture"),
    );
    write(
        &dir.join("manifest.json"),
        &serde_json::to_vec_pretty(&fixture.manifest).expect("synthetic operator fixture"),
    );
    write(&dir.join("payload.ndjson"), &fixture.payload);
    let verified = success(
        command()
            .arg("verify-payload")
            .arg("--inventory")
            .arg(dir.join("inventory.json"))
            .arg("--payload")
            .arg(dir.join("payload.ndjson"))
            .arg("--inventory-sha256")
            .arg(codifix_area::inventory_digest(&fixture.inventory).expect("digest"))
            .output()
            .expect("verifier command"),
    );
    assert_eq!(
        verified["status"],
        "payload_verified_only_not_approved_or_imported"
    );
    assert_eq!(verified["payload_file_sha256"], digest(&fixture.payload));
    failure(
        command()
            .arg("verify-payload")
            .arg("--inventory")
            .arg(dir.join("inventory.json"))
            .arg("--payload")
            .arg(dir.join("payload.ndjson"))
            .arg("--inventory-sha256")
            .arg("0".repeat(64))
            .output()
            .expect("bad verifier binding"),
        "digest does not match",
    );
    let inspection = success(
        command()
            .arg("inspect")
            .arg("--inventory")
            .arg(dir.join("inventory.json"))
            .arg("--manifest")
            .arg(dir.join("manifest.json"))
            .output()
            .expect("synthetic operator fixture"),
    );
    assert_eq!(inspection["status"], "inspected_only_not_approved");
    assert_eq!(inspection["plan_valid"], true);
    assert_eq!(inspection["blocker_count"], 0);
    // Interrupted input cannot produce a complete receipt/archive, and its
    // partial must not be mistaken for a resumable preparation.
    let interrupted = private_dir();
    fs::write(
        dir.join("payload.ndjson"),
        &fixture.payload[..fixture.payload.len() / 2],
    )
    .expect("synthetic operator fixture");
    failure(
        command()
            .arg("verify-payload")
            .arg("--inventory")
            .arg(dir.join("inventory.json"))
            .arg("--payload")
            .arg(dir.join("payload.ndjson"))
            .arg("--inventory-sha256")
            .arg(codifix_area::inventory_digest(&fixture.inventory).expect("digest"))
            .output()
            .expect("interrupted verifier command"),
        "payload verification failed",
    );
    let mut incomplete = command();
    incomplete.arg("prepare");
    source(&mut incomplete, dir, &inspection);
    incomplete.arg("--output-dir").arg(interrupted.path());
    failure(
        incomplete.output().expect("synthetic operator fixture"),
        "materialization failed",
    );
    assert!(!interrupted.path().join("prepared.json").exists());
    assert!(!interrupted.path().join("archive.ndjson").exists());
    assert!(interrupted.path().join("archive.partial").exists());
    failure(
        incomplete.output().expect("synthetic operator fixture"),
        "private file boundary",
    );
    fs::write(dir.join("payload.ndjson"), &fixture.payload).expect("synthetic operator fixture");
    let mut prepare = command();
    prepare.arg("prepare");
    source(&mut prepare, dir, &inspection);
    prepare.arg("--output-dir").arg(artifacts.path());
    let receipt = success(prepare.output().expect("synthetic operator fixture"));
    assert_eq!(receipt["status"], "prepared_only_not_imported");
    assert_eq!(
        receipt["prepared_sha256"],
        digest(
            &fs::read(artifacts.path().join("prepared.json")).expect("synthetic operator fixture")
        )
    );
    let archive =
        fs::read(artifacts.path().join("archive.ndjson")).expect("synthetic operator fixture");
    assert!(String::from_utf8_lossy(&archive).contains(support::PRIVATE));
    for name in ["archive.ndjson", "prepared.json"] {
        private_fixture::assert_private(&artifacts.path().join(name));
    }
    failure(
        prepare.output().expect("synthetic operator fixture"),
        "private file boundary",
    );
    assert!(!artifacts.path().join("archive.partial").exists());

    let options = target.pool.connect_options();
    assert!(matches!(options.get_host(), "127.0.0.1" | "localhost"));
    let url = options.to_url_lossy();
    let password = url.password().map(|p| {
        percent_encoding::percent_decode_str(p)
            .decode_utf8()
            .expect("synthetic operator fixture")
            .into_owned()
    });
    let descriptor = serde_json::to_vec(&json!({
        "format_version":1, "instance_id":support::TARGET,"area":"general",
        "connection":{"kind":"loopback","port":options.get_port()},
        "database":options.get_database().expect("synthetic operator fixture"),"username":options.get_username(),"password":password
    })).expect("synthetic operator fixture");
    write(&dir.join("target.json"), &descriptor);
    let target_sha = digest(&descriptor);
    let import = || import_command(dir, artifacts.path(), &inspection, &receipt, &target_sha);
    failure(
        import().output().expect("synthetic operator fixture"),
        "execution assertions",
    );
    let ready = || {
        let mut cmd = import();
        cmd.args(["--execute", "--writers-stopped", "--archive-backed-up"]);
        cmd
    };
    failure(
        ready()
            .env("PGOPTIONS", support::PRIVATE)
            .output()
            .expect("synthetic operator fixture"),
        "environment overrides",
    );

    // Changes to every reviewed artifact fail before the first target write.
    for path in [
        dir.join("payload.ndjson"),
        dir.join("manifest.json"),
        dir.join("inventory.json"),
        dir.join("target.json"),
        artifacts.path().join("archive.ndjson"),
        artifacts.path().join("prepared.json"),
    ] {
        let old = fs::read(&path).expect("synthetic operator fixture");
        OpenOptions::new()
            .append(true)
            .open(&path)
            .expect("synthetic operator fixture")
            .write_all(b" ")
            .expect("synthetic operator fixture");
        let error = if path.ends_with("payload.ndjson") {
            "materialization failed"
        } else {
            "digest does not match"
        };
        failure(ready().output().expect("synthetic operator fixture"), error);
        fs::write(&path, old).expect("synthetic operator fixture");
    }
    support::assert_empty(&target.pool).await;
    let result = success(ready().output().expect("synthetic operator fixture"));
    assert_eq!(result["already_applied"], false);
    assert_eq!(result["projection_count"], 1);
    assert_eq!(result["target_instance"], support::TARGET.to_string());
    assert_eq!(
        success(ready().output().expect("synthetic operator fixture"))["already_applied"],
        true
    );
    assert!(support::rows(&target.pool, "public.tasks").await.is_empty());
    assert_eq!(
        support::rows(&target.pool, "public.memory_entries").await,
        support::rows(&original.pool, "public.memory_entries").await
    );
    let sources = support::rows(&target.pool, "public.memory_sources")
        .await
        .join("\n");
    assert!(!sources.contains(support::PRIVATE));
    assert!(!sources.contains("/synthetic/private-source"));
    support::execute(
        &target.pool,
        "UPDATE loop_global_state SET kill_switch_enabled=false",
    )
    .await;
    failure(
        ready().output().expect("synthetic operator fixture"),
        "inactive installation",
    );
    // The separate existing-target path can inspect an active installation,
    // but execution requires explicit fencing and exact reviewed preview bytes.
    let reconcile = || {
        let original = import();
        let args: Vec<_> = original.get_args().skip(1).map(|s| s.to_owned()).collect();
        let mut cmd = command();
        cmd.arg("reconcile").args(args);
        cmd
    };
    let review = success(reconcile().output().expect("preview command"));
    write(
        &dir.join("merge-preview.json"),
        &serde_json::to_vec(&review["preview"]).expect("preview"),
    );
    let mut inspect = command();
    inspect
        .arg("reconciliation-digest")
        .arg("--preview")
        .arg(dir.join("merge-preview.json"));
    assert_eq!(
        success(inspect.output().expect("digest"))["preview_sha256"],
        review["preview_sha256"]
    );
    let apply = || {
        let mut cmd = reconcile();
        cmd.arg("--execute")
            .arg("--preview")
            .arg(dir.join("merge-preview.json"))
            .arg("--preview-sha256")
            .arg(review["preview_sha256"].as_str().expect("digest"));
        cmd
    };
    failure(
        apply().output().expect("missing fence"),
        "execution assertions",
    );
    let first = success(
        apply()
            .args(["--writers-stopped", "--archive-backed-up"])
            .output()
            .expect("merge"),
    );
    assert_eq!(first["already_applied"], false);
    assert_eq!(first["inserted"], 0);
    assert_eq!(
        success(
            apply()
                .args(["--writers-stopped", "--archive-backed-up"])
                .output()
                .expect("merge replay")
        )["already_applied"],
        true
    );
    // The CA receives only its reviewed partition, never the full source files.
    let portable = private_dir();
    let mut export = command();
    export.arg("export-portable");
    source(&mut export, dir, &inspection);
    export
        .arg("--prepared")
        .arg(artifacts.path().join("prepared.json"))
        .arg("--prepared-sha256")
        .arg(
            receipt["prepared_sha256"]
                .as_str()
                .expect("synthetic portable fixture"),
        )
        .args([
            "--area",
            "general",
            "--target-instance",
            &support::TARGET.to_string(),
        ])
        .arg("--output-dir")
        .arg(portable.path());
    let exported = success(export.output().expect("single area export"));
    let bundle_path = portable.path().join("portable-partition.json");
    let bundle = fs::read(&bundle_path).expect("synthetic portable fixture");
    assert!(!String::from_utf8_lossy(&bundle).contains(support::PRIVATE));
    let portable_cmd = || {
        let mut cmd = command();
        cmd.arg("reconcile-portable")
            .arg("--bundle")
            .arg(&bundle_path)
            .arg("--bundle-sha256")
            .arg(
                exported["bundle_sha256"]
                    .as_str()
                    .expect("synthetic portable fixture"),
            )
            .arg("--target")
            .arg(dir.join("target.json"))
            .arg("--target-sha256")
            .arg(&target_sha)
            .arg("--operation-id")
            .arg(uuid::Uuid::from_u128(88888).to_string());
        cmd
    };
    let portable_preview = success(portable_cmd().output().expect("synthetic portable fixture"));
    assert_eq!(portable_preview, review["preview"]);
    // Tampering fails before any connection or target mutation.
    fs::write(&bundle_path, [bundle.as_slice(), b" "].concat())
        .expect("synthetic portable fixture");
    failure(
        portable_cmd().output().expect("synthetic portable fixture"),
        "digest mismatch",
    );
    fs::write(&bundle_path, &bundle).expect("synthetic portable fixture");
    let portable_apply = || {
        let mut cmd = portable_cmd();
        cmd.args(["--execute", "--writers-stopped", "--archive-backed-up"])
            .arg("--preview")
            .arg(dir.join("merge-preview.json"))
            .arg("--preview-sha256")
            .arg(
                review["preview_sha256"]
                    .as_str()
                    .expect("synthetic portable fixture"),
            );
        cmd
    };
    assert_eq!(
        success(
            portable_apply()
                .output()
                .expect("synthetic portable fixture")
        )["already_applied"],
        false
    );
    assert_eq!(
        success(
            portable_apply()
                .output()
                .expect("synthetic portable fixture")
        )["already_applied"],
        true
    );
    let persisted: serde_json::Value =
        sqlx::query_scalar("SELECT preview_json FROM area_reconciliation_receipts WHERE id=$1")
            .bind(uuid::Uuid::from_u128(88888))
            .fetch_one(&target.pool)
            .await
            .expect("immutable projection journal");
    assert_eq!(persisted["format_version"], 2);
    assert_eq!(persisted["projections"], review["preview"]["projections"]);
    assert_eq!(
        persisted["projections"]
            .as_array()
            .expect("projections")
            .len(),
        1
    );
    assert_eq!(
        persisted["projections"][0]["binding"]["archived_task"]["record"]["key"],
        support::id(3).to_string()
    );
    assert!(
        persisted["projections"][0]["decision_sha256"]
            .as_str()
            .is_some_and(|s| s.len() == 64)
    );
    assert!(
        !serde_json::to_string(&persisted)
            .expect("journal JSON")
            .contains(support::PRIVATE)
    );
    let mut forged = persisted.clone();
    forged["projections"][0]["binding"]["archived_task"]["row_sha256"] = json!("0".repeat(64));
    let forged: codifix_area_store::reconciliation::Preview =
        serde_json::from_value(forged).expect("synthetic forged projection");
    let forged_path = dir.join("forged-provenance.json");
    write(
        &forged_path,
        &serde_json::to_vec(&forged).expect("forged JSON"),
    );
    failure(
        portable_cmd()
            .args(["--execute", "--writers-stopped", "--archive-backed-up"])
            .arg("--preview")
            .arg(&forged_path)
            .arg("--preview-sha256")
            .arg(forged.digest().expect("forged digest"))
            .output()
            .expect("forged provenance replay"),
        "invalid",
    );
    target.cleanup().await.expect("synthetic operator fixture");
    original
        .cleanup()
        .await
        .expect("synthetic operator fixture");
}

#[test]
fn private_input_boundaries_and_errors_are_sanitized() {
    let dir = private_dir();
    let inventory = dir.path().join("inventory.json");
    let manifest = dir.path().join("manifest.json");
    write(&inventory, support::PRIVATE.as_bytes());
    write(&manifest, b"{}");
    let inspect = || {
        let mut cmd = command();
        cmd.arg("inspect")
            .arg("--inventory")
            .arg(&inventory)
            .arg("--manifest")
            .arg(&manifest);
        cmd
    };
    failure(
        inspect().output().expect("synthetic operator fixture"),
        "JSON contract is invalid",
    );
    private_fixture::public_read(&inventory);
    failure(
        inspect().output().expect("synthetic operator fixture"),
        "private file boundary",
    );
    private_fixture::restore_private(&inventory);
    let alias = dir.path().join("alias");
    fs::hard_link(&inventory, &alias).expect("synthetic operator fixture");
    failure(
        inspect().output().expect("synthetic operator fixture"),
        "private file boundary",
    );
    fs::remove_file(&alias).expect("synthetic operator fixture");
    // A directory junction exercises the native reparse boundary without
    // requiring developer mode or symbolic-link privileges on Windows.
    #[cfg(unix)]
    {
        fs::rename(&inventory, &alias).expect("synthetic operator fixture");
        std::os::unix::fs::symlink(&alias, &inventory).expect("synthetic operator fixture");
    }
    #[cfg(windows)]
    {
        fs::remove_file(&inventory).expect("fixture file");
        private_fixture::directory_alias(dir.path(), &inventory);
    }
    failure(
        inspect().output().expect("synthetic operator fixture"),
        "private file boundary",
    );
    #[cfg(unix)]
    {
        fs::remove_file(&inventory).expect("synthetic operator fixture");
        fs::rename(&alias, &inventory).expect("synthetic operator fixture");
    }
    #[cfg(windows)]
    {
        private_fixture::remove_directory_alias(&inventory);
        write(&inventory, support::PRIVATE.as_bytes());
    }
    private_fixture::public_read(dir.path());
    failure(
        inspect().output().expect("synthetic operator fixture"),
        "private file boundary",
    );
    failure(
        command()
            .arg(support::PRIVATE)
            .output()
            .expect("synthetic operator fixture"),
        "supplied values suppressed",
    );
}
