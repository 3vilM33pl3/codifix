// SPDX-License-Identifier: AGPL-3.0-or-later
//! Credential-free AppContainer process primitive, not a Pi/provider adapter.
//! No network capabilities, loopback exemptions, inherited user environment or
//! implicit filesystem grants. Authority files keep their existing validators;
//! temporary sandbox grants apply only to a dedicated code copy and scratch.
use anyhow::{Context, Result, bail, ensure};
use mem_platform_files::windows::{Directory, Input};
use sha2::{Digest, Sha256};
use std::{
    ffi::{OsStr, OsString, c_void},
    fs::{File, OpenOptions},
    io::Read,
    mem::{size_of, size_of_val},
    os::windows::{
        ffi::{OsStrExt, OsStringExt},
        fs::OpenOptionsExt,
        io::{AsRawHandle, BorrowedHandle, FromRawHandle, OwnedHandle},
    },
    path::{Path, PathBuf},
    ptr::{null, null_mut},
    time::{Duration, Instant},
};
use windows_sys::Win32::{
    Foundation::{DUPLICATE_SAME_ACCESS, DuplicateHandle, LocalFree, WAIT_OBJECT_0, WAIT_TIMEOUT},
    Security::{
        Authorization::{
            ConvertStringSecurityDescriptorToSecurityDescriptorW, EXPLICIT_ACCESS_W, GRANT_ACCESS,
            GetSecurityInfo, SDDL_REVISION_1, SE_FILE_OBJECT, SetEntriesInAclW, SetSecurityInfo,
            TRUSTEE_IS_SID, TRUSTEE_IS_UNKNOWN, TRUSTEE_W,
        },
        CONTAINER_INHERIT_ACE, DACL_SECURITY_INFORMATION, EqualSid, FreeSid,
        GetSecurityDescriptorControl, GetSecurityDescriptorSacl, GetTokenInformation,
        INHERIT_ONLY_ACE,
        Isolation::{CreateAppContainerProfile, DeleteAppContainerProfile},
        LABEL_SECURITY_INFORMATION, OBJECT_INHERIT_ACE, PROTECTED_DACL_SECURITY_INFORMATION, PSID,
        SE_DACL_PROTECTED, SECURITY_ATTRIBUTES, SECURITY_CAPABILITIES,
        TOKEN_APPCONTAINER_INFORMATION, TOKEN_QUERY, TokenAppContainerSid, TokenCapabilities,
        TokenIsAppContainer, UNPROTECTED_DACL_SECURITY_INFORMATION,
    },
    Storage::FileSystem::{
        BY_HANDLE_FILE_INFORMATION, DELETE, FILE_DELETE_CHILD, FILE_FLAG_BACKUP_SEMANTICS,
        FILE_FLAG_OPEN_REPARSE_POINT, FILE_GENERIC_EXECUTE, FILE_GENERIC_READ, FILE_GENERIC_WRITE,
        FILE_READ_ATTRIBUTES, FILE_SHARE_READ, FILE_TRAVERSE, FILE_TYPE_PIPE,
        GetFileInformationByHandle, GetFileType, READ_CONTROL, WRITE_DAC, WRITE_OWNER,
    },
    System::{
        Com::CoTaskMemFree,
        JobObjects::{
            AssignProcessToJobObject, CreateJobObjectW, JOB_OBJECT_CPU_RATE_CONTROL_ENABLE,
            JOB_OBJECT_CPU_RATE_CONTROL_HARD_CAP, JOB_OBJECT_LIMIT_ACTIVE_PROCESS,
            JOB_OBJECT_LIMIT_DIE_ON_UNHANDLED_EXCEPTION, JOB_OBJECT_LIMIT_JOB_MEMORY,
            JOB_OBJECT_LIMIT_KILL_ON_JOB_CLOSE, JOB_OBJECT_LIMIT_PROCESS_MEMORY,
            JOBOBJECT_BASIC_ACCOUNTING_INFORMATION, JOBOBJECT_CPU_RATE_CONTROL_INFORMATION,
            JOBOBJECT_CPU_RATE_CONTROL_INFORMATION_0, JOBOBJECT_EXTENDED_LIMIT_INFORMATION,
            JobObjectBasicAccountingInformation, JobObjectBasicProcessIdList,
            JobObjectCpuRateControlInformation, JobObjectExtendedLimitInformation,
            QueryInformationJobObject, SetInformationJobObject, TerminateJobObject,
        },
        Pipes::{CreatePipe, GetNamedPipeInfo},
        SystemInformation::GetWindowsDirectoryW,
        Threading::{
            ALL_PROCESSOR_GROUPS, CREATE_NO_WINDOW, CREATE_SUSPENDED, CREATE_UNICODE_ENVIRONMENT,
            CreateProcessW, DeleteProcThreadAttributeList, EXTENDED_STARTUPINFO_PRESENT,
            GetActiveProcessorCount, GetCurrentProcess, GetCurrentProcessId, GetExitCodeProcess,
            InitializeProcThreadAttributeList, OpenProcessToken, PROC_THREAD_ATTRIBUTE_HANDLE_LIST,
            PROC_THREAD_ATTRIBUTE_SECURITY_CAPABILITIES, PROCESS_INFORMATION, ResumeThread,
            STARTF_USESTDHANDLES, STARTUPINFOEXW, TerminateProcess, UpdateProcThreadAttribute,
            WaitForSingleObject,
        },
    },
    UI::Shell::{FOLDERID_LocalAppData, SHGetKnownFolderPath},
};

const MAX_CODE_BYTES: u64 = 512 * 1024 * 1024;
const JOB_MEMORY_BYTES: usize = 512 * 1024 * 1024;

/// The OS uses LOCALAPPDATA while constructing an AppContainer's redirected
/// profile environment. Resolve that one launch input from the current Windows
/// identity, not inherited process variables. This conveys a path, not an ACL
/// grant; the child remains subject to its zero-capability token.
pub(crate) fn local_app_data() -> Result<OsString> {
    struct Folder(*mut u16);
    impl Drop for Folder {
        fn drop(&mut self) {
            unsafe { CoTaskMemFree(self.0.cast()) };
        }
    }
    let mut path = null_mut();
    let hr = unsafe { SHGetKnownFolderPath(&FOLDERID_LocalAppData, 0, null_mut(), &mut path) };
    let path = Folder(path);
    ensure!(
        hr >= 0 && !path.0.is_null(),
        "local app data folder unavailable"
    );
    // Windows filesystem paths are bounded to 32,767 UTF-16 code units. Never
    // scan an unbounded foreign string, and always release the COM allocation.
    let mut units = Vec::new();
    for index in 0..32_768 {
        let unit = unsafe { *path.0.add(index) };
        if unit == 0 {
            let value = OsString::from_wide(&units);
            ensure!(
                Path::new(&value).is_absolute(),
                "local app data folder is not absolute"
            );
            return Ok(value);
        }
        units.push(unit);
    }
    bail!("local app data folder exceeds Windows path limit")
}

