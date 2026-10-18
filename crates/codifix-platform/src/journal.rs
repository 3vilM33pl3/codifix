// SPDX-License-Identifier: AGPL-3.0-or-later

//! Private local operation receipts for effects that cannot commit with SQL.
//! A dropped owner is uncertainty, never permission to repeat a command.
use serde::{Deserialize, Serialize};
use sha2::{Digest, Sha256};
#[cfg(not(windows))]
use std::fs::OpenOptions;
use std::{
    fs::{self, File},
    io::{self, Read, Write},
    path::{Path, PathBuf},
};
use uuid::Uuid;

#[derive(Debug, Serialize, Deserialize)]
pub struct Intent {
    pub id: Uuid,
    pub kind: String,
    pub resource: PathBuf,
    pub input_sha256: String,
    pub started_at_unix_ms: u128,
}

pub struct Operation {
    pub intent: Intent,
    directory: PathBuf,
    // Kept open and exclusively locked for the entire external operation.
    _owner: LockedFile,
}

// Construct only after successful acquisition. Closing one descriptor does not
// release a Unix lock while an inherited/duplicated descriptor remains open.
struct LockedFile(File);

impl Drop for LockedFile {
    fn drop(&mut self) {
        let _ = self.0.unlock();
    }
}

fn private_file(path: &Path) -> io::Result<File> {
    #[cfg(windows)]
    {
        crate::private_windows::create_private_file(&std::path::absolute(path)?)
    }
    #[cfg(not(windows))]
    {
        let mut options = OpenOptions::new();
        options.write(true).read(true).create_new(true);
        #[cfg(unix)]
        {
            use std::os::unix::fs::OpenOptionsExt;
            options.mode(0o600);
        }
        options.open(path)
    }
}

fn replace_file(source: &Path, target: &Path) -> io::Result<()> {
    #[cfg(windows)]
    {
        crate::private_windows::replace_file(source, target)
    }
    #[cfg(not(windows))]
    {
        fs::rename(source, target)
    }
}

fn sync_directory(path: &Path) -> io::Result<()> {
    #[cfg(unix)]
    {
        File::open(path)?.sync_all()
    }
    // std does not provide portable directory fsync on Windows. File contents
    // are synced; power-loss metadata durability must be qualified separately.
    #[cfg(not(unix))]
    {
        let _ = path;
        Ok(())
    }
}

fn create_root(root: &Path) -> io::Result<()> {
    let mut missing = Vec::new();
    let mut parent = root;
    while !parent.exists() {
        missing.push(parent);
        let Some(next) = parent.parent() else {
            break;
        };
        if next.as_os_str().is_empty() {
            break;
        }
        parent = next;
    }
    #[cfg(windows)]
    {
        for path in missing.iter().rev() {
            match crate::private_windows::create_private_directory(&std::path::absolute(path)?) {
                Ok(()) => {}
                Err(error) if error.kind() == io::ErrorKind::AlreadyExists => {}
                Err(error) => return Err(error),
            }
        }
        crate::private_windows::Directory::open(&std::path::absolute(root)?)?;
    }
    #[cfg(not(windows))]
    fs::create_dir_all(root)?;
    for path in missing.into_iter().rev() {
        sync_directory(path)?;
        if let Some(parent) = path.parent().filter(|p| !p.as_os_str().is_empty()) {
            sync_directory(parent)?;
        }
    }
    Ok(())
}

fn receipt(directory: &Path, name: &str, bytes: &[u8]) -> io::Result<()> {
    // Readers only observe an atomically installed, fsynced receipt. Unique
    // owner/recovery locks ensure this never replaces a concurrent receipt.
    let temporary = directory.join(format!("{}.tmp", Uuid::new_v4()));
    let mut file = private_file(&temporary)?;
    file.write_all(bytes)?;
    file.sync_all()?;
    replace_file(&temporary, &directory.join(name))?;
    sync_directory(directory)
}

