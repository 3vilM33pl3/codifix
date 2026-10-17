// SPDX-License-Identifier: AGPL-3.0-or-later
//! Native protected files. Every ancestor is pinned without delete sharing;
//! reparse points, alternate streams, ambiguous names and multi-linked files
//! are rejected. The current identity, SYSTEM and administrators are trusted.
//! No ACL validation is delegated to a pathname-based subprocess.
use std::{
    ffi::{OsStr, c_void},
    fs::{File, OpenOptions},
    io::{self, Read, Seek, SeekFrom},
    mem::{offset_of, size_of, zeroed},
    os::windows::{
        ffi::OsStrExt,
        fs::OpenOptionsExt,
        io::{AsRawHandle, FromRawHandle, OwnedHandle},
    },
    path::{Component, Path, PathBuf, Prefix},
    ptr::{addr_of_mut, null_mut},
};
use windows_sys::Win32::{
    Foundation::{GENERIC_ALL, GENERIC_READ, GENERIC_WRITE, INVALID_HANDLE_VALUE, LocalFree},
    Security::{
        ACCESS_ALLOWED_ACE, ACE_HEADER,
        Authorization::{
            ConvertSidToStringSidW, ConvertStringSecurityDescriptorToSecurityDescriptorW,
            GetSecurityInfo, SDDL_REVISION_1, SE_FILE_OBJECT,
        },
        DACL_SECURITY_INFORMATION, GetAce, GetTokenInformation, INHERIT_ONLY_ACE,
        OWNER_SECURITY_INFORMATION, SECURITY_ATTRIBUTES, TOKEN_QUERY, TOKEN_USER, TokenUser,
    },
    Storage::FileSystem::{
        BY_HANDLE_FILE_INFORMATION, CREATE_NEW, CreateDirectoryW, CreateFileW, DELETE,
        FILE_ATTRIBUTE_DIRECTORY, FILE_ATTRIBUTE_NORMAL, FILE_ATTRIBUTE_REPARSE_POINT,
        FILE_DELETE_CHILD, FILE_FLAG_BACKUP_SEMANTICS, FILE_FLAG_OPEN_REPARSE_POINT,
        FILE_READ_ATTRIBUTES, FILE_RENAME_INFO, FILE_SHARE_DELETE, FILE_SHARE_READ,
        FILE_SHARE_WRITE, FileRenameInfoEx, GetFileInformationByHandle, MOVEFILE_REPLACE_EXISTING,
        MOVEFILE_WRITE_THROUGH, MoveFileExW, OPEN_ALWAYS, OPEN_EXISTING, READ_CONTROL,
        SetFileInformationByHandle, WRITE_DAC, WRITE_OWNER,
    },
    System::Threading::{GetCurrentProcess, OpenProcessToken},
};

fn denied() -> io::Error {
    io::Error::new(
        io::ErrorKind::PermissionDenied,
        "protected Windows file policy rejected",
    )
}

fn wide(value: &OsStr) -> Vec<u16> {
    value.encode_wide().chain(Some(0)).collect()
}

fn valid_name(value: &OsStr) -> io::Result<()> {
    let value = value.to_str().ok_or_else(denied)?;
    if value.is_empty()
        || value.ends_with(['.', ' '])
        || value
            .chars()
            .any(|c| c.is_control() || "<>:\"/\\|?*".contains(c))
    {
        return Err(denied());
    }
    let stem = value
        .split('.')
        .next()
        .unwrap_or_default()
        .trim_end_matches(['.', ' '])
        .to_ascii_uppercase();
    if matches!(
        stem.as_str(),
        "CON" | "PRN" | "AUX" | "NUL" | "CONIN$" | "CONOUT$"
    ) || ["COM", "LPT"].iter().any(|prefix| {
        stem.strip_prefix(prefix).is_some_and(|suffix| {
            matches!(
                suffix,
                "1" | "2" | "3" | "4" | "5" | "6" | "7" | "8" | "9" | "¹" | "²" | "³"
            )
        })
    }) {
        return Err(denied());
    }
    Ok(())
}

fn validate_path(path: &Path) -> io::Result<()> {
    let mut parts = path.components();
    if !matches!(parts.next(), Some(Component::Prefix(p)) if matches!(p.kind(), Prefix::Disk(_) | Prefix::VerbatimDisk(_)))
        || parts.next() != Some(Component::RootDir)
    {
        return Err(denied());
    }
    for part in parts {
        match part {
            Component::Normal(name) => valid_name(name)?,
            _ => return Err(denied()),
        }
    }
    Ok(())
}

// Raw Win32 creation calls do not receive Rust std's extended-length path
// conversion. Rebuild only already-validated local drive paths; never turn a
// relative, UNC, device, traversal or reserved-name path into an accepted one.
fn creation_path(path: &Path) -> io::Result<Vec<u16>> {
    validate_path(path)?;
    let mut parts = path.components();
    let drive = match parts.next() {
        Some(Component::Prefix(prefix)) => match prefix.kind() {
            Prefix::Disk(drive) | Prefix::VerbatimDisk(drive) => drive,
            _ => return Err(denied()),
        },
        _ => return Err(denied()),
    };
    if parts.next() != Some(Component::RootDir) {
        return Err(denied());
    }
    let mut extended = PathBuf::from(format!("\\\\?\\{}:\\", char::from(drive)));
    for part in parts {
        match part {
            Component::Normal(name) => extended.push(name),
            _ => return Err(denied()),
        }
    }
    Ok(wide(extended.as_os_str()))
}

struct LocalAllocation(*mut c_void);
impl Drop for LocalAllocation {
    fn drop(&mut self) {
        // SAFETY: all instances own a buffer allocated by a Win32 LocalAlloc API.
        unsafe { LocalFree(self.0) };
    }
}