/// Flat, dedicated execution bundle. Every executable/runtime data file is
/// individually hash-pinned; directories must not contain authority material.
pub struct CodeFile {
    pub path: PathBuf,
    pub sha256: [u8; 32],
}
pub struct ProcessSpec<'a> {
    pub executable: CodeFile,
    pub runtime_files: Vec<CodeFile>,
    pub scratch: PathBuf,
    pub arguments: Vec<OsString>,
    /// Only these broker-controlled pipes are inherited. Files and arbitrary
    /// handles are rejected; every inherited duplicate is closed after spawn.
    pub stdin: BorrowedHandle<'a>,
    pub stdout: BorrowedHandle<'a>,
    pub stderr: BorrowedHandle<'a>,
    /// Dedicated, already admitted broker channel, separate from diagnostics.
    pub broker: BorrowedHandle<'a>,
}

pub(super) fn ok(value: i32, operation: &'static str) -> Result<()> {
    if value == 0 {
        Err(std::io::Error::last_os_error()).context(operation)
    } else {
        Ok(())
    }
}
pub(super) fn wide(value: &OsStr) -> Result<Vec<u16>> {
    let value: Vec<_> = value.encode_wide().collect();
    ensure!(!value.contains(&0), "NUL in sandbox argument");
    Ok(value.into_iter().chain(Some(0)).collect())
}
pub(super) fn quote(value: &OsStr) -> Result<OsString> {
    let mut result = vec![b'"' as u16];
    let mut slashes = 0;
    for unit in value.encode_wide() {
        ensure!(unit != 0, "NUL in sandbox argument");
        if unit == b'\\' as u16 {
            slashes += 1;
            continue;
        }
        result.extend(std::iter::repeat_n(
            b'\\' as u16,
            if unit == b'"' as u16 {
                slashes * 2 + 1
            } else {
                slashes
            },
        ));
        result.push(unit);
        slashes = 0;
    }
    result.extend(std::iter::repeat_n(b'\\' as u16, slashes * 2));
    result.push(b'"' as u16);
    Ok(OsString::from_wide(&result))
}

pub(super) struct Profile {
    name: Vec<u16>,
    pub(super) sid: PSID,
    deleted: bool,
}
impl Profile {
    pub(super) fn create() -> Result<Self> {
        let name = wide(OsStr::new(&format!(
            "MemoryLayer.Sandbox.{}",
            uuid::Uuid::new_v4()
        )))?;
        let mut sid = null_mut();
        // SAFETY: strings remain live and no capability SID is supplied.
        let hr = unsafe {
            CreateAppContainerProfile(
                name.as_ptr(),
                name.as_ptr(),
                name.as_ptr(),
                null(),
                0,
                &mut sid,
            )
        };
        ensure!(
            hr >= 0 && !sid.is_null(),
            "could not create fresh AppContainer"
        );
        Ok(Self {
            name,
            sid,
            deleted: false,
        })
    }
    fn delete(&mut self) -> Result<()> {
        self.delete_with(|name| unsafe { DeleteAppContainerProfile(name) })
    }
    fn delete_with(&mut self, deletion: impl FnOnce(*const u16) -> i32) -> Result<()> {
        if self.deleted {
            return Ok(());
        }
        let hr = deletion(self.name.as_ptr());
        ensure!(
            hr >= 0,
            "AppContainer profile cleanup unresolved: {}",
            OsString::from_wide(&self.name[..self.name.len() - 1]).to_string_lossy()
        );
        self.deleted = true;
        Ok(())
    }
}
impl Drop for Profile {
    fn drop(&mut self) {
        // SAFETY: this instance owns exactly this fresh profile and SID.
        unsafe {
            if !self.deleted {
                DeleteAppContainerProfile(self.name.as_ptr());
            }
            FreeSid(self.sid);
        }
    }
}

fn file_identity(file: &File) -> Result<(u32, u64)> {
    let mut value = BY_HANDLE_FILE_INFORMATION::default();
    ok(
        unsafe { GetFileInformationByHandle(file.as_raw_handle(), &mut value) },
        "inspect sandbox object",
    )?;
    Ok((
        value.dwVolumeSerialNumber,
        (u64::from(value.nFileIndexHigh) << 32) | u64::from(value.nFileIndexLow),
    ))
}
struct Allocation(*mut c_void);
impl Drop for Allocation {
    fn drop(&mut self) {
        unsafe {
            LocalFree(self.0);
        }
    }
}