/// Serialize read/modify/replace across local processes. The data file is
/// atomically installed; its receipt may still need reconciliation after a
/// crash. No application payload is copied into the operational journal.
pub fn update_json<T, R>(path: &Path, change: impl FnOnce(&mut T) -> io::Result<R>) -> io::Result<R>
where
    T: Serialize + for<'de> Deserialize<'de> + Default,
{
    let parent = path
        .parent()
        .ok_or_else(|| io::Error::new(io::ErrorKind::InvalidInput, "state path needs a parent"))?;
    create_root(parent)?;
    reconcile(&parent.join("operation-journal"), 100)?;
    let lock_path = path.with_extension("json.lock");
    #[cfg(windows)]
    let lock = crate::private_windows::open_private_lock(&std::path::absolute(&lock_path)?)?;
    #[cfg(not(windows))]
    let lock = {
        let mut options = OpenOptions::new();
        options.read(true).write(true).create(true).truncate(false);
        #[cfg(unix)]
        {
            use std::os::unix::fs::OpenOptionsExt;
            options.mode(0o600);
        }
        options.open(lock_path)?
    };
    lock.lock()?;
    let _lock = LockedFile(lock);
    let mut state = match File::open(path) {
        Ok(file) => {
            let mut bytes = Vec::new();
            file.take(32 * 1024 * 1024 + 1).read_to_end(&mut bytes)?;
            if bytes.len() > 32 * 1024 * 1024 {
                return Err(io::Error::new(
                    io::ErrorKind::InvalidData,
                    "local state exceeds limit",
                ));
            }
            serde_json::from_slice(&bytes).map_err(io::Error::other)?
        }
        Err(error) if error.kind() == io::ErrorKind::NotFound => T::default(),
        Err(error) => return Err(error),
    };
    let result = change(&mut state)?;
    let bytes = serde_json::to_vec_pretty(&state).map_err(io::Error::other)?;
    if bytes.len() > 32 * 1024 * 1024 {
        return Err(io::Error::new(
            io::ErrorKind::InvalidData,
            "local state exceeds limit",
        ));
    }
    let operation = Operation::begin(
        &parent.join("operation-journal"),
        "file_replace",
        path,
        &bytes,
    )?;
    let temporary = parent.join(format!("{}.tmp", operation.intent.id));
    let mut file = private_file(&temporary)?;
    file.write_all(&bytes)?;
    file.sync_all()?;
    replace_file(&temporary, path)?;
    sync_directory(parent)?;
    operation.observed(true)?;
    Ok(result)
}

impl Operation {
    pub fn begin(root: &Path, kind: &str, resource: &Path, input: &[u8]) -> io::Result<Self> {
        if kind.is_empty()
            || kind.len() > 80
            || !kind.bytes().all(|b| b.is_ascii_lowercase() || b == b'_')
        {
            return Err(io::Error::new(
                io::ErrorKind::InvalidInput,
                "invalid operation kind",
            ));
        }
        if resource.as_os_str().len() > 4096 {
            return Err(io::Error::new(
                io::ErrorKind::InvalidInput,
                "operation resource too long",
            ));
        }
        create_root(root)?;
        let intent = Intent {
            id: Uuid::new_v4(),
            kind: kind.into(),
            resource: resource.into(),
            input_sha256: format!("{:x}", Sha256::digest(input)),
            started_at_unix_ms: std::time::SystemTime::now()
                .duration_since(std::time::UNIX_EPOCH)
                .map_err(io::Error::other)?
                .as_millis(),
        };
        let directory = root.join(intent.id.to_string());
        #[cfg(windows)]
        crate::private_windows::create_private_directory(&std::path::absolute(&directory)?)?;
        #[cfg(not(windows))]
        {
            let mut builder = fs::DirBuilder::new();
            #[cfg(unix)]
            {
                use std::os::unix::fs::DirBuilderExt;
                builder.mode(0o700);
            }
            builder.create(&directory)?;
        }
        sync_directory(root)?;
        #[cfg(windows)]
        let owner = crate::private_windows::open_private_lock(&std::path::absolute(
            directory.join("owner.lock"),
        )?)?;
        #[cfg(not(windows))]
        let owner = private_file(&directory.join("owner.lock"))?;
        owner.lock()?;
        let owner = LockedFile(owner);
        receipt(
            &directory,
            "intent.json",
            &serde_json::to_vec(&intent).map_err(io::Error::other)?,
        )?;
        Ok(Self {
            intent,
            directory,
            _owner: owner,
        })
    }

