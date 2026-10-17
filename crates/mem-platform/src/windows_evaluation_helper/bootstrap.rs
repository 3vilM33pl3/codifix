// SPDX-License-Identifier: AGPL-3.0-or-later
//! Prepare-only trusted launcher, never an agent sandbox or paid campaign path.
//! Private source/executable handles remain pinned while a fresh outer Job owns
//! Node and all descendants. No generic argv or inherited environment is used.
use crate::{
    private_windows::{Directory, Input, create_private_directory},
    windows_evaluation::{ToolchainTree, pin_trees},
    windows_process::CodeFile,
    windows_trusted_process,
};
use anyhow::{Context, Result, ensure};
use serde::{Deserialize, Serialize};
use serde_json::Value;
use sha2::{Digest, Sha256};
use std::{
    ffi::OsString,
    io::Read,
    path::{Component, Path, PathBuf},
    thread,
    time::{Duration, Instant},
};

const DESCRIPTOR_LIMIT: u64 = 16 * 1024 * 1024;
const OUTPUT_LIMIT: u64 = 8 * 1024 * 1024;
#[derive(Deserialize)]
#[serde(deny_unknown_fields)]
struct BoundFile {
    path: PathBuf,
    sha256: String,
}
#[derive(Deserialize)]
#[serde(deny_unknown_fields)]
struct Source {
    root: PathBuf,
    files: Vec<BoundFile>,
}
#[derive(Deserialize)]
#[serde(deny_unknown_fields)]
struct Binaries {
    memory: PathBuf,
    snapshot: PathBuf,
    score: PathBuf,
    retriever: PathBuf,
}
#[derive(Deserialize)]
#[serde(deny_unknown_fields)]
struct Descriptor {
    schema_version: u32,
    profile: String,
    node: BoundFile,
    source: Source,
    execution: BoundFile,
    destination: PathBuf,
    compiler: PathBuf,
    binaries: Binaries,
    database_url: String,
    one_family_per_track: bool,
    deadline_ms: u64,
}
#[derive(Serialize)]
pub struct Receipt {
    format: &'static str,
    state: &'static str,
    descriptor_sha256: String,
    node_sha256: String,
    execution_sha256: String,
    source_files: usize,
    stdout_sha256: String,
    stderr_sha256: String,
    destination: PathBuf,
    outer_job_reaped: bool,
    official_evidence: bool,
    execution_started: bool,
}