pub(super) struct Grant {
    file: File,
    descriptor: Allocation,
    dacl: *mut windows_sys::Win32::Security::ACL,
    protection: u32,
    label: Option<(Allocation, *mut windows_sys::Win32::Security::ACL)>,
}
impl Grant {
    pub(super) fn apply(
        path: &Path,
        pinned: Option<&File>,
        sid: PSID,
        permissions: &[(u32, u32)],
    ) -> Result<Self> {
        // Callers retain ordinary protected Input/Directory handles first. This
        // second handle may change DACLs, but cannot follow or replace the path.
        let file = OpenOptions::new()
            .access_mode(READ_CONTROL | WRITE_DAC | WRITE_OWNER | FILE_READ_ATTRIBUTES)
            .share_mode(FILE_SHARE_READ)
            .custom_flags(FILE_FLAG_BACKUP_SEMANTICS | FILE_FLAG_OPEN_REPARSE_POINT)
            .open(path)?;
        if let Some(pinned) = pinned {
            ensure!(
                file_identity(&file)? == file_identity(pinned)?,
                "sandbox file identity changed"
            );
        }
        let mut dacl = null_mut();
        let mut raw = null_mut();
        let status = unsafe {
            GetSecurityInfo(
                file.as_raw_handle(),
                SE_FILE_OBJECT,
                DACL_SECURITY_INFORMATION,
                null_mut(),
                null_mut(),
                &mut dacl,
                null_mut(),
                &mut raw,
            )
        };
        ensure!(status == 0 && !dacl.is_null(), "sandbox DACL unavailable");
        let descriptor = Allocation(raw);
        let mut control = 0;
        let mut revision = 0;
        ok(
            unsafe { GetSecurityDescriptorControl(raw, &mut control, &mut revision) },
            "inspect sandbox DACL control",
        )?;
        let protection = if control & SE_DACL_PROTECTED != 0 {
            PROTECTED_DACL_SECURITY_INFORMATION
        } else {
            UNPROTECTED_DACL_SECURITY_INFORMATION
        };
        let entries: Vec<_> = permissions
            .iter()
            .map(|(mask, inheritance)| EXPLICIT_ACCESS_W {
                grfAccessPermissions: *mask,
                grfAccessMode: GRANT_ACCESS,
                grfInheritance: *inheritance,
                Trustee: TRUSTEE_W {
                    TrusteeForm: TRUSTEE_IS_SID,
                    TrusteeType: TRUSTEE_IS_UNKNOWN,
                    ptstrName: sid.cast(),
                    ..Default::default()
                },
            })
            .collect();
        let mut expanded = null_mut();
        ensure!(
            unsafe {
                SetEntriesInAclW(entries.len() as u32, entries.as_ptr(), dacl, &mut expanded)
            } == 0,
            "could not build sandbox grant"
        );
        let _expanded = Allocation(expanded.cast());
        ensure!(
            unsafe {
                SetSecurityInfo(
                    file.as_raw_handle(),
                    SE_FILE_OBJECT,
                    DACL_SECURITY_INFORMATION | protection,
                    null_mut(),
                    null_mut(),
                    expanded,
                    null_mut(),
                )
            } == 0,
            "could not grant sandbox access"
        );
        Ok(Self {
            file,
            descriptor,
            dacl,
            protection,
            label: None,
        })
    }
    pub(super) fn scratch_integrity(&mut self) -> Result<()> {
        let mut sacl = null_mut();
        let mut descriptor = null_mut();
        ensure!(
            unsafe {
                GetSecurityInfo(
                    self.file.as_raw_handle(),
                    SE_FILE_OBJECT,
                    LABEL_SECURITY_INFORMATION,
                    null_mut(),
                    null_mut(),
                    null_mut(),
                    &mut sacl,
                    &mut descriptor,
                )
            } == 0,
            "could not read scratch integrity label"
        );
        self.label = Some((Allocation(descriptor), sacl));
        let text = wide(OsStr::new("S:(ML;OICI;NW;;;LW)"))?;
        let mut low = null_mut();
        ok(
            unsafe {
                ConvertStringSecurityDescriptorToSecurityDescriptorW(
                    text.as_ptr(),
                    SDDL_REVISION_1,
                    &mut low,
                    null_mut(),
                )
            },
            "build low-integrity scratch label",
        )?;
        let _low = Allocation(low);
        let mut present = 0;
        let mut defaulted = 0;
        let mut low_sacl = null_mut();
        ok(
            unsafe { GetSecurityDescriptorSacl(low, &mut present, &mut low_sacl, &mut defaulted) },
            "inspect scratch label",
        )?;
        ensure!(
            present != 0 && !low_sacl.is_null(),
            "missing low-integrity scratch label"
        );
        ensure!(
            unsafe {
                SetSecurityInfo(
                    self.file.as_raw_handle(),
                    SE_FILE_OBJECT,
                    LABEL_SECURITY_INFORMATION,
                    null_mut(),
                    null_mut(),
                    null_mut(),
                    low_sacl,
                )
            } == 0,
            "could not lower dedicated scratch integrity"
        );
        Ok(())
    }
}
impl Drop for Grant {
    fn drop(&mut self) {
        // SAFETY: original descriptor and WRITE_DAC handle are retained until
        // the process/job has been stopped. Never perform a new path lookup.
        let _ = self.restore();
    }
}
impl Grant {
    fn restore(&self) -> Result<()> {
        if let Some((_, sacl)) = &self.label {
            ensure!(
                unsafe {
                    SetSecurityInfo(
                        self.file.as_raw_handle(),
                        SE_FILE_OBJECT,
                        LABEL_SECURITY_INFORMATION,
                        null_mut(),
                        null_mut(),
                        null_mut(),
                        *sacl,
                    )
                } == 0,
                "could not restore scratch integrity label"
            );
        }
        ensure!(
            unsafe {
                SetSecurityInfo(
                    self.file.as_raw_handle(),
                    SE_FILE_OBJECT,
                    DACL_SECURITY_INFORMATION | self.protection,
                    null_mut(),
                    null_mut(),
                    self.dacl,
                    null_mut(),
                )
            } == 0,
            "could not restore sandbox DACL; retain protected runtime for repair"
        );
        let _ = &self.descriptor;
        Ok(())
    }
}

pub(super) struct Attributes {
    storage: Vec<usize>,
    initialized: bool,
}
impl Attributes {
    pub(super) fn new() -> Result<Self> {
        let mut size = 0;
        unsafe {
            InitializeProcThreadAttributeList(null_mut(), 2, 0, &mut size);
        }
        ensure!(
            (1..=65536).contains(&size),
            "invalid process attribute allocation"
        );
        let mut value = Self {
            storage: vec![0; size.div_ceil(size_of::<usize>())],
            initialized: false,
        };
        ok(
            unsafe { InitializeProcThreadAttributeList(value.pointer(), 2, 0, &mut size) },
            "initialize process attributes",
        )?;
        value.initialized = true;
        Ok(value)
    }
    pub(super) fn pointer(&mut self) -> *mut c_void {
        self.storage.as_mut_ptr().cast()
    }
    pub(super) fn set<T>(&mut self, attribute: u32, value: &T) -> Result<()> {
        ok(
            unsafe {
                UpdateProcThreadAttribute(
                    self.pointer(),
                    0,
                    attribute as usize,
                    (value as *const T).cast(),
                    size_of_val(value),
                    null_mut(),
                    null(),
                )
            },
            "set sandbox process attribute",
        )
    }
}
impl Drop for Attributes {
    fn drop(&mut self) {
        if self.initialized {
            unsafe {
                DeleteProcThreadAttributeList(self.pointer());
            }
        }
    }
}

fn token_information(token: &OwnedHandle, class: i32) -> Result<Vec<usize>> {
    let mut size = 0;
    unsafe {
        GetTokenInformation(token.as_raw_handle(), class, null_mut(), 0, &mut size);
    }
    ensure!(
        (1..=65536).contains(&size),
        "invalid sandbox token information size"
    );
    let mut value = vec![0usize; (size as usize).div_ceil(size_of::<usize>())];
    ok(
        unsafe {
            GetTokenInformation(
                token.as_raw_handle(),
                class,
                value.as_mut_ptr().cast(),
                size,
                &mut size,
            )
        },
        "query sandbox token",
    )?;
    Ok(value)
}
pub(super) fn verify_token(process: &OwnedHandle, expected_sid: PSID) -> Result<()> {
    let mut raw = null_mut();
    ok(
        unsafe { OpenProcessToken(process.as_raw_handle(), TOKEN_QUERY, &mut raw) },
        "open child token",
    )?;
    let token = unsafe { OwnedHandle::from_raw_handle(raw) };
    let app = token_information(&token, TokenIsAppContainer)?;
    ensure!(
        unsafe { *app.as_ptr().cast::<u32>() } == 1,
        "child is not AppContainer"
    );
    let info = token_information(&token, TokenAppContainerSid)?;
    let actual = unsafe { &*info.as_ptr().cast::<TOKEN_APPCONTAINER_INFORMATION>() };
    ensure!(
        unsafe { EqualSid(actual.TokenAppContainer, expected_sid) } != 0,
        "wrong child AppContainer SID"
    );
    let capabilities = token_information(&token, TokenCapabilities)?;
    ensure!(
        unsafe { *capabilities.as_ptr().cast::<u32>() } == 0,
        "unexpected child network/other capabilities"
    );
    Ok(())
}

fn job() -> Result<OwnedHandle> {
    job_with_limits(2, JOB_MEMORY_BYTES)
}