fn sid_string(sid: *mut c_void) -> io::Result<String> {
    if sid.is_null() {
        return Err(denied());
    }
    let mut value = null_mut();
    // SAFETY: caller supplies a SID owned by a live token/security descriptor.
    if unsafe { ConvertSidToStringSidW(sid, &mut value) } == 0 {
        return Err(io::Error::last_os_error());
    }
    let _allocation = LocalAllocation(value.cast());
    let mut length = 0;
    // SAFETY: successful ConvertSidToStringSidW returns a NUL-terminated buffer.
    unsafe {
        while *value.add(length) != 0 {
            length += 1;
        }
        String::from_utf16(std::slice::from_raw_parts(value, length)).map_err(|_| denied())
    }
}

fn current_sid() -> io::Result<String> {
    let mut raw = null_mut();
    // SAFETY: valid current process pseudo-handle, output pointer lives through call.
    if unsafe { OpenProcessToken(GetCurrentProcess(), TOKEN_QUERY, &mut raw) } == 0 {
        return Err(io::Error::last_os_error());
    }
    // SAFETY: successful OpenProcessToken transfers an owned handle.
    let token = unsafe { OwnedHandle::from_raw_handle(raw) };
    let mut length = 0;
    // SAFETY: sizing call, no output buffer, expected insufficient-buffer response.
    unsafe { GetTokenInformation(token.as_raw_handle(), TokenUser, null_mut(), 0, &mut length) };
    if length < size_of::<TOKEN_USER>() as u32 || length > 65_536 {
        return Err(denied());
    }
    // usize storage supplies alignment for TOKEN_USER and its trailing SID.
    let mut buffer = vec![0usize; (length as usize).div_ceil(size_of::<usize>())];
    // SAFETY: aligned output is at least the reported size and token remains open.
    if unsafe {
        GetTokenInformation(
            token.as_raw_handle(),
            TokenUser,
            buffer.as_mut_ptr().cast(),
            length,
            &mut length,
        )
    } == 0
    {
        return Err(io::Error::last_os_error());
    }
    // SAFETY: successful TokenUser response starts with TOKEN_USER.
    sid_string(unsafe { (*buffer.as_ptr().cast::<TOKEN_USER>()).User.Sid })
}

fn trusted(sid: &str, current: &str, ancestor: bool) -> bool {
    sid == current
        || sid == "S-1-5-18"
        || sid == "S-1-5-32-544"
        || (ancestor && sid == "S-1-5-80-956008885-3418522649-1831038044-1853292631-2271478464")
}

fn acl(file: &File, private: bool) -> io::Result<()> {
    acl_reader(file, private, None)
}

fn acl_reader(file: &File, private: bool, reader: Option<&ServiceReader>) -> io::Result<()> {
    let mut owner = null_mut();
    let mut dacl = null_mut();
    let mut descriptor = null_mut();
    // SAFETY: handle is live, outputs belong to the returned security descriptor.
    let error = unsafe {
        GetSecurityInfo(
            file.as_raw_handle(),
            SE_FILE_OBJECT,
            OWNER_SECURITY_INFORMATION | DACL_SECURITY_INFORMATION,
            &mut owner,
            null_mut(),
            &mut dacl,
            null_mut(),
            &mut descriptor,
        )
    };
    if error != 0 {
        return Err(io::Error::from_raw_os_error(error as i32));
    }
    let _allocation = LocalAllocation(descriptor);
    let current = current_sid()?;
    if dacl.is_null() {
        return Err(io::Error::new(
            io::ErrorKind::PermissionDenied,
            "protected file has no DACL",
        ));
    }
    if !trusted(&sid_string(owner)?, &current, !private) {
        return Err(io::Error::new(
            io::ErrorKind::PermissionDenied,
            "protected file owner is untrusted",
        ));
    }
    // SAFETY: GetSecurityInfo returned a valid ACL in the live descriptor.
    let count = unsafe { (*dacl).AceCount };
    for index in 0..u32::from(count) {
        let mut raw = null_mut();
        // SAFETY: index is within the valid ACL's ACE count.
        if unsafe { GetAce(dacl, index, &mut raw) } == 0 {
            return Err(io::Error::last_os_error());
        }
        // SAFETY: GetAce returns an ACE starting with ACE_HEADER.
        let header = unsafe { &*raw.cast::<ACE_HEADER>() };
        if u32::from(header.AceFlags) & INHERIT_ONLY_ACE != 0 {
            continue;
        }
        match header.AceType {
            1 => continue,             // ACCESS_DENIED_ACE can only narrow access.
            0 => {}                    // ACCESS_ALLOWED_ACE
            _ => return Err(denied()), // Object/callback policies require explicit review.
        }
        if usize::from(header.AceSize) < size_of::<ACCESS_ALLOWED_ACE>() {
            return Err(denied());
        }
        // SAFETY: size/type validated, SID is inline at SidStart in the live ACE.
        let ace = unsafe { &mut *raw.cast::<ACCESS_ALLOWED_ACE>() };
        let sid = sid_string(addr_of_mut!(ace.SidStart).cast())?;
        if reader.is_some_and(|reader| reader.sid == sid) {
            // One SCM-derived extra principal may READ, never own/control or
            // mutate this object. No service wildcard enters normal policy.
            let allowed = GENERIC_READ
                | windows_sys::Win32::Foundation::GENERIC_EXECUTE
                | windows_sys::Win32::Storage::FileSystem::FILE_GENERIC_READ
                | windows_sys::Win32::Storage::FileSystem::FILE_GENERIC_EXECUTE;
            if ace.Mask & !allowed != 0 {
                return Err(io::Error::new(
                    io::ErrorKind::PermissionDenied,
                    "service reader ACL exceeds read-only access",
                ));
            }
            continue;
        }
        if !trusted(&sid, &current, !private) {
            // Ancestors may allow creating unrelated children. Their open handles
            // deny replacement; no untrusted identity may delete/control this chain.
            let dangerous =
                GENERIC_ALL | GENERIC_WRITE | WRITE_DAC | WRITE_OWNER | DELETE | FILE_DELETE_CHILD;
            if private && ace.Mask != 0 || !private && ace.Mask & dangerous != 0 {
                return Err(io::Error::new(
                    io::ErrorKind::PermissionDenied,
                    "protected file ACL includes an unapproved principal",
                ));
            }
        }
    }
    Ok(())
}

