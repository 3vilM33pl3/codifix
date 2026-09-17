// SPDX-License-Identifier: AGPL-3.0-or-later

use std::{
    fs::File,
    os::unix::fs::PermissionsExt,
    path::{Component, Path, PathBuf},
};

use rustix::fs::{Mode, OFlags, open, openat};
use sha2::{Digest, Sha256};
use tokio::io::{AsyncReadExt, AsyncWriteExt};

use crate::{Failure, LinuxArchitecture, ProbeConfig};

const MAX_ARTIFACT_BYTES: u64 = 512 * 1024 * 1024;
const SETTINGS: &[u8] = br#"{"retry":{"enabled":false,"maxRetries":0,"fallbackChains":{},"maxFailoversPerTurn":0},"compaction":{"enabled":false}}"#;

pub(crate) struct Prepared {
    pub scratch: tempfile::TempDir,
    pub executable: PathBuf,
    pub home: PathBuf,
    pub cwd: PathBuf,
    pub agent: PathBuf,
    pub settings: PathBuf,
}

fn open_directory(path: &Path, private_ancestry: bool) -> Result<File, Failure> {
    if !path.is_absolute() {
        return Err(Failure::InvalidConfiguration);
    }
    let flags = OFlags::RDONLY | OFlags::DIRECTORY | OFlags::NOFOLLOW | OFlags::CLOEXEC;
    let mut directory =
        open("/", flags, Mode::empty()).map_err(|_| Failure::ArtifactUnavailable)?;
    for component in path.components() {
        match component {
            Component::RootDir => {}
            Component::Normal(name) => {
                if private_ancestry {
                    // A private leaf is insufficient if another user can
                    // rename it through a shared, non-sticky ancestor.
                    use std::os::unix::fs::MetadataExt;
                    let metadata = File::from(
                        directory
                            .try_clone()
                            .map_err(|_| Failure::ScratchUnavailable)?,
                    )
                    .metadata()
                    .map_err(|_| Failure::ScratchUnavailable)?;
                    let trusted_owner = metadata.uid() == 0
                        || metadata.uid() == rustix::process::geteuid().as_raw();
                    let shared_writable = metadata.mode() & 0o022 != 0;
                    let protected_sticky = trusted_owner && metadata.mode() & 0o1000 != 0;
                    if !trusted_owner || (shared_writable && !protected_sticky) {
                        return Err(Failure::ScratchUnavailable);
                    }
                }
                directory = openat(&directory, name, flags, Mode::empty())
                    .map_err(|_| Failure::ArtifactUnavailable)?;
            }
            _ => return Err(Failure::InvalidConfiguration),
        }
    }
    Ok(directory.into())
}

fn open_artifact(path: &Path) -> Result<File, Failure> {
    let parent = path.parent().ok_or(Failure::InvalidConfiguration)?;
    let name = path.file_name().ok_or(Failure::InvalidConfiguration)?;
    let directory = open_directory(parent, false)?;
    let file = openat(
        &directory,
        name,
        OFlags::RDONLY | OFlags::NOFOLLOW | OFlags::CLOEXEC | OFlags::NONBLOCK,
        Mode::empty(),
    )
    .map_err(|_| Failure::ArtifactUnavailable)?;
    let file: File = file.into();
    let metadata = file.metadata().map_err(|_| Failure::ArtifactUnavailable)?;
    if !metadata.is_file() || metadata.len() == 0 || metadata.len() > MAX_ARTIFACT_BYTES {
        return Err(Failure::ArtifactUnavailable);
    }
    Ok(file)
}

pub(crate) async fn prepare(config: &ProbeConfig) -> Result<Prepared, Failure> {
    let host_matches = cfg!(target_os = "linux")
        && match config.architecture {
            LinuxArchitecture::Amd64 => cfg!(target_arch = "x86_64"),
            LinuxArchitecture::Arm64 => cfg!(target_arch = "aarch64"),
        };
    if !host_matches {
        return Err(Failure::WrongArchitecture);
    }
    use std::os::unix::fs::MetadataExt;
    let parent =
        open_directory(&config.scratch_parent, true).map_err(|_| Failure::ScratchUnavailable)?;
    let metadata = parent.metadata().map_err(|_| Failure::ScratchUnavailable)?;
    if metadata.uid() != rustix::process::geteuid().as_raw() || metadata.mode() & 0o7777 != 0o700 {
        return Err(Failure::ScratchUnavailable);
    }
    // The parent is owned by this user and private. Other users cannot replace
    // the newly created scratch directory or its settings/artifact children.
    let scratch = tempfile::Builder::new()
        .prefix("memory-pi-probe-")
        .permissions(std::fs::Permissions::from_mode(0o700))
        .tempdir_in(&config.scratch_parent)
        .map_err(|_| Failure::ScratchUnavailable)?;
    let home = scratch.path().join("home");
    let cwd = scratch.path().join("workspace");
    let agent = scratch.path().join("agent");
    for directory in [&home, &cwd, &agent] {
        tokio::fs::create_dir(directory)
            .await
            .map_err(|_| Failure::ScratchUnavailable)?;
        tokio::fs::set_permissions(directory, std::fs::Permissions::from_mode(0o700))
            .await
            .map_err(|_| Failure::ScratchUnavailable)?;
    }
    let settings = agent.join("settings.json");
    tokio::fs::write(&settings, SETTINGS)
        .await
        .map_err(|_| Failure::ScratchUnavailable)?;
    let executable = scratch.path().join("pi");
    let mut source = tokio::fs::File::from_std(open_artifact(&config.executable)?);
    let mut output = tokio::fs::OpenOptions::new()
        .write(true)
        .create_new(true)
        .open(&executable)
        .await
        .map_err(|_| Failure::ScratchUnavailable)?;
    let mut checksum = Sha256::new();
    let mut buffer = [0_u8; 16_384];
    let mut total = 0_u64;
    loop {
        let read = source
            .read(&mut buffer)
            .await
            .map_err(|_| Failure::ArtifactUnavailable)?;
        if read == 0 {
            break;
        }
        total += read as u64;
        if total > MAX_ARTIFACT_BYTES {
            return Err(Failure::ArtifactUnavailable);
        }
        checksum.update(&buffer[..read]);
        output
            .write_all(&buffer[..read])
            .await
            .map_err(|_| Failure::ScratchUnavailable)?;
    }
    output
        .flush()
        .await
        .map_err(|_| Failure::ScratchUnavailable)?;
    // Tokio may still own an OS write handle after its async wrapper is dropped.
    // Wait for that handle and close it before exec (otherwise Linux may return
    // ETXTBSY when the child races a deferred blocking-file operation).
    drop(output.into_std().await);
    if format!("{:x}", checksum.finalize()) != config.expected_digest {
        return Err(Failure::ArtifactDigestMismatch);
    }
    tokio::fs::set_permissions(&executable, std::fs::Permissions::from_mode(0o500))
        .await
        .map_err(|_| Failure::ScratchUnavailable)?;
    Ok(Prepared {
        scratch,
        executable,
        home,
        cwd,
        agent,
        settings,
    })
}