pub(super) fn job_with_limits(processes: u32, memory_bytes: usize) -> Result<OwnedHandle> {
    let raw = unsafe { CreateJobObjectW(null(), null()) };
    ensure!(!raw.is_null(), "could not create sandbox job");
    let job = unsafe { OwnedHandle::from_raw_handle(raw) };
    let mut limits = JOBOBJECT_EXTENDED_LIMIT_INFORMATION::default();
    limits.BasicLimitInformation.LimitFlags = JOB_OBJECT_LIMIT_KILL_ON_JOB_CLOSE
        | JOB_OBJECT_LIMIT_ACTIVE_PROCESS
        | JOB_OBJECT_LIMIT_JOB_MEMORY
        | JOB_OBJECT_LIMIT_PROCESS_MEMORY
        | JOB_OBJECT_LIMIT_DIE_ON_UNHANDLED_EXCEPTION;
    limits.BasicLimitInformation.ActiveProcessLimit = processes;
    limits.JobMemoryLimit = memory_bytes;
    limits.ProcessMemoryLimit = memory_bytes;
    ok(
        unsafe {
            SetInformationJobObject(
                job.as_raw_handle(),
                JobObjectExtendedLimitInformation,
                (&limits as *const JOBOBJECT_EXTENDED_LIMIT_INFORMATION).cast(),
                size_of_val(&limits) as u32,
            )
        },
        "set sandbox job limits",
    )?;
    let cpus = unsafe { GetActiveProcessorCount(ALL_PROCESSOR_GROUPS) };
    ensure!(cpus != 0, "could not determine CPU budget");
    let cpu = JOBOBJECT_CPU_RATE_CONTROL_INFORMATION {
        ControlFlags: JOB_OBJECT_CPU_RATE_CONTROL_ENABLE | JOB_OBJECT_CPU_RATE_CONTROL_HARD_CAP,
        Anonymous: JOBOBJECT_CPU_RATE_CONTROL_INFORMATION_0 {
            CpuRate: (10000 / cpus).max(1),
        },
    };
    ok(
        unsafe {
            SetInformationJobObject(
                job.as_raw_handle(),
                JobObjectCpuRateControlInformation,
                (&cpu as *const JOBOBJECT_CPU_RATE_CONTROL_INFORMATION).cast(),
                size_of_val(&cpu) as u32,
            )
        },
        "set sandbox CPU cap",
    )?;
    // UI-limited jobs cannot be nested beneath the SCM service host's job.
    // AppContainer capabilities remain empty and creation is headless.
    Ok(job)
}

/// Owning guard. Drop cancels the entire job and reaps it before reverting
/// filesystem grants. It is deliberately not Clone or a detached process ID.
pub struct SandboxProcess {
    pub(super) job: OwnedHandle,
    pub(super) process: OwnedHandle,
    pub(super) profile: Option<Profile>,
    pub(super) grants: Vec<Grant>,
    pub(super) inputs: Vec<Input>,
    pub(super) directories: Vec<Directory>,
    pub(super) pid: u32,
}
impl SandboxProcess {
    pub fn id(&self) -> u32 {
        self.pid
    }
    pub fn profile_name(&self) -> OsString {
        self.profile
            .as_ref()
            .map(|profile| OsString::from_wide(&profile.name[..profile.name.len() - 1]))
            .unwrap_or_default()
    }
    /// Poll without extending a deadline or cancelling the child. A caller may
    /// combine this with its own cancellation channel on the owning thread.
    pub fn try_wait(&mut self) -> Result<Option<u32>> {
        let status = unsafe { WaitForSingleObject(self.process.as_raw_handle(), 0) };
        if status == WAIT_TIMEOUT {
            return Ok(None);
        }
        ensure!(status == WAIT_OBJECT_0, "could not poll sandbox process");
        let mut code = 0;
        ok(
            unsafe { GetExitCodeProcess(self.process.as_raw_handle(), &mut code) },
            "read sandbox exit code",
        )?;
        self.terminate()?;
        self.restore_grants()?;
        self.delete_profile()?;
        Ok(Some(code))
    }
    pub fn wait(&mut self, deadline: Duration) -> Result<u32> {
        ensure!(
            deadline <= Duration::from_secs(24 * 60 * 60),
            "sandbox deadline too large"
        );
        let status = unsafe {
            WaitForSingleObject(self.process.as_raw_handle(), deadline.as_millis() as u32)
        };
        if status == WAIT_TIMEOUT {
            self.terminate()?;
            bail!("sandbox deadline expired");
        }
        ensure!(
            status == WAIT_OBJECT_0,
            "could not wait for sandbox process"
        );
        let mut code = 0;
        ok(
            unsafe { GetExitCodeProcess(self.process.as_raw_handle(), &mut code) },
            "read sandbox exit code",
        )?;
        self.terminate()?;
        self.restore_grants()?;
        self.delete_profile()?;
        Ok(code)
    }
    pub fn terminate(&mut self) -> Result<()> {
        ok(
            unsafe { TerminateJobObject(self.job.as_raw_handle(), 1) },
            "terminate sandbox job",
        )?;
        if unsafe { WaitForSingleObject(self.process.as_raw_handle(), 0) } != WAIT_OBJECT_0 {
            // Also handles token/assignment failure while the child is still
            // suspended and not yet attached to the job.
            ok(
                unsafe { TerminateProcess(self.process.as_raw_handle(), 1) },
                "terminate suspended sandbox process",
            )?;
        }
        ensure!(
            unsafe { WaitForSingleObject(self.process.as_raw_handle(), 5000) } == WAIT_OBJECT_0,
            "sandbox process did not reap"
        );
        let deadline = Instant::now() + Duration::from_secs(5);
        loop {
            let mut value = JOBOBJECT_BASIC_ACCOUNTING_INFORMATION::default();
            ok(
                unsafe {
                    QueryInformationJobObject(
                        self.job.as_raw_handle(),
                        JobObjectBasicAccountingInformation,
                        (&mut value as *mut JOBOBJECT_BASIC_ACCOUNTING_INFORMATION).cast(),
                        size_of_val(&value) as u32,
                        null_mut(),
                    )
                },
                "query sandbox job termination",
            )?;
            if value.ActiveProcesses == 0 {
                return Ok(());
            }
            ensure!(
                Instant::now() < deadline,
                "sandbox descendants did not reap"
            );
            std::thread::sleep(Duration::from_millis(10));
        }
    }
    fn restore_grants(&mut self) -> Result<()> {
        for grant in self.grants.iter().rev() {
            grant.restore()?;
        }
        self.grants.clear();
        for input in &self.inputs {
            input.unchanged()?;
        }
        Ok(())
    }
    fn delete_profile(&mut self) -> Result<()> {
        if let Some(profile) = &mut self.profile {
            profile.delete()?;
        }
        Ok(())
    }
}
impl Drop for SandboxProcess {
    fn drop(&mut self) {
        if self
            .terminate()
            .and_then(|()| self.restore_grants())
            .and_then(|()| self.delete_profile())
            .is_err()
        {
            // Fail closed on cleanup uncertainty: closing the job still kills
            // its tree, but do not revoke access beneath a possibly-live child
            // or delete its profile. Operator state needs explicit repair.
            std::mem::forget(std::mem::take(&mut self.grants));
            std::mem::forget(std::mem::take(&mut self.inputs));
            std::mem::forget(std::mem::take(&mut self.directories));
            if let Some(profile) = self.profile.take() {
                std::mem::forget(profile);
            }
            return;
        }
        self.inputs.clear();
        self.directories.clear();
    }
}

