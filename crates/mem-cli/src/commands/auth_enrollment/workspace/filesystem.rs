// SPDX-License-Identifier: AGPL-3.0-or-later
use super::Journal;
use anyhow::{Result, bail};
use serde::Serialize;
use sha2::{Digest, Sha256};
use std::{
    fs,
    io::{Read, Write},
    path::{Component, Path, PathBuf},
};
use uuid::Uuid;

pub(super) fn digest(value: &impl Serialize) -> Result<String> {
    Ok(bytes_digest(&serde_json::to_vec(value)?))
}
pub(super) fn bytes_digest(bytes: &[u8]) -> String {
    format!("{:x}", Sha256::digest(bytes))
}

pub(super) fn https_url(value: &str) -> Result<()> {
    if value.len() > 2048 || value.chars().any(|c| c.is_control() || c.is_whitespace()) {
        bail!("reviewed URL is invalid");
    }
    let url = reqwest::Url::parse(value).map_err(|_| anyhow::anyhow!("reviewed URL is invalid"))?;
    if url.scheme() != "https"
        || url.host_str().is_none()
        || !url.username().is_empty()
        || url.password().is_some()
        || url.query().is_some()
        || url.fragment().is_some()
    {
        bail!(
            "reviewed artifacts/repositories require credential-free HTTPS URLs without query or fragment"
        );
    }
    Ok(())
}

pub(super) fn exists(path: &Path) -> Result<bool> {
    match fs::symlink_metadata(path) {
        Ok(_) => Ok(true),
        Err(error) if error.kind() == std::io::ErrorKind::NotFound => Ok(false),
        Err(_) => bail!("could not inspect local workspace path"),
    }
}
fn no_symlinks(path: &Path) -> Result<()> {
    let mut current = PathBuf::new();
    for component in path.components() {
        match component {
            Component::RootDir | Component::Normal(_) => current.push(component.as_os_str()),
            _ => bail!("paths must be absolute without traversal"),
        }
        if let Ok(metadata) = fs::symlink_metadata(&current) {
            if metadata.file_type().is_symlink() {
                bail!(
                    "symlinked workspace/journal paths are not supported; existing paths preserved"
                );
            }
        }
    }
    Ok(())
}
pub(super) fn absolute_leaf(path: &Path) -> Result<PathBuf> {
    if !path.is_absolute()
        || path.file_name().is_none()
        || path.as_os_str().len() > 4096
        || path.to_string_lossy().chars().any(char::is_control)
    {
        bail!("workspace/journal requires a bounded absolute path with a final name");
    }
    no_symlinks(path)?;
    let parent = path
        .parent()
        .ok_or_else(|| anyhow::anyhow!("path parent unavailable"))?;
    if !parent.is_dir() {
        bail!("workspace/journal parent must already exist");
    }
    Ok(path.to_owned())
}

#[cfg(unix)]
fn identity(path: &Path) -> Result<(u64, u64)> {
    use std::os::unix::fs::MetadataExt;
    let metadata = fs::metadata(path)?;
    Ok((metadata.dev(), metadata.ino()))
}