/// An elevated operator's explicit read-only exception for ONE exact virtual
/// service account resolved from SCM, never an identity from configuration.
pub struct ServiceReader {
    sid: String,
}
impl ServiceReader {
    pub fn for_named_service(name: &str) -> io::Result<Self> {
        use windows_sys::Win32::{
            Security::{
                Authorization::ConvertStringSidToSidW, CheckTokenMembership, LookupAccountNameW,
            },
            System::Services::{
                CloseServiceHandle, OpenSCManagerW, OpenServiceW, QUERY_SERVICE_CONFIGW,
                QueryServiceConfigW, SC_MANAGER_CONNECT, SERVICE_QUERY_CONFIG,
            },
        };
        if !name.starts_with("MemoryLayer-")
            || name.len() > 60
            || !name.bytes().all(|b| b.is_ascii_alphanumeric() || b == b'-')
        {
            return Err(denied());
        }
        let mut admin = null_mut();
        if unsafe { ConvertStringSidToSidW(wide(OsStr::new("S-1-5-32-544")).as_ptr(), &mut admin) }
            == 0
        {
            return Err(io::Error::last_os_error());
        }
        let _admin = LocalAllocation(admin);
        let mut member = 0;
        if unsafe { CheckTokenMembership(null_mut(), admin, &mut member) } == 0
            || (member == 0 && current_sid()? != "S-1-5-18")
        {
            return Err(denied());
        }
        struct Sc(*mut c_void);
        impl Drop for Sc {
            fn drop(&mut self) {
                unsafe {
                    CloseServiceHandle(self.0);
                }
            }
        }
        let raw = unsafe { OpenSCManagerW(std::ptr::null(), std::ptr::null(), SC_MANAGER_CONNECT) };
        if raw.is_null() {
            return Err(io::Error::last_os_error());
        }
        let manager = Sc(raw);
        let raw = unsafe {
            OpenServiceW(
                manager.0,
                wide(OsStr::new(name)).as_ptr(),
                SERVICE_QUERY_CONFIG,
            )
        };
        if raw.is_null() {
            return Err(io::Error::last_os_error());
        }
        let service = Sc(raw);
        let mut needed = 0;
        unsafe {
            QueryServiceConfigW(service.0, null_mut(), 0, &mut needed);
        }
        if needed < size_of::<QUERY_SERVICE_CONFIGW>() as u32 || needed > 65536 {
            return Err(denied());
        }
        let mut buffer = vec![0usize; (needed as usize).div_ceil(size_of::<usize>())];
        if unsafe {
            QueryServiceConfigW(service.0, buffer.as_mut_ptr().cast(), needed, &mut needed)
        } == 0
        {
            return Err(io::Error::last_os_error());
        }
        let config = unsafe { &*buffer.as_ptr().cast::<QUERY_SERVICE_CONFIGW>() };
        let account = format!("NT SERVICE\\{name}");
        let expected = wide(OsStr::new(&account));
        let pointer = config.lpServiceStartName as usize;
        let begin = buffer.as_ptr() as usize;
        if pointer < begin
            || pointer
                .checked_add(expected.len() * 2)
                .is_none_or(|end| end > begin + buffer.len() * size_of::<usize>())
        {
            return Err(denied());
        }
        let actual =
            unsafe { std::slice::from_raw_parts(config.lpServiceStartName, expected.len()) };
        if actual.last() != Some(&0)
            || !String::from_utf16(&actual[..actual.len() - 1])
                .map_err(|_| denied())?
                .eq_ignore_ascii_case(&account)
        {
            return Err(denied());
        }
        let (mut sid_len, mut domain_len, mut kind) = (0, 0, 0);
        unsafe {
            LookupAccountNameW(
                std::ptr::null(),
                expected.as_ptr(),
                null_mut(),
                &mut sid_len,
                null_mut(),
                &mut domain_len,
                &mut kind,
            );
        }
        if sid_len == 0 || sid_len > 256 || domain_len > 1024 {
            return Err(denied());
        }
        let mut sid = vec![0usize; (sid_len as usize).div_ceil(size_of::<usize>())];
        let mut domain = vec![0u16; domain_len as usize];
        if unsafe {
            LookupAccountNameW(
                std::ptr::null(),
                expected.as_ptr(),
                sid.as_mut_ptr().cast(),
                &mut sid_len,
                domain.as_mut_ptr(),
                &mut domain_len,
                &mut kind,
            )
        } == 0
        {
            return Err(io::Error::last_os_error());
        }
        let sid = sid_string(sid.as_mut_ptr().cast())?;
        if !sid.starts_with("S-1-5-80-") {
            return Err(denied());
        }
        Ok(Self { sid })
    }
}

