// SPDX-License-Identifier: AGPL-3.0-or-later
//! Native evaluation launch seam, not campaign or output-export acceptance.
//! The trusted driver supplies initial workspace bytes over stdin. Only complete
//! private, digest-bound toolchain trees receive RX; fresh scratch receives RWX.
//! Hidden tests, provider credentials and other projects receive no grants.
//! There is no broker, network capability, ambient environment or disk quota.
use crate::windows_process::{
    Attributes, CodeFile, Grant, Profile, SandboxProcess, job_with_limits, ok, quote, verify_token,
    wide,
};
use anyhow::{Context, Result, ensure};
use mem_platform_files::windows::{Directory, Input};
use sha2::{Digest, Sha256};
use std::{
    collections::{BTreeMap, BTreeSet},
    ffi::{OsStr, OsString},
    io::Read,
    mem::size_of,
    os::windows::{
        ffi::OsStringExt,
        fs::MetadataExt,
        io::{AsRawHandle, BorrowedHandle, FromRawHandle, OwnedHandle},
    },
    path::PathBuf,
    ptr::{null, null_mut},
    sync::mpsc,
    thread::JoinHandle,
    time::{Duration, Instant},
};
use windows_sys::Win32::{
    Foundation::{
        DUPLICATE_SAME_ACCESS, DuplicateHandle, ERROR_HANDLE_EOF, GetLastError,
        INVALID_HANDLE_VALUE,
    },
    Security::{
        CONTAINER_INHERIT_ACE, INHERIT_ONLY_ACE, OBJECT_INHERIT_ACE, SECURITY_CAPABILITIES,
    },
    Storage::FileSystem::{
        DELETE, FILE_ATTRIBUTE_REPARSE_POINT, FILE_DELETE_CHILD, FILE_GENERIC_EXECUTE,
        FILE_GENERIC_READ, FILE_GENERIC_WRITE, FILE_TRAVERSE, FILE_TYPE_PIPE, FindClose,
        FindFirstStreamW, FindNextStreamW, FindStreamInfoStandard, GetFileType,
        WIN32_FIND_STREAM_DATA,
    },
    System::{
        JobObjects::{AssignProcessToJobObject, TerminateJobObject},
        SystemInformation::GetWindowsDirectoryW,
        Threading::{
            CREATE_NO_WINDOW, CREATE_SUSPENDED, CREATE_UNICODE_ENVIRONMENT, CreateProcessW,
            EXTENDED_STARTUPINFO_PRESENT, GetCurrentProcess, PROC_THREAD_ATTRIBUTE_HANDLE_LIST,
            PROC_THREAD_ATTRIBUTE_SECURITY_CAPABILITIES, PROCESS_INFORMATION, ResumeThread,
            STARTF_USESTDHANDLES, STARTUPINFOEXW,
        },
    },
};

const MAX_FILES: usize = 65_536;
const MAX_BYTES: u64 = 8 * 1024 * 1024 * 1024;
const MAX_DIRECTORIES: usize = 16_384;
/// These are evaluation-only limits. Pi retains its two-process/512MiB profile.
pub const PROCESS_LIMIT: u32 = 16;
pub const MEMORY_LIMIT: usize = 2 * 1024 * 1024 * 1024;
pub const DISK_HARD_LIMIT_ENFORCED: bool = false;

pub struct ToolchainTree {
    pub root: PathBuf,
    /// Complete regular-file inventory, including runtime libraries and scripts.
    /// Paths are absolute; every entry must be beneath this one dedicated root.
    pub files: Vec<CodeFile>,
}
pub struct EvaluationSpec<'a> {
    pub executable: PathBuf,
    pub toolchains: Vec<ToolchainTree>,
    pub scratch: PathBuf,
    pub arguments: Vec<OsString>,
    /// Real wall-clock deadline, independently of any semantic benchmark time.
    pub deadline: Duration,
    /// Exact directories in the reviewed trees; no ambient PATH/LIB inheritance.
    pub search_path: Vec<PathBuf>,
    pub library_path: Vec<PathBuf>,
    pub stdin: BorrowedHandle<'a>,
    pub stdout: BorrowedHandle<'a>,
    pub stderr: BorrowedHandle<'a>,
}

