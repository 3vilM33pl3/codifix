// SPDX-License-Identifier: AGPL-3.0-or-later
//! Pinned directory descriptors. No symlinks, shared writable ancestors, special
//! files or hard-linked input. Root/same-UID mutation is a trusted operator risk.
use crate::{Failure, Result};
use rustix::fs::{AtFlags, Mode, OFlags, linkat, open, openat, statat, unlinkat};
use std::{
    fs::{File, Metadata},
    os::unix::fs::MetadataExt,
    path::{Component, Path},
};

pub struct Directory(File);
impl Directory {
    pub fn open(path: &Path) -> Result<Self> {
        if !path.is_absolute() {
            return Err(Failure::PrivateFiles);
        }
        let flags = OFlags::RDONLY | OFlags::DIRECTORY | OFlags::NOFOLLOW | OFlags::CLOEXEC;
        let mut dir: File = open("/", flags, Mode::empty())
            .map_err(|_| Failure::PrivateFiles)?
            .into();
        for part in path.components() {
            match part {
                Component::RootDir => {}
                Component::Normal(name) => {
                    let meta = dir.metadata().map_err(|_| Failure::PrivateFiles)?;
                    let trusted =
                        meta.uid() == 0 || meta.uid() == rustix::process::geteuid().as_raw();
                    if !trusted || (meta.mode() & 0o022 != 0 && meta.mode() & 0o1000 == 0) {
                        return Err(Failure::PrivateFiles);
                    }
                    dir = openat(&dir, name, flags, Mode::empty())
                        .map_err(|_| Failure::PrivateFiles)?
                        .into();
                }
                _ => return Err(Failure::PrivateFiles),
            }
        }
        let meta = dir.metadata().map_err(|_| Failure::PrivateFiles)?;
        if meta.uid() != rustix::process::geteuid().as_raw() || meta.mode() & 0o777 != 0o700 {
            return Err(Failure::PrivateFiles);
        }
        Ok(Self(dir))
    }
    pub fn create(&self, name: &str) -> Result<File> {
        let file: File = openat(
            &self.0,
            name,
            OFlags::WRONLY | OFlags::CREATE | OFlags::EXCL | OFlags::NOFOLLOW | OFlags::CLOEXEC,
            Mode::RUSR | Mode::WUSR,
        )
        .map_err(|_| Failure::PrivateFiles)?
        .into();
        secure(&file.metadata().map_err(|_| Failure::PrivateFiles)?)?;
        self.sync()?;
        Ok(file)
    }
    pub fn ensure_absent(&self, names: &[&str]) -> Result<()> {
        for name in names {
            match statat(&self.0, *name, AtFlags::SYMLINK_NOFOLLOW) {
                Err(rustix::io::Errno::NOENT) => {}
                _ => return Err(Failure::PrivateFiles),
            }
        }
        Ok(())
    }
    pub fn publish(&self, partial: &str, name: &str, file: &File) -> Result<()> {
        self.check_entry(partial, file)?;
        file.sync_all().map_err(|_| Failure::PrivateFiles)?;
        linkat(&self.0, partial, &self.0, name, AtFlags::empty())
            .map_err(|_| Failure::PrivateFiles)?;
        unlinkat(&self.0, partial, AtFlags::empty()).map_err(|_| Failure::PrivateFiles)?;
        self.sync()?;
        self.check_entry(name, file)
    }
    pub fn sync(&self) -> Result<()> {
        self.0.sync_all().map_err(|_| Failure::PrivateFiles)
    }
    fn check_entry(&self, name: &str, file: &File) -> Result<()> {
        let entry: File = openat(
            &self.0,
            name,
            OFlags::RDONLY | OFlags::NOFOLLOW | OFlags::NONBLOCK | OFlags::CLOEXEC,
            Mode::empty(),
        )
        .map_err(|_| Failure::PrivateFiles)?
        .into();
        let a = file.metadata().map_err(|_| Failure::PrivateFiles)?;
        let b = entry.metadata().map_err(|_| Failure::PrivateFiles)?;
        secure(&a)?;
        secure(&b)?;
        if a.dev() != b.dev() || a.ino() != b.ino() {
            return Err(Failure::PrivateFiles);
        }
        Ok(())
    }
}
fn secure(meta: &Metadata) -> Result<()> {
    if !meta.is_file()
        || meta.uid() != rustix::process::geteuid().as_raw()
        || meta.mode() & 0o777 != 0o600
        || meta.nlink() != 1
    {
        return Err(Failure::PrivateFiles);
    }
    Ok(())
}
pub struct Input {
    pub file: File,
    before: Metadata,
    directory: Directory,
    name: String,
}
impl Input {
    pub fn open(path: &Path, limit: u64) -> Result<Self> {
        let directory = Directory::open(path.parent().ok_or(Failure::PrivateFiles)?)?;
        let name = path
            .file_name()
            .and_then(|v| v.to_str())
            .ok_or(Failure::PrivateFiles)?
            .to_owned();
        let file: File = openat(
            &directory.0,
            name.as_str(),
            OFlags::RDONLY | OFlags::NOFOLLOW | OFlags::NONBLOCK | OFlags::CLOEXEC,
            Mode::empty(),
        )
        .map_err(|_| Failure::PrivateFiles)?
        .into();
        let before = file.metadata().map_err(|_| Failure::PrivateFiles)?;
        secure(&before)?;
        if before.len() > limit {
            return Err(Failure::Size);
        }
        Ok(Self {
            file,
            before,
            directory,
            name,
        })
    }
    pub fn unchanged(&self) -> Result<()> {
        self.directory.check_entry(&self.name, &self.file)?;
        let after = self.file.metadata().map_err(|_| Failure::PrivateFiles)?;
        if (
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
        ) {
            return Err(Failure::PrivateFiles);
        }
        Ok(())
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::{
        fs,
        io::Write,
        os::unix::fs::{PermissionsExt, symlink},
    };

    fn directory() -> tempfile::TempDir {
        tempfile::Builder::new()
            .permissions(fs::Permissions::from_mode(0o700))
            .tempdir()
            .expect("synthetic operator fixture")
    }

    #[test]
    fn pinned_input_detects_replacement_mutation_and_size_limits() {
        let root = directory();
        let dir = Directory::open(root.path()).expect("synthetic operator fixture");
        let mut file = dir.create("input").expect("synthetic operator fixture");
        file.write_all(b"a").expect("synthetic operator fixture");
        let path = root.path().join("input");
        assert!(matches!(Input::open(&path, 0), Err(Failure::Size)));
        let input = Input::open(&path, 10).expect("synthetic operator fixture");
        input.unchanged().expect("synthetic operator fixture");
        file.write_all(b"b").expect("synthetic operator fixture");
        assert!(input.unchanged().is_err());
        let input = Input::open(&path, 10).expect("synthetic operator fixture");
        fs::rename(&path, root.path().join("previous")).expect("synthetic operator fixture");
        dir.create("input")
            .expect("synthetic operator fixture")
            .write_all(b"ab")
            .expect("synthetic operator fixture");
        assert!(input.unchanged().is_err());
    }

    #[test]
    fn ancestry_must_be_private_or_trusted_sticky_without_symlinks() {
        let root = directory();
        let child = root.path().join("child");
        fs::create_dir(&child).expect("synthetic operator fixture");
        fs::set_permissions(&child, fs::Permissions::from_mode(0o700))
            .expect("synthetic operator fixture");
        let alias = root.path().join("alias");
        symlink(&child, &alias).expect("synthetic operator fixture");
        assert!(Directory::open(&alias).is_err());
        assert!(Directory::open(Path::new("relative")).is_err());
        assert!(Directory::open(&child.join("..")).is_err());
        fs::set_permissions(root.path(), fs::Permissions::from_mode(0o777))
            .expect("synthetic operator fixture");
        assert!(Directory::open(&child).is_err());
    }

    #[test]
    fn publishing_is_exclusive_and_pins_partial_identity() {
        let root = directory();
        let dir = Directory::open(root.path()).expect("synthetic operator fixture");
        let mut file = dir.create("partial").expect("synthetic operator fixture");
        file.write_all(b"private synthetic bytes")
            .expect("synthetic operator fixture");
        assert!(dir.ensure_absent(&["partial", "complete"]).is_err());
        dir.publish("partial", "complete", &file)
            .expect("synthetic operator fixture");
        assert!(!root.path().join("partial").exists());
        let again = dir.create("partial").expect("synthetic operator fixture");
        assert!(dir.publish("partial", "complete", &again).is_err());
        fs::rename(root.path().join("partial"), root.path().join("renamed"))
            .expect("synthetic operator fixture");
        dir.create("partial").expect("synthetic operator fixture");
        assert!(dir.publish("partial", "another", &again).is_err());
        assert!(!root.path().join("another").exists());
        symlink("absent", root.path().join("dangling")).expect("synthetic operator fixture");
        assert!(dir.ensure_absent(&["dangling"]).is_err());
        assert_eq!(
            fs::read(root.path().join("complete")).expect("synthetic operator fixture"),
            b"private synthetic bytes"
        );
    }
}