pub fn spawn(spec: ProcessSpec<'_>) -> Result<SandboxProcess> {
    ensure!(
        spec.runtime_files.len() <= 64 && spec.arguments.len() <= 128,
        "sandbox input limits exceeded"
    );
    let code = spec
        .executable
        .path
        .parent()
        .context("missing code directory")?
        .to_owned();
    ensure!(
        code.is_absolute()
            && spec.scratch.is_absolute()
            && code != spec.scratch
            && !code.starts_with(&spec.scratch)
            && !spec.scratch.starts_with(&code),
        "dedicated code/scratch directories required"
    );
    let code_directory = Directory::open(&code)?;
    let scratch_directory = Directory::open(&spec.scratch)?;
    ensure!(
        std::fs::read_dir(&spec.scratch)?.next().is_none(),
        "sandbox scratch must start empty"
    );
    let directory_handle = |path: &Path| {
        OpenOptions::new()
            .access_mode(READ_CONTROL | FILE_READ_ATTRIBUTES)
            .share_mode(FILE_SHARE_READ)
            .custom_flags(FILE_FLAG_BACKUP_SEMANTICS | FILE_FLAG_OPEN_REPARSE_POINT)
            .open(path)
    };
    let code_handle = directory_handle(&code)?;
    let scratch_handle = directory_handle(&spec.scratch)?;
    ensure!(
        file_identity(&code_handle)? != file_identity(&scratch_handle)?,
        "code and scratch alias the same directory"
    );
    let mut files = vec![spec.executable];
    files.extend(spec.runtime_files);
    let mut inputs = Vec::new();
    let mut total = 0;
    let mut identities = std::collections::HashSet::new();
    for source in &files {
        ensure!(
            source.path.parent() == Some(code.as_path()),
            "sandbox runtime bundle must be flat"
        );
        let mut input = Input::open(&source.path, MAX_CODE_BYTES)?;
        ensure!(
            identities.insert(file_identity(&input.file)?),
            "duplicate sandbox code identity"
        );
        let mut digest = Sha256::new();
        let mut buffer = [0u8; 65536];
        loop {
            let count = input.file.read(&mut buffer)?;
            if count == 0 {
                break;
            }
            total += count as u64;
            ensure!(total <= MAX_CODE_BYTES, "sandbox bundle exceeds byte limit");
            digest.update(&buffer[..count]);
        }
        ensure!(
            digest.finalize().as_slice() == source.sha256,
            "sandbox code digest mismatch"
        );
        input.unchanged()?;
        inputs.push(input);
    }
    let expected: std::collections::HashSet<_> = files
        .iter()
        .map(|file| file.path.file_name().map(OsStr::to_owned))
        .collect();
    let actual: std::collections::HashSet<_> = std::fs::read_dir(&code)?
        .map(|entry| entry.map(|entry| Some(entry.file_name())))
        .collect::<std::io::Result<_>>()?;
    ensure!(actual == expected, "code bundle contains unreviewed files");
    let profile = Profile::create()?;
    let mut grants = vec![Grant::apply(
        &code,
        Some(&code_handle),
        profile.sid,
        &[(FILE_GENERIC_READ | FILE_GENERIC_EXECUTE, 0)],
    )?];
    for (source, input) in files.iter().zip(&inputs) {
        grants.push(Grant::apply(
            &source.path,
            Some(&input.file),
            profile.sid,
            &[(FILE_GENERIC_READ | FILE_GENERIC_EXECUTE, 0)],
        )?);
    }
    // Root traversal plus separate inheritable file/directory rights. Scratch
    // files gain read/write/delete, not execute permission.
    let rw = FILE_GENERIC_READ | FILE_GENERIC_WRITE | DELETE;
    grants.push(Grant::apply(
        &spec.scratch,
        Some(&scratch_handle),
        profile.sid,
        &[
            (rw | FILE_TRAVERSE | FILE_DELETE_CHILD, 0),
            (rw, OBJECT_INHERIT_ACE | INHERIT_ONLY_ACE),
            (
                rw | FILE_TRAVERSE | FILE_DELETE_CHILD,
                CONTAINER_INHERIT_ACE | INHERIT_ONLY_ACE,
            ),
        ],
    )?);
    grants
        .last_mut()
        .context("missing scratch grant")?
        .scratch_integrity()?;
    let mut inherited = Vec::new();
    for original in [spec.stdin, spec.stdout, spec.stderr, spec.broker] {
        ensure!(
            unsafe { GetFileType(original.as_raw_handle()) } == FILE_TYPE_PIPE,
            "sandbox stdio must be broker pipes"
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
            "duplicate sandbox broker pipe",
        )?;
        inherited.push(unsafe { OwnedHandle::from_raw_handle(raw) });
    }
    let handles = [
        inherited[0].as_raw_handle(),
        inherited[1].as_raw_handle(),
        inherited[2].as_raw_handle(),
        inherited[3].as_raw_handle(),
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
    let executable = wide(files[0].path.as_os_str())?;
    let mut command = quote(files[0].path.as_os_str())?;
    for argument in spec.arguments {
        command.push(" ");
        command.push(quote(&argument)?);
    }
    let mut command = wide(&command)?;
    ensure!(command.len() <= 32767, "sandbox command too long");
    let directory = wide(spec.scratch.as_os_str())?;
    let mut windows = vec![0u16; 32768];
    let count =
        unsafe { GetWindowsDirectoryW(windows.as_mut_ptr(), windows.len() as u32) } as usize;
    ensure!(
        count > 0 && count < windows.len(),
        "Windows directory unavailable"
    );
    let windows = OsString::from_wide(&windows[..count]);
    let local = local_app_data()?;
    let broker = OsString::from(format!("{}", handles[3] as usize));
    let environment = [
        ("HOME", spec.scratch.as_os_str()),
        ("LOCALAPPDATA", local.as_os_str()),
        ("MEMORY_BROKER_HANDLE", broker.as_os_str()),
        ("PATH", code.as_os_str()),
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
    let job = job()?;
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
        "create suspended AppContainer",
    )?;
    let process = unsafe { OwnedHandle::from_raw_handle(info.hProcess) };
    let thread = unsafe { OwnedHandle::from_raw_handle(info.hThread) };
    let child = SandboxProcess {
        job,
        process,
        profile: Some(profile),
        grants,
        inputs,
        directories: vec![code_directory, scratch_directory],
        pid: info.dwProcessId,
    };
    verify_token(&child.process, capabilities.AppContainerSid)?;
    ok(
        unsafe {
            AssignProcessToJobObject(child.job.as_raw_handle(), child.process.as_raw_handle())
        },
        "fence AppContainer in job",
    )?;
    ensure!(
        unsafe { ResumeThread(thread.as_raw_handle()) } == 1,
        "could not resume fenced AppContainer"
    );
    Ok(child)
}

/// A Pi descendant of an already admitted driver. It inherits the existing
/// AppContainer and Job; it cannot create a new profile or a breakaway process.
pub struct InheritedProcess {
    process: OwnedHandle,
    pub stdin: Option<File>,
    pub stdout: Option<File>,
    pub stderr: Option<File>,
}
impl InheritedProcess {
    pub fn try_wait(&self) -> Result<Option<u32>> {
        let status = unsafe { WaitForSingleObject(self.process.as_raw_handle(), 0) };
        if status == WAIT_TIMEOUT {
            return Ok(None);
        }
        ensure!(status == WAIT_OBJECT_0, "could not poll inherited child");
        let mut code = 0;
        ok(
            unsafe { GetExitCodeProcess(self.process.as_raw_handle(), &mut code) },
            "read inherited child result",
        )?;
        Ok(Some(code))
    }
    pub fn terminate(&self) -> Result<()> {
        if self.try_wait()?.is_none() {
            ok(
                unsafe { TerminateProcess(self.process.as_raw_handle(), 1) },
                "terminate inherited child",
            )?;
        }
        ensure!(
            unsafe { WaitForSingleObject(self.process.as_raw_handle(), 5000) } == WAIT_OBJECT_0,
            "inherited child reap unconfirmed"
        );
        Ok(())
    }
}
impl Drop for InheritedProcess {
    fn drop(&mut self) {
        let _ = self.terminate();
    }
}

fn anonymous_pipe() -> Result<(File, File)> {
    let mut reader = null_mut();
    let mut writer = null_mut();
    let attrs = SECURITY_ATTRIBUTES {
        nLength: size_of::<SECURITY_ATTRIBUTES>() as u32,
        lpSecurityDescriptor: null_mut(),
        bInheritHandle: 0,
    };
    ok(
        unsafe { CreatePipe(&mut reader, &mut writer, &attrs, 4096) },
        "create driver RPC pipe",
    )?;
    Ok(unsafe { (File::from_raw_handle(reader), File::from_raw_handle(writer)) })
}

/// Safely own a fresh non-inheritable duplicate of a broker pipe handle value.
/// This proves ownership/type only, not admission: the parent's exact inherited
/// handle list and one-work bearer remain the broker authentication boundary.
pub fn duplicate_broker_pipe(raw: usize) -> Result<File> {
    ensure!(raw != 0 && raw != usize::MAX, "invalid broker handle value");
    let mut duplicate = null_mut();
    ok(
        unsafe {
            DuplicateHandle(
                GetCurrentProcess(),
                raw as _,
                GetCurrentProcess(),
                &mut duplicate,
                0,
                0,
                DUPLICATE_SAME_ACCESS,
            )
        },
        "duplicate inherited broker pipe",
    )?;
    // SAFETY: DuplicateHandle succeeded; this function owns only its new handle.
    // Never adopt/close the untrusted original or request DUPLICATE_CLOSE_SOURCE.
    let owned = unsafe { OwnedHandle::from_raw_handle(duplicate) };
    ensure!(
        unsafe { GetFileType(owned.as_raw_handle()) } == FILE_TYPE_PIPE,
        "broker handle is not a pipe"
    );
    // FILE_TYPE_PIPE alone also includes sockets; require an actual Win32 pipe.
    ok(
        unsafe {
            GetNamedPipeInfo(
                owned.as_raw_handle(),
                null_mut(),
                null_mut(),
                null_mut(),
                null_mut(),
            )
        },
        "validate inherited broker pipe",
    )?;
    Ok(File::from(owned))
}

/// Fixed phase classes for explicit native rehearsal diagnosis.
#[derive(Debug, Clone, Copy, PartialEq, Eq, serde::Serialize, serde::Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum InheritedSpawnPhase {
    ProcessPin,
    TokenInspection,
    CodeAccess,
    CodeDigest,
    PipeCreation,
    HandleDuplication,
    ProcessAttributes,
    Environment,
    ProcessCreation,
    ChildToken,
    JobVerification,
    Resume,
    Other,
}

