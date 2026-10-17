// SPDX-License-Identifier: AGPL-3.0-or-later
//! Trusted orchestration process lifecycle, NOT an agent isolation boundary.
//! Starts suspended, inherits only explicit standard streams, joins a fresh
//! kill-on-close Job before running, and receives no ambient environment.
use anyhow::{Context, Result, ensure};
use std::{
    ffi::{OsStr, OsString},
    fs::File,
    mem::{size_of, size_of_val},
    os::windows::{
        ffi::OsStrExt,
        io::{AsRawHandle, FromRawHandle, OwnedHandle},
    },
    path::Path,
    ptr::{null, null_mut},
};
use windows_sys::Win32::{
    Foundation::{DUPLICATE_SAME_ACCESS, DuplicateHandle, WAIT_OBJECT_0, WAIT_TIMEOUT},
    System::{
        JobObjects::{
            AssignProcessToJobObject, CreateJobObjectW, JOB_OBJECT_LIMIT_KILL_ON_JOB_CLOSE,
            JOBOBJECT_BASIC_ACCOUNTING_INFORMATION, JOBOBJECT_EXTENDED_LIMIT_INFORMATION,
            JobObjectBasicAccountingInformation, JobObjectExtendedLimitInformation,
            QueryInformationJobObject, SetInformationJobObject, TerminateJobObject,
        },
        Threading::{
            CREATE_SUSPENDED, CREATE_UNICODE_ENVIRONMENT, CreateProcessW,
            DeleteProcThreadAttributeList, EXTENDED_STARTUPINFO_PRESENT, GetCurrentProcess,
            GetExitCodeProcess, InitializeProcThreadAttributeList,
            PROC_THREAD_ATTRIBUTE_HANDLE_LIST, PROCESS_INFORMATION, ResumeThread,
            STARTF_USESTDHANDLES, STARTUPINFOEXW, TerminateProcess, UpdateProcThreadAttribute,
            WaitForSingleObject,
        },
    },
};

fn ok(value: i32) -> Result<()> {
    ensure!(
        value != 0,
        "Windows process operation failed: {}",
        std::io::Error::last_os_error()
    );
    Ok(())
}
fn wide(value: &OsStr) -> Result<Vec<u16>> {
    let mut result: Vec<_> = value.encode_wide().collect();
    ensure!(!result.contains(&0), "NUL in process argument");
    result.push(0);
    Ok(result)
}
fn quoted(value: &OsStr) -> Result<Vec<u16>> {
    let mut result = vec![34];
    let mut slashes = 0;
    for unit in value.encode_wide() {
        ensure!(unit != 0, "NUL in process argument");
        if unit == 92 {
            slashes += 1;
            continue;
        }
        result.extend(std::iter::repeat_n(
            92,
            if unit == 34 { slashes * 2 + 1 } else { slashes },
        ));
        result.push(unit);
        slashes = 0;
    }
    result.extend(std::iter::repeat_n(92, slashes * 2));
    result.push(34);
    Ok(result)
}
struct Attributes(Vec<usize>);
impl Drop for Attributes {
    fn drop(&mut self) {
        unsafe {
            DeleteProcThreadAttributeList(self.0.as_mut_ptr().cast());
        }
    }
}

pub struct Process {
    process: OwnedHandle,
    job: OwnedHandle,
}
impl Process {
    /// Signal all descendants, not just the root executable.
    pub fn terminate(&self) -> Result<()> {
        ok(unsafe { TerminateJobObject(self.job.as_raw_handle(), 1) })
    }
    pub fn active_processes(&self) -> Result<u32> {
        let mut info = JOBOBJECT_BASIC_ACCOUNTING_INFORMATION::default();
        ok(unsafe {
            QueryInformationJobObject(
                self.job.as_raw_handle(),
                JobObjectBasicAccountingInformation,
                (&mut info as *mut JOBOBJECT_BASIC_ACCOUNTING_INFORMATION).cast(),
                size_of_val(&info) as u32,
                null_mut(),
            )
        })?;
        Ok(info.ActiveProcesses)
    }
    pub fn try_wait(&self) -> Result<Option<u32>> {
        match unsafe { WaitForSingleObject(self.process.as_raw_handle(), 0) } {
            WAIT_TIMEOUT => Ok(None),
            WAIT_OBJECT_0 => {
                let mut code = 0;
                ok(unsafe { GetExitCodeProcess(self.process.as_raw_handle(), &mut code) })?;
                Ok(Some(code))
            }
            _ => anyhow::bail!("could not poll trusted process"),
        }
    }
}

