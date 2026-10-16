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
    if dacl.is_null() || !trusted(&sid_string(owner)?, &current, !private) {
        return Err(denied());
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
        if !trusted(&sid, &current, !private) {
            // Ancestors may allow creating unrelated children. Their open handles
            // deny replacement; no untrusted identity may delete/control this chain.
            let dangerous =
                GENERIC_ALL | GENERIC_WRITE | WRITE_DAC | WRITE_OWNER | DELETE | FILE_DELETE_CHILD;
            if private && ace.Mask != 0 || !private && ace.Mask & dangerous != 0 {
                return Err(denied());
            }
        }
    }
    Ok(())
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
        .access_mode(GENERIC_READ | READ_CONTROL)
        .share_mode(FILE_SHARE_READ)
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
}
impl Input {
    pub fn identity(&self) -> Identity {
        self.before
    }
    pub fn open(path: &Path, limit: u64) -> io::Result<Self> {
        validate_path(path)?;
        let directory = Directory::open(path.parent().ok_or_else(denied)?)?;
        let name = path
            .file_name()
            .and_then(OsStr::to_str)
            .ok_or_else(denied)?
            .to_owned();
        let file = open(path, false, true)?;
        let before = information(&file, false)?;
        if before.size > limit {
            return Err(denied());
        }
        Ok(Self {
            file,
            before,
            directory,
            name,
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
    let path = wide(path.as_os_str());
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
    let path = wide(path.as_os_str());
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
/// and path replacement. Windows 11 POSIX rename is required to replace a target
/// while its reviewed read handle stays open; no weaker fallback is attempted.
/// As on Unix, a malicious same-user POSIX renamer is not an OS compare-and-swap.
pub fn publish_reviewed_settings(
    source: &Directory,
    source_name: &str,
    source_identity: Identity,
    target: &Directory,
    target_name: &str,
    reviewed_target: Option<(&Input, &[u8])>,
    expected: &[u8],
) -> io::Result<()> {
    if expected.len() > 256 * 1024 || source.identity()?.volume != target.identity()?.volume {
        return Err(denied());
    }
    valid_name(OsStr::new(source_name))?;
    valid_name(OsStr::new(target_name))?;
    let mut file = OpenOptions::new()
        .access_mode(GENERIC_READ | GENERIC_WRITE | DELETE | READ_CONTROL)
        .share_mode(FILE_SHARE_READ | FILE_SHARE_DELETE)
        .custom_flags(FILE_FLAG_OPEN_REPARSE_POINT)
        .open(source.path.join(source_name))?;
    acl(&file, true)?;
    if information(&file, false)? != source_identity {
        return Err(denied());
    }
    source.check_entry(source_name, &file)?;
    let mut bytes = Vec::new();
    Read::by_ref(&mut file)
        .take(256 * 1024 + 1)
        .read_to_end(&mut bytes)?;
    if bytes != expected {
        return Err(denied());
    }
    match reviewed_target {
        Some((input, before)) => {
            if before.len() > 256 * 1024 {
                return Err(denied());
            }
            input.unchanged()?;
            target.check_entry(target_name, &input.file)?;
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
    source.check_entry(source_name, &file)?;
    file.sync_all()?;
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
            return Err(io::Error::last_os_error());
        }
    }
    target.check_entry(target_name, &file)?;
    file.sync_all()?;
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
    fn reviewed_cross_directory_settings_publish_retains_old_reader_and_rejects_stale_inputs() {
        let root = private_root();
        let private = root.path().join("private");
        create_private_directory(&private.join("stage")).unwrap();
        create_private_directory(&private.join("target")).unwrap();
        let source = Directory::open(&private.join("stage")).unwrap();
        let target = Directory::open(&private.join("target")).unwrap();
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
        let before = Input::open(&private.join("target/settings"), 6).unwrap();
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