/// Safe diagnostic metadata, never arbitrary error messages or paths. Emission
/// is the caller's explicit diagnostic-build concern, not a production log.
#[derive(Debug, Clone, Copy, PartialEq, Eq, serde::Serialize, serde::Deserialize)]
#[serde(deny_unknown_fields)]
pub struct InheritedSpawnDiagnostic {
    pub phase: InheritedSpawnPhase,
    pub raw_os_error: Option<i32>,
}

pub fn inherited_spawn_diagnostic(error: &anyhow::Error) -> InheritedSpawnDiagnostic {
    use InheritedSpawnPhase as Phase;
    let phase = error
        .chain()
        .find_map(|cause| {
            Some(match cause.to_string().as_str() {
                "pin driver process" => Phase::ProcessPin,
                "inspect driver token"
                | "driver is not an AppContainer"
                | "inspect driver AppContainer identity"
                | "verify driver AppContainer token" => Phase::TokenInspection,
                "open pinned Pi executable"
                | "read pinned Pi executable"
                | "driver code directory unavailable"
                | "locate current driver executable"
                | "inspect pinned Pi executable"
                | "Pi must be in the pinned driver bundle" => Phase::CodeAccess,
                "Pi artifact too large" | "Pi artifact digest mismatch" => Phase::CodeDigest,
                "create driver RPC pipe"
                | "create Pi stdin pipe"
                | "create Pi stdout pipe"
                | "create Pi stderr pipe" => Phase::PipeCreation,
                "driver handles must be pipes" | "duplicate Pi broker/RPC handle" => {
                    Phase::HandleDuplication
                }
                "initialize process attributes"
                | "set sandbox process attribute"
                | "prepare Pi process attributes" => Phase::ProcessAttributes,
                "missing admitted driver environment"
                | "invalid driver environment key"
                | "unapproved Pi environment override"
                | "invalid broker bearer"
                | "provider credentials cannot enter Pi"
                | "missing driver scratch" => Phase::Environment,
                "create inherited Pi process" => Phase::ProcessCreation,
                "open child token"
                | "child is not AppContainer"
                | "wrong child AppContainer SID"
                | "unexpected child network/other capabilities"
                | "query sandbox token" => Phase::ChildToken,
                "verify inherited Pi job"
                | "unexpected inherited job process count"
                | "Pi did not inherit admitted driver job" => Phase::JobVerification,
                "could not resume admitted Pi" => Phase::Resume,
                _ => return None,
            })
        })
        .unwrap_or(Phase::Other);
    let raw_os_error = error.chain().find_map(|cause| {
        cause
            .downcast_ref::<std::io::Error>()
            .and_then(std::io::Error::raw_os_error)
    });
    InheritedSpawnDiagnostic {
        phase,
        raw_os_error,
    }
}