fn digest(bytes: &[u8]) -> String {
    format!("{:x}", Sha256::digest(bytes))
}
fn hash(value: &str) -> Result<[u8; 32]> {
    ensure!(
        value.len() == 64
            && value
                .bytes()
                .all(|b| b.is_ascii_digit() || (b'a'..=b'f').contains(&b)),
        "explicit lowercase SHA-256 required"
    );
    let mut bytes = [0; 32];
    for (index, byte) in bytes.iter_mut().enumerate() {
        *byte = u8::from_str_radix(&value[index * 2..index * 2 + 2], 16)?;
    }
    Ok(bytes)
}
fn pin(file: &BoundFile, limit: u64) -> Result<(Input, Vec<u8>)> {
    hash(&file.sha256)?;
    let input = Input::open(&file.path, limit)?;
    let mut bytes = Vec::new();
    input
        .file
        .try_clone()?
        .take(limit + 1)
        .read_to_end(&mut bytes)?;
    ensure!(
        bytes.len() as u64 <= limit && digest(&bytes) == file.sha256,
        "bootstrap input changed"
    );
    input.unchanged()?;
    Ok((input, bytes))
}
fn absolute(path: &Path) -> bool {
    path.is_absolute()
        && path.components().all(|part| {
            matches!(
                part,
                Component::Prefix(_) | Component::RootDir | Component::Normal(_)
            )
        })
}
fn path_string(value: &Value, pointer: &str) -> Result<PathBuf> {
    Ok(PathBuf::from(
        value
            .pointer(pointer)
            .and_then(Value::as_str)
            .context("missing native binding")?,
    ))
}
fn reviewed_file(profile: &Value, expected: &Path) -> Result<()> {
    let mut found = 0;
    for tree in profile
        .pointer("/profile/toolchains")
        .and_then(Value::as_array)
        .context("missing native inventory")?
    {
        for file in tree
            .get("files")
            .and_then(Value::as_array)
            .context("missing native files")?
        {
            if file.get("path").and_then(Value::as_str).map(Path::new) == Some(expected) {
                hash(
                    file.get("sha256")
                        .and_then(Value::as_str)
                        .context("missing native digest")?,
                )?;
                found += 1;
            }
        }
    }
    ensure!(
        found == 1,
        "executable absent or ambiguous in reviewed native inventory"
    );
    Ok(())
}
fn validate(descriptor: &Descriptor, profile: &Value, output: &Path) -> Result<PathBuf> {
    ensure!(
        descriptor.schema_version == 1
            && descriptor.profile == "memory-r3-native-bootstrap-prepare-v1"
            && (1000..=120000).contains(&descriptor.deadline_ms)
            && descriptor.database_url.starts_with("postgres")
            && descriptor.database_url.len() <= 16384
            && !descriptor.database_url.contains('\0'),
        "explicit bounded disposable bootstrap required"
    );
    ensure!(
        profile.get("kind").and_then(Value::as_str) == Some("windows-native-v1"),
        "explicit native execution required"
    );
    for path in [
        &descriptor.node.path,
        &descriptor.source.root,
        &descriptor.execution.path,
        &descriptor.destination,
        &descriptor.compiler,
        output,
        &descriptor.binaries.memory,
        &descriptor.binaries.snapshot,
        &descriptor.binaries.score,
        &descriptor.binaries.retriever,
    ] {
        ensure!(absolute(path), "explicit absolute bootstrap paths required");
    }
    ensure!(
        descriptor
            .node
            .path
            .file_name()
            .is_some_and(|v| v.eq_ignore_ascii_case("node.exe"))
            && descriptor
                .compiler
                .file_name()
                .is_some_and(|v| v.eq_ignore_ascii_case("rustc.exe"))
            && descriptor.node.path == path_string(profile, "/profile/node/path")?
            && profile
                .pointer("/profile/node/sha256")
                .and_then(Value::as_str)
                == Some(&descriptor.node.sha256),
        "reviewed native Node/compiler required"
    );
    for executable in [
        &descriptor.compiler,
        &descriptor.binaries.memory,
        &descriptor.binaries.snapshot,
        &descriptor.binaries.score,
        &descriptor.binaries.retriever,
    ] {
        ensure!(
            executable
                .extension()
                .is_some_and(|v| v.eq_ignore_ascii_case("exe")),
            "native executable required"
        );
        reviewed_file(profile, executable)?;
    }
    let io_root = path_string(profile, "/profile/io_root")?;
    let parent = descriptor
        .destination
        .parent()
        .context("fresh pack parent")?;
    let id = parent
        .file_name()
        .and_then(|v| v.to_str())
        .and_then(|v| uuid::Uuid::parse_str(v).ok())
        .context("fresh UUID required")?;
    ensure!(
        !id.is_nil()
            && parent.file_name().and_then(|v| v.to_str()) == Some(&id.to_string())
            && descriptor
                .destination
                .file_name()
                .is_some_and(|v| v == "pack")
            && parent.parent() == Some(io_root.as_path())
            && output.parent() == Some(io_root.as_path())
            && output != parent,
        "distinct fresh output and pack UUIDs below private I/O root required"
    );
    let output_id = output
        .file_name()
        .and_then(|v| v.to_str())
        .and_then(|v| uuid::Uuid::parse_str(v).ok())
        .context("fresh output UUID required")?;
    ensure!(
        !output_id.is_nil()
            && output.file_name().and_then(|v| v.to_str()) == Some(&output_id.to_string()),
        "canonical output UUID required"
    );
    ensure!(
        !descriptor.source.files.is_empty() && descriptor.source.files.len() <= 65536,
        "complete bounded source inventory required"
    );
    let script = descriptor.source.root.join("rehearse.mjs");
    ensure!(
        descriptor
            .source
            .files
            .iter()
            .filter(|file| file.path == script)
            .count()
            == 1,
        "bootstrap script must be in reviewed source inventory"
    );
    Ok(script)
}

