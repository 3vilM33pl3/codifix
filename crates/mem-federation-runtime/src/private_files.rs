// SPDX-License-Identifier: AGPL-3.0-or-later
//! Enrollment-only protected reads. Pin each directory and final file; never
//! validate one pathname and then read a different lookup. Root/same-UID
//! mutation remains a trusted operator risk. Legacy runtime loaders are unchanged.
use crate::{Result, RuntimeError};
use std::path::Path;

#[cfg(unix)]
pub(crate) fn read(path: &Path, limit: u64) -> Result<Vec<u8>> {
    unix::Input::open(path, limit)?.read(limit)
}

#[cfg(not(unix))]
pub(crate) fn read(_path: &Path, _limit: u64) -> Result<Vec<u8>> {
    // Do not silently discard ownership/no-follow guarantees on other platforms.
    Err(RuntimeError::Configuration)
}

#[cfg(unix)]
mod unix {
    use super::*;
    use rustix::fs::{Mode, OFlags, open, openat};
    use std::{
        ffi::OsString,
        fs::{File, Metadata},
        io::Read,
        os::unix::fs::MetadataExt,
        path::Component,
    };

    fn failure<T>(_: T) -> RuntimeError {
        RuntimeError::Configuration
    }

    fn secure(meta: &Metadata) -> Result<()> {
        if !meta.is_file()
            || meta.uid() != rustix::process::geteuid().as_raw()
            || meta.mode() & 0o777 != 0o600
            || meta.nlink() != 1
        {
            return Err(RuntimeError::Configuration);
        }
        Ok(())
    }

    fn directory(path: &Path) -> Result<File> {
        if !path.is_absolute() {
            return Err(RuntimeError::Configuration);
        }
        let flags = OFlags::RDONLY | OFlags::DIRECTORY | OFlags::NOFOLLOW | OFlags::CLOEXEC;
        let mut dir: File = open("/", flags, Mode::empty()).map_err(failure)?.into();
        for part in path.components() {
            match part {
                Component::RootDir => {}
                Component::Normal(name) => {
                    let meta = dir.metadata().map_err(failure)?;
                    if (meta.uid() != 0 && meta.uid() != rustix::process::geteuid().as_raw())
                        || (meta.mode() & 0o022 != 0 && meta.mode() & 0o1000 == 0)
                    {
                        return Err(RuntimeError::Configuration);
                    }
                    dir = openat(&dir, name, flags, Mode::empty())
                        .map_err(failure)?
                        .into();
                }
                _ => return Err(RuntimeError::Configuration),
            }
        }
        let meta = dir.metadata().map_err(failure)?;
        if meta.uid() != rustix::process::geteuid().as_raw() || meta.mode() & 0o777 != 0o700 {
            return Err(RuntimeError::Configuration);
        }
        Ok(dir)
    }

    pub(super) struct Input {
        directory: File,
        name: OsString,
        file: File,
        before: Metadata,
    }

    impl Input {
        pub(super) fn open(path: &Path, limit: u64) -> Result<Self> {
            let directory = directory(path.parent().ok_or(RuntimeError::Configuration)?)?;
            let name = path
                .file_name()
                .ok_or(RuntimeError::Configuration)?
                .to_owned();
            let file = Self::entry(&directory, &name)?;
            let before = file.metadata().map_err(failure)?;
            secure(&before)?;
            if before.len() > limit {
                return Err(RuntimeError::Configuration);
            }
            Ok(Self {
                directory,
                name,
                file,
                before,
            })
        }

        fn entry(directory: &File, name: &std::ffi::OsStr) -> Result<File> {
            openat(
                directory,
                name,
                OFlags::RDONLY | OFlags::NOFOLLOW | OFlags::NONBLOCK | OFlags::CLOEXEC,
                Mode::empty(),
            )
            .map(File::from)
            .map_err(failure)
        }

        pub(super) fn read(mut self, limit: u64) -> Result<Vec<u8>> {
            let mut bytes = Vec::new();
            self.file
                .by_ref()
                .take(limit.checked_add(1).ok_or(RuntimeError::Configuration)?)
                .read_to_end(&mut bytes)
                .map_err(failure)?;
            if bytes.len() as u64 > limit {
                return Err(RuntimeError::Configuration);
            }
            self.unchanged()?;
            Ok(bytes)
        }

