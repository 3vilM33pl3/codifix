// SPDX-License-Identifier: AGPL-3.0-or-later
//! Directory handles fence path substitution; the lock coordinates Memory writers.
//! A non-cooperating same-user writer in the final rename window is not a POSIX CAS.
use super::{MAX_BYTES, bytes_hash, changed};
use anyhow::{Result, ensure};
use codifix_record::CompanyAgentCaller;
use rustix::fs::{self, FileType, Mode, OFlags, RenameFlags};
use std::{
    fs::File,
    io::{Read, Write},
    path::{Component, Path, PathBuf},
};
use uuid::Uuid;
type FileIdentity = (u64, u64);
type Snapshot = (Vec<u8>, FileIdentity);
fn error() -> anyhow::Error {
    anyhow::anyhow!(
        "protected settings filesystem operation failed; existing settings were not deliberately overwritten"
    )
}
fn directory(path: &Path) -> Result<File> {
    ensure!(path.is_absolute(), "absolute protected directory required");
    let mut dir: File = fs::open(
        "/",
        OFlags::RDONLY | OFlags::DIRECTORY | OFlags::CLOEXEC,
        Mode::empty(),
    )
    .map_err(|_| error())?
    .into();
    for part in path.components() {
        match part {
            Component::RootDir => {}
            Component::Normal(name) => {
                dir = fs::openat(
                    &dir,
                    name,
                    OFlags::RDONLY | OFlags::DIRECTORY | OFlags::NOFOLLOW | OFlags::CLOEXEC,
                    Mode::empty(),
                )
                .map_err(|_| error())?
                .into();
            }
            _ => return Err(error()),
        }
    }
    let stat = fs::fstat(&dir).map_err(|_| error())?;
    ensure!(
        stat.st_uid == rustix::process::getuid().as_raw() && stat.st_mode & 0o022 == 0,
        "protected directory must be owned by current user and not shared writable"
    );
    Ok(dir)
}
#[allow(clippy::unnecessary_cast)] // Stat field width differs on supported Unix targets.
fn identity(file: &File) -> Result<FileIdentity> {
    let s = fs::fstat(file).map_err(|_| error())?;
    Ok((s.st_dev as u64, s.st_ino as u64))
}
fn regular(file: &File) -> Result<()> {
    let s = fs::fstat(file).map_err(|_| error())?;
    ensure!(
        FileType::from_raw_mode(s.st_mode) == FileType::RegularFile
            && s.st_uid == rustix::process::getuid().as_raw()
            && s.st_nlink == 1
            && s.st_mode & 0o022 == 0,
        "settings must be unlinked private regular files owned by current user"
    );
    Ok(())
}
fn read_at(dir: &File, name: &str) -> Result<Option<Snapshot>> {
    let fd = match fs::openat(
        dir,
        name,
        OFlags::RDONLY | OFlags::NOFOLLOW | OFlags::CLOEXEC | OFlags::NONBLOCK,
        Mode::empty(),
    ) {
        Ok(v) => v,
        Err(rustix::io::Errno::NOENT) => return Ok(None),
        Err(_) => return Err(error()),
    };
    let mut file: File = fd.into();
    regular(&file)?;
    let id = identity(&file)?;
    let mut bytes = Vec::new();
    Read::by_ref(&mut file)
        .take(MAX_BYTES as u64 + 1)
        .read_to_end(&mut bytes)
        .map_err(|_| error())?;
    ensure!(bytes.len() <= MAX_BYTES, "settings file exceeds size bound");
    Ok(Some((bytes, id)))
}
pub(super) struct Settings {
    workspace: PathBuf,
    root: File,
    target: Option<File>,
    stage: File,
    caller: CompanyAgentCaller,
    initial: Option<FileIdentity>,
}
impl Settings {
    pub(super) fn open(
        workspace: &Path,
        caller: CompanyAgentCaller,
        journal: &Path,
    ) -> Result<Self> {
        let root = directory(workspace)?;
        let parent = journal.parent().ok_or_else(error)?;
        ensure!(
            !parent.starts_with(workspace),
            "settings metadata/staging must remain outside the project"
        );
        // Physical workspace lock, independent of journal path and aliases.
        // Both providers serialize; no repository lock sidecar is introduced.
        root.try_lock()
            .map_err(|_| anyhow::anyhow!("another settings operation holds the project lock"))?;
        let stage = directory(workspace.parent().ok_or_else(error)?)?;
        let target = match caller {
            CompanyAgentCaller::Claude => Some(root.try_clone().map_err(|_| error())?),
            CompanyAgentCaller::Codex => match fs::openat(
                &root,
                ".codex",
                OFlags::RDONLY | OFlags::DIRECTORY | OFlags::NOFOLLOW | OFlags::CLOEXEC,
                Mode::empty(),
            ) {
                Ok(fd) => {
                    let file: File = fd.into();
                    let s = fs::fstat(&file).map_err(|_| error())?;
                    ensure!(
                        s.st_uid == rustix::process::getuid().as_raw() && s.st_mode & 0o022 == 0,
                        "Codex settings directory is not private"
                    );
                    Some(file)
                }
                Err(rustix::io::Errno::NOENT) => None,
                Err(_) => return Err(error()),
            },
        };
        ensure!(
            fs::fstat(&stage).map_err(|_| error())?.st_dev
                == fs::fstat(target.as_ref().unwrap_or(&root))
                    .map_err(|_| error())?
                    .st_dev,
            "settings parent staging must share target filesystem; mount-root workspaces or mounted settings directories are unsupported"
        );
        let mut value = Self {
            workspace: workspace.into(),
            root,
            target,
            stage,
            caller,
            initial: None,
        };
        value.initial = value.snapshot()?.map(|(_, id)| id);
        Ok(value)
    }
    fn leaf(&self) -> &'static str {
        match self.caller {
            CompanyAgentCaller::Codex => "config.toml",
            CompanyAgentCaller::Claude => ".mcp.json",
        }
    }
    fn snapshot(&self) -> Result<Option<Snapshot>> {
        self.target
            .as_ref()
            .map(|dir| read_at(dir, self.leaf()))
            .transpose()
            .map(Option::flatten)
    }
    pub(super) fn read(&self) -> Result<Option<Vec<u8>>> {
        Ok(self.snapshot()?.map(|(bytes, _)| bytes))
    }
    pub(super) fn replace(
        &mut self,
        before: Option<&str>,
        after: &str,
        result: &[u8],
        id: Uuid,
    ) -> Result<()> {
        ensure!(
            bytes_hash(result) == after && result.len() <= MAX_BYTES,
            "settings result changed"
        );
        let fresh = directory(&self.workspace)?;
        ensure!(
            identity(&fresh)? == identity(&self.root)?,
            "workspace directory changed"
        );
        if self.target.is_none() {
            fs::mkdirat(&self.root, ".codex", Mode::from_raw_mode(0o700)).map_err(|_| error())?;
            self.target = Some(directory(&self.workspace.join(".codex"))?);
        }
        let target = self.target.as_ref().ok_or_else(error)?;
        ensure!(
            fs::fstat(&self.stage).map_err(|_| error())?.st_dev
                == fs::fstat(target).map_err(|_| error())?.st_dev,
            "settings parent staging must share target filesystem; mounted target directories require an ordinary workspace layout"
        );
        let current_dir = directory(&match self.caller {
            CompanyAgentCaller::Codex => self.workspace.join(".codex"),
            CompanyAgentCaller::Claude => self.workspace.clone(),
        })?;
        ensure!(
            identity(target)? == identity(&current_dir)?,
            "settings directory changed"
        );
        let current = self.snapshot()?;
        if current
            .as_ref()
            .is_some_and(|(bytes, _)| bytes_hash(bytes) == after)
        {
            return Ok(());
        }
        ensure!(
            current.as_ref().map(|(b, _)| bytes_hash(b)).as_deref() == before
                && current.as_ref().map(|(_, id)| *id) == self.initial,
            "settings changed since preview; no overwrite performed"
        );
        let name = format!(".memory-agent-settings-{id}.tmp");
        let stage_identity = match fs::openat(
            &self.stage,
            name.as_str(),
            OFlags::WRONLY | OFlags::CREATE | OFlags::EXCL | OFlags::NOFOLLOW | OFlags::CLOEXEC,
            Mode::from_raw_mode(0o600),
        ) {
            Ok(fd) => {
                let mut file: File = fd.into();
                regular(&file)?;
                file.write_all(result).map_err(|_| error())?;
                file.sync_all().map_err(|_| error())?;
                identity(&file)?
            }
            Err(rustix::io::Errno::EXIST) => {
                let saved = read_at(&self.stage, &name)?.ok_or_else(error)?;
                ensure!(
                    result.starts_with(&saved.0),
                    "interrupted settings staging conflicts; unknown bytes preserved"
                );
                if saved.0.len() < result.len() {
                    // Resume only the exact intent-derived prefix, never
                    // truncate unknown bytes after an interruption or ENOSPC.
                    let fd = fs::openat(
                        &self.stage,
                        name.as_str(),
                        OFlags::RDWR | OFlags::NOFOLLOW | OFlags::CLOEXEC | OFlags::NONBLOCK,
                        Mode::empty(),
                    )
                    .map_err(|_| error())?;
                    let mut file: File = fd.into();
                    regular(&file)?;
                    ensure!(
                        identity(&file)? == saved.1,
                        "partial stage identity changed"
                    );
                    let mut bytes = Vec::new();
                    Read::by_ref(&mut file)
                        .take(MAX_BYTES as u64 + 1)
                        .read_to_end(&mut bytes)
                        .map_err(|_| error())?;
                    ensure!(bytes == saved.0, "partial staging bytes changed; preserved");
                    let latest = self.snapshot()?;
                    ensure!(
                        latest.as_ref().map(|(b, _)| bytes_hash(b)).as_deref() == before
                            && latest.as_ref().map(|(_, id)| *id) == self.initial,
                        "settings changed before partial-stage recovery"
                    );
                    ensure!(
                        identity(&directory(&self.workspace)?)? == identity(&self.root)?
                            && identity(&current_dir)?
                                == identity(&directory(&match self.caller {
                                    CompanyAgentCaller::Codex => self.workspace.join(".codex"),
                                    CompanyAgentCaller::Claude => self.workspace.clone(),
                                })?)?,
                        "directory changed before partial-stage recovery"
                    );
                    file.write_all(&result[bytes.len()..])
                        .map_err(|_| error())?;
                    file.sync_all().map_err(|_| error())?;
                }
                saved.1
            }
            Err(_) => return Err(error()),
        };
        // Immediate hash + inode recheck under our lock, after staging I/O.
        let latest = self.snapshot()?;
        ensure!(
            latest.as_ref().map(|(b, _)| bytes_hash(b)).as_deref() == before
                && latest.as_ref().map(|(_, id)| *id) == self.initial,
            "settings changed during staging; no overwrite performed"
        );
        ensure!(
            identity(&directory(&self.workspace)?)? == identity(&self.root)?
                && identity(&current_dir)?
                    == identity(&directory(&match self.caller {
                        CompanyAgentCaller::Codex => self.workspace.join(".codex"),
                        CompanyAgentCaller::Claude => self.workspace.clone(),
                    })?)?,
            "settings directory changed during staging"
        );
        let staged = read_at(&self.stage, &name)?.ok_or_else(error)?;
        ensure!(
            staged.1 == stage_identity && bytes_hash(&staged.0) == after,
            "settings staging identity or bytes changed"
        );
        if before.is_none() {
            fs::renameat_with(
                &self.stage,
                name.as_str(),
                target,
                self.leaf(),
                RenameFlags::NOREPLACE,
            )
            .map_err(|_| changed())?;
        } else {
            fs::renameat(&self.stage, name.as_str(), target, self.leaf()).map_err(|_| error())?;
        }
        fs::fsync(target).map_err(|_| error())?;
        fs::fsync(&self.stage).map_err(|_| error())?;
        ensure!(
            self.read()?.as_deref().map(bytes_hash).as_deref() == Some(after),
            "installed settings changed; inspect before retrying"
        );
        Ok(())
    }
}