pub fn run(file: &Path, expected: &str, output: &Path) -> Result<Receipt> {
    let (descriptor_pin, bytes) = pin(
        &BoundFile {
            path: file.to_owned(),
            sha256: expected.to_owned(),
        },
        DESCRIPTOR_LIMIT,
    )?;
    let descriptor: Descriptor = serde_json::from_slice(&bytes)?;
    let (execution_pin, execution_bytes) = pin(&descriptor.execution, DESCRIPTOR_LIMIT)?;
    let profile: Value = serde_json::from_slice(&execution_bytes)?;
    let script = validate(&descriptor, &profile, output)?;
    let helper: BoundFile = serde_json::from_value(
        profile
            .pointer("/profile/helper")
            .context("missing helper pin")?
            .clone(),
    )?;
    ensure!(
        helper.path == std::env::current_exe()?,
        "bootstrap must use the reviewed helper executable"
    );
    let (helper_pin, _) = pin(&helper, 512 * 1024 * 1024)?;
    let (node_pin, _) = pin(&descriptor.node, 512 * 1024 * 1024)?;
    let io_root = Directory::open(&path_string(&profile, "/profile/io_root")?)?;
    let source_root = Directory::open(&descriptor.source.root)?;
    ensure!(
        !source_root.is_beneath(&io_root)? && !io_root.is_beneath(&source_root)?,
        "source and output trees must be disjoint"
    );
    // This audited inventory guard rejects extra files, ADS, aliases and reparse
    // points and retains read-only file/directory handles throughout launch.
    let trees = [ToolchainTree {
        root: descriptor.source.root.clone(),
        files: descriptor
            .source
            .files
            .iter()
            .map(|file| {
                Ok(CodeFile {
                    path: file.path.clone(),
                    sha256: hash(&file.sha256)?,
                })
            })
            .collect::<Result<_>>()?,
    }];
    let source_pin = pin_trees(&trees, &io_root)?;
    create_private_directory(output)?;
    let output_pin = Directory::open(output)?;
    let stdout = output_pin.create("stdout.bin")?;
    let stderr = output_pin.create("stderr.bin")?;
    let mut system = vec![0u16; 32768];
    let count = unsafe {
        windows_sys::Win32::System::SystemInformation::GetWindowsDirectoryW(
            system.as_mut_ptr(),
            system.len() as u32,
        )
    } as usize;
    ensure!(
        count > 0 && count < system.len(),
        "Windows system directory unavailable"
    );
    use std::os::windows::ffi::OsStringExt;
    let system = OsString::from_wide(&system[..count]);
    let environment = vec![
        ("SystemRoot".into(), system.clone()),
        ("WINDIR".into(), system),
        ("PATH".into(), "".into()),
        ("HOME".into(), output.as_os_str().to_owned()),
        ("USERPROFILE".into(), output.as_os_str().to_owned()),
        ("TEMP".into(), output.as_os_str().to_owned()),
        ("TMP".into(), output.as_os_str().to_owned()),
        ("MEMORY_LAYER_PROFILE".into(), "prod".into()),
        ("MEMORY_LAYER_TEST_REQUIRE_DB".into(), "1".into()),
        (
            "MEMORY_LAYER_TEST_DATABASE_URL".into(),
            descriptor.database_url.clone().into(),
        ),
        (
            "MEMORY_R3_NATIVE_EXECUTION".into(),
            descriptor.execution.path.as_os_str().to_owned(),
        ),
        (
            "MEMORY_R3_NATIVE_COMPILER".into(),
            descriptor.compiler.as_os_str().to_owned(),
        ),
        (
            "MEMORY_R3_CLI_BIN".into(),
            descriptor.binaries.memory.as_os_str().to_owned(),
        ),
        (
            "MEMORY_R3_SNAPSHOT_BIN".into(),
            descriptor.binaries.snapshot.as_os_str().to_owned(),
        ),
        (
            "MEMORY_R3_SCORE_BIN".into(),
            descriptor.binaries.score.as_os_str().to_owned(),
        ),
        (
            "MEMORY_R3_RETRIEVER_BIN".into(),
            descriptor.binaries.retriever.as_os_str().to_owned(),
        ),
    ];
    let mut arguments = vec![
        script.into_os_string(),
        descriptor.destination.as_os_str().to_owned(),
        "--native".into(),
        "--prepare-only".into(),
    ];
    if descriptor.one_family_per_track {
        arguments.push("--one-family-per-track".into());
    }
    let process = windows_trusted_process::spawn(
        &descriptor.node.path,
        &arguments,
        output,
        &environment,
        &stdout,
        &stderr,
    )?;
    let deadline = Instant::now() + Duration::from_millis(descriptor.deadline_ms);
    let outcome = (|| -> Result<u32> {
        loop {
            ensure!(
                stdout.metadata()?.len() + stderr.metadata()?.len() <= OUTPUT_LIMIT,
                "bootstrap diagnostic limit exceeded"
            );
            if let Some(code) = process.try_wait()? {
                break Ok(code);
            }
            ensure!(Instant::now() < deadline, "bootstrap deadline exceeded");
            thread::sleep(Duration::from_millis(20));
        }
    })();
    process.terminate()?;
    let reaped = Instant::now() + Duration::from_secs(5);
    while process.active_processes()? != 0 {
        ensure!(Instant::now() < reaped, "bootstrap descendants not reaped");
        thread::sleep(Duration::from_millis(10));
    }
    drop(process);
    let code = outcome?;
    ensure!(
        stdout.metadata()?.len() + stderr.metadata()?.len() <= OUTPUT_LIMIT,
        "bootstrap diagnostic limit exceeded"
    );
    for input in [&descriptor_pin, &execution_pin, &helper_pin, &node_pin] {
        input.unchanged()?;
    }
    for (_, input) in &source_pin.files {
        input.unchanged()?;
    }
    ensure!(code == 0, "native bootstrap preparation did not succeed");
    drop((stdout, stderr));
    let stdout_bytes = Input::open(&output.join("stdout.bin"), OUTPUT_LIMIT)?.read(OUTPUT_LIMIT)?;
    let stderr_bytes = Input::open(&output.join("stderr.bin"), OUTPUT_LIMIT)?.read(OUTPUT_LIMIT)?;
    let last = std::str::from_utf8(&stdout_bytes)?
        .lines()
        .last()
        .context("missing bootstrap summary")?;
    let summary: Value = serde_json::from_str(last)?;
    ensure!(
        summary.get("prepared") == Some(&Value::Bool(true))
            && summary.get("native") == Some(&Value::Bool(true))
            && summary.get("official_evidence") == Some(&Value::Bool(false))
            && summary.get("execution_started") == Some(&Value::Bool(false))
            && summary.get("root").and_then(Value::as_str).map(Path::new)
                == Some(descriptor.destination.as_path()),
        "prepare-only bootstrap summary required"
    );
    Ok(Receipt {
        format: "memory-r3-native-bootstrap-prepare-result-v1",
        state: "prepared",
        descriptor_sha256: expected.to_owned(),
        node_sha256: descriptor.node.sha256,
        execution_sha256: descriptor.execution.sha256,
        source_files: source_pin.files.len(),
        stdout_sha256: digest(&stdout_bytes),
        stderr_sha256: digest(&stderr_bytes),
        destination: descriptor.destination,
        outer_job_reaped: true,
        official_evidence: false,
        execution_started: false,
    })
}

