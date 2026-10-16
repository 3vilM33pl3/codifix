// SPDX-License-Identifier: AGPL-3.0-or-later
use anyhow::{Context, Result, bail};
use std::{
    ffi::{OsStr, OsString, c_void},
    os::windows::ffi::{OsStrExt, OsStringExt},
    path::Path,
    ptr::null,
    sync::{
        OnceLock,
        atomic::{AtomicBool, Ordering},
    },
};
use windows_sys::Win32::{
    Foundation::{CloseHandle, HANDLE, WAIT_OBJECT_0, WAIT_TIMEOUT},
    System::{
        JobObjects::{
            AssignProcessToJobObject, CreateJobObjectW, JOB_OBJECT_LIMIT_KILL_ON_JOB_CLOSE,
            JOBOBJECT_EXTENDED_LIMIT_INFORMATION, JobObjectExtendedLimitInformation,
            SetInformationJobObject, TerminateJobObject,
        },
        Services::*,
        Threading::{
            CREATE_SUSPENDED, CreateProcessW, GetExitCodeProcess, PROCESS_INFORMATION,
            ResumeThread, STARTUPINFOW, TerminateProcess, WaitForSingleObject,
        },
    },
};

static SPEC: OnceLock<Spec> = OnceLock::new();
static STOP: AtomicBool = AtomicBool::new(false);

struct Spec {
    name: Vec<u16>,
    executable: Vec<u16>,
    command: Vec<u16>,
    directory: Vec<u16>,
}

fn wide(value: &OsStr) -> Result<Vec<u16>> {
    let units: Vec<u16> = value.encode_wide().collect();
    if units.contains(&0) {
        bail!("service argument contains NUL");
    }
    Ok(units.into_iter().chain([0]).collect())
}

// CommandLineToArgvW-compatible escaping, not cmd.exe/PowerShell evaluation.
fn quote(value: &OsStr) -> OsString {
    let mut result = vec![b'"' as u16];
    let mut slashes = 0;
    for unit in value.encode_wide() {
        if unit == b'\\' as u16 {
            slashes += 1;
            continue;
        }
        let count = if unit == b'"' as u16 {
            slashes * 2 + 1
        } else {
            slashes
        };
        result.extend(std::iter::repeat_n(b'\\' as u16, count));
        result.push(unit);
        slashes = 0;
    }
    result.extend(std::iter::repeat_n(b'\\' as u16, slashes * 2));
    result.push(b'"' as u16);
    OsString::from_wide(&result)
}

fn parse(arguments: Vec<OsString>) -> Result<Spec> {
    if arguments.len() < 6
        || arguments[0] != "--name"
        || arguments[2] != "--directory"
        || arguments[4] != "--"
    {
        bail!(
            "usage: memory-windows-service --name MemoryLayer-<instance> --directory <absolute-state-dir> -- <absolute-executable> [arguments]"
        );
    }
    let name = arguments[1]
        .to_str()
        .context("service name must be ASCII")?;
    if !name.starts_with("MemoryLayer-")
        || name.len() > 128
        || !name.bytes().all(|b| b.is_ascii_alphanumeric() || b == b'-')
    {
        bail!("service name must be MemoryLayer- followed by letters, numbers or hyphens");
    }
    for value in [&arguments[3], &arguments[5]] {
        if !Path::new(value).is_absolute() {
            bail!("service executable and directory must be absolute");
        }
    }
    let mut command = OsString::new();
    for argument in &arguments[5..] {
        if !command.is_empty() {
            command.push(" ");
        }
        command.push(quote(argument));
    }
    let command = wide(&command)?;
    if command.len() > 32767 {
        bail!("service command exceeds Windows limit");
    }
    Ok(Spec {
        name: wide(&arguments[1])?,
        executable: wide(&arguments[5])?,
        directory: wide(&arguments[3])?,
        command,
    })
}