pub struct EvaluationProcess {
    child: SandboxProcess,
    deadline: Instant,
    cancel_deadline: mpsc::Sender<()>,
    deadline_thread: Option<JoinHandle<()>>,
}
impl EvaluationProcess {
    fn stop_deadline(&mut self) {
        let _ = self.cancel_deadline.send(());
        if let Some(thread) = self.deadline_thread.take() {
            let _ = thread.join();
        }
    }
    pub fn try_wait(&mut self) -> Result<Option<u32>> {
        if Instant::now() >= self.deadline {
            self.child.terminate()?;
            anyhow::bail!("native evaluation deadline expired");
        }
        let result = self.child.try_wait();
        if matches!(result, Ok(Some(_))) {
            self.stop_deadline();
        }
        result
    }
    pub fn wait(&mut self) -> Result<u32> {
        if Instant::now() >= self.deadline {
            self.child.terminate()?;
            anyhow::bail!("native evaluation deadline expired");
        }
        let result = self
            .child
            .wait(self.deadline.saturating_duration_since(Instant::now()));
        if result.is_ok() {
            self.stop_deadline();
        }
        result
    }
    pub fn terminate(&mut self) -> Result<()> {
        let result = self.child.terminate();
        if result.is_ok() {
            self.stop_deadline();
        }
        result
    }
}
impl Drop for EvaluationProcess {
    fn drop(&mut self) {
        self.stop_deadline();
    }
}

