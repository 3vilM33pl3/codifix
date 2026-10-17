// SPDX-License-Identifier: AGPL-3.0-or-later
//! Bounded native helper. Does not implement scoring or a campaign backend.
//! The descriptor and stdin file are operator-approved private inputs. Never put
//! hidden grader expectations in them or in the reviewed toolchain inventory.
pub mod bootstrap;
mod export;
pub mod host_journal;

use crate::{
    private_windows::{Directory, Input, create_private_directory},
    windows_evaluation::{self, EvaluationSpec, ToolchainTree},
    windows_process::CodeFile,
};
use anyhow::{Context, Result, ensure};
use serde::{Deserialize, Serialize};
use sha2::{Digest, Sha256};
use std::{
    ffi::OsString,
    fs::File,
    io::{Read, Write},
    os::windows::io::{AsHandle, FromRawHandle},
    path::{Path, PathBuf},
    ptr::null_mut,
    sync::{
        Arc,
        atomic::{AtomicBool, AtomicUsize, Ordering},
    },
    thread,
    time::{Duration, SystemTime, UNIX_EPOCH},
};
use windows_sys::Win32::System::Pipes::CreatePipe;

const INPUT_LIMIT: u64 = 32 * 1024 * 1024;
const DESCRIPTOR_LIMIT: u64 = 16 * 1024 * 1024;
const OUTPUT_LIMIT: usize = 4 * 1024 * 1024;

#[derive(Deserialize)]
#[serde(deny_unknown_fields)]
struct BoundFile {
    path: PathBuf,
    sha256: String,
}
#[derive(Deserialize)]
#[serde(deny_unknown_fields)]
struct BoundTree {
    root: PathBuf,
    files: Vec<BoundFile>,
}
#[derive(Deserialize)]
#[serde(deny_unknown_fields)]
struct Descriptor {
    schema_version: u32,
    profile: String,
    executable: PathBuf,
    toolchains: Vec<BoundTree>,
    arguments: Vec<String>,
    search_path: Vec<PathBuf>,
    library_path: Vec<PathBuf>,
    deadline_ms: u64,
    max_output_bytes: usize,
    stdin_file: Option<BoundFile>,
    exports: Vec<PathBuf>,
    #[serde(default)]
    allow_missing_exports: bool,
}

#[derive(Serialize)]
pub struct PreparedIo {
    format: &'static str,
    request_id: uuid::Uuid,
    directory: PathBuf,
    root_identity: (String, String),
    directory_identity: (String, String),
}
/// Validate an existing operator-owned root before any caller writes payloads.
/// All ancestors stay pinned until the exclusively created private child is
/// validated. This never creates, changes or relaxes the existing root's ACL.
pub fn prepare_io(root: &Path, request_id: uuid::Uuid) -> Result<PreparedIo> {
    ensure!(!request_id.is_nil(), "nonzero request identity required");
    let parent = Directory::open(root)?;
    let path = root.join(request_id.to_string());
    create_private_directory(&path)?;
    let child = Directory::open(&path)?;
    let identity = |directory: &Directory| -> Result<(String, String)> {
        let (volume, index) = directory.identity()?.stable_key();
        Ok((format!("{volume:x}"), format!("{index:x}")))
    };
    Ok(PreparedIo {
        format: "memory-windows-evaluation-io-v1",
        request_id,
        directory: path,
        root_identity: identity(&parent)?,
        directory_identity: identity(&child)?,
    })
}

#[derive(Serialize)]
pub struct OuterJob {
    format: &'static str,
    caller_pid: u32,
    helper_pid: u32,
    kill_on_close: bool,
    breakaway_allowed: bool,
}

