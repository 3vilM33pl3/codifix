//! Unix-only private bootstrap/state files. No secrets go through argv or environment.
use crate::{Error, Result};
use std::{
    fs::{self, File, OpenOptions},
    io::{Read, Write},
    os::unix::fs::{MetadataExt, OpenOptionsExt, PermissionsExt},
    path::{Path, PathBuf},
};
use zeroize::Zeroizing;

fn private_dir(path: &Path) -> Result<()> {
    // Every path component below the root is checked for symlinks. The containing
    // directory must be private; writable ancestors are the administrator's trust boundary.
    let mut part = PathBuf::new();
    for component in path.components() {
        part.push(component);
        if fs::symlink_metadata(&part)
            .map_err(|_| Error::ProtectedFile)?
            .file_type()
            .is_symlink()
        {
            return Err(Error::ProtectedFile);
        }
    }
    let m = fs::metadata(path).map_err(|_| Error::ProtectedFile)?;
    if !m.is_dir() || m.uid() != unsafe { libc::geteuid() } || m.mode() & 0o077 != 0 {
        return Err(Error::ProtectedFile);
    }
    Ok(())
}

pub fn read(path: &Path) -> Result<Zeroizing<Vec<u8>>> {
    private_dir(path.parent().ok_or(Error::ProtectedFile)?)?;
    let mut f = OpenOptions::new()
        .read(true)
        .custom_flags(libc::O_NOFOLLOW | libc::O_CLOEXEC)
        .open(path)
        .map_err(|_| Error::ProtectedFile)?;
    let m = f.metadata().map_err(|_| Error::ProtectedFile)?;
    if !m.is_file()
        || m.uid() != unsafe { libc::geteuid() }
        || m.mode() & 0o077 != 0
        || m.nlink() != 1
        || m.len() > 65536
    {
        return Err(Error::ProtectedFile);
    }
    let mut out = Zeroizing::new(Vec::new());
    f.read_to_end(&mut out).map_err(|_| Error::ProtectedFile)?;
    Ok(out)
}

pub fn write(path: &Path, bytes: &[u8]) -> Result<()> {
    let dir = path.parent().ok_or(Error::ProtectedFile)?;
    private_dir(dir)?;
    let tmp = dir.join(format!(".agency-{}.tmp", uuid::Uuid::new_v4()));
    let mut f = OpenOptions::new()
        .write(true)
        .create_new(true)
        .mode(0o600)
        .custom_flags(libc::O_NOFOLLOW | libc::O_CLOEXEC)
        .open(&tmp)
        .map_err(|_| Error::ProtectedFile)?;
    let result = (|| {
        f.write_all(bytes).map_err(|_| Error::ProtectedFile)?;
        f.sync_all().map_err(|_| Error::ProtectedFile)?;
        fs::rename(&tmp, path).map_err(|_| Error::ProtectedFile)?;
        File::open(dir)
            .and_then(|d| d.sync_all())
            .map_err(|_| Error::ProtectedFile)
    })();
    let _ = fs::remove_file(tmp);
    result
}

pub struct StateLock {
    _file: File,
}
impl StateLock {
    pub fn acquire(state: &Path) -> Result<Self> {
        private_dir(state.parent().ok_or(Error::ProtectedFile)?)?;
        let lock = state.with_extension("lock");
        let f = OpenOptions::new()
            .read(true)
            .write(true)
            .create(true)
            .truncate(false)
            .mode(0o600)
            .custom_flags(libc::O_NOFOLLOW | libc::O_CLOEXEC)
            .open(lock)
            .map_err(|_| Error::ProtectedFile)?;
        let m = f.metadata().map_err(|_| Error::ProtectedFile)?;
        if !m.is_file()
            || m.uid() != unsafe { libc::geteuid() }
            || m.mode() & 0o077 != 0
            || m.nlink() != 1
        {
            return Err(Error::ProtectedFile);
        }
        if unsafe {
            libc::flock(
                std::os::fd::AsRawFd::as_raw_fd(&f),
                libc::LOCK_EX | libc::LOCK_NB,
            )
        } != 0
        {
            return Err(Error::Busy);
        }
        Ok(Self { _file: f })
    }
}

pub fn initialize<T: serde::Serialize>(path: &Path, value: &T) -> Result<()> {
    let _guard = StateLock::acquire(path)?;
    if path.try_exists().map_err(|_| Error::ProtectedFile)? {
        return Err(Error::AlreadyEnrolled);
    }
    save(path, value)
}
pub fn load<T: serde::de::DeserializeOwned>(path: &Path) -> Result<T> {
    serde_json::from_slice(&read(path)?).map_err(|_| Error::ProtectedFile)
}
pub fn save<T: serde::Serialize>(path: &Path, value: &T) -> Result<()> {
    let bytes = Zeroizing::new(serde_json::to_vec(value).map_err(|_| Error::ProtectedFile)?);
    write(path, &bytes)
}

/// Bootstrap test/operator helper: private directories are never created implicitly by services.
pub fn create_private_directory(path: &Path) -> Result<()> {
    fs::create_dir(path).map_err(|_| Error::ProtectedFile)?;
    fs::set_permissions(path, fs::Permissions::from_mode(0o700)).map_err(|_| Error::ProtectedFile)
}

/// Stable local directory binding. Replacing/moving storage requires enrollment.
pub fn directory_identity(path: &Path) -> Result<(u64, u64)> {
    private_dir(path)?;
    let metadata = fs::metadata(path).map_err(|_| Error::ProtectedFile)?;
    Ok((metadata.dev(), metadata.ino()))
}
