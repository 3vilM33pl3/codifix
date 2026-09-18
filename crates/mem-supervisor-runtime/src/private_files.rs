//! Descriptor-relative reads and immutable per-run copies. Same-UID/root
//! mutation is an operator trust boundary, never a Pi capability.
use crate::launcher::LaunchError;
use rustix::fs::{Mode, OFlags, open, openat};
use sha2::{Digest, Sha256};
use std::{
    fs::File,
    io::{Read, Write},
    os::unix::fs::{MetadataExt, PermissionsExt},
    path::{Component, Path},
};

pub(crate) fn directory(path: &Path, private: bool) -> Result<File, LaunchError> {
    if !path.is_absolute() || path.to_string_lossy().contains([',', '\n', '\r']) {
        return Err(LaunchError::Files);
    }
    let flags = OFlags::RDONLY | OFlags::DIRECTORY | OFlags::NOFOLLOW | OFlags::CLOEXEC;
    let mut fd: File = open("/", flags, Mode::empty())
        .map_err(|_| LaunchError::Files)?
        .into();
    for part in path.components() {
        match part {
            Component::RootDir => {}
            Component::Normal(name) => {
                trusted(&fd, true)?;
                fd = openat(&fd, name, flags, Mode::empty())
                    .map_err(|_| LaunchError::Files)?
                    .into();
            }
            _ => return Err(LaunchError::Files),
        }
    }
    trusted(&fd, true)?;
    let metadata = fd.metadata().map_err(|_| LaunchError::Files)?;
    if private
        && (metadata.uid() != rustix::process::geteuid().as_raw()
            || metadata.mode() & 0o777 != 0o700)
    {
        return Err(LaunchError::Files);
    }
    Ok(fd)
}
fn trusted(file: &File, directory: bool) -> Result<(), LaunchError> {
    let m = file.metadata().map_err(|_| LaunchError::Files)?;
    let owner = m.uid() == 0 || m.uid() == rustix::process::geteuid().as_raw();
    if !owner || (m.mode() & 0o022 != 0 && !(directory && m.mode() & 0o1000 != 0)) {
        return Err(LaunchError::Files);
    }
    Ok(())
}
fn input(path: &Path, limit: u64, private: bool) -> Result<File, LaunchError> {
    let parent = directory(path.parent().ok_or(LaunchError::Files)?, private)?;
    let file: File = openat(
        &parent,
        path.file_name().ok_or(LaunchError::Files)?,
        OFlags::RDONLY | OFlags::NOFOLLOW | OFlags::NONBLOCK | OFlags::CLOEXEC,
        Mode::empty(),
    )
    .map_err(|_| LaunchError::Files)?
    .into();
    trusted(&file, false)?;
    let m = file.metadata().map_err(|_| LaunchError::Files)?;
    if !m.is_file()
        || m.len() == 0
        || m.len() > limit
        || (private
            && (m.uid() != rustix::process::geteuid().as_raw()
                || m.mode() & 0o777 != 0o600
                || m.nlink() != 1))
    {
        return Err(LaunchError::Files);
    }
    Ok(file)
}
pub(crate) fn digest(value: &str) -> Result<[u8; 32], LaunchError> {
    if value.len() != 64 || !value.bytes().all(|byte| byte.is_ascii_hexdigit()) {
        return Err(LaunchError::Artifact);
    }
    let mut result = [0; 32];
    for (i, byte) in result.iter_mut().enumerate() {
        *byte =
            u8::from_str_radix(&value[i * 2..i * 2 + 2], 16).map_err(|_| LaunchError::Artifact)?;
    }
    Ok(result)
}
pub(crate) fn read(path: &Path, hash: &str) -> Result<Vec<u8>, LaunchError> {
    let mut bytes = Vec::new();
    input(path, 65_536, true)?
        .take(65_537)
        .read_to_end(&mut bytes)
        .map_err(|_| LaunchError::Files)?;
    if bytes.len() > 65_536 || <[u8; 32]>::from(Sha256::digest(&bytes)) != digest(hash)? {
        return Err(LaunchError::Artifact);
    }
    Ok(bytes)
}
pub(crate) fn copy(path: &Path, target: &Path, hash: &str) -> Result<(), LaunchError> {
    let mut source = input(path, 512 * 1024 * 1024, false)?;
    let mut dest = std::fs::OpenOptions::new()
        .write(true)
        .create_new(true)
        .open(target)
        .map_err(|_| LaunchError::Files)?;
    let mut total = 0usize;
    let mut bytes = [0u8; 16384];
    let mut digest_actual = Sha256::new();
    loop {
        let count = source.read(&mut bytes).map_err(|_| LaunchError::Files)?;
        if count == 0 {
            break;
        }
        total += count;
        if total > 512 * 1024 * 1024 {
            return Err(LaunchError::Artifact);
        }
        digest_actual.update(&bytes[..count]);
        dest.write_all(&bytes[..count])
            .map_err(|_| LaunchError::Files)?;
    }
    dest.sync_all().map_err(|_| LaunchError::Files)?;
    if <[u8; 32]>::from(digest_actual.finalize()) != digest(hash)? {
        return Err(LaunchError::Artifact);
    }
    dest.set_permissions(std::fs::Permissions::from_mode(0o500))
        .map_err(|_| LaunchError::Files)?;
    Ok(())
}

#[cfg(test)]
mod tests {
    use super::*;
    #[test]
    fn malformed_hashes_including_utf8_cannot_panic() {
        for value in ["é".repeat(32), "x".repeat(64), "0".repeat(63)] {
            assert!(digest(&value).is_err());
        }
        assert_eq!(digest(&"f".repeat(64)).expect("hex"), [255; 32]);
    }
    #[test]
    fn private_manifest_checks_exact_bytes_modes_links_and_bounds() {
        let dir = tempfile::Builder::new()
            .permissions(std::fs::Permissions::from_mode(0o700))
            .tempdir()
            .expect("private");
        let path = dir.path().join("manifest.json");
        let bytes = b"{}";
        std::fs::write(&path, bytes).expect("synthetic file");
        let hash = format!("{:x}", Sha256::digest(bytes));
        std::fs::set_permissions(&path, std::fs::Permissions::from_mode(0o600)).expect("private");
        assert_eq!(read(&path, &hash).expect("read"), bytes);
        assert!(read(&path, &"0".repeat(64)).is_err());
        let link = dir.path().join("link");
        std::os::unix::fs::symlink(&path, &link).expect("symlink");
        assert!(read(&link, &hash).is_err());
        std::fs::set_permissions(&path, std::fs::Permissions::from_mode(0o644)).expect("public");
        assert!(read(&path, &hash).is_err());
        std::fs::set_permissions(&path, std::fs::Permissions::from_mode(0o600)).expect("private");
        std::fs::write(&path, vec![b'x'; 65537]).expect("oversize");
        assert!(read(&path, &hash).is_err());
    }
}