/// Read-only input with an explicit reader exception. Normal Input/Directory
/// remain unchanged. Every ancestor and final entry retains its usual checks.
pub struct ServiceInput {
    file: File,
    before: Identity,
    ancestors: Vec<File>,
    path: PathBuf,
    reader: ServiceReader,
}
impl ServiceInput {
    pub fn open(path: &Path, limit: u64, reader: &ServiceReader) -> io::Result<Self> {
        validate_path(path)?;
        let mut current = PathBuf::new();
        let mut ancestors = Vec::new();
        let parent = path.parent().ok_or_else(denied)?;
        for (index, component) in parent.components().enumerate() {
            current.push(component.as_os_str());
            if index == 0 {
                continue;
            }
            let file = OpenOptions::new()
                .read(true)
                .access_mode(GENERIC_READ | READ_CONTROL)
                .share_mode(FILE_SHARE_READ)
                .custom_flags(FILE_FLAG_OPEN_REPARSE_POINT | FILE_FLAG_BACKUP_SEMANTICS)
                .open(&current)?;
            information(&file, true)?;
            acl_reader(&file, current == parent, Some(reader)).map_err(|error| {
                io::Error::new(
                    error.kind(),
                    format!(
                        "service authority ancestor policy (depth {index}, private {}): {error}",
                        current == parent
                    ),
                )
            })?;
            ancestors.push(file);
        }
        if ancestors.is_empty() {
            return Err(denied());
        }
        let file = OpenOptions::new()
            .read(true)
            .access_mode(GENERIC_READ | READ_CONTROL)
            .share_mode(FILE_SHARE_READ)
            .custom_flags(FILE_FLAG_OPEN_REPARSE_POINT)
            .open(path)?;
        let before = information(&file, false)?;
        acl_reader(&file, true, Some(reader)).map_err(|error| {
            io::Error::new(
                error.kind(),
                format!("service authority file policy: {error}"),
            )
        })?;
        if before.size > limit {
            return Err(denied());
        }
        Ok(Self {
            file,
            before,
            ancestors,
            path: path.to_owned(),
            reader: ServiceReader {
                sid: reader.sid.clone(),
            },
        })
    }
    pub fn read(mut self, limit: u64) -> io::Result<Vec<u8>> {
        let mut bytes = Vec::new();
        self.file
            .by_ref()
            .take(limit.checked_add(1).ok_or_else(denied)?)
            .read_to_end(&mut bytes)?;
        let entry = OpenOptions::new()
            .read(true)
            .access_mode(READ_CONTROL | FILE_READ_ATTRIBUTES)
            .share_mode(FILE_SHARE_READ | FILE_SHARE_WRITE | FILE_SHARE_DELETE)
            .custom_flags(FILE_FLAG_OPEN_REPARSE_POINT)
            .open(&self.path)?;
        acl_reader(&self.file, true, Some(&self.reader))?;
        acl_reader(&entry, true, Some(&self.reader))?;
        if bytes.len() as u64 > limit
            || self.before != information(&self.file, false)?
            || self.before != information(&entry, false)?
        {
            return Err(denied());
        }
        let _ = self.ancestors;
        Ok(bytes)
    }
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct Identity {
    volume: u32,
    index: u64,
    size: u64,
    written: u64,
    links: u32,
}
impl Identity {
    /// Stable filesystem object identity, excluding mutable length/timestamps.
    pub fn stable_key(self) -> (u64, u64) {
        (u64::from(self.volume), self.index)
    }
}

fn information(file: &File, directory: bool) -> io::Result<Identity> {
    // SAFETY: zero is a valid initial value for this plain Win32 output struct.
    let mut info: BY_HANDLE_FILE_INFORMATION = unsafe { zeroed() };
    // SAFETY: live file handle, correctly sized output pointer.
    if unsafe { GetFileInformationByHandle(file.as_raw_handle(), &mut info) } == 0 {
        return Err(io::Error::last_os_error());
    }
    if info.dwFileAttributes & FILE_ATTRIBUTE_REPARSE_POINT != 0
        || (info.dwFileAttributes & FILE_ATTRIBUTE_DIRECTORY != 0) != directory
        || (!directory && info.nNumberOfLinks != 1)
    {
        return Err(denied());
    }
    Ok(Identity {
        volume: info.dwVolumeSerialNumber,
        index: u64::from(info.nFileIndexHigh) << 32 | u64::from(info.nFileIndexLow),
        size: u64::from(info.nFileSizeHigh) << 32 | u64::from(info.nFileSizeLow),
        written: u64::from(info.ftLastWriteTime.dwHighDateTime) << 32
            | u64::from(info.ftLastWriteTime.dwLowDateTime),
        links: info.nNumberOfLinks,
    })
}

fn open(path: &Path, directory: bool, private: bool) -> io::Result<File> {
    let file = OpenOptions::new()
        .read(true)
        // Directory data access makes deny-delete sharing an actual rename
        // fence; metadata-only handles do not participate in that share check.
        // Write sharing permits child insertion without sharing directory delete.
        .access_mode(GENERIC_READ | READ_CONTROL)
        .share_mode(FILE_SHARE_READ | if directory { FILE_SHARE_WRITE } else { 0 })
        .custom_flags(FILE_FLAG_OPEN_REPARSE_POINT | FILE_FLAG_BACKUP_SEMANTICS)
        .open(path)?;
    information(&file, directory)?;
    acl(&file, private)?;
    Ok(file)
}

/// Keep ancestors open so another identity cannot replace the verified path.
pub struct Directory {
    path: PathBuf,
    _ancestors: Vec<File>,
}
impl Directory {
    pub fn open(path: &Path) -> io::Result<Self> {
        validate_path(path)?;
        let mut current = PathBuf::new();
        let mut pinned = Vec::new();
        let count = path.components().count();
        for (index, component) in path.components().enumerate() {
            current.push(component.as_os_str());
            if index == 0 {
                continue;
            } // Open only after C:\ has been assembled.
            pinned.push(open(&current, true, index + 1 == count)?);
        }
        if pinned.is_empty() {
            return Err(denied());
        }
        Ok(Self {
            path: path.to_owned(),
            _ancestors: pinned,
        })
    }

    pub fn create(&self, name: &str) -> io::Result<File> {
        valid_name(OsStr::new(name))?;
        create_private_file(&self.path.join(name))
    }

    pub fn identity(&self) -> io::Result<Identity> {
        information(self._ancestors.last().ok_or_else(denied)?, true)
    }

    /// Physical ancestry, including self, independent of path casing/8.3 names.
    pub fn is_beneath(&self, other: &Self) -> io::Result<bool> {
        let wanted = other.identity()?.stable_key();
        for file in &self._ancestors {
            if information(file, true)?.stable_key() == wanted {
                return Ok(true);
            }
        }
        Ok(false)
    }

    /// Resume an existing, private operation-owned staging file without
    /// truncation. Ordinary competing content writers are denied by sharing.
    pub fn resume_private_file(&self, name: &str) -> io::Result<File> {
        valid_name(OsStr::new(name))?;
        let file = create_file(&self.path.join(name), false, true)?;
        self.check_entry(name, &file)?;
        Ok(file)
    }