/// Physical ancestor checks catch APFS case aliases, symlinks and mount aliases
/// without assuming that canonical path spelling or ASCII case is authority.
#[cfg(unix)]
pub(super) fn separate(journal: &Path, workspace: &Path) -> Result<()> {
    for path in [journal, workspace] {
        if !path.is_absolute()
            || path
                .components()
                .any(|part| !matches!(part, Component::RootDir | Component::Normal(_)))
        {
            bail!("physical containment requires absolute paths without traversal");
        }
    }
    fn inside(child: &Path, root: &Path) -> Result<bool> {
        if !exists(root)? {
            return Ok(false);
        }
        let root = identity(root)?;
        let mut existing = child;
        while !exists(existing)? {
            existing = existing
                .parent()
                .ok_or_else(|| anyhow::anyhow!("physical ancestor unavailable"))?;
        }
        let existing = fs::canonicalize(existing)?;
        for ancestor in existing.ancestors() {
            if identity(ancestor)? == root {
                return Ok(true);
            }
        }
        Ok(false)
    }
    let journal_lock = lock_path(journal)?;
    let workspace_lock = lock_path(workspace)?;
    // Missing leaves have no inode yet. With the same physical parent, refuse
    // ambiguous ASCII case aliases conservatively, and unsupported Unicode
    // names until they exist and can be checked by physical identity.
    for (journal, workspace) in [
        (journal, workspace),
        (journal_lock.as_path(), workspace),
        (journal, workspace_lock.as_path()),
    ] {
        if inside(journal, workspace)? || inside(workspace, journal)? {
            bail!(
                "the metadata journal and lock sidecars must be physically outside the project workspace"
            );
        }
        if !exists(journal)? && !exists(workspace)? {
            let jp = journal
                .parent()
                .ok_or_else(|| anyhow::anyhow!("journal parent unavailable"))?;
            let wp = workspace
                .parent()
                .ok_or_else(|| anyhow::anyhow!("workspace parent unavailable"))?;
            if identity(jp)? == identity(wp)? {
                let j = journal
                    .file_name()
                    .and_then(|name| name.to_str())
                    .ok_or_else(|| anyhow::anyhow!("new journal name must be ASCII"))?;
                let w = workspace
                    .file_name()
                    .and_then(|name| name.to_str())
                    .ok_or_else(|| anyhow::anyhow!("new workspace name must be ASCII"))?;
                if !j.is_ascii() || !w.is_ascii() || j.eq_ignore_ascii_case(w) {
                    bail!(
                        "new journal/workspace names have ambiguous filesystem aliases; use distinct ASCII names"
                    );
                }
            }
        }
    }
    Ok(())
}
#[cfg(not(unix))]
pub(super) fn separate(_: &Path, _: &Path) -> Result<()> {
    bail!(
        "physical workspace containment validation is unsupported on this platform; no local writes performed"
    )
}
pub(super) fn is_repository(path: &Path) -> Result<bool> {
    no_symlinks(path)?;
    if !path.is_dir() {
        return Ok(false);
    }
    let git = path.join(".git");
    match fs::symlink_metadata(git) {
        Ok(metadata) => {
            Ok(!metadata.file_type().is_symlink() && (metadata.is_dir() || metadata.is_file()))
        }
        Err(error) if error.kind() == std::io::ErrorKind::NotFound => Ok(false),
        Err(_) => bail!("repository metadata unavailable"),
    }
}

pub(super) struct Lock {
    _file: fs::File,
}
fn lock_path(path: &Path) -> Result<PathBuf> {
    Ok(path.with_file_name(format!(
        "{}.memory-lock",
        path.file_name()
            .ok_or_else(|| anyhow::anyhow!("journal name unavailable"))?
            .to_string_lossy()
    )))
}
impl Lock {
    pub(super) fn take(path: &Path) -> Result<Self> {
        let lock = lock_path(path)?;
        no_symlinks(&lock)?;
        let mut options = fs::OpenOptions::new();
        options.write(true).create(true).truncate(false);
        #[cfg(unix)]
        {
            use std::os::unix::fs::OpenOptionsExt;
            options.mode(0o600);
        }
        let file = options.open(lock)?;
        file.try_lock().map_err(|_| {
            anyhow::anyhow!("journal is in use by another process; no effects performed")
        })?;
        // OS lock releases on interruption; leave the same inode for all retries.
        Ok(Self { _file: file })
    }
}