#[cfg(test)]
mod tests {
    use super::*;

    fn fixture() -> (Descriptor, Value, PathBuf) {
        let output = PathBuf::from("C:/private/runs/22222222-2222-4222-8222-222222222222");
        let descriptor = serde_json::json!({"schema_version":1,"profile":"memory-r3-native-bootstrap-prepare-v1",
            "node":{"path":"C:/private/tools/node.exe","sha256":"a".repeat(64)},
            "source":{"root":"C:/private/source/r3","files":[{"path":"C:/private/source/r3/rehearse.mjs","sha256":"b".repeat(64)}]},
            "execution":{"path":"C:/private/execution.json","sha256":"c".repeat(64)},
            "destination":"C:/private/runs/11111111-1111-4111-8111-111111111111/pack",
            "compiler":"C:/private/tools/rustc.exe",
            "binaries":{"memory":"C:/private/tools/memory.exe","snapshot":"C:/private/tools/r3_snapshot.exe",
                "score":"C:/private/tools/r3_score.exe","retriever":"C:/private/tools/r3_retriever.exe"},
            "database_url":"postgresql://synthetic@localhost/disposable","one_family_per_track":true,"deadline_ms":120000});
        let files: Vec<_> = ["rustc", "memory", "r3_snapshot", "r3_score", "r3_retriever"].iter()
            .map(|name| serde_json::json!({"path":format!("C:/private/tools/{name}.exe"),"sha256":"d".repeat(64)})).collect();
        let profile = serde_json::json!({"kind":"windows-native-v1","profile":{"node":{"path":"C:/private/tools/node.exe","sha256":"a".repeat(64)},
            "io_root":"C:/private/runs","toolchains":[{"files":files}]}});
        (serde_json::from_value(descriptor).unwrap(), profile, output)
    }