        fn unchanged(&self) -> Result<()> {
            let entry = Self::entry(&self.directory, &self.name)?;
            let entry = entry.metadata().map_err(failure)?;
            let after = self.file.metadata().map_err(failure)?;
            secure(&entry)?;
            secure(&after)?;
            if entry.dev() != after.dev()
                || entry.ino() != after.ino()
                || (
                    self.before.len(),
                    self.before.mtime(),
                    self.before.mtime_nsec(),
                    self.before.ctime(),
                    self.before.ctime_nsec(),
                ) != (
                    after.len(),
                    after.mtime(),
                    after.mtime_nsec(),
                    after.ctime(),
                    after.ctime_nsec(),
                )
            {
                return Err(RuntimeError::Configuration);
            }
            Ok(())
        }
    }

    #[cfg(test)]
    #[allow(clippy::unwrap_used)]
    mod tests {
        use super::*;
        use std::{
            fs,
            io::Write,
            os::unix::fs::{OpenOptionsExt, PermissionsExt, symlink},
        };

        fn private_dir() -> tempfile::TempDir {
            tempfile::Builder::new()
                .permissions(fs::Permissions::from_mode(0o700))
                .tempdir()
                .unwrap()
        }

        fn fixture(path: &Path, content: &[u8]) {
            fs::OpenOptions::new()
                .write(true)
                .create_new(true)
                .mode(0o600)
                .open(path)
                .unwrap()
                .write_all(content)
                .unwrap();
        }

        #[test]
        fn pinned_entry_replacement_mutation_and_limits_fail_closed() {
            let root = private_dir();
            let path = root.path().join("config");
            fixture(&path, b"synthetic");
            assert_eq!(read(&path, 9).unwrap(), b"synthetic");
            assert!(read(&path, 8).is_err());
            let input = Input::open(&path, 64).unwrap();
            fs::rename(&path, root.path().join("previous")).unwrap();
            fixture(&path, b"synthetic");
            assert!(input.read(64).is_err());
            let input = Input::open(&path, 64).unwrap();
            fs::OpenOptions::new()
                .append(true)
                .open(&path)
                .unwrap()
                .write_all(b"changed")
                .unwrap();
            assert!(input.read(64).is_err());
            let input = Input::open(&path, 64).unwrap();
            assert!(input.read(1).is_err());
        }

        #[test]
        fn links_modes_and_unsafe_ancestors_are_rejected() {
            let root = private_dir();
            let child = root.path().join("private");
            fs::create_dir(&child).unwrap();
            fs::set_permissions(&child, fs::Permissions::from_mode(0o700)).unwrap();
            let path = child.join("token");
            fixture(&path, b"synthetic");
            symlink(&path, child.join("alias")).unwrap();
            assert!(read(&child.join("alias"), 64).is_err());
            symlink(&child, root.path().join("alias")).unwrap();
            assert!(read(&root.path().join("alias/token"), 64).is_err());
            assert!(read(Path::new("relative"), 64).is_err());
            assert!(read(&child.join("../private/token"), 64).is_err());
            fs::set_permissions(&path, fs::Permissions::from_mode(0o644)).unwrap();
            assert!(read(&path, 64).is_err());
            fs::set_permissions(&path, fs::Permissions::from_mode(0o600)).unwrap();
            fs::set_permissions(&child, fs::Permissions::from_mode(0o755)).unwrap();
            assert!(read(&path, 64).is_err());
            fs::set_permissions(&child, fs::Permissions::from_mode(0o700)).unwrap();
            fs::set_permissions(root.path(), fs::Permissions::from_mode(0o777)).unwrap();
            assert!(read(&path, 64).is_err());
            fs::set_permissions(root.path(), fs::Permissions::from_mode(0o700)).unwrap();
            fs::hard_link(&path, child.join("hardlink")).unwrap();
            assert!(read(&path, 64).is_err());
        }

        #[test]
        fn special_files_fail_without_blocking() {
            let root = private_dir();
            let path = root.path().join("fifo");
            rustix::fs::mkfifoat(rustix::fs::CWD, &path, Mode::RUSR | Mode::WUSR).unwrap();
            assert!(read(&path, 64).is_err());
        }

        #[test]
        fn foreign_owner_is_rejected_when_fixture_chown_is_available() {
            if !rustix::process::geteuid().is_root() {
                return; // No credential changes or privileged helper on ordinary test hosts.
            }
            let root = private_dir();
            let path = root.path().join("foreign");
            fixture(&path, b"synthetic");
            rustix::fs::chown(&path, Some(rustix::process::Uid::from_raw(65534)), None).unwrap();
            assert!(read(&path, 64).is_err());
        }
    }
}