pub(crate) struct Pinned {
    pub(crate) files: Vec<(PathBuf, Input)>,
    directories: Vec<(PathBuf, Directory)>,
}
fn no_alternate_streams(path: &std::path::Path) -> Result<()> {
    let mut data = WIN32_FIND_STREAM_DATA::default();
    let handle = unsafe {
        FindFirstStreamW(
            wide(path.as_os_str())?.as_ptr(),
            FindStreamInfoStandard,
            (&mut data as *mut WIN32_FIND_STREAM_DATA).cast(),
            0,
        )
    };
    if handle == INVALID_HANDLE_VALUE {
        ensure!(
            unsafe { GetLastError() } == ERROR_HANDLE_EOF,
            "toolchain stream enumeration failed"
        );
        return Ok(());
    }
    struct Search(windows_sys::Win32::Foundation::HANDLE);
    impl Drop for Search {
        fn drop(&mut self) {
            unsafe {
                FindClose(self.0);
            }
        }
    }
    let search = Search(handle);
    loop {
        let count = data
            .cStreamName
            .iter()
            .position(|c| *c == 0)
            .context("invalid stream name")?;
        ensure!(
            String::from_utf16(&data.cStreamName[..count])? == "::$DATA",
            "unreviewed toolchain alternate data stream"
        );
        if unsafe { FindNextStreamW(search.0, (&mut data as *mut WIN32_FIND_STREAM_DATA).cast()) }
            == 0
        {
            ensure!(
                unsafe { GetLastError() } == ERROR_HANDLE_EOF,
                "toolchain stream enumeration interrupted"
            );
            return Ok(());
        }
    }
}
pub(crate) fn pin_trees(trees: &[ToolchainTree], scratch: &Directory) -> Result<Pinned> {
    ensure!(
        (1..=8).contains(&trees.len()),
        "evaluation toolchain count exceeded"
    );
    let mut pinned = Pinned {
        files: vec![],
        directories: vec![],
    };
    let mut file_ids = BTreeSet::new();
    let mut dir_ids = BTreeSet::new();
    let mut total = 0u64;
    for tree in trees {
        ensure!(
            !tree.files.is_empty() && tree.files.len() <= MAX_FILES - pinned.files.len(),
            "toolchain manifest file limit exceeded"
        );
        let root = Directory::open(&tree.root)?;
        ensure!(
            !root.is_beneath(scratch)? && !scratch.is_beneath(&root)?,
            "toolchain and scratch overlap"
        );
        let mut expected = BTreeMap::new();
        for file in &tree.files {
            ensure!(
                file.path != tree.root && file.path.starts_with(&tree.root),
                "toolchain file outside its root"
            );
            ensure!(
                expected.insert(file.path.clone(), file.sha256).is_none(),
                "duplicate toolchain path"
            );
        }
        let mut pending = vec![(tree.root.clone(), root)];
        while let Some((path, directory)) = pending.pop() {
            ensure!(
                pinned.directories.len() < MAX_DIRECTORIES,
                "toolchain directory limit exceeded"
            );
            ensure!(
                dir_ids.insert(directory.identity()?.stable_key()),
                "aliased toolchain directory"
            );
            no_alternate_streams(&path)?;
            for entry in std::fs::read_dir(&path)? {
                let entry = entry?;
                let metadata = std::fs::symlink_metadata(entry.path())?;
                ensure!(
                    metadata.file_attributes() & FILE_ATTRIBUTE_REPARSE_POINT == 0,
                    "toolchain reparse point rejected"
                );
                if metadata.is_dir() {
                    ensure!(
                        pending.len() + pinned.directories.len() < MAX_DIRECTORIES,
                        "toolchain directory limit exceeded"
                    );
                    pending.push((entry.path(), Directory::open(&entry.path())?));
                } else {
                    ensure!(
                        metadata.is_file() && pinned.files.len() < MAX_FILES,
                        "toolchain file limit or special entry rejected"
                    );
                    let digest = expected
                        .remove(&entry.path())
                        .context("unreviewed toolchain file")?;
                    let mut input = Input::open(&entry.path(), MAX_BYTES)?;
                    no_alternate_streams(&entry.path())?;
                    ensure!(
                        file_ids.insert(input.identity().stable_key()),
                        "aliased toolchain file"
                    );
                    let mut hash = Sha256::new();
                    let mut buffer = [0; 65_536];
                    loop {
                        let count = input.file.read(&mut buffer)?;
                        if count == 0 {
                            break;
                        }
                        total = total
                            .checked_add(count as u64)
                            .context("toolchain size overflow")?;
                        ensure!(total <= MAX_BYTES, "toolchain byte limit exceeded");
                        hash.update(&buffer[..count]);
                    }
                    ensure!(
                        hash.finalize().as_slice() == digest,
                        "toolchain digest mismatch"
                    );
                    input.unchanged()?;
                    pinned.files.push((entry.path(), input));
                }
            }
            pinned.directories.push((path, directory));
        }
        ensure!(
            expected.is_empty(),
            "toolchain manifest contains missing files"
        );
    }
    Ok(pinned)
}