/// Read-only preflight for trusted Node orchestration. Its spawned helpers and
/// services must inherit a non-breakaway kill-on-close Job owned by the outer
/// native campaign launcher. This does not create or modify any Job or ACL.
pub fn verify_outer_job(caller_pid: u32) -> Result<OuterJob> {
    use std::mem::size_of_val;
    use windows_sys::Win32::System::JobObjects::{
        JOB_OBJECT_LIMIT_BREAKAWAY_OK, JOB_OBJECT_LIMIT_KILL_ON_JOB_CLOSE,
        JOB_OBJECT_LIMIT_SILENT_BREAKAWAY_OK, JOBOBJECT_EXTENDED_LIMIT_INFORMATION,
        JobObjectBasicProcessIdList, JobObjectExtendedLimitInformation, QueryInformationJobObject,
    };
    ensure!(
        caller_pid != 0 && caller_pid != std::process::id(),
        "distinct caller process required"
    );
    let mut limits = JOBOBJECT_EXTENDED_LIMIT_INFORMATION::default();
    ensure!(
        unsafe {
            QueryInformationJobObject(
                null_mut(),
                JobObjectExtendedLimitInformation,
                (&mut limits as *mut JOBOBJECT_EXTENDED_LIMIT_INFORMATION).cast(),
                size_of_val(&limits) as u32,
                null_mut(),
            )
        } != 0,
        "trusted orchestration requires an outer Job"
    );
    let flags = limits.BasicLimitInformation.LimitFlags;
    ensure!(
        flags & JOB_OBJECT_LIMIT_KILL_ON_JOB_CLOSE != 0
            && flags & (JOB_OBJECT_LIMIT_BREAKAWAY_OK | JOB_OBJECT_LIMIT_SILENT_BREAKAWAY_OK) == 0,
        "outer Job must kill on close and forbid breakaway"
    );
    #[repr(C)]
    struct Members {
        assigned: u32,
        returned: u32,
        ids: [usize; 4096],
    }
    let mut members = Members {
        assigned: 0,
        returned: 0,
        ids: [0; 4096],
    };
    ensure!(
        unsafe {
            QueryInformationJobObject(
                null_mut(),
                JobObjectBasicProcessIdList,
                (&mut members as *mut Members).cast(),
                size_of_val(&members) as u32,
                null_mut(),
            )
        } != 0
            && members.assigned == members.returned
            && (2..=4096).contains(&members.returned),
        "outer Job process inventory unavailable or excessive"
    );
    let ids = &members.ids[..members.returned as usize];
    ensure!(
        ids.contains(&(caller_pid as usize)) && ids.contains(&(std::process::id() as usize)),
        "caller is not a member of the helper's outer Job"
    );
    Ok(OuterJob {
        format: "memory-windows-trusted-outer-job-v1",
        caller_pid,
        helper_pid: std::process::id(),
        kill_on_close: true,
        breakaway_allowed: false,
    })
}

#[derive(Serialize)]
pub struct Artifact {
    pub source: String,
    pub file: String,
    pub sha256: String,
    pub bytes: usize,
}
#[derive(Serialize)]
pub struct Receipt {
    format: &'static str,
    pub state: &'static str,
    reason: &'static str,
    profile: &'static str,
    architecture: &'static str,
    descriptor_sha256: String,
    descriptor_verified: bool,
    stdin_sha256: Option<String>,
    exit_code: Option<u32>,
    started_at_unix_ms: u128,
    completed_at_unix_ms: Option<u128>,
    disk_hard_limit_enforced: bool,
    artifacts: Vec<Artifact>,
}
fn now() -> Result<u128> {
    Ok(SystemTime::now().duration_since(UNIX_EPOCH)?.as_millis())
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
        "invalid input digest"
    );
    let mut result = [0; 32];
    for (slot, pair) in result.iter_mut().zip(value.as_bytes().as_chunks::<2>().0) {
        let digit = |b: u8| {
            if b.is_ascii_digit() {
                b - b'0'
            } else {
                b - b'a' + 10
            }
        };
        *slot = digit(pair[0]) * 16 + digit(pair[1]);
    }
    Ok(result)
}
fn bound_input(file: &BoundFile, limit: u64) -> Result<Vec<u8>> {
    hash(&file.sha256)?;
    let bytes = Input::open(&file.path, limit)?.read(limit)?;
    ensure!(
        digest(&bytes) == file.sha256,
        "private input digest changed"
    );
    Ok(bytes)
}
struct Budget {
    remaining: AtomicUsize,
    exceeded: AtomicBool,
}
impl Budget {
    fn new(bytes: usize) -> Self {
        Self {
            remaining: AtomicUsize::new(bytes),
            exceeded: AtomicBool::new(false),
        }
    }
    fn take(&self, bytes: usize) -> Result<()> {
        if self
            .remaining
            .try_update(Ordering::SeqCst, Ordering::SeqCst, |left| {
                left.checked_sub(bytes)
            })
            .is_err()
        {
            self.exceeded.store(true, Ordering::SeqCst);
            anyhow::bail!("native evaluation output limit exceeded");
        }
        Ok(())
    }
}
fn drain(mut pipe: File, budget: Arc<Budget>, mut partial: File) -> Result<Vec<u8>> {
    let mut result = Vec::new();
    let mut buffer = [0; 4096];
    loop {
        let read = pipe.read(&mut buffer)?;
        if read == 0 {
            return Ok(result);
        }
        budget.take(read)?;
        // Private bounded diagnostics remain inspectable even if a later Job
        // reap/cleanup error prevents final artifacts. These partial files are
        // never returned as validated exports or described as completed output.
        partial.write_all(&buffer[..read])?;
        result.extend_from_slice(&buffer[..read]);
    }
}
fn pipe() -> Result<(File, File)> {
    let mut reader = null_mut();
    let mut writer = null_mut();
    ensure!(
        unsafe { CreatePipe(&mut reader, &mut writer, std::ptr::null(), 4096) } != 0,
        "native evaluation pipe creation failed"
    );
    Ok(unsafe { (File::from_raw_handle(reader), File::from_raw_handle(writer)) })
}
fn save(root: &Directory, receipt: &Receipt) -> Result<()> {
    let mut file = root.create("result.json")?;
    file.write_all(&serde_json::to_vec(receipt)?)?;
    file.sync_all()?;
    Ok(())
}
fn artifact(root: &Directory, name: String, source: String, bytes: &[u8]) -> Result<Artifact> {
    let mut file = root.create(&name)?;
    file.write_all(bytes)?;
    file.sync_all()?;
    Ok(Artifact {
        source,
        file: name,
        sha256: digest(bytes),
        bytes: bytes.len(),
    })
}

