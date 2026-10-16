// SPDX-License-Identifier: AGPL-3.0-or-later
//! Read-only physical path checks. These do not grant filesystem authority;
//! journal access separately uses the protected Windows handle/ACL primitives.
use anyhow::{Result, ensure};
use std::{
    ffi::OsStr,
    fs::{Metadata, OpenOptions},
    os::windows::{
        fs::{MetadataExt, OpenOptionsExt},
        io::AsRawHandle,
    },
    path::Path,
};
use windows_sys::Win32::Storage::FileSystem::{
    BY_HANDLE_FILE_INFORMATION, FILE_ATTRIBUTE_REPARSE_POINT, FILE_FLAG_BACKUP_SEMANTICS,
    FILE_FLAG_OPEN_REPARSE_POINT, FILE_READ_ATTRIBUTES, FILE_SHARE_DELETE, FILE_SHARE_READ,
    FILE_SHARE_WRITE, GetFileInformationByHandle,
};

pub(super) fn validate_name(name: &OsStr) -> Result<()> {
    let value = name
        .to_str()
        .ok_or_else(|| anyhow::anyhow!("invalid Windows path name"))?;
    ensure!(
        !value.is_empty()
            && !value.ends_with(['.', ' '])
            && !value
                .chars()
                .any(|c| c.is_control() || "<>:\"/\\|?*".contains(c)),
        "ambiguous Windows path name or alternate data stream"
    );
    let stem = value
        .split('.')
        .next()
        .unwrap_or_default()
        .trim_end_matches(['.', ' '])
        .to_ascii_uppercase();
    ensure!(
        !matches!(
            stem.as_str(),
            "CON" | "PRN" | "AUX" | "NUL" | "CONIN$" | "CONOUT$"
        ) && !["COM", "LPT"]
            .iter()
            .any(|prefix| stem.strip_prefix(prefix).is_some_and(|s| matches!(
                s,
                "1" | "2" | "3" | "4" | "5" | "6" | "7" | "8" | "9" | "¹" | "²" | "³"
            ))),
        "reserved Windows device name"
    );
    Ok(())
}
pub(super) fn reject_reparse(metadata: &Metadata) -> Result<()> {
    ensure!(
        metadata.file_attributes() & FILE_ATTRIBUTE_REPARSE_POINT == 0,
        "workspace/journal reparse point rejected; existing data preserved"
    );
    Ok(())
}
pub(super) fn identity(path: &Path) -> Result<(u64, u64)> {
    let file = OpenOptions::new()
        .access_mode(FILE_READ_ATTRIBUTES)
        .share_mode(FILE_SHARE_READ | FILE_SHARE_WRITE | FILE_SHARE_DELETE)
        .custom_flags(FILE_FLAG_OPEN_REPARSE_POINT | FILE_FLAG_BACKUP_SEMANTICS)
        .open(path)?;
    let mut info = BY_HANDLE_FILE_INFORMATION::default();
    // SAFETY: handle and correctly sized output remain live during the call.
    ensure!(
        unsafe { GetFileInformationByHandle(file.as_raw_handle(), &mut info) } != 0,
        "could not inspect Windows object identity"
    );
    ensure!(
        info.dwFileAttributes & FILE_ATTRIBUTE_REPARSE_POINT == 0,
        "physical containment encountered a reparse point"
    );
    Ok((
        u64::from(info.dwVolumeSerialNumber),
        u64::from(info.nFileIndexHigh) << 32 | u64::from(info.nFileIndexLow),
    ))
}

#[cfg(test)]
mod tests {
    use super::*;
    #[test]
    fn reserved_names_and_streams_fail_before_io() {
        for name in [
            "NUL",
            "con.txt",
            "COM1",
            "LPT³.log",
            "file:stream",
            "trailing.",
            "trailing ",
        ] {
            assert!(validate_name(OsStr::new(name)).is_err(), "{name}");
        }
        assert!(validate_name(OsStr::new("Mémory project")).is_ok());
    }
    #[test]
    fn native_identity_detects_case_aliases_and_hardlinks() {
        let root =
            std::env::temp_dir().join(format!("memory-path-identity-{}", uuid::Uuid::new_v4()));
        std::fs::create_dir(&root).unwrap();
        let a = root.join("Source");
        std::fs::write(&a, b"synthetic").unwrap();
        std::fs::hard_link(&a, root.join("linked")).unwrap();
        assert_eq!(
            identity(&a).unwrap(),
            identity(&root.join("source")).unwrap()
        );
        assert_eq!(
            identity(&a).unwrap(),
            identity(&root.join("linked")).unwrap()
        );
        assert!(super::super::separate(&a, &root.join("linked")).is_err());
        std::fs::remove_dir_all(root).unwrap();
    }
}