/// Called only by the pinned driver inside its existing AppContainer. `broker`
/// is its already inherited one-work channel. Overrides are restricted to Pi's
/// private settings and a 64-hex one-work broker bearer, never provider keys.
pub fn spawn_inherited(
    executable: CodeFile,
    arguments: &[OsString],
    broker: BorrowedHandle<'_>,
    overrides: &[(OsString, OsString)],
) -> Result<InheritedProcess> {
    let mut raw = null_mut();
    ok(
        unsafe {
            DuplicateHandle(
                GetCurrentProcess(),
                GetCurrentProcess(),
                GetCurrentProcess(),
                &mut raw,
                0,
                0,
                DUPLICATE_SAME_ACCESS,
            )
        },
        "pin driver process",
    )?;
    let current = unsafe { OwnedHandle::from_raw_handle(raw) };
    let mut token = null_mut();
    ok(
        unsafe { OpenProcessToken(current.as_raw_handle(), TOKEN_QUERY, &mut token) },
        "inspect driver token",
    )?;
    let token = unsafe { OwnedHandle::from_raw_handle(token) };
    let app_info = token_information(&token, TokenAppContainerSid)
        .context("inspect driver AppContainer identity")?;
    let app_sid =
        unsafe { (*app_info.as_ptr().cast::<TOKEN_APPCONTAINER_INFORMATION>()).TokenAppContainer };
    ensure!(!app_sid.is_null(), "driver is not an AppContainer");
    verify_token(&current, app_sid).context("verify driver AppContainer token")?;
    let code = std::env::current_exe()
        .context("locate current driver executable")?
        .parent()
        .context("driver code directory unavailable")?
        .to_owned();
    ensure!(
        executable.path.parent() == Some(code.as_path()),
        "Pi must be in the pinned driver bundle"
    );
    let mut source = OpenOptions::new()
        .read(true)
        .share_mode(FILE_SHARE_READ)
        .custom_flags(FILE_FLAG_OPEN_REPARSE_POINT)
        .open(&executable.path)
        .context("open pinned Pi executable")?;
    ensure!(
        source
            .metadata()
            .context("inspect pinned Pi executable")?
            .len()
            <= MAX_CODE_BYTES,
        "Pi artifact too large"
    );
    let mut digest = Sha256::new();
    let mut bytes = [0u8; 65536];
    loop {
        let count = source
            .read(&mut bytes)
            .context("read pinned Pi executable")?;
        if count == 0 {
            break;
        }
        digest.update(&bytes[..count]);
    }
    ensure!(
        digest.finalize().as_slice() == executable.sha256,
        "Pi artifact digest mismatch"
    );
    let (stdin_read, stdin_write) = anonymous_pipe().context("create Pi stdin pipe")?;
    let (stdout_read, stdout_write) = anonymous_pipe().context("create Pi stdout pipe")?;
    let (stderr_read, stderr_write) = anonymous_pipe().context("create Pi stderr pipe")?;
    let mut inherited = Vec::new();
    for original in [
        stdin_read.as_raw_handle(),
        stdout_write.as_raw_handle(),
        stderr_write.as_raw_handle(),
        broker.as_raw_handle(),
    ] {
        ensure!(
            unsafe { GetFileType(original) } == FILE_TYPE_PIPE,
            "driver handles must be pipes"
        );
        let mut copy = null_mut();
        ok(
            unsafe {
                DuplicateHandle(
                    GetCurrentProcess(),
                    original,
                    GetCurrentProcess(),
                    &mut copy,
                    0,
                    1,
                    DUPLICATE_SAME_ACCESS,
                )
            },
            "duplicate Pi broker/RPC handle",
        )?;
        inherited.push(unsafe { OwnedHandle::from_raw_handle(copy) });
    }
    let handles = [
        inherited[0].as_raw_handle(),
        inherited[1].as_raw_handle(),
        inherited[2].as_raw_handle(),
        inherited[3].as_raw_handle(),
    ];
    let mut attrs = Attributes::new().context("prepare Pi process attributes")?;
    attrs.set(PROC_THREAD_ATTRIBUTE_HANDLE_LIST, &handles)?;
    let mut startup = STARTUPINFOEXW::default();
    startup.StartupInfo.cb = size_of::<STARTUPINFOEXW>() as u32;
    startup.StartupInfo.dwFlags = STARTF_USESTDHANDLES;
    startup.StartupInfo.hStdInput = handles[0];
    startup.StartupInfo.hStdOutput = handles[1];
    startup.StartupInfo.hStdError = handles[2];
    startup.lpAttributeList = attrs.pointer();
    let mut environment = std::collections::BTreeMap::<OsString, OsString>::new();
    for key in [
        "HOME",
        "LOCALAPPDATA",
        "PATH",
        "SystemRoot",
        "TEMP",
        "TMP",
        "USERPROFILE",
    ] {
        environment.insert(
            key.into(),
            std::env::var_os(key).context("missing admitted driver environment")?,
        );
    }
    environment.insert(
        "MEMORY_BROKER_HANDLE".into(),
        (handles[3] as usize).to_string().into(),
    );
    for (name, value) in overrides {
        let name_text = name.to_str().context("invalid driver environment key")?;
        ensure!(
            matches!(
                name_text,
                "PI_CODING_AGENT_DIR"
                    | "PI_PACKAGE_DIR"
                    | "PI_SESSIONS_DIR"
                    | "PI_CONFIG_PATH"
                    | "PI_CACHE_RETENTION"
                    | "OPENAI_API_KEY"
            ),
            "unapproved Pi environment override"
        );
        if name_text == "OPENAI_API_KEY" {
            let value = value.to_str().context("invalid broker bearer")?;
            ensure!(
                value.len() == 64 && value.bytes().all(|c| c.is_ascii_hexdigit()),
                "provider credentials cannot enter Pi"
            );
        }
        environment.insert(name.clone(), value.clone());
    }
    let mut env = Vec::new();
    for (name, value) in environment {
        env.extend(wide(&name)?.into_iter().filter(|c| *c != 0));
        env.push(b'=' as u16);
        env.extend(wide(&value)?);
    }
    env.push(0);
    let mut command = quote(executable.path.as_os_str())?;
    for arg in arguments {
        command.push(" ");
        command.push(quote(arg)?);
    }
    let mut command = wide(&command)?;
    ensure!(command.len() <= 32767, "Pi command too long");
    let exe = wide(executable.path.as_os_str())?;
    let cwd = wide(&std::env::var_os("HOME").context("missing driver scratch")?)?;
    let mut info = PROCESS_INFORMATION::default();
    ok(
        unsafe {
            CreateProcessW(
                exe.as_ptr(),
                command.as_mut_ptr(),
                null(),
                null(),
                1,
                CREATE_SUSPENDED
                    | CREATE_UNICODE_ENVIRONMENT
                    | CREATE_NO_WINDOW
                    | EXTENDED_STARTUPINFO_PRESENT,
                env.as_ptr().cast(),
                cwd.as_ptr(),
                &startup.StartupInfo,
                &mut info,
            )
        },
        "create inherited Pi process",
    )?;
    let child = InheritedProcess {
        process: unsafe { OwnedHandle::from_raw_handle(info.hProcess) },
        stdin: Some(stdin_write),
        stdout: Some(stdout_read),
        stderr: Some(stderr_read),
    };
    let thread = unsafe { OwnedHandle::from_raw_handle(info.hThread) };
    verify_token(&child.process, app_sid)?;
    // NULL denotes the driver's current inner job. Its exact process list must
    // contain both driver and the newly created Pi; an unrelated job is not enough.
    let mut ids = [0usize; 8];
    ok(
        unsafe {
            QueryInformationJobObject(
                null_mut(),
                JobObjectBasicProcessIdList,
                ids.as_mut_ptr().cast(),
                size_of_val(&ids) as u32,
                null_mut(),
            )
        },
        "verify inherited Pi job",
    )?;
    let header = ids.as_ptr().cast::<u32>();
    let count = unsafe { *header.add(1) } as usize;
    ensure!(count <= 2, "unexpected inherited job process count");
    let process_ids = unsafe {
        std::slice::from_raw_parts((ids.as_ptr().cast::<u8>()).add(8).cast::<usize>(), count)
    };
    ensure!(
        process_ids.contains(&(info.dwProcessId as usize))
            && process_ids.contains(&(unsafe { GetCurrentProcessId() } as usize)),
        "Pi did not inherit admitted driver job"
    );
    ensure!(
        unsafe { ResumeThread(thread.as_raw_handle()) } == 1,
        "could not resume admitted Pi"
    );
    Ok(child)
}

