// SPDX-License-Identifier: AGPL-3.0-or-later
//! Post-Job untrusted output collection. Never reuse sandbox ACLs at destination.
use crate::private_windows::Directory;
use anyhow::{Context, Result, ensure};
use std::{
    fs::{File, OpenOptions},
    io::Read,
    mem::zeroed,
    os::windows::{ffi::OsStrExt, fs::OpenOptionsExt, io::AsRawHandle},
    path::{Component, Path},
};
use windows_sys::Win32::{
    Foundation::{ERROR_HANDLE_EOF, GetLastError, INVALID_HANDLE_VALUE},
    Storage::FileSystem::{
        BY_HANDLE_FILE_INFORMATION, FILE_ATTRIBUTE_DIRECTORY, FILE_ATTRIBUTE_REPARSE_POINT,
        FILE_FLAG_BACKUP_SEMANTICS, FILE_FLAG_OPEN_REPARSE_POINT, FILE_SHARE_READ, FindClose,
        FindFirstStreamW, FindNextStreamW, FindStreamInfoStandard, GetFileInformationByHandle,
        WIN32_FIND_STREAM_DATA,
    },
};

pub(super) fn relative(path: &Path) -> Result<()> {
    ensure!(
        !path.as_os_str().is_empty()
            && path.as_os_str().encode_wide().count() <= 1024
            && path.components().count() <= 16,
        "invalid export path"
    );
    for component in path.components() {
        let Component::Normal(name) = component else {
            anyhow::bail!("export path must be relative");
        };
        let text = name.to_str().context("invalid Unicode export path")?;
        ensure!(
            !text.ends_with(['.', ' '])
                && !text
                    .chars()
                    .any(|c| c.is_control() || "<>:\"/\\|?*".contains(c)),
            "ambiguous export path"
        );
        let stem = text
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
                .any(|prefix| stem.strip_prefix(prefix).is_some_and(|n| matches!(
                    n,
                    "1" | "2" | "3" | "4" | "5" | "6" | "7" | "8" | "9" | "¹" | "²" | "³"
                ))),
            "reserved export path"
        );
    }
    Ok(())
}
fn open(path: &Path, directory: bool) -> Result<(File, BY_HANDLE_FILE_INFORMATION)> {
    // Creation-time package ACEs may remain after profile deletion. Do not relax
    // the authority-file Input validator for these deliberately untrusted files.
    // Pin every path segment against replacement and forbid concurrent writers.
    let file = OpenOptions::new()
        .read(true)
        .share_mode(FILE_SHARE_READ)
        .custom_flags(FILE_FLAG_OPEN_REPARSE_POINT | FILE_FLAG_BACKUP_SEMANTICS)
        .open(path)?;
    let mut info: BY_HANDLE_FILE_INFORMATION = unsafe { zeroed() };
    ensure!(
        unsafe { GetFileInformationByHandle(file.as_raw_handle(), &mut info) } != 0,
        "export identity unavailable"
    );
    ensure!(
        info.dwFileAttributes & FILE_ATTRIBUTE_REPARSE_POINT == 0
            && (info.dwFileAttributes & FILE_ATTRIBUTE_DIRECTORY != 0) == directory
            && (directory || info.nNumberOfLinks == 1),
        "unsafe export filesystem object"
    );
    streams(path)?;
    Ok((file, info))
}
fn streams(path: &Path) -> Result<()> {
    let path: Vec<_> = path.as_os_str().encode_wide().chain(Some(0)).collect();
    let mut data = WIN32_FIND_STREAM_DATA::default();
    let handle = unsafe {
        FindFirstStreamW(
            path.as_ptr(),
            FindStreamInfoStandard,
            (&mut data as *mut WIN32_FIND_STREAM_DATA).cast(),
            0,
        )
    };
    if handle == INVALID_HANDLE_VALUE {
        ensure!(
            unsafe { GetLastError() } == ERROR_HANDLE_EOF,
            "export streams unavailable"
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
            .context("invalid export stream")?;
        ensure!(
            String::from_utf16(&data.cStreamName[..count])? == "::$DATA",
            "alternate export stream rejected"
        );
        if unsafe { FindNextStreamW(search.0, (&mut data as *mut WIN32_FIND_STREAM_DATA).cast()) }
            == 0
        {
            ensure!(
                unsafe { GetLastError() } == ERROR_HANDLE_EOF,
                "export stream enumeration interrupted"
            );
            return Ok(());
        }
    }
}

/// Must only be called after verified Job-empty completion. Each selected file
/// is copied as untrusted bytes into a new private file; no permissions, links,
/// attributes, executable naming or alternate streams are propagated.
pub(super) struct Collected {
    pub bytes: Vec<u8>,
    pub identity: (u32, u64),
}
pub(super) fn read(root: &Path, path: &Path, limit: usize) -> Result<Collected> {
    relative(path)?;
    let _root = Directory::open(root)?;
    let mut pinned = Vec::new();
    let mut current = root.to_owned();
    let parts: Vec<_> = path.components().collect();
    for (index, part) in parts.iter().enumerate() {
        current.push(part.as_os_str());
        let last = index + 1 == parts.len();
        let (file, info) = open(&current, !last)?;
        if last {
            let size = (u64::from(info.nFileSizeHigh) << 32) | u64::from(info.nFileSizeLow);
            ensure!(size <= limit as u64, "export output budget exceeded");
            let mut result = Vec::new();
            file.take(limit as u64 + 1).read_to_end(&mut result)?;
            ensure!(
                result.len() <= limit && result.len() as u64 == size,
                "export changed or exceeded output budget"
            );
            return Ok(Collected {
                bytes: result,
                identity: (
                    info.dwVolumeSerialNumber,
                    (u64::from(info.nFileIndexHigh) << 32) | u64::from(info.nFileIndexLow),
                ),
            });
        }
        pinned.push(file);
    }
    anyhow::bail!("empty export path")
}

#[cfg(test)]
mod tests {
    use super::*;
    #[test]
    fn native_exports_deny_escape_alias_streams_and_preserve_plain_bytes() {
        let base =
            std::env::temp_dir().join(format!("memory-eval-export-{}", uuid::Uuid::new_v4()));
        crate::private_windows::create_private_directory(&base).unwrap();
        let path = base.join("source.rs");
        std::fs::write(&path, b"synthetic source").unwrap();
        assert_eq!(
            read(&base, Path::new("source.rs"), 32).unwrap().bytes,
            b"synthetic source"
        );
        assert!(read(&base, Path::new("source.rs"), 3).is_err());
        for bad in [
            "../secret",
            "C:\\secret",
            "source.rs:stream",
            "NUL.rs",
            "file.",
            "a/../../secret",
        ] {
            assert!(relative(Path::new(bad)).is_err(), "{bad}");
        }
        let alias = base.join("alias.rs");
        std::fs::hard_link(&path, &alias).unwrap();
        assert!(read(&base, Path::new("source.rs"), 32).is_err());
        std::fs::remove_file(alias).unwrap();
        std::fs::write(format!("{}:private", path.display()), b"hidden").unwrap();
        assert!(read(&base, Path::new("source.rs"), 32).is_err());
        std::fs::remove_dir_all(base).unwrap();
    }
    #[test]
    fn native_exports_do_not_follow_junction_into_host_material() {
        let base = std::env::temp_dir().join(format!(
            "memory-eval-export-junction-{}",
            uuid::Uuid::new_v4()
        ));
        crate::private_windows::create_private_directory(&base).unwrap();
        for name in ["scratch", "hidden"] {
            crate::private_windows::create_private_directory(&base.join(name)).unwrap();
        }
        std::fs::write(
            base.join("hidden/expectations"),
            b"never export this host material",
        )
        .unwrap();
        let junction = base.join("scratch/escape");
        let status = std::process::Command::new("cmd.exe")
            .args(["/d", "/c", "mklink", "/J"])
            .arg(&junction)
            .arg(base.join("hidden"))
            .status()
            .unwrap();
        assert!(status.success());
        let denied = read(&base.join("scratch"), Path::new("escape/expectations"), 64).is_err();
        std::fs::remove_dir(junction).unwrap();
        assert!(denied);
        std::fs::remove_dir_all(base).unwrap();
    }
}
