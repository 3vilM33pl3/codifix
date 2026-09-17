// SPDX-License-Identifier: AGPL-3.0-or-later
//! Exercise the shipped command, never an existing installation.
#[allow(dead_code)]
#[path = "../../mem-area-store/tests/support/mod.rs"]
mod support;

use serde_json::{Value, json};
use sha2::{Digest, Sha256};
use sqlx::ConnectOptions;
use std::{
    fs::{self, OpenOptions},
    io::Write,
    os::unix::fs::{OpenOptionsExt, PermissionsExt, symlink},
    path::Path,
    process::{Command, Output},
};
use tempfile::TempDir;

fn private_dir() -> TempDir {
    tempfile::Builder::new()
        .permissions(fs::Permissions::from_mode(0o700))
        .tempdir()
        .expect("synthetic operator fixture")
}

fn write(path: &Path, bytes: &[u8]) {
    let mut file = OpenOptions::new()
        .write(true)
        .create_new(true)
        .mode(0o600)
        .open(path)
        .expect("synthetic operator fixture");
    file.write_all(bytes).expect("synthetic operator fixture");
}
fn digest(bytes: &[u8]) -> String {
    format!("{:x}", Sha256::digest(bytes))
}
fn command() -> Command {
    let mut cmd = Command::new(env!("CARGO_BIN_EXE_memory-area-import"));
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
            .arg(mem_area::inventory_digest(&fixture.inventory).expect("digest"))
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
            .arg(mem_area::inventory_digest(&fixture.inventory).expect("digest"))
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
        assert_eq!(
            fs::metadata(artifacts.path().join(name))
                .expect("synthetic operator fixture")
                .permissions()
                .mode()
                & 0o777,
            0o600
        );
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
    fs::set_permissions(&inventory, fs::Permissions::from_mode(0o644))
        .expect("synthetic operator fixture");
    failure(
        inspect().output().expect("synthetic operator fixture"),
        "private file boundary",
    );
    fs::set_permissions(&inventory, fs::Permissions::from_mode(0o600))
        .expect("synthetic operator fixture");
    let alias = dir.path().join("alias");
    fs::hard_link(&inventory, &alias).expect("synthetic operator fixture");
    failure(
        inspect().output().expect("synthetic operator fixture"),
        "private file boundary",
    );
    fs::remove_file(&alias).expect("synthetic operator fixture");
    fs::rename(&inventory, &alias).expect("synthetic operator fixture");
    symlink(&alias, &inventory).expect("synthetic operator fixture");
    failure(
        inspect().output().expect("synthetic operator fixture"),
        "private file boundary",
    );
    fs::remove_file(&inventory).expect("synthetic operator fixture");
    fs::rename(&alias, &inventory).expect("synthetic operator fixture");
    fs::set_permissions(dir.path(), fs::Permissions::from_mode(0o750))
        .expect("synthetic operator fixture");
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