/// The output directory must be fresh. Failure details stay metadata-only;
/// stdout/stderr and exported bytes are separate private bounded artifacts.
pub fn run(descriptor_path: &Path, expected_hash: &str, output: &Path) -> Result<Receipt> {
    hash(expected_hash)?;
    let _parent = Directory::open(output.parent().context("output parent missing")?)?;
    create_private_directory(output)?;
    let root = Directory::open(output)?;
    let mut receipt = Receipt {
        format: "memory-windows-evaluation-result-v1",
        state: "failed",
        reason: "preflight_failed",
        profile: "windows-appcontainer-evaluation-v1",
        architecture: std::env::consts::ARCH,
        descriptor_sha256: expected_hash.to_owned(),
        descriptor_verified: false,
        stdin_sha256: None,
        exit_code: None,
        started_at_unix_ms: now()?,
        completed_at_unix_ms: None,
        disk_hard_limit_enforced: false,
        artifacts: vec![],
    };
    if execute(descriptor_path, expected_hash, output, &root, &mut receipt).is_ok() {
        receipt.state = "succeeded";
        receipt.reason = "completed";
    }
    receipt.completed_at_unix_ms = Some(now()?);
    save(&root, &receipt)?;
    Ok(receipt)
}

fn execute(
    descriptor_path: &Path,
    expected_hash: &str,
    output: &Path,
    root: &Directory,
    receipt: &mut Receipt,
) -> Result<()> {
    let bytes = Input::open(descriptor_path, DESCRIPTOR_LIMIT)?.read(DESCRIPTOR_LIMIT)?;
    ensure!(digest(&bytes) == expected_hash, "descriptor hash changed");
    let descriptor: Descriptor = serde_json::from_slice(&bytes)?;
    ensure!(
        descriptor.schema_version == 1
            && descriptor.profile == "windows-appcontainer-evaluation-v1",
        "unsupported native evaluation profile"
    );
    ensure!(
        (1..=300_000).contains(&descriptor.deadline_ms)
            && (1..=OUTPUT_LIMIT).contains(&descriptor.max_output_bytes)
            && descriptor.exports.len() <= 128,
        "native evaluation limits rejected"
    );
    let mut selected = std::collections::BTreeSet::new();
    for path in &descriptor.exports {
        export::relative(path)?;
        ensure!(
            selected.insert(path.to_string_lossy().to_lowercase()),
            "duplicate export selection"
        );
    }
    receipt.descriptor_verified = true;
    let input = descriptor
        .stdin_file
        .as_ref()
        .map(|file| bound_input(file, INPUT_LIMIT))
        .transpose()?
        .unwrap_or_default();
    receipt.stdin_sha256 = descriptor
        .stdin_file
        .as_ref()
        .map(|file| file.sha256.clone());
    let toolchains = descriptor
        .toolchains
        .into_iter()
        .map(|tree| {
            Ok(ToolchainTree {
                root: tree.root,
                files: tree
                    .files
                    .into_iter()
                    .map(|file| {
                        Ok(CodeFile {
                            path: file.path,
                            sha256: hash(&file.sha256)?,
                        })
                    })
                    .collect::<Result<Vec<_>>>()?,
            })
        })
        .collect::<Result<Vec<_>>>()?;
    let scratch = output.join("scratch");
    create_private_directory(&scratch)?;
    let (stdin_read, mut stdin_write) = pipe()?;
    let (stdout_read, stdout_write) = pipe()?;
    let (stderr_read, stderr_write) = pipe()?;
    receipt.reason = "execution_failed";
    let mut child = windows_evaluation::spawn(EvaluationSpec {
        executable: descriptor.executable,
        toolchains,
        scratch: scratch.clone(),
        arguments: descriptor
            .arguments
            .into_iter()
            .map(OsString::from)
            .collect(),
        deadline: Duration::from_millis(descriptor.deadline_ms),
        search_path: descriptor.search_path,
        library_path: descriptor.library_path,
        stdin: stdin_read.as_handle(),
        stdout: stdout_write.as_handle(),
        stderr: stderr_write.as_handle(),
    })?;
    drop((stdin_read, stdout_write, stderr_write));
    let budget = Arc::new(Budget::new(descriptor.max_output_bytes));
    let stdout_partial = root.create("stdout.partial.bin")?;
    let stderr_partial = root.create("stderr.partial.bin")?;
    let stdout_budget = budget.clone();
    let stdout = thread::Builder::new()
        .name("eval-stdout".into())
        .spawn(move || drain(stdout_read, stdout_budget, stdout_partial))?;
    let stderr_budget = budget.clone();
    let stderr = thread::Builder::new()
        .name("eval-stderr".into())
        .spawn(move || drain(stderr_read, stderr_budget, stderr_partial))?;
    let stdin = thread::Builder::new()
        .name("eval-stdin".into())
        .spawn(move || stdin_write.write_all(&input))?;
    let result = loop {
        if budget.exceeded.load(Ordering::SeqCst) {
            receipt.reason = "output_limit";
            break Err(anyhow::anyhow!("output limit exceeded"));
        }
        match child.try_wait() {
            Ok(Some(code)) => {
                receipt.exit_code = Some(code);
                break Ok(code);
            }
            Ok(None) => thread::sleep(Duration::from_millis(10)),
            Err(_) => {
                receipt.reason = "deadline_or_cleanup_failed";
                break Err(anyhow::anyhow!("evaluation did not complete"));
            }
        }
    };
    // Never wait indefinitely for pipe threads if descendant reaping could not
    // be established. The helper fails with retained metadata; no export occurs.
    if result.is_err() {
        child.terminate()?;
    }
    drop(child); // Reap all descendants, restore grants, release toolchain pins.
    let input_result = stdin
        .join()
        .map_err(|_| anyhow::anyhow!("stdin thread failed"))?;
    let stdout_result = stdout
        .join()
        .map_err(|_| anyhow::anyhow!("stdout thread failed"))?;
    let stderr_result = stderr
        .join()
        .map_err(|_| anyhow::anyhow!("stderr thread failed"))?;
    if budget.exceeded.load(Ordering::SeqCst) {
        receipt.reason = "output_limit";
    }
    if let Ok(bytes) = &stdout_result {
        receipt
            .artifacts
            .push(artifact(root, "stdout.bin".into(), "stdout".into(), bytes)?);
    }
    if let Ok(bytes) = &stderr_result {
        receipt
            .artifacts
            .push(artifact(root, "stderr.bin".into(), "stderr".into(), bytes)?);
    }
    ensure!(result? == 0, "evaluation exited unsuccessfully");
    input_result?;
    stdout_result?;
    stderr_result?;
    receipt.reason = "export_rejected";
    let _scratch = Directory::open(&scratch)?;
    let mut exported_identities = std::collections::BTreeSet::new();
    for (index, path) in descriptor.exports.iter().enumerate() {
        let collected = match export::read(&scratch, path, budget.remaining.load(Ordering::SeqCst))
        {
            Err(error)
                if descriptor.allow_missing_exports
                    && error
                        .downcast_ref::<std::io::Error>()
                        .is_some_and(|error| error.kind() == std::io::ErrorKind::NotFound) =>
            {
                continue;
            }
            result => result?,
        };
        ensure!(
            exported_identities.insert(collected.identity),
            "aliased export selection"
        );
        let bytes = collected.bytes;
        budget.take(bytes.len())?;
        receipt.artifacts.push(artifact(
            root,
            format!("export-{index:03}.bin"),
            path.to_string_lossy().into_owned(),
            &bytes,
        )?);
    }
    Ok(())
}