/// Launch a pinned trusted evaluation driver in a zero-capability AppContainer.
/// The driver may create/execute workspace files, unlike Pi. Caller owns bounded
/// pipe draining; a host deadline thread terminates the whole Job even if the
/// caller stops polling. Cancellation/drop also target all descendants.
/// Output export is intentionally NOT provided by this seam: never blindly copy
/// scratch into a grader or treat this API as a validated full evaluation run.
pub fn spawn(spec: EvaluationSpec<'_>) -> Result<EvaluationProcess> {
    ensure!(
        spec.deadline >= Duration::from_millis(1) && spec.deadline <= Duration::from_secs(300),
        "evaluation deadline outside bounds"
    );
    let deadline = Instant::now() + spec.deadline;
    ensure!(
        spec.arguments.len() <= 128
            && spec.search_path.len() <= 32
            && spec.library_path.len() <= 32,
        "evaluation launch limits exceeded"
    );
    let scratch = Directory::open(&spec.scratch)?;
    ensure!(
        std::fs::read_dir(&spec.scratch)?.next().is_none(),
        "evaluation scratch must start empty"
    );
    let pinned = pin_trees(&spec.toolchains, &scratch)?;
    ensure!(
        pinned
            .files
            .iter()
            .any(|(path, _)| *path == spec.executable),
        "evaluation executable is not in reviewed inventory"
    );
    for path in spec.search_path.iter().chain(&spec.library_path) {
        ensure!(
            pinned.directories.iter().any(|(known, _)| known == path),
            "evaluation search directory is not pinned"
        );
    }
    let profile = Profile::create()?;
    let mut grants = Vec::new();
    for (path, _) in &pinned.directories {
        grants.push(Grant::apply(
            path,
            None,
            profile.sid,
            &[(FILE_GENERIC_READ | FILE_GENERIC_EXECUTE, 0)],
        )?);
    }
    for (path, input) in &pinned.files {
        grants.push(Grant::apply(
            path,
            Some(&input.file),
            profile.sid,
            &[(FILE_GENERIC_READ | FILE_GENERIC_EXECUTE, 0)],
        )?);
    }
    let rwx = FILE_GENERIC_READ | FILE_GENERIC_WRITE | FILE_GENERIC_EXECUTE | DELETE;
    grants.push(Grant::apply(
        &spec.scratch,
        None,
        profile.sid,
        &[
            (rwx | FILE_TRAVERSE | FILE_DELETE_CHILD, 0),
            (rwx, OBJECT_INHERIT_ACE | INHERIT_ONLY_ACE),
            (
                rwx | FILE_TRAVERSE | FILE_DELETE_CHILD,
                CONTAINER_INHERIT_ACE | INHERIT_ONLY_ACE,
            ),
        ],
    )?);
    grants
        .last_mut()
        .context("missing workspace grant")?
        .scratch_integrity()?;
    let mut inherited = Vec::new();
    for original in [spec.stdin, spec.stdout, spec.stderr] {
        ensure!(
            unsafe { GetFileType(original.as_raw_handle()) } == FILE_TYPE_PIPE,
            "evaluation stdio must be explicit pipes"
        );
        let mut raw = null_mut();
        ok(
            unsafe {
                DuplicateHandle(
                    GetCurrentProcess(),
                    original.as_raw_handle(),
                    GetCurrentProcess(),
                    &mut raw,
                    0,
                    1,
                    DUPLICATE_SAME_ACCESS,
                )
            },
            "duplicate evaluation pipe",
        )?;
        inherited.push(unsafe { OwnedHandle::from_raw_handle(raw) });
    }
    let handles = [
        inherited[0].as_raw_handle(),
        inherited[1].as_raw_handle(),
        inherited[2].as_raw_handle(),
    ];
    let capabilities = SECURITY_CAPABILITIES {
        AppContainerSid: profile.sid,
        ..Default::default()
    };
    let mut attributes = Attributes::new()?;
    attributes.set(PROC_THREAD_ATTRIBUTE_SECURITY_CAPABILITIES, &capabilities)?;
    attributes.set(PROC_THREAD_ATTRIBUTE_HANDLE_LIST, &handles)?;
    let mut startup = STARTUPINFOEXW::default();
    startup.StartupInfo.cb = size_of::<STARTUPINFOEXW>() as u32;
    startup.StartupInfo.dwFlags = STARTF_USESTDHANDLES;
    startup.StartupInfo.hStdInput = handles[0];
    startup.StartupInfo.hStdOutput = handles[1];
    startup.StartupInfo.hStdError = handles[2];
    startup.lpAttributeList = attributes.pointer();
    let executable = wide(spec.executable.as_os_str())?;
    let mut command = quote(spec.executable.as_os_str())?;
    for argument in spec.arguments {
        command.push(" ");
        command.push(quote(&argument)?);
    }
    let mut command = wide(&command)?;
    ensure!(command.len() <= 32_767, "evaluation command too long");
    let directory = wide(spec.scratch.as_os_str())?;
    let mut windows = vec![0u16; 32_768];
    let count =
        unsafe { GetWindowsDirectoryW(windows.as_mut_ptr(), windows.len() as u32) } as usize;
    ensure!(
        count > 0 && count < windows.len(),
        "Windows directory unavailable"
    );
    let windows = OsString::from_wide(&windows[..count]);
    let local = crate::windows_process::local_app_data()?;
    let path = std::env::join_paths(&spec.search_path)?;
    let libraries = std::env::join_paths(&spec.library_path)?;
    let environment = [
        ("HOME", spec.scratch.as_os_str()),
        ("LIB", libraries.as_os_str()),
        ("LOCALAPPDATA", local.as_os_str()),
        ("PATH", path.as_os_str()),
        ("SystemRoot", windows.as_os_str()),
        ("TEMP", spec.scratch.as_os_str()),
        ("TMP", spec.scratch.as_os_str()),
        ("USERPROFILE", spec.scratch.as_os_str()),
    ];
    let mut env = Vec::new();
    for (key, value) in environment {
        env.extend(wide(OsStr::new(key))?.into_iter().filter(|c| *c != 0));
        env.push(b'=' as u16);
        env.extend(wide(value)?);
    }
    env.push(0);
    let job = job_with_limits(PROCESS_LIMIT, MEMORY_LIMIT)?;
    ensure!(
        Instant::now() < deadline,
        "evaluation preparation exceeded deadline"
    );
    let mut info = PROCESS_INFORMATION::default();
    ok(
        unsafe {
            CreateProcessW(
                executable.as_ptr(),
                command.as_mut_ptr(),
                null(),
                null(),
                1,
                CREATE_SUSPENDED
                    | CREATE_UNICODE_ENVIRONMENT
                    | CREATE_NO_WINDOW
                    | EXTENDED_STARTUPINFO_PRESENT,
                env.as_ptr().cast(),
                directory.as_ptr(),
                &startup.StartupInfo,
                &mut info,
            )
        },
        "create evaluation AppContainer",
    )?;
    let process = unsafe { OwnedHandle::from_raw_handle(info.hProcess) };
    let thread = unsafe { OwnedHandle::from_raw_handle(info.hThread) };
    let mut directories: Vec<_> = pinned
        .directories
        .into_iter()
        .map(|(_, directory)| directory)
        .collect();
    directories.push(scratch);
    let child = SandboxProcess {
        job,
        process,
        profile: Some(profile),
        grants,
        inputs: pinned.files.into_iter().map(|(_, input)| input).collect(),
        directories,
        pid: info.dwProcessId,
    };
    verify_token(&child.process, capabilities.AppContainerSid)?;
    ok(
        unsafe {
            AssignProcessToJobObject(child.job.as_raw_handle(), child.process.as_raw_handle())
        },
        "fence evaluation AppContainer",
    )?;
    ensure!(
        unsafe { ResumeThread(thread.as_raw_handle()) } == 1,
        "could not resume evaluation AppContainer"
    );
    let mut raw = null_mut();
    ok(
        unsafe {
            DuplicateHandle(
                GetCurrentProcess(),
                child.job.as_raw_handle(),
                GetCurrentProcess(),
                &mut raw,
                0,
                0,
                DUPLICATE_SAME_ACCESS,
            )
        },
        "duplicate evaluation deadline job",
    )?;
    let deadline_job = unsafe { OwnedHandle::from_raw_handle(raw) };
    let (cancel_deadline, receiver) = mpsc::channel();
    let deadline_thread = std::thread::Builder::new()
        .name("memory-eval-deadline".into())
        .spawn(move || {
            if receiver.recv_timeout(deadline.saturating_duration_since(Instant::now()))
                == Err(mpsc::RecvTimeoutError::Timeout)
            {
                unsafe {
                    TerminateJobObject(deadline_job.as_raw_handle(), 1);
                }
            }
        })?;
    Ok(EvaluationProcess {
        child,
        deadline,
        cancel_deadline,
        deadline_thread: Some(deadline_thread),
    })
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::io::Write;

    struct Fixture(PathBuf);
    impl Fixture {
        fn new() -> Self {
            let root =
                std::env::temp_dir().join(format!("memory-native-eval-{}", uuid::Uuid::new_v4()));
            mem_platform_files::windows::create_private_directory(&root).unwrap();
            for name in ["tool", "tool/bin", "scratch"] {
                mem_platform_files::windows::create_private_directory(&root.join(name)).unwrap();
            }
            let mut file =
                mem_platform_files::windows::create_private_file(&root.join("tool/bin/driver.exe"))
                    .unwrap();
            file.write_all(b"synthetic-not-executable").unwrap();
            Self(root)
        }
        fn tree(&self) -> ToolchainTree {
            ToolchainTree {
                root: self.0.join("tool"),
                files: vec![CodeFile {
                    path: self.0.join("tool/bin/driver.exe"),
                    sha256: Sha256::digest(b"synthetic-not-executable").into(),
                }],
            }
        }
        fn pin(&self, tree: ToolchainTree) -> Result<Pinned> {
            pin_trees(&[tree], &Directory::open(&self.0.join("scratch"))?)
        }
    }
    impl Drop for Fixture {
        fn drop(&mut self) {
            let _ = std::fs::remove_dir_all(&self.0);
        }
    }
    #[test]
    fn native_recursive_toolchain_inventory_is_complete_and_hash_bound() {
        let fixture = Fixture::new();
        let pinned = fixture.pin(fixture.tree()).unwrap();
        assert_eq!(pinned.files.len(), 1);
        assert_eq!(pinned.directories.len(), 2);
        assert!(
            std::fs::OpenOptions::new()
                .write(true)
                .open(fixture.0.join("tool/bin/driver.exe"))
                .is_err()
        );
        drop(pinned);
        let mut wrong = fixture.tree();
        wrong.files[0].sha256 = [0; 32];
        assert!(fixture.pin(wrong).is_err());
        std::fs::write(fixture.0.join("tool/unreviewed"), b"not reviewed").unwrap();
        assert!(fixture.pin(fixture.tree()).is_err());
    }
    #[test]
    fn native_toolchain_rejects_aliases_public_acl_and_workspace_overlap() {
        let fixture = Fixture::new();
        let file = fixture.0.join("tool/bin/driver.exe");
        let alias = fixture.0.join("tool/bin/alias.exe");
        std::fs::hard_link(&file, &alias).unwrap();
        let mut tree = fixture.tree();
        tree.files.push(CodeFile {
            path: alias.clone(),
            sha256: tree.files[0].sha256,
        });
        assert!(fixture.pin(tree).is_err());
        std::fs::remove_file(alias).unwrap();
        let tree = fixture.tree();
        let root = Directory::open(&fixture.0.join("tool")).unwrap();
        assert!(pin_trees(&[tree], &root).is_err());
        drop(root);
        let status = std::process::Command::new("icacls.exe")
            .arg(&file)
            .args(["/grant", "*S-1-1-0:R"])
            .status()
            .unwrap();
        assert!(status.success());
        assert!(fixture.pin(fixture.tree()).is_err());
    }
    #[test]
    fn native_toolchain_rejects_junction_inventory() {
        let fixture = Fixture::new();
        // cmd's built-ins interpret embedded forward slashes as switches.
        let junction = fixture.0.join("tool").join("alias");
        let output = std::process::Command::new(
            PathBuf::from(std::env::var_os("SystemRoot").unwrap())
                .join("System32")
                .join("cmd.exe"),
        )
        .args(["/d", "/c", "mklink", "/J"])
        .arg(&junction)
        .arg(fixture.0.join("scratch"))
        .output()
        .unwrap();
        assert!(
            output.status.success(),
            "junction fixture: {}",
            String::from_utf8_lossy(&output.stderr)
        );
        let rejected = fixture.pin(fixture.tree()).is_err();
        std::fs::remove_dir(junction).unwrap();
        assert!(rejected);
    }
    #[test]
    fn native_toolchain_rejects_unhashed_alternate_stream() {
        let fixture = Fixture::new();
        let stream = format!(
            "{}:unreviewed",
            fixture.0.join("tool/bin/driver.exe").display()
        );
        std::fs::write(stream, b"not in the reviewed primary stream digest").unwrap();
        assert!(fixture.pin(fixture.tree()).is_err());
    }
}