struct OwnedHandle(HANDLE);
impl Drop for OwnedHandle {
    fn drop(&mut self) {
        // SAFETY: constructed only from a successfully created owned handle.
        unsafe {
            CloseHandle(self.0);
        }
    }
}

fn os_ok(success: i32, operation: &str) -> Result<()> {
    if success == 0 {
        Err(std::io::Error::last_os_error()).context(operation.to_owned())
    } else {
        Ok(())
    }
}

fn status(handle: SERVICE_STATUS_HANDLE, state: u32, exit: u32) -> Result<()> {
    let pending = state == SERVICE_START_PENDING || state == SERVICE_STOP_PENDING;
    let report = SERVICE_STATUS {
        dwServiceType: SERVICE_WIN32_OWN_PROCESS,
        dwCurrentState: state,
        dwControlsAccepted: if state == SERVICE_RUNNING {
            SERVICE_ACCEPT_STOP | SERVICE_ACCEPT_SHUTDOWN
        } else {
            0
        },
        dwWin32ExitCode: exit,
        dwCheckPoint: u32::from(pending),
        dwWaitHint: if pending { 10000 } else { 0 },
        ..Default::default()
    };
    // SAFETY: SCM supplied handle; the report lives through the synchronous call.
    os_ok(
        unsafe { SetServiceStatus(handle, &report) },
        "report service state",
    )
}

unsafe extern "system" fn control(code: u32, _: u32, _: *mut c_void, _: *mut c_void) -> u32 {
    match code {
        SERVICE_CONTROL_STOP | SERVICE_CONTROL_SHUTDOWN => {
            STOP.store(true, Ordering::Release);
            0
        }
        SERVICE_CONTROL_INTERROGATE => 0,
        _ => 120, // ERROR_CALL_NOT_IMPLEMENTED
    }
}

fn supervise(spec: &Spec, handle: SERVICE_STATUS_HANDLE) -> Result<u32> {
    // SAFETY: no inherited name/security attributes; handle is checked before use.
    let job = unsafe { CreateJobObjectW(null(), null()) };
    if job.is_null() {
        return Err(std::io::Error::last_os_error()).context("create service process job");
    }
    let job = OwnedHandle(job);
    let mut limits = JOBOBJECT_EXTENDED_LIMIT_INFORMATION::default();
    limits.BasicLimitInformation.LimitFlags = JOB_OBJECT_LIMIT_KILL_ON_JOB_CLOSE;
    // SAFETY: correct typed structure and length for the requested information class.
    os_ok(
        unsafe {
            SetInformationJobObject(
                job.0,
                JobObjectExtendedLimitInformation,
                (&limits as *const JOBOBJECT_EXTENDED_LIMIT_INFORMATION).cast(),
                std::mem::size_of_val(&limits) as u32,
            )
        },
        "set service kill-on-close job",
    )?;
    let startup = STARTUPINFOW {
        cb: std::mem::size_of::<STARTUPINFOW>() as u32,
        ..Default::default()
    };
    let mut process = PROCESS_INFORMATION::default();
    let mut command = spec.command.clone();
    // SAFETY: all UTF-16 buffers are NUL-terminated, command is writable. No
    // inherited handles. Start suspended so no child escapes before job binding.
    os_ok(
        unsafe {
            CreateProcessW(
                spec.executable.as_ptr(),
                command.as_mut_ptr(),
                null(),
                null(),
                0,
                CREATE_SUSPENDED,
                null(),
                spec.directory.as_ptr(),
                &startup,
                &mut process,
            )
        },
        "create suspended service process",
    )?;
    let child = OwnedHandle(process.hProcess);
    let thread = OwnedHandle(process.hThread);
    // SAFETY: handles are valid and owned. A binding failure kills the suspended
    // child rather than allowing an uncontained process to continue.
    if unsafe { AssignProcessToJobObject(job.0, child.0) } == 0 {
        let error = std::io::Error::last_os_error();
        unsafe {
            TerminateProcess(child.0, 1);
        }
        return Err(error).context("bind service child to job");
    }
    if unsafe { ResumeThread(thread.0) } == u32::MAX {
        return Err(std::io::Error::last_os_error()).context("resume service child");
    }
    status(handle, SERVICE_RUNNING, 0)?;
    loop {
        if STOP.load(Ordering::Acquire) {
            status(handle, SERVICE_STOP_PENDING, 0)?;
            // Stop is a process-tree cancellation, not a successful transaction
            // acknowledgement. Application leases/outbox intents recover later.
            os_ok(
                unsafe { TerminateJobObject(job.0, 0) },
                "stop service process tree",
            )?;
            if unsafe { WaitForSingleObject(child.0, 10000) } != WAIT_OBJECT_0 {
                bail!("service child did not stop");
            }
            return Ok(0);
        }
        match unsafe { WaitForSingleObject(child.0, 200) } {
            WAIT_OBJECT_0 => {
                let mut code = 0;
                os_ok(
                    unsafe { GetExitCodeProcess(child.0, &mut code) },
                    "read service child outcome",
                )?;
                return Ok(code);
            }
            WAIT_TIMEOUT => {}
            _ => return Err(std::io::Error::last_os_error()).context("wait for service child"),
        }
    }
}

