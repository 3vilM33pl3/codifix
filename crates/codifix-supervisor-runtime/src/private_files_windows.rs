//! Handle-pinned Windows authority reads; no AppContainer grants are permitted.
use crate::launcher::LaunchError;
use codifix_platform_files::windows::{Directory, Input, create_private_file};
use sha2::{Digest, Sha256};
use std::{
    io::{Read, Write},
    path::Path,
};

pub(crate) fn directory(path: &Path, _private: bool) -> Result<Directory, LaunchError> {
    Directory::open(path).map_err(|_| LaunchError::Files)
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
    let bytes = Input::open(path, 65_536)
        .and_then(|input| input.read(65_536))
        .map_err(|_| LaunchError::Files)?;
    if <[u8; 32]>::from(Sha256::digest(&bytes)) != digest(hash)? {
        return Err(LaunchError::Artifact);
    }
    Ok(bytes)
}
pub(crate) fn copy(path: &Path, target: &Path, hash: &str) -> Result<(), LaunchError> {
    // Operator artifacts currently require a private staging directory. This is
    // deliberately stricter than accepting arbitrary Program Files ACLs.
    let mut source = Input::open(path, 512 * 1024 * 1024).map_err(|_| LaunchError::Files)?;
    let mut dest = create_private_file(target).map_err(|_| LaunchError::Files)?;
    let mut total = 0u64;
    let mut bytes = [0; 65536];
    let mut actual = Sha256::new();
    loop {
        let count = source
            .file
            .read(&mut bytes)
            .map_err(|_| LaunchError::Files)?;
        if count == 0 {
            break;
        }
        total += count as u64;
        if total > 512 * 1024 * 1024 {
            return Err(LaunchError::Artifact);
        }
        actual.update(&bytes[..count]);
        dest.write_all(&bytes[..count])
            .map_err(|_| LaunchError::Files)?;
    }
    dest.sync_all().map_err(|_| LaunchError::Files)?;
    source.unchanged().map_err(|_| LaunchError::Files)?;
    if <[u8; 32]>::from(actual.finalize()) != digest(hash)? {
        return Err(LaunchError::Artifact);
    }
    Ok(())
}