    pub fn check_entry(&self, name: &str, file: &File) -> io::Result<()> {
        valid_name(OsStr::new(name))?;
        // Metadata-only lookup must coexist with the creator's writable handle.
        // The original input handle still denies concurrent content writers.
        let entry = OpenOptions::new()
            .read(true)
            .access_mode(READ_CONTROL | FILE_READ_ATTRIBUTES)
            .share_mode(FILE_SHARE_READ | FILE_SHARE_WRITE | FILE_SHARE_DELETE)
            .custom_flags(FILE_FLAG_OPEN_REPARSE_POINT | FILE_FLAG_BACKUP_SEMANTICS)
            .open(self.path.join(name))?;
        acl(&entry, true)?;
        acl(file, true)?;
        if information(file, false)? != information(&entry, false)? {
            return Err(denied());
        }
        Ok(())
    }

    pub fn ensure_absent(&self, names: &[&str]) -> io::Result<()> {
        for name in names {
            valid_name(OsStr::new(name))?;
            match std::fs::symlink_metadata(self.path.join(name)) {
                Err(error) if error.kind() == io::ErrorKind::NotFound => {}
                _ => return Err(denied()),
            }
        }
        Ok(())
    }

    pub fn publish(&self, partial: &str, name: &str, file: &File) -> io::Result<()> {
        self.check_entry(partial, file)?;
        valid_name(OsStr::new(name))?;
        file.sync_all()?;
        move_file(&self.path.join(partial), &self.path.join(name), false)?;
        self.check_entry(name, file)
    }

    pub fn sync(&self) -> io::Result<()> {
        // File flushes and MOVEFILE_WRITE_THROUGH are the Windows contract.
        // Directory FlushFileBuffers is unsupported; no power-loss claim here.
        Ok(())
    }
}

pub struct Input {
    pub file: File,
    before: Identity,
    directory: Directory,
    name: String,
    replacement_review: bool,
}
impl Input {
    pub fn identity(&self) -> Identity {
        self.before
    }
    pub fn open(path: &Path, limit: u64) -> io::Result<Self> {
        Self::open_mode(path, limit, false)
    }

    /// Settings-only review handle for an atomic replacement which must retain
    /// the old readable object. Windows requires delete sharing on that object.
    /// Content writers stay fenced, but same-user pathname renames are possible:
    /// callers must serialize their operations and recheck identity/bytes before
    /// publication. This is not an OS compare-and-swap against arbitrary renamers.
    pub fn open_for_reviewed_replacement(path: &Path, limit: u64) -> io::Result<Self> {
        Self::open_mode(path, limit, true)
    }

    fn open_mode(path: &Path, limit: u64, replacement_review: bool) -> io::Result<Self> {
        validate_path(path)?;
        let directory = Directory::open(path.parent().ok_or_else(denied)?)?;
        let name = path
            .file_name()
            .and_then(OsStr::to_str)
            .ok_or_else(denied)?
            .to_owned();
        let file = if replacement_review {
            let file = OpenOptions::new()
                .read(true)
                .access_mode(GENERIC_READ | READ_CONTROL)
                .share_mode(FILE_SHARE_READ | FILE_SHARE_DELETE)
                .custom_flags(FILE_FLAG_OPEN_REPARSE_POINT)
                .open(path)?;
            information(&file, false)?;
            acl(&file, true)?;
            file
        } else {
            open(path, false, true)?
        };
        let before = information(&file, false)?;
        if before.size > limit {
            return Err(denied());
        }
        Ok(Self {
            file,
            before,
            directory,
            name,
            replacement_review,
        })
    }

    pub fn unchanged(&self) -> io::Result<()> {
        self.directory.check_entry(&self.name, &self.file)?;
        if self.before != information(&self.file, false)? {
            return Err(denied());
        }
        Ok(())
    }