    /// This confirms only the observed external operation, not a subsequent
    /// database/timeline acknowledgement. Calling code must name its boundary.
    pub fn observed(&self, succeeded: bool) -> io::Result<()> {
        let bytes: &[u8] = if succeeded {
            b"{\"status\":\"confirmed\"}"
        } else {
            b"{\"status\":\"uncertain\"}"
        };
        if let Ok(existing) = fs::read(self.directory.join("observed.json")) {
            return if existing == bytes {
                Ok(())
            } else {
                Err(io::Error::new(
                    io::ErrorKind::AlreadyExists,
                    "operation outcome is immutable",
                ))
            };
        }
        receipt(&self.directory, "observed.json", bytes)
            .map_err(|error| io::Error::other(format!("operation {} acknowledgement failed: {error}; external effects were not rolled back", self.intent.id)))
    }
}

/// Inspect a bounded batch of committed intents. Live owners are excluded;
/// interrupted work receives an immutable uncertainty receipt, never replay.
/// Journals are private host-local state, not project timeline/export content.
pub fn reconcile(root: &Path, limit: usize) -> io::Result<Vec<Intent>> {
    if !root.exists() {
        return Ok(Vec::new());
    }
    let mut interrupted = Vec::new();
    for entry in fs::read_dir(root)? {
        if interrupted.len() >= limit.clamp(1, 100) {
            break;
        }
        let entry = entry?;
        if !entry.file_type()?.is_dir()
            || Uuid::parse_str(&entry.file_name().to_string_lossy()).is_err()
        {
            continue;
        }
        let directory = entry.path();
        if !directory.join("intent.json").exists()
            || valid_receipt(&directory, "observed.json")?
            || valid_receipt(&directory, "interrupted.json")?
        {
            continue;
        }
        #[cfg(windows)]
        let owner = crate::private_windows::open_existing_private_lock(&std::path::absolute(
            directory.join("owner.lock"),
        )?)?;
        #[cfg(not(windows))]
        let owner = OpenOptions::new()
            .read(true)
            .write(true)
            .open(directory.join("owner.lock"))?;
        match owner.try_lock() {
            Ok(()) => {}
            Err(std::fs::TryLockError::WouldBlock) => continue,
            Err(std::fs::TryLockError::Error(error)) => return Err(error),
        }
        let _owner = LockedFile(owner);
        // Recheck after ownership: the original caller may have just finished.
        if valid_receipt(&directory, "observed.json")?
            || valid_receipt(&directory, "interrupted.json")?
        {
            continue;
        }
        let mut bytes = Vec::new();
        File::open(directory.join("intent.json"))?
            .take(32769)
            .read_to_end(&mut bytes)?;
        if bytes.len() > 32768 {
            return Err(io::Error::new(
                io::ErrorKind::InvalidData,
                "oversized local intent",
            ));
        }
        let intent = serde_json::from_slice(&bytes).map_err(io::Error::other)?;
        receipt(
            &directory,
            "interrupted.json",
            b"{\"status\":\"uncertain\",\"reason\":\"owner_disconnected\"}",
        )?;
        interrupted.push(intent);
    }
    Ok(interrupted)
}