fn exclusive(path: &Path) -> Result<fs::File> {
    let mut options = fs::OpenOptions::new();
    options.write(true).create_new(true);
    #[cfg(unix)]
    {
        use std::os::unix::fs::OpenOptionsExt;
        options.mode(0o600);
    }
    options
        .open(path)
        .map_err(|_| anyhow::anyhow!("refusing to replace an existing local file"))
}
pub(super) fn read(path: &Path) -> Result<Journal> {
    no_symlinks(path)?;
    let mut bytes = Vec::new();
    fs::File::open(path)?
        .take(512 * 1024 + 1)
        .read_to_end(&mut bytes)?;
    if bytes.len() > 512 * 1024 {
        bail!("journal exceeds metadata limit");
    }
    let journal: Journal = serde_json::from_slice(&bytes)
        .map_err(|_| anyhow::anyhow!("journal metadata is invalid"))?;
    if journal.schema_version != 1
        || journal.request_id.is_nil()
        || journal.plan.device_id.is_nil()
        || journal.plan_sha256 != digest(&journal.plan)?
        || journal.plan.manifest_sha256 != digest(&journal.plan.manifest)?
        || digest(&journal.plan.templates)? != digest(&super::templates())?
    {
        bail!("journal immutable inputs or template version conflict; no effects performed");
    }
    absolute_leaf(&journal.plan.workspace)?;
    separate(path, &journal.plan.workspace)?;
    if super::CompanyOrigin::parse(&journal.plan.company, true)?.0 != journal.plan.company {
        bail!("journal company origin is not canonical");
    }
    https_url(&journal.plan.manifest.entry.repository_url)?;
    super::repository::branch(&journal.plan.manifest.entry.default_branch)?;
    Ok(journal)
}
pub(super) fn create_journal(path: &Path, journal: &Journal) -> Result<()> {
    separate(path, &journal.plan.workspace)?;
    let mut file = exclusive(path)?;
    file.write_all(&serde_json::to_vec_pretty(journal)?)?;
    file.sync_all()?;
    Ok(())
}
pub(super) fn save(path: &Path, journal: &Journal) -> Result<()> {
    separate(path, &journal.plan.workspace)?;
    no_symlinks(path)?;
    let temporary = path.with_file_name(format!(".memory-journal-{}.tmp", Uuid::new_v4()));
    create_journal(&temporary, journal)?;
    fs::rename(&temporary, path).map_err(|_| {
        anyhow::anyhow!("journal update unconfirmed; inspect saved metadata before retry")
    })?;
    Ok(())
}

/// Missing-only write, with exclusive creation. A custom file or symlink is
/// preserved rather than being interpreted as a managed config needing upgrade.
pub(super) fn install_missing(root: &Path, relative: &str, bytes: &[u8]) -> Result<bool> {
    no_symlinks(root)?;
    let relative = Path::new(relative);
    if relative
        .components()
        .any(|part| !matches!(part, Component::Normal(_)))
    {
        bail!("unsafe managed file path");
    }
    let target = root.join(relative);
    if exists(&target)? {
        return Ok(false);
    }
    let parent = target
        .parent()
        .ok_or_else(|| anyhow::anyhow!("managed file parent unavailable"))?;
    let mut current = root.to_owned();
    let parents = parent.strip_prefix(root)?;
    for component in parents.components() {
        current.push(component.as_os_str());
        if exists(&current)? {
            let metadata = fs::symlink_metadata(&current)?;
            if metadata.file_type().is_symlink() || !metadata.is_dir() {
                return Ok(false);
            }
        } else {
            fs::create_dir(&current)?;
        }
    }
    // Publish an already complete file with an atomic no-replace hard link.
    // Crashes cannot leave a partially written custom-config-looking target.
    let temporary = parent.join(format!(".memory-file-{}.tmp", Uuid::new_v4()));
    let mut file = exclusive(&temporary)?;
    file.write_all(bytes)?;
    file.sync_all()?;
    let published = fs::hard_link(&temporary, &target);
    let _ = fs::remove_file(&temporary);
    match published {
        Ok(()) => Ok(true),
        Err(_) if exists(&target)? => Ok(false),
        Err(_) => bail!("managed file publication failed; existing files preserved"),
    }
}

pub(super) fn owned_directory(path: &Path) -> Result<()> {
    if exists(path)? {
        no_symlinks(path)?;
        if !path.is_dir() {
            bail!("managed staging path is not a directory");
        }
    } else {
        fs::create_dir(path)?;
    }
    Ok(())
}

pub(super) fn receipt_matches(root: &Path, relative: &str, bytes: &[u8]) -> Result<bool> {
    let path = root.join(relative);
    if !exists(&path)? {
        return Ok(false);
    }
    no_symlinks(&path)?;
    if fs::metadata(&path)?.len() > 4096 {
        bail!("managed publication receipt exceeds limit");
    }
    let mut actual = Vec::new();
    fs::File::open(path)?.take(4097).read_to_end(&mut actual)?;
    if actual != bytes {
        bail!("managed publication belongs to different immutable inputs");
    }
    Ok(true)
}