#[cfg(test)]
mod tests {
    use super::*;
    #[test]
    fn native_partial_diagnostics_are_private_and_respect_the_shared_budget() {
        for (limit, succeeds) in [(8, true), (5, false)] {
            let path = std::env::temp_dir().join(format!("eval-drain-{}", uuid::Uuid::new_v4()));
            create_private_directory(&path).unwrap();
            let root = Directory::open(&path).unwrap();
            let (reader, mut writer) = pipe().unwrap();
            writer.write_all(b"abcdef").unwrap();
            drop(writer);
            let result = drain(
                reader,
                Arc::new(Budget::new(limit)),
                root.create("partial.bin").unwrap(),
            );
            assert_eq!(result.is_ok(), succeeds);
            let bytes = Input::open(&path.join("partial.bin"), 8)
                .unwrap()
                .read(8)
                .unwrap();
            assert_eq!(bytes, if succeeds { b"abcdef".as_slice() } else { b"" });
            drop(root);
            std::fs::remove_file(path.join("partial.bin")).unwrap();
            std::fs::remove_dir(path).unwrap();
        }
    }

    #[test]
    fn native_output_budget_is_shared_and_never_wraps() {
        let budget = Arc::new(Budget::new(10));
        let first = budget.clone();
        thread::spawn(move || first.take(6))
            .join()
            .unwrap()
            .unwrap();
        assert!(budget.take(5).is_err());
        assert_eq!(budget.remaining.load(Ordering::SeqCst), 4);
        assert!(budget.exceeded.load(Ordering::SeqCst));
        assert!(hash(&"a".repeat(64)).is_ok());
        assert!(hash(&"A".repeat(64)).is_err());
    }
    #[test]
    fn native_descriptor_uses_portable_string_arguments_not_osstring_enum() {
        let mut value = serde_json::json!({"schema_version":1,"profile":"windows-appcontainer-evaluation-v1",
            "executable":"C:/private/driver.exe","toolchains":[],"arguments":["with spaces", "Unicode café"],
            "search_path":[],"library_path":[],"deadline_ms":1000,"max_output_bytes":4096,"stdin_file":null,"exports":[]});
        let parsed: Descriptor = serde_json::from_value(value.clone()).unwrap();
        assert_eq!(parsed.arguments, ["with spaces", "Unicode café"]);
        assert!(!parsed.allow_missing_exports);
        value["shell"] = serde_json::json!("never accepted");
        assert!(serde_json::from_value::<Descriptor>(value).is_err());
    }
    #[test]
    fn native_prepare_io_requires_existing_private_root_and_unique_identity() {
        let base = std::env::temp_dir().join(format!("memory-eval-io-{}", uuid::Uuid::new_v4()));
        create_private_directory(&base).unwrap();
        let id = uuid::Uuid::new_v4();
        let prepared = prepare_io(&base, id).unwrap();
        assert_eq!(prepared.directory, base.join(id.to_string()));
        assert!(Directory::open(&prepared.directory).is_ok());
        assert!(prepare_io(&base, id).is_err());
        assert!(prepare_io(&base.join("missing"), uuid::Uuid::new_v4()).is_err());
        assert!(prepare_io(&base, uuid::Uuid::nil()).is_err());
        let status = std::process::Command::new("icacls.exe")
            .arg(&base)
            .args(["/grant", "*S-1-1-0:R"])
            .status()
            .unwrap();
        assert!(status.success());
        assert!(prepare_io(&base, uuid::Uuid::new_v4()).is_err());
        std::fs::remove_dir_all(base).unwrap();
    }
    #[test]
    fn native_outer_job_fixture_process() {
        match std::env::var("MEMORY_OUTER_JOB_FIXTURE").as_deref() {
            Ok("root") => {
                let status = std::process::Command::new(std::env::current_exe().unwrap())
                    .args([
                        "--exact",
                        "windows_evaluation_helper::tests::native_outer_job_fixture_process",
                        "--nocapture",
                    ])
                    .env_clear()
                    .env("MEMORY_OUTER_JOB_FIXTURE", "leaf")
                    .env("MEMORY_OUTER_JOB_CALLER", std::process::id().to_string())
                    .status()
                    .unwrap();
                assert!(status.success());
            }
            Ok("leaf") => {
                let caller = std::env::var("MEMORY_OUTER_JOB_CALLER")
                    .unwrap()
                    .parse()
                    .unwrap();
                let receipt = verify_outer_job(caller).unwrap();
                assert!(receipt.kill_on_close);
                assert!(!receipt.breakaway_allowed);
                assert_eq!(receipt.caller_pid, caller);
                assert_eq!(receipt.helper_pid, std::process::id());
                assert!(verify_outer_job(u32::MAX).is_err());
                assert!(verify_outer_job(0).is_err());
                assert!(verify_outer_job(std::process::id()).is_err());
                std::fs::write("verified.json", serde_json::to_vec(&receipt).unwrap()).unwrap();
            }
            _ => {}
        }
    }
    #[test]
    fn native_outer_job_verifies_exact_caller_membership_without_changing_job() {
        let base = std::env::temp_dir().join(format!("memory-outer-job-{}", uuid::Uuid::new_v4()));
        create_private_directory(&base).unwrap();
        let stdout = File::create(base.join("stdout")).unwrap();
        let stderr = File::create(base.join("stderr")).unwrap();
        let process = crate::windows_trusted_process::spawn(
            &std::env::current_exe().unwrap(),
            &[
                "--exact".into(),
                "windows_evaluation_helper::tests::native_outer_job_fixture_process".into(),
                "--nocapture".into(),
            ],
            &base,
            &[("MEMORY_OUTER_JOB_FIXTURE".into(), "root".into())],
            &stdout,
            &stderr,
        )
        .unwrap();
        let deadline = std::time::Instant::now() + Duration::from_secs(15);
        loop {
            if let Some(code) = process.try_wait().unwrap() {
                assert_eq!(code, 0);
                break;
            }
            assert!(std::time::Instant::now() < deadline);
            thread::sleep(Duration::from_millis(10));
        }
        let value: serde_json::Value =
            serde_json::from_slice(&std::fs::read(base.join("verified.json")).unwrap()).unwrap();
        assert_eq!(value["format"], "memory-windows-trusted-outer-job-v1");
        while process.active_processes().unwrap() != 0 {
            assert!(std::time::Instant::now() < deadline);
            thread::sleep(Duration::from_millis(10));
        }
        drop(process);
        drop((stdout, stderr));
        std::fs::remove_dir_all(base).unwrap();
    }
    #[test]
    fn native_preflight_failure_retains_private_metadata_without_payloads() {
        let base =
            std::env::temp_dir().join(format!("memory-eval-helper-{}", uuid::Uuid::new_v4()));
        create_private_directory(&base).unwrap();
        let path = base.join("descriptor.json");
        let payload = b"{\"secret\":\"not-an-operational-listing\"}";
        std::fs::write(&path, payload).unwrap();
        let output = base.join("result");
        let receipt = run(&path, &digest(payload), &output).unwrap();
        assert_eq!(receipt.state, "failed");
        assert!(!receipt.descriptor_verified);
        let metadata = std::fs::read_to_string(output.join("result.json")).unwrap();
        assert!(!metadata.contains("not-an-operational-listing"));
        assert!(run(&path, &digest(payload), &output).is_err());
        assert_eq!(
            std::fs::read_to_string(output.join("result.json")).unwrap(),
            metadata
        );
        let input = BoundFile {
            path,
            sha256: digest(payload),
        };
        assert_eq!(bound_input(&input, 1024).unwrap(), payload);
        assert!(bound_input(&input, 2).is_err());
        std::fs::remove_dir_all(base).unwrap();
    }
}