fn valid_receipt(directory: &Path, name: &str) -> io::Result<bool> {
    let path = directory.join(name);
    let metadata = match fs::symlink_metadata(&path) {
        Ok(metadata) => metadata,
        Err(error) if error.kind() == io::ErrorKind::NotFound => return Ok(false),
        Err(error) => return Err(error),
    };
    if !metadata.is_file() || metadata.len() > 256 {
        return Err(io::Error::new(
            io::ErrorKind::InvalidData,
            "invalid local operation receipt",
        ));
    }
    let value: serde_json::Value =
        serde_json::from_slice(&fs::read(path)?).map_err(io::Error::other)?;
    if !matches!(
        value.get("status").and_then(|v| v.as_str()),
        Some("confirmed" | "uncertain")
    ) {
        return Err(io::Error::new(
            io::ErrorKind::InvalidData,
            "invalid local operation receipt status",
        ));
    }
    Ok(true)
}

#[cfg(test)]
mod tests {
    use super::*;
    #[test]
    fn journal_observes_owner_loss_without_repeating_or_exposing_inputs() {
        let root = std::env::temp_dir().join(format!("memory-journal-{}", Uuid::new_v4()));
        let operation = Operation::begin(
            &root,
            "command",
            Path::new("fixture"),
            b"secret command argument",
        )
        .unwrap();
        let directory = root.join(operation.intent.id.to_string());
        assert!(
            !fs::read_to_string(directory.join("intent.json"))
                .unwrap()
                .contains("secret")
        );
        assert!(reconcile(&root, 10).unwrap().is_empty());
        // A duplicate models an inherited descriptor in another process. Its
        // lifetime must not extend the operation's ownership after owner drop.
        #[cfg(unix)]
        let retained_descriptor = operation._owner.0.try_clone().unwrap();
        assert!(reconcile(&root, 10).unwrap().is_empty());
        drop(operation);
        assert_eq!(reconcile(&root, 10).unwrap().len(), 1);
        assert!(reconcile(&root, 10).unwrap().is_empty());
        let done = Operation::begin(&root, "file_replace", Path::new("fixture"), b"bytes").unwrap();
        done.observed(true).unwrap();
        done.observed(true).unwrap();
        assert!(done.observed(false).is_err());
        drop(done);
        assert!(reconcile(&root, 10).unwrap().is_empty());
        #[cfg(unix)]
        drop(retained_descriptor);
        fs::remove_dir_all(root).unwrap();
    }

    #[test]
    fn concurrent_state_updates_preserve_each_mutation_and_reject_failed_edits() {
        let root = std::env::temp_dir().join(format!("memory-state-{}", Uuid::new_v4()));
        let path = root.join("state.json");
        let writers: Vec<_> = (0..8)
            .map(|_| {
                let path = path.clone();
                std::thread::spawn(move || {
                    update_json::<u64, _>(&path, |value| {
                        *value += 1;
                        Ok(())
                    })
                    .unwrap()
                })
            })
            .collect();
        for writer in writers {
            writer.join().unwrap();
        }
        assert_eq!(fs::read_to_string(&path).unwrap(), "8");
        assert!(
            update_json::<u64, ()>(&path, |value| {
                *value = 99;
                Err(io::Error::other("rejected edit"))
            })
            .is_err()
        );
        assert_eq!(fs::read_to_string(&path).unwrap(), "8");
        assert!(
            reconcile(&root.join("operation-journal"), 100)
                .unwrap()
                .is_empty()
        );
        fs::remove_dir_all(root).unwrap();
    }

    #[test]
    fn lost_outcome_receipt_does_not_undo_external_bytes() {
        let root = std::env::temp_dir().join(format!("memory-outcome-{}", Uuid::new_v4()));
        let target = root.join("external.txt");
        let operation = Operation::begin(
            &root.join("journal"),
            "file_replace",
            &target,
            b"observed bytes",
        )
        .unwrap();
        fs::write(&target, b"observed bytes").unwrap();
        // Force only the acknowledgement to fail, after the external write.
        fs::create_dir(operation.directory.join("observed.json")).unwrap();
        let error = operation.observed(true).unwrap_err();
        assert!(error.to_string().contains("not rolled back"));
        assert_eq!(fs::read(&target).unwrap(), b"observed bytes");
        drop(operation);
        assert!(reconcile(&root.join("journal"), 100).is_err());
        fs::remove_dir_all(root).unwrap();
    }
}