unsafe extern "system" fn service_main(_: u32, _: *mut *mut u16) {
    let Some(spec) = SPEC.get() else {
        return;
    };
    // SAFETY: spec is process-lifetime immutable and callback uses atomic state.
    let handle =
        unsafe { RegisterServiceCtrlHandlerExW(spec.name.as_ptr(), Some(control), null()) };
    if handle.is_null() {
        return;
    }
    let result = status(handle, SERVICE_START_PENDING, 0).and_then(|()| supervise(spec, handle));
    let exit = match result {
        Ok(code) => code,
        Err(error) => {
            eprintln!("Memory service host failed: {error:#}");
            1066
        }
    };
    let _ = status(handle, SERVICE_STOPPED, exit);
}

pub fn run() -> Result<()> {
    let spec = parse(std::env::args_os().skip(1).collect())?;
    SPEC.set(spec)
        .map_err(|_| anyhow::anyhow!("service host initialized twice"))?;
    let spec = SPEC.get().context("service host specification missing")?;
    let entries = [
        SERVICE_TABLE_ENTRYW {
            lpServiceName: spec.name.as_ptr().cast_mut(),
            lpServiceProc: Some(service_main),
        },
        SERVICE_TABLE_ENTRYW::default(),
    ];
    // SAFETY: null-terminated dispatch table and process-lifetime name/callback.
    os_ok(
        unsafe { StartServiceCtrlDispatcherW(entries.as_ptr()) },
        "connect native service to SCM",
    )
}

#[cfg(test)]
#[allow(clippy::unwrap_used)]
mod tests {
    use super::*;
    #[test]
    fn command_is_literal_and_arguments_are_not_shell_evaluated() {
        assert_eq!(quote(OsStr::new(r#"a b\"#)), OsString::from(r#""a b\\""#));
        assert_eq!(quote(OsStr::new("a\"b")), OsString::from("\"a\\\"b\""));
        let spec = parse(
            [
                "--name",
                "MemoryLayer-test",
                "--directory",
                r"C:\state",
                "--",
                r"C:\release\memory.exe",
                "--config",
                r"C:\state\my config.toml",
            ]
            .map(OsString::from)
            .to_vec(),
        )
        .unwrap();
        assert!(String::from_utf16_lossy(&spec.command).contains("my config.toml\""));
    }
    #[test]
    fn rejects_unscoped_names_relative_paths_and_nul() {
        for (name, path) in [
            ("OtherService", r"C:\memory.exe"),
            ("MemoryLayer-test", "memory.exe"),
            ("MemoryLayer-test", "C:\\x\0.exe"),
        ] {
            assert!(
                parse(
                    ["--name", name, "--directory", r"C:\state", "--", path]
                        .map(OsString::from)
                        .to_vec()
                )
                .is_err()
            );
        }
    }
}