#[cfg(test)]
mod tests {
    use super::*;
    #[test]
    fn inherited_spawn_diagnostic_preserves_stage_and_os_code_without_error_text() {
        let error = anyhow::Error::from(std::io::Error::from_raw_os_error(5))
            .context("create driver RPC pipe")
            .context("private source path or bearer must never enter metadata");
        let diagnostic = inherited_spawn_diagnostic(&error);
        assert_eq!(diagnostic.phase, InheritedSpawnPhase::PipeCreation);
        assert_eq!(diagnostic.raw_os_error, Some(5));
        assert_eq!(
            serde_json::to_value(diagnostic).expect("typed metadata"),
            serde_json::json!({"phase":"pipe_creation","raw_os_error":5})
        );
        let other = inherited_spawn_diagnostic(&anyhow::anyhow!("unreviewed secret/path"));
        assert_eq!(other.phase, InheritedSpawnPhase::Other);
        assert_eq!(other.raw_os_error, None);
        let driver = inherited_spawn_diagnostic(
            &anyhow::anyhow!("query sandbox token").context("verify driver AppContainer token"),
        );
        assert_eq!(driver.phase, InheritedSpawnPhase::TokenInspection);
        assert!(
            serde_json::from_value::<InheritedSpawnDiagnostic>(
                serde_json::json!({"phase":"pipe_creation","raw_os_error":5,"stderr":"private"})
            )
            .is_err()
        );
    }
    use std::io::Write;
    #[test]
    fn broker_duplicate_rejects_invalid_pseudo_and_disk_handles_without_adoption() {
        for value in [0, usize::MAX, usize::MAX - 1] {
            assert!(duplicate_broker_pipe(value).is_err());
        }
        let path =
            std::env::temp_dir().join(format!("memory-broker-disk-{}", uuid::Uuid::new_v4()));
        let mut original = OpenOptions::new()
            .read(true)
            .write(true)
            .create_new(true)
            .open(&path)
            .unwrap();
        assert!(duplicate_broker_pipe(original.as_raw_handle() as usize).is_err());
        original.write_all(b"original remains usable").unwrap();
        drop(original);
        std::fs::remove_file(path).unwrap();
    }
    #[test]
    fn broker_duplicate_is_noninheritable_and_does_not_close_original() {
        use windows_sys::Win32::Foundation::{GetHandleInformation, HANDLE_FLAG_INHERIT};
        let (mut original, mut writer) = anonymous_pipe().unwrap();
        let raw = original.as_raw_handle() as usize;
        let mut duplicate = duplicate_broker_pipe(raw).unwrap();
        let mut flags = 0;
        ok(
            unsafe { GetHandleInformation(duplicate.as_raw_handle(), &mut flags) },
            "read duplicate flags",
        )
        .unwrap();
        assert_eq!(flags & HANDLE_FLAG_INHERIT, 0);
        writer.write_all(b"ab").unwrap();
        let mut byte = [0; 1];
        duplicate.read_exact(&mut byte).unwrap();
        assert_eq!(&byte, b"a");
        drop(duplicate);
        original.read_exact(&mut byte).unwrap();
        assert_eq!(&byte, b"b");
        drop(original);
        // No allocation intervenes between closure and duplicate attempt.
        assert!(duplicate_broker_pipe(raw).is_err());
    }
    #[test]
    fn native_local_app_data_comes_from_current_identity_known_folder() {
        let folder = local_app_data().unwrap();
        assert!(Path::new(&folder).is_absolute());
        assert!(Path::new(&folder).is_dir());
        assert!(!folder.encode_wide().any(|unit| unit == 0));
        assert!(folder.encode_wide().count() < 32_768);
    }
    #[test]
    fn default_and_evaluation_job_profiles_remain_distinct() {
        for (job, processes, bytes) in [
            (job().unwrap(), 2, JOB_MEMORY_BYTES),
            (
                job_with_limits(16, 2 * 1024 * 1024 * 1024).unwrap(),
                16,
                2 * 1024 * 1024 * 1024,
            ),
        ] {
            let mut limits = JOBOBJECT_EXTENDED_LIMIT_INFORMATION::default();
            ok(
                unsafe {
                    QueryInformationJobObject(
                        job.as_raw_handle(),
                        JobObjectExtendedLimitInformation,
                        (&mut limits as *mut JOBOBJECT_EXTENDED_LIMIT_INFORMATION).cast(),
                        size_of_val(&limits) as u32,
                        null_mut(),
                    )
                },
                "query tested job limits",
            )
            .unwrap();
            assert_eq!(limits.BasicLimitInformation.ActiveProcessLimit, processes);
            assert_eq!(limits.JobMemoryLimit, bytes);
            assert_eq!(limits.ProcessMemoryLimit, bytes);
            assert_ne!(
                limits.BasicLimitInformation.LimitFlags & JOB_OBJECT_LIMIT_KILL_ON_JOB_CLOSE,
                0
            );
        }
    }
    #[test]
    fn process_arguments_are_not_shell_commands() {
        assert_eq!(quote(OsStr::new("a b")).unwrap(), OsString::from("\"a b\""));
        assert_eq!(
            quote(OsStr::new("a\"b")).unwrap(),
            OsString::from("\"a\\\"b\"")
        );
        assert_eq!(
            quote(OsStr::new("a\\")).unwrap(),
            OsString::from("\"a\\\\\"")
        );
        assert!(quote(OsStr::new("a\0b")).is_err());
        assert!(wide(OsStr::new("a\0b")).is_err());
    }
    #[test]
    fn profile_deletion_failure_retains_identity_for_exact_retry() {
        let mut profile = Profile::create().unwrap();
        let identity = profile.name.clone();
        assert!(profile.delete_with(|_| 0x80004005u32 as i32).is_err());
        assert!(!profile.deleted);
        assert_eq!(profile.name, identity);
        profile.delete().unwrap();
        assert!(profile.deleted);
        profile
            .delete_with(|_| {
                panic!("already deleted profile must not be recreated or removed twice")
            })
            .unwrap();
    }
}