/// Reserve the final directory exclusively, then publish without replacing any
/// file. A receipt permits interruption recovery, but never grants remote access.
pub(super) fn publish_directory(
    source: &Path,
    target: &Path,
    receipt: &str,
    bytes: &[u8],
) -> Result<()> {
    no_symlinks(source)?;
    no_symlinks(target)?;
    if exists(target)? {
        if !receipt_matches(target, receipt, bytes)? {
            bail!("publication target already exists; no directory was replaced");
        }
    } else {
        fs::create_dir(target)?; // Atomic no-replace reservation, including empty directories.
        if !install_missing(target, receipt, bytes)? {
            bail!("publication receipt conflict");
        }
    }
    if exists(source)? {
        // Receipt is already durably present at the final target. Removing this
        // private duplicate avoids treating its different inode as a collision.
        let source_receipt = source.join(receipt);
        if exists(&source_receipt)? {
            if !receipt_matches(source, receipt, bytes)? {
                bail!("private receipt changed");
            }
            fs::remove_file(source_receipt)?;
        }
        move_tree(source, target)?;
        fs::remove_dir(source)?;
    }
    Ok(())
}

fn move_tree(source: &Path, target: &Path) -> Result<()> {
    for entry in fs::read_dir(source)? {
        let entry = entry?;
        let src = entry.path();
        let dst = target.join(entry.file_name());
        let metadata = fs::symlink_metadata(&src)?;
        if metadata.is_dir() {
            if exists(&dst)? {
                let existing = fs::symlink_metadata(&dst)?;
                if !existing.is_dir() || existing.file_type().is_symlink() {
                    bail!("publication directory collision; user files preserved");
                }
            } else {
                fs::create_dir(&dst)?;
            }
            move_tree(&src, &dst)?;
            fs::remove_dir(&src)?;
        } else if metadata.is_file() {
            if let Err(error) = fs::hard_link(&src, &dst) {
                #[cfg(unix)]
                {
                    use std::os::unix::fs::MetadataExt;
                    // Recover an interruption between link and source unlink.
                    if let Ok(existing) = fs::symlink_metadata(&dst) {
                        if existing.is_file()
                            && metadata.dev() == existing.dev()
                            && metadata.ino() == existing.ino()
                        {
                            fs::remove_file(&src)?;
                            continue;
                        }
                    }
                }
                if let Ok(existing) = fs::symlink_metadata(&dst) {
                    // A rehydrated pinned skill stage may duplicate files already
                    // published before interruption. Never replace divergent data.
                    if existing.is_file()
                        && metadata.len() == existing.len()
                        && metadata.len() <= 2 * 1024 * 1024
                    {
                        let mut left = Vec::new();
                        let mut right = Vec::new();
                        fs::File::open(&src)?
                            .take(2 * 1024 * 1024 + 1)
                            .read_to_end(&mut left)?;
                        fs::File::open(&dst)?
                            .take(2 * 1024 * 1024 + 1)
                            .read_to_end(&mut right)?;
                        if left == right {
                            fs::remove_file(&src)?;
                            continue;
                        }
                    }
                }
                let _ = error;
                bail!("publication file collision; existing user data preserved");
            }
            fs::remove_file(&src)?;
        } else if metadata.file_type().is_symlink() {
            #[cfg(unix)]
            {
                let link = fs::read_link(&src)?;
                if exists(&dst)? {
                    if !fs::symlink_metadata(&dst)?.file_type().is_symlink()
                        || fs::read_link(&dst)? != link
                    {
                        bail!("repository symlink collision; existing path preserved");
                    }
                } else {
                    std::os::unix::fs::symlink(link, &dst)?;
                }
                fs::remove_file(&src)?;
            }
            #[cfg(not(unix))]
            {
                bail!("repository symlink publication unsupported on this platform");
            }
        } else {
            bail!("publication contains unsupported special file");
        }
    }
    Ok(())
}