    #[test]
    fn native_bootstrap_descriptor_accepts_only_explicit_prepare_profile_and_paths() {
        let (mut descriptor, profile, output) = fixture();
        assert_eq!(
            validate(&descriptor, &profile, &output).unwrap(),
            descriptor.source.root.join("rehearse.mjs")
        );
        descriptor.deadline_ms = 120001;
        assert!(validate(&descriptor, &profile, &output).is_err());
        descriptor.deadline_ms = 1000;
        descriptor.destination = PathBuf::from("C:/public/pack");
        assert!(validate(&descriptor, &profile, &output).is_err());
        let (mut descriptor, profile, output) = fixture();
        descriptor.binaries.memory = PathBuf::from("C:/unreviewed/memory.exe");
        assert!(validate(&descriptor, &profile, &output).is_err());
        let (mut descriptor, profile, output) = fixture();
        descriptor.source.files.clear();
        assert!(validate(&descriptor, &profile, &output).is_err());
        let (descriptor, mut profile, output) = fixture();
        profile["kind"] = serde_json::json!("linux-container-v1");
        assert!(validate(&descriptor, &profile, &output).is_err());
        assert!(hash(&"A".repeat(64)).is_err());
    }

    #[test]
    fn native_bootstrap_descriptor_has_no_generic_arguments_or_provider_settings() {
        let json = serde_json::json!({"schema_version":1,"profile":"memory-r3-native-bootstrap-prepare-v1",
            "node":{"path":"C:/private/node.exe","sha256":"a".repeat(64)},"source":{"root":"C:/private/r3","files":[]},
            "execution":{"path":"C:/private/execution.json","sha256":"b".repeat(64)},"destination":"C:/private/pack",
            "compiler":"C:/private/rustc.exe","binaries":{"memory":"C:/private/memory.exe","snapshot":"C:/private/snapshot.exe",
                "score":"C:/private/score.exe","retriever":"C:/private/retriever.exe"},"database_url":"postgresql://synthetic",
            "one_family_per_track":true,"deadline_ms":1000});
        for name in ["arguments", "provider", "api_key", "environment", "shell"] {
            let mut value = json.clone();
            value[name] = serde_json::json!("never accepted");
            assert!(serde_json::from_value::<Descriptor>(value).is_err());
        }
    }

    #[test]
    fn native_configured_bootstrap_prepares_through_real_helper_outer_job() {
        let names = [
            "MEMORY_R3_BOOTSTRAP_HELPER",
            "MEMORY_R3_BOOTSTRAP_DESCRIPTOR",
            "MEMORY_R3_BOOTSTRAP_SHA256",
            "MEMORY_R3_BOOTSTRAP_OUTPUT",
        ];
        let configured = names.iter().all(|name| std::env::var_os(name).is_some());
        if std::env::var("MEMORY_R3_REQUIRE_NATIVE_BOOTSTRAP").as_deref() == Ok("1") {
            assert!(configured, "required native bootstrap fixture missing");
        }
        if !configured {
            return;
        }
        let result = std::process::Command::new(std::env::var_os(names[0]).unwrap())
            .arg("--bootstrap-prepare")
            .arg(std::env::var_os(names[1]).unwrap())
            .arg("--sha256")
            .arg(std::env::var_os(names[2]).unwrap())
            .arg("--output")
            .arg(std::env::var_os(names[3]).unwrap())
            .env_clear()
            .env(
                "SystemRoot",
                std::env::var_os("SystemRoot").unwrap_or_else(|| "C:\\Windows".into()),
            )
            .output()
            .unwrap();
        assert!(
            result.status.success(),
            "native bootstrap did not confirm preparation; retain private logs"
        );
        let receipt: Value = serde_json::from_slice(&result.stdout).unwrap();
        assert_eq!(
            receipt["format"],
            "memory-r3-native-bootstrap-prepare-result-v1"
        );
        assert_eq!(receipt["state"], "prepared");
        assert_eq!(receipt["outer_job_reaped"], true);
        assert_eq!(receipt["execution_started"], false);
        assert_eq!(receipt["official_evidence"], false);
    }
}