/// The caller pins executable/script handles and validates their digests for
/// the entire lifetime. Dropping this value terminates surviving descendants.
pub fn spawn(
    executable: &Path,
    arguments: &[OsString],
    directory: &Path,
    environment: &[(OsString, OsString)],
    stdout: &File,
    stderr: &File,
) -> Result<Process> {
    ensure!(
        executable.is_absolute() && directory.is_absolute(),
        "absolute process paths required"
    );
    let executable = wide(executable.as_os_str())?;
    let directory = wide(directory.as_os_str())?;
    let mut command = quoted(OsStr::new("node"))?;
    for argument in arguments {
        command.push(32);
        command.extend(quoted(argument)?);
    }
    command.push(0);
    let mut sorted = std::collections::BTreeMap::new();
    for (key, value) in environment {
        ensure!(
            !key.is_empty() && !key.to_string_lossy().contains('='),
            "invalid environment key"
        );
        ensure!(
            sorted.insert(key.to_ascii_uppercase(), value).is_none(),
            "duplicate environment key"
        );
    }
    let mut env = Vec::<u16>::new();
    for (key, value) in sorted {
        env.extend(wide(&key)?.into_iter().take_while(|v| *v != 0));
        env.push(61);
        env.extend(wide(value)?);
    }
    if env.is_empty() {
        env.push(0);
    }
    env.push(0);
    let stdin = File::open("NUL")?;
    let mut inherited = Vec::new();
    for file in [&stdin, stdout, stderr] {
        let mut raw = null_mut();
        ok(unsafe {
            DuplicateHandle(
                GetCurrentProcess(),
                file.as_raw_handle(),
                GetCurrentProcess(),
                &mut raw,
                0,
                1,
                DUPLICATE_SAME_ACCESS,
            )
        })?;
        inherited.push(unsafe { OwnedHandle::from_raw_handle(raw) });
    }
    let handles: Vec<_> = inherited.iter().map(AsRawHandle::as_raw_handle).collect();
    let mut bytes = 0;
    unsafe {
        InitializeProcThreadAttributeList(null_mut(), 1, 0, &mut bytes);
    }
    ensure!((1..=65536).contains(&bytes), "invalid attribute allocation");
    let mut storage = vec![0usize; bytes.div_ceil(size_of::<usize>())];
    ok(unsafe {
        InitializeProcThreadAttributeList(storage.as_mut_ptr().cast(), 1, 0, &mut bytes)
    })?;
    let mut attributes = Attributes(storage);
    ok(unsafe {
        UpdateProcThreadAttribute(
            attributes.0.as_mut_ptr().cast(),
            0,
            PROC_THREAD_ATTRIBUTE_HANDLE_LIST as usize,
            handles.as_ptr().cast(),
            size_of_val(handles.as_slice()),
            null_mut(),
            null(),
        )
    })?;
    let raw_job = unsafe { CreateJobObjectW(null(), null()) };
    ensure!(!raw_job.is_null(), "create process job failed");
    let job = unsafe { OwnedHandle::from_raw_handle(raw_job) };
    let mut limits = JOBOBJECT_EXTENDED_LIMIT_INFORMATION::default();
    limits.BasicLimitInformation.LimitFlags = JOB_OBJECT_LIMIT_KILL_ON_JOB_CLOSE;
    ok(unsafe {
        SetInformationJobObject(
            job.as_raw_handle(),
            JobObjectExtendedLimitInformation,
            (&limits as *const JOBOBJECT_EXTENDED_LIMIT_INFORMATION).cast(),
            size_of_val(&limits) as u32,
        )
    })?;
    let mut startup = STARTUPINFOEXW::default();
    startup.StartupInfo.cb = size_of::<STARTUPINFOEXW>() as u32;
    startup.StartupInfo.dwFlags = STARTF_USESTDHANDLES;
    startup.StartupInfo.hStdInput = handles[0];
    startup.StartupInfo.hStdOutput = handles[1];
    startup.StartupInfo.hStdError = handles[2];
    startup.lpAttributeList = attributes.0.as_mut_ptr().cast();
    let mut info = PROCESS_INFORMATION::default();
    ok(unsafe {
        CreateProcessW(
            executable.as_ptr(),
            command.as_mut_ptr(),
            null(),
            null(),
            1,
            CREATE_SUSPENDED | CREATE_UNICODE_ENVIRONMENT | EXTENDED_STARTUPINFO_PRESENT,
            env.as_ptr().cast(),
            directory.as_ptr(),
            &startup.StartupInfo,
            &mut info,
        )
    })?;
    let child = unsafe { OwnedHandle::from_raw_handle(info.hProcess) };
    let thread = unsafe { OwnedHandle::from_raw_handle(info.hThread) };
    if unsafe { AssignProcessToJobObject(job.as_raw_handle(), child.as_raw_handle()) } == 0 {
        let error = std::io::Error::last_os_error();
        unsafe {
            TerminateProcess(child.as_raw_handle(), 1);
        }
        return Err(error).context("bind trusted child to Job");
    }
    let process = Process {
        process: child,
        job,
    };
    ensure!(
        unsafe { ResumeThread(thread.as_raw_handle()) } == 1,
        "resume trusted child failed"
    );
    Ok(process)
}