    pub fn read(mut self, limit: u64) -> io::Result<Vec<u8>> {
        let mut bytes = Vec::new();
        self.file
            .by_ref()
            .take(limit.checked_add(1).ok_or_else(denied)?)
            .read_to_end(&mut bytes)?;
        if bytes.len() as u64 > limit {
            return Err(denied());
        }
        self.unchanged()?;
        Ok(bytes)
    }
}

fn security(directory: bool) -> io::Result<LocalAllocation> {
    let sid = current_sid()?;
    let inherit = if directory { "OICI" } else { "" };
    let text =
        format!("O:{sid}D:P(A;{inherit};FA;;;SY)(A;{inherit};FA;;;BA)(A;{inherit};FA;;;{sid})");
    let text = wide(OsStr::new(&text));
    let mut descriptor = null_mut();
    // SAFETY: valid NUL-terminated SDDL; output allocation is owned below.
    if unsafe {
        ConvertStringSecurityDescriptorToSecurityDescriptorW(
            text.as_ptr(),
            SDDL_REVISION_1,
            &mut descriptor,
            null_mut(),
        )
    } == 0
    {
        return Err(io::Error::last_os_error());
    }
    Ok(LocalAllocation(descriptor))
}

pub fn create_private_directory(path: &Path) -> io::Result<()> {
    validate_path(path)?;
    let descriptor = security(true)?;
    let attributes = SECURITY_ATTRIBUTES {
        nLength: size_of::<SECURITY_ATTRIBUTES>() as u32,
        lpSecurityDescriptor: descriptor.0,
        bInheritHandle: 0,
    };
    let path = creation_path(path)?;
    // SAFETY: path and security buffers live for the synchronous creation call.
    if unsafe { CreateDirectoryW(path.as_ptr(), &attributes) } == 0 {
        return Err(io::Error::last_os_error());
    }
    Ok(())
}

pub fn create_private_file(path: &Path) -> io::Result<File> {
    create_file(path, false, false)
}

/// Stable lock inode: competing owners may open it, but cannot replace it while
/// any owner holds it. The standard-library lock provides mutual exclusion.
pub fn open_private_lock(path: &Path) -> io::Result<File> {
    create_file(path, true, false)
}

pub fn open_existing_private_lock(path: &Path) -> io::Result<File> {
    create_file(path, true, true)
}

fn create_file(path: &Path, lock: bool, existing: bool) -> io::Result<File> {
    validate_path(path)?;
    let descriptor = security(false)?;
    let attributes = SECURITY_ATTRIBUTES {
        nLength: size_of::<SECURITY_ATTRIBUTES>() as u32,
        lpSecurityDescriptor: descriptor.0,
        bInheritHandle: 0,
    };
    let path = creation_path(path)?;
    // SAFETY: path/security buffers live; CREATE_NEW cannot overwrite data.
    let raw = unsafe {
        CreateFileW(
            path.as_ptr(),
            GENERIC_READ | GENERIC_WRITE | READ_CONTROL,
            FILE_SHARE_READ
                | if lock {
                    FILE_SHARE_WRITE
                } else {
                    FILE_SHARE_DELETE
                },
            &attributes,
            if existing {
                OPEN_EXISTING
            } else if lock {
                OPEN_ALWAYS
            } else {
                CREATE_NEW
            },
            FILE_ATTRIBUTE_NORMAL | FILE_FLAG_OPEN_REPARSE_POINT,
            null_mut(),
        )
    };
    if raw == INVALID_HANDLE_VALUE {
        return Err(io::Error::last_os_error());
    }
    // SAFETY: successful CreateFileW transferred ownership of one file handle.
    let file = unsafe { File::from_raw_handle(raw) };
    information(&file, false)?;
    acl(&file, true)?;
    Ok(file)
}

fn move_file(source: &Path, target: &Path, replace: bool) -> io::Result<()> {
    validate_path(source)?;
    validate_path(target)?;
    if source.parent() != target.parent() {
        return Err(denied());
    }
    let source = wide(source.as_os_str());
    let target = wide(target.as_os_str());
    let flags = MOVEFILE_WRITE_THROUGH
        | if replace {
            MOVEFILE_REPLACE_EXISTING
        } else {
            0
        };
    // SAFETY: two NUL-terminated paths live through synchronous same-volume rename.
    if unsafe { MoveFileExW(source.as_ptr(), target.as_ptr(), flags) } == 0 {
        return Err(io::Error::last_os_error());
    }
    Ok(())
}

pub fn replace_file(source: &Path, target: &Path) -> io::Result<()> {
    move_file(
        &std::path::absolute(source)?,
        &std::path::absolute(target)?,
        true,
    )
}

/// Publish only reviewed settings from private outside-workspace staging.
/// Unlike the generic rename helper this intentionally supports cross-directory
/// moves on the same volume. Retained handles fence ordinary content writers
/// and ancestor replacement. A target must use the explicit settings-only
/// replacement review handle: it shares deletion so the old readable object
/// survives Windows 11 POSIX replacement. Target identity/ACL/bytes are checked
/// immediately before publication; there is no OS CAS against same-user renames.
pub fn publish_reviewed_settings(
    source: &Directory,
    source_name: &str,
    source_identity: Identity,
    target: &Directory,
    target_name: &str,
    reviewed_target: Option<(&Input, &[u8])>,
    expected: &[u8],
) -> io::Result<()> {
    // Metadata-only diagnostic boundaries: preserve error kind and the native
    // error text, without printing filenames, settings bytes or credentials.
    fn at(stage: &'static str) -> impl FnOnce(io::Error) -> io::Error {
        move |error| io::Error::new(error.kind(), format!("reviewed settings {stage}: {error}"))
    }
    if expected.len() > 256 * 1024 || source.identity()?.volume != target.identity()?.volume {
        return Err(denied());
    }
    valid_name(OsStr::new(source_name))?;
    valid_name(OsStr::new(target_name))?;
    let mut file = OpenOptions::new()
        .access_mode(GENERIC_READ | GENERIC_WRITE | DELETE | READ_CONTROL)
        .share_mode(FILE_SHARE_READ | FILE_SHARE_DELETE)
        .custom_flags(FILE_FLAG_OPEN_REPARSE_POINT)
        .open(source.path.join(source_name))
        .map_err(at("source open"))?;
    acl(&file, true).map_err(at("source ACL"))?;
    if information(&file, false)? != source_identity {
        return Err(denied());
    }
    source
        .check_entry(source_name, &file)
        .map_err(at("source entry before read"))?;
    let mut bytes = Vec::new();
    Read::by_ref(&mut file)
        .take(256 * 1024 + 1)
        .read_to_end(&mut bytes)?;
    if bytes != expected {
        return Err(denied());
    }
    match reviewed_target {
        Some((input, before)) => {
            if before.len() > 256 * 1024 || !input.replacement_review {
                return Err(denied());
            }
            input.unchanged().map_err(at("reviewed target unchanged"))?;
            target
                .check_entry(target_name, &input.file)
                .map_err(at("reviewed target entry"))?;
            let mut reader = &input.file;
            reader.seek(SeekFrom::Start(0))?;
            let mut actual = Vec::new();
            Read::by_ref(&mut reader)
                .take(256 * 1024 + 1)
                .read_to_end(&mut actual)?;
            if actual != before {
                return Err(denied());
            }
            input.unchanged()?;
        }
        None => target.ensure_absent(&[target_name])?,
    }
    source
        .check_entry(source_name, &file)
        .map_err(at("source entry before rename"))?;
    file.sync_all().map_err(at("source flush"))?;
    if let Some((input, _)) = reviewed_target {
        input
            .unchanged()
            .map_err(at("reviewed target before rename"))?;
    }
    let filename: Vec<u16> = target
        .path
        .join(target_name)
        .as_os_str()
        .encode_wide()
        .collect();
    let offset = offset_of!(FILE_RENAME_INFO, FileName);
    let length = offset
        .checked_add(filename.len().checked_mul(2).ok_or_else(denied)?)
        .ok_or_else(denied)?;
    let length_u32 = u32::try_from(length).map_err(|_| denied())?;
    let mut buffer = vec![0usize; length.div_ceil(size_of::<usize>())];
    let header = buffer.as_mut_ptr().cast::<FILE_RENAME_INFO>();
    // SAFETY: aligned storage spans the variable-length FILE_RENAME_INFO; the
    // entire UTF-16 destination is copied into its trailing filename member.
    unsafe {
        (*header).Anonymous.Flags = 2 | u32::from(reviewed_target.is_some());
        (*header).RootDirectory = null_mut();
        (*header).FileNameLength = (filename.len() * 2) as u32;
        std::ptr::copy_nonoverlapping(
            filename.as_ptr(),
            buffer.as_mut_ptr().cast::<u8>().add(offset).cast::<u16>(),
            filename.len(),
        );
        if SetFileInformationByHandle(
            file.as_raw_handle(),
            FileRenameInfoEx,
            buffer.as_ptr().cast(),
            length_u32,
        ) == 0
        {
            return Err(at("FileRenameInfoEx")(io::Error::last_os_error()));
        }
    }
    target
        .check_entry(target_name, &file)
        .map_err(at("published target entry"))?;
    file.sync_all().map_err(at("published target flush"))?;
    Ok(())
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::io::Write;

    fn private_root() -> tempfile::TempDir {
        let root = tempfile::tempdir().unwrap();
        create_private_directory(&root.path().join("private")).unwrap();
        root
    }

    #[test]
    fn creation_paths_extend_local_drives_without_accepting_other_namespaces() {
        assert_eq!(
            creation_path(Path::new(r"C:\private\file")).unwrap(),
            creation_path(Path::new(r"\\?\C:\private\file")).unwrap()
        );
        for path in [
            r"relative\file",
            r"C:relative",
            r"\\server\share\file",
            r"\\.\pipe\name",
            r"C:\private\..\other",
            r"C:\private\NUL",
            r"C:\private\file:stream",
        ] {
            assert!(creation_path(Path::new(path)).is_err(), "{path}");
        }
    }

    #[test]
    fn protected_creation_supports_paths_beyond_max_path() {
        let root = private_root();
        let mut directory = root.path().join("private");
        while directory.as_os_str().encode_wide().count() <= 300 {
            directory.push("reviewed-private-directory");
            create_private_directory(&directory).unwrap();
        }
        let target = directory.join("long-owned-file.json");
        let mut file = create_private_file(&target).unwrap();
        file.write_all(b"synthetic").unwrap();
        file.sync_all().unwrap();
        drop(file);
        assert_eq!(
            Input::open(&target, 9).unwrap().read(9).unwrap(),
            b"synthetic"
        );
        assert!(create_private_file(&target).is_err());
    }

    fn synthetic_service_acl(path: &Path, sid: &str, rights: &str) {
        // A synthetic SID deliberately has no account mapping. Construct its
        // ACE directly rather than making icacls resolve a nonexistent service.
        let current = current_sid().unwrap();
        let sddl = wide(OsStr::new(&format!(
            "D:P(A;;FA;;;SY)(A;;FA;;;BA)(A;;FA;;;{current})(A;;{rights};;;{sid})"
        )));
        let mut descriptor = null_mut();
        // SAFETY: trusted test-only SDDL and live output allocation.
        assert_ne!(
            unsafe {
                ConvertStringSecurityDescriptorToSecurityDescriptorW(
                    sddl.as_ptr(),
                    SDDL_REVISION_1,
                    &mut descriptor,
                    null_mut(),
                )
            },
            0
        );
        let allocation = LocalAllocation(descriptor);
        let path = wide(path.as_os_str());
        // SAFETY: both NUL-terminated path and security descriptor live through call.
        assert_ne!(
            unsafe {
                windows_sys::Win32::Security::SetFileSecurityW(
                    path.as_ptr(),
                    DACL_SECURITY_INFORMATION,
                    allocation.0,
                )
            },
            0
        );
    }

    #[test]
    fn explicit_service_reader_is_read_only_exact_and_does_not_relax_normal_inputs() {
        let root = private_root();
        let parent = root.path().join("private");
        let path = parent.join("synthetic.json");
        let mut file = create_private_file(&path).unwrap();
        file.write_all(b"synthetic").unwrap();
        drop(file);
        let sid = "S-1-5-80-1-2-3-4-5";
        let reader = ServiceReader { sid: sid.into() };
        for target in [&parent, &path] {
            synthetic_service_acl(target, sid, "GRGX");
        }
        assert!(Input::open(&path, 9).is_err());
        assert_eq!(
            ServiceInput::open(&path, 9, &reader)
                .unwrap()
                .read(9)
                .unwrap(),
            b"synthetic"
        );
        let other = ServiceReader {
            sid: "S-1-5-80-5-4-3-2-1".into(),
        };
        assert!(ServiceInput::open(&path, 9, &other).is_err());
        let held = ServiceInput::open(&path, 9, &reader).unwrap();
        assert!(OpenOptions::new().write(true).open(&path).is_err());
        drop(held);
        synthetic_service_acl(&path, sid, "GRGXGW");
        assert!(ServiceInput::open(&path, 9, &reader).is_err());
        assert!(ServiceReader::for_named_service("OtherService").is_err());
        assert!(ServiceReader::for_named_service("MemoryLayer-../bad").is_err());
    }

    #[test]
    fn reviewed_cross_directory_settings_publish_retains_old_reader_and_rejects_stale_inputs() {
        let root = private_root();
        let private = root.path().join("private");
        create_private_directory(&private.join("stage")).unwrap();
        create_private_directory(&private.join("target")).unwrap();
        let source = Directory::open(&private.join("stage")).unwrap();
        let target = Directory::open(&private.join("target")).unwrap();
        // Destination-directory write sharing is needed for child insertion,
        // not replacement of the verified directory or its parent chain.
        assert!(std::fs::rename(private.join("target"), private.join("moved-target")).is_err());
        assert!(std::fs::rename(&private, root.path().join("moved-private")).is_err());
        let mut file = source.create("after").unwrap();
        file.write_all(b"after").unwrap();
        file.sync_all().unwrap();
        drop(file);
        let source_identity = {
            let file = Input::open(&private.join("stage/after"), 5).unwrap();
            file.identity()
        };
        let mut file = target.create("settings").unwrap();
        file.write_all(b"before").unwrap();
        file.sync_all().unwrap();
        drop(file);
        let pinned = Input::open(&private.join("target/settings"), 6).unwrap();
        assert!(
            OpenOptions::new()
                .write(true)
                .open(private.join("target/settings"))
                .is_err()
        );
        assert!(
            std::fs::rename(
                private.join("target/settings"),
                private.join("target/moved-settings")
            )
            .is_err()
        );
        assert!(
            publish_reviewed_settings(
                &source,
                "after",
                source_identity,
                &target,
                "settings",
                Some((&pinned, b"before")),
                b"after",
            )
            .is_err()
        );
        drop(pinned);
        let before =
            Input::open_for_reviewed_replacement(&private.join("target/settings"), 6).unwrap();
        assert!(
            OpenOptions::new()
                .write(true)
                .open(private.join("target/settings"))
                .is_err()
        );
        // The opt-in review permits same-user rename, not content mutation.
        // A substituted pathname must be detected before publication and leave
        // the replacement and staged reviewed bytes untouched.
        std::fs::rename(
            private.join("target/settings"),
            private.join("target/moved-settings"),
        )
        .unwrap();
        let mut substituted = target.create("settings").unwrap();
        substituted.write_all(b"other").unwrap();
        drop(substituted);
        assert!(
            publish_reviewed_settings(
                &source,
                "after",
                source_identity,
                &target,
                "settings",
                Some((&before, b"before")),
                b"after",
            )
            .is_err()
        );
        assert_eq!(
            std::fs::read(private.join("target/settings")).unwrap(),
            b"other"
        );
        assert!(private.join("stage/after").is_file());
        std::fs::remove_file(private.join("target/settings")).unwrap();
        std::fs::rename(
            private.join("target/moved-settings"),
            private.join("target/settings"),
        )
        .unwrap();
        assert!(
            publish_reviewed_settings(
                &source,
                "after",
                source_identity,
                &target,
                "settings",
                Some((&before, b"different")),
                b"after"
            )
            .is_err()
        );
        assert!(private.join("stage/after").is_file());
        publish_reviewed_settings(
            &source,
            "after",
            source_identity,
            &target,
            "settings",
            Some((&before, b"before")),
            b"after",
        )
        .unwrap();
        // The reviewed prior file stays available to an existing reader even
        // though the new settings pathname is atomically published.
        let mut old = &before.file;
        old.seek(SeekFrom::Start(0)).unwrap();
        let mut bytes = Vec::new();
        old.read_to_end(&mut bytes).unwrap();
        assert_eq!(bytes, b"before");
        assert_eq!(
            Input::open(&private.join("target/settings"), 5)
                .unwrap()
                .read(5)
                .unwrap(),
            b"after"
        );
        assert!(
            source
                .is_beneath(&Directory::open(&private).unwrap())
                .unwrap()
        );
        assert!(!source.is_beneath(&target).unwrap());
    }

    #[test]
    fn reads_bounds_replay_and_atomic_replacement() {
        let root = private_root();
        let private = root.path().join("private");
        let directory = Directory::open(&private).unwrap();
        let mut file = directory.create("source").unwrap();
        file.write_all(b"synthetic").unwrap();
        directory.publish("source", "final", &file).unwrap();
        drop(file);
        assert_eq!(
            Input::open(&private.join("final"), 9)
                .unwrap()
                .read(9)
                .unwrap(),
            b"synthetic"
        );
        assert!(Input::open(&private.join("final"), 8).is_err());
        assert!(directory.create("final").is_err());
        let mut next = directory.create("replacement").unwrap();
        next.write_all(b"next").unwrap();
        next.sync_all().unwrap();
        drop(next);
        replace_file(&private.join("replacement"), &private.join("final")).unwrap();
        assert_eq!(
            Input::open(&private.join("final"), 9)
                .unwrap()
                .read(9)
                .unwrap(),
            b"next"
        );
    }

    #[test]
    fn aliases_reparse_links_and_hardlinks_fail_closed() {
        for name in [
            "CON",
            "con .json",
            "nul.txt",
            "COM1.json",
            "lpt³",
            "a:stream",
            "a.",
            "a ",
            "../escape",
        ] {
            assert!(valid_name(OsStr::new(name)).is_err(), "{name}");
        }
        assert!(validate_path(Path::new("\\\\server\\share\\secret")).is_err());
        let root = private_root();
        let private = root.path().join("private");
        let mut file = create_private_file(&private.join("secret")).unwrap();
        file.write_all(b"synthetic").unwrap();
        drop(file);
        std::fs::hard_link(private.join("secret"), private.join("alias")).unwrap();
        assert!(Input::open(&private.join("secret"), 64).is_err());
        assert!(Input::open(&private.join("alias"), 64).is_err());
    }

    #[test]
    fn public_acl_junction_and_replacement_are_rejected() {
        let root = private_root();
        let private = root.path().join("private");
        let path = private.join("secret");
        let mut file = create_private_file(&path).unwrap();
        file.write_all(b"synthetic").unwrap();
        drop(file);
        let input = Input::open(&path, 64).unwrap();
        // An input pins its exact file and forbids replacement while reading.
        assert!(std::fs::rename(&path, private.join("replaced")).is_err());
        assert!(std::fs::OpenOptions::new().write(true).open(&path).is_err());
        input.unchanged().unwrap();
        drop(input);
        let status = std::process::Command::new("icacls.exe")
            .arg(&path)
            .args(["/grant", "*S-1-1-0:R"])
            .status()
            .unwrap();
        assert!(status.success());
        assert!(Input::open(&path, 64).is_err());
        let junction = root.path().join("junction");
        let status = std::process::Command::new("cmd.exe")
            .args(["/d", "/c", "mklink", "/J"])
            .arg(&junction)
            .arg(&private)
            .status()
            .unwrap();
        assert!(status.success());
        assert!(Directory::open(&junction).is_err());
        // Remove the junction only, never its target; TempDir cleanup stays safe.
        std::fs::remove_dir(junction).unwrap();
    }
}