#[cfg(test)]
mod tests {
    use super::*;
    #[test]
    fn native_process_helper() {
        match std::env::var("CODIFIX_TRUSTED_HELPER").as_deref() {
            Ok("exit-seven") => std::process::exit(7),
            Ok("root") => {
                // PATH is present in the test host but deliberately absent
                // from the explicit launch environment.
                assert!(std::env::var_os("PATH").is_none());
                println!("explicit-stdout");
                eprintln!("explicit-stderr");
                let mut child = std::process::Command::new(std::env::current_exe().unwrap())
                    .args([
                        "--exact",
                        "windows_trusted_process::tests::native_process_helper",
                        "--nocapture",
                    ])
                    .env_clear()
                    .env("CODIFIX_TRUSTED_HELPER", "leaf")
                    .spawn()
                    .unwrap();
                std::fs::write("descendant.pid", child.id().to_string()).unwrap();
                child.wait().unwrap();
            }
            Ok("leaf") => loop {
                std::thread::sleep(std::time::Duration::from_millis(50));
            },
            _ => {}
        }
    }
    #[test]
    fn native_explicit_streams_environment_and_descendants_are_fenced() {
        assert!(std::env::var_os("PATH").is_some());
        let root =
            std::env::temp_dir().join(format!("memory-trusted-tree-{}", uuid::Uuid::new_v4()));
        std::fs::create_dir(&root).unwrap();
        let stdout = File::create(root.join("stdout")).unwrap();
        let stderr = File::create(root.join("stderr")).unwrap();
        let process = spawn(
            &std::env::current_exe().unwrap(),
            &[
                "--exact".into(),
                "windows_trusted_process::tests::native_process_helper".into(),
                "--nocapture".into(),
            ],
            &root,
            &[("CODIFIX_TRUSTED_HELPER".into(), "root".into())],
            &stdout,
            &stderr,
        )
        .unwrap();
        let deadline = std::time::Instant::now() + std::time::Duration::from_secs(15);
        while !root.join("descendant.pid").exists() || process.active_processes().unwrap() < 2 {
            assert!(std::time::Instant::now() < deadline);
            std::thread::sleep(std::time::Duration::from_millis(10));
        }
        let pid: u32 = std::fs::read_to_string(root.join("descendant.pid"))
            .unwrap()
            .parse()
            .unwrap();
        use windows_sys::Win32::System::Threading::{OpenProcess, PROCESS_SYNCHRONIZE};
        let leaf = unsafe { OpenProcess(PROCESS_SYNCHRONIZE, 0, pid) };
        assert!(!leaf.is_null());
        let leaf = unsafe { OwnedHandle::from_raw_handle(leaf) };
        // Guard cancellation must kill both root and leaf, not just root.
        drop(process);
        assert_eq!(
            unsafe { WaitForSingleObject(leaf.as_raw_handle(), 10000) },
            WAIT_OBJECT_0
        );
        assert!(
            std::fs::read_to_string(root.join("stdout"))
                .unwrap()
                .contains("explicit-stdout")
        );
        assert!(
            std::fs::read_to_string(root.join("stderr"))
                .unwrap()
                .contains("explicit-stderr")
        );
        drop(leaf);
        drop(stdout);
        drop(stderr);
        std::fs::remove_dir_all(root).unwrap();
    }
    #[test]
    fn native_launch_failure_does_not_execute_or_leave_output() {
        let root =
            std::env::temp_dir().join(format!("memory-trusted-failure-{}", uuid::Uuid::new_v4()));
        std::fs::create_dir(&root).unwrap();
        let output = File::create(root.join("out")).unwrap();
        let error = File::create(root.join("err")).unwrap();
        assert!(spawn(&root.join("missing.exe"), &[], &root, &[], &output, &error).is_err());
        assert_eq!(output.metadata().unwrap().len(), 0);
        assert_eq!(error.metadata().unwrap().len(), 0);
        drop(output);
        drop(error);
        std::fs::remove_dir_all(root).unwrap();
    }
    #[test]
    fn literal_windows_arguments_escape_quotes_and_trailing_slashes() {
        assert_eq!(
            String::from_utf16(&quoted(OsStr::new("space end\\")).unwrap()).unwrap(),
            "\"space end\\\\\""
        );
        assert_eq!(
            String::from_utf16(&quoted(OsStr::new("a\"b")).unwrap()).unwrap(),
            "\"a\\\"b\""
        );
        assert!(quoted(OsStr::new("a\0b")).is_err());
    }
    #[test]
    fn native_trusted_child_exit_and_job_cancellation() {
        let root =
            std::env::temp_dir().join(format!("memory-trusted-job-{}", uuid::Uuid::new_v4()));
        std::fs::create_dir(&root).unwrap();
        let output = File::create(root.join("out")).unwrap();
        let error = File::create(root.join("err")).unwrap();
        let process = spawn(
            &std::env::current_exe().unwrap(),
            &[
                "--exact".into(),
                "windows_trusted_process::tests::native_process_helper".into(),
                "--nocapture".into(),
            ],
            &root,
            &[("CODIFIX_TRUSTED_HELPER".into(), "exit-seven".into())],
            &output,
            &error,
        )
        .unwrap();
        let deadline = std::time::Instant::now() + std::time::Duration::from_secs(10);
        loop {
            if let Some(code) = process.try_wait().unwrap() {
                assert_eq!(code, 7);
                break;
            }
            assert!(std::time::Instant::now() < deadline);
            std::thread::sleep(std::time::Duration::from_millis(10));
        }
        process.terminate().unwrap();
        while process.active_processes().unwrap() != 0 {
            assert!(std::time::Instant::now() < deadline);
            std::thread::sleep(std::time::Duration::from_millis(10));
        }
        drop(process);
        drop(output);
        drop(error);
        std::fs::remove_dir_all(root).unwrap();
    }
}
