// SPDX-License-Identifier: AGPL-3.0-or-later
//! Service credentials must never be written with inherited public Windows ACLs.
use anyhow::{Context, Result, bail};
use mem_platform::private_windows::{self, Directory, Input};
use std::{
    fs,
    io::{Read, Write},
    path::Path,
};
use uuid::Uuid;

pub(super) fn write(path: &Path, key: &str, value: &str) -> Result<()> {
    if key.is_empty()
        || !key.bytes().all(|b| b.is_ascii_alphanumeric() || b == b'_')
        || value.contains(['\r', '\n', '\0'])
    {
        bail!("invalid environment entry");
    }
    let path = std::path::absolute(path)?;
    let parent = path.parent().context("environment file has no parent")?;
    let name = path
        .file_name()
        .and_then(|v| v.to_str())
        .context("invalid environment filename")?;
    let mut missing = Vec::new();
    let mut ancestor = parent;
    while !ancestor.exists() {
        missing.push(ancestor);
        ancestor = ancestor
            .parent()
            .context("environment directory has no ancestor")?;
    }
    for directory in missing.iter().rev() {
        match private_windows::create_private_directory(directory) {
            Ok(()) => {}
            Err(error) if error.kind() == std::io::ErrorKind::AlreadyExists => {}
            Err(error) => return Err(error.into()),
        }
    }
    let directory = Directory::open(parent).context("environment directory is not private")?;
    let lock_path = parent.join(format!(".{name}.lock"));
    let lock = private_windows::open_private_lock(&lock_path)?;
    lock.lock()?;
    // Read through a retained handle, never a symlink-following ordinary read.
    let input = match Input::open(&path, 256 * 1024) {
        Ok(input) => Some(input),
        Err(error) if error.kind() == std::io::ErrorKind::NotFound => None,
        Err(error) => return Err(error.into()),
    };
    let mut before = Vec::new();
    if let Some(input) = &input {
        (&input.file)
            .take(256 * 1024 + 1)
            .read_to_end(&mut before)?;
        input.unchanged()?;
    }
    let mut lines: Vec<_> = std::str::from_utf8(&before)?
        .lines()
        .map(str::to_owned)
        .collect();
    let wanted = format!("{key}={value}");
    let mut replaced = false;
    for line in &mut lines {
        if line
            .split_once('=')
            .is_some_and(|(existing, _)| existing.trim() == key)
        {
            *line = wanted.clone();
            replaced = true;
        }
    }
    if !replaced {
        lines.push(wanted);
    }
    let content = lines.join("\n") + "\n";
    if content.len() > 256 * 1024 {
        bail!("environment file exceeds its size limit");
    }
    let temporary = format!(".memory-env-{}.partial", Uuid::new_v4());
    let mut file = directory.create(&temporary)?;
    let result = (|| -> Result<()> {
        file.write_all(content.as_bytes())?;
        file.sync_all()?;
        directory.check_entry(&temporary, &file)?;
        // Windows sharing checks are symmetric: a read-only Input cannot
        // coexist with this writer because Input deliberately denies writers.
        // Close the creator before reviewing; publication verifies both this
        // identity and the complete expected bytes again before rename.
        drop(file);
        let identity =
            private_windows::Input::open(&parent.join(&temporary), 256 * 1024)?.identity();
        private_windows::publish_reviewed_settings(
            &directory,
            &temporary,
            identity,
            &directory,
            name,
            input.as_ref().map(|input| (input, before.as_slice())),
            content.as_bytes(),
        )?;
        Ok(())
    })();
    if result.is_err() {
        let _ = fs::remove_file(parent.join(&temporary));
    }
    result
}

#[cfg(test)]
mod tests {
    use super::*;
    struct Root(std::path::PathBuf);
    impl Root {
        fn new() -> Self {
            let path = std::env::temp_dir().join(format!("memory-env-test-{}", Uuid::new_v4()));
            private_windows::create_private_directory(&path).unwrap();
            Self(path)
        }
    }
    impl Drop for Root {
        fn drop(&mut self) {
            let _ = fs::remove_dir_all(&self.0);
        }
    }

    #[test]
    fn private_credentials_replace_without_losing_unrelated_entries() {
        let root = Root::new();
        let path = root.0.join("shared.env");
        write(&path, "FIRST", "old").unwrap();
        write(&path, "SECOND", "kept").unwrap();
        write(&path, "FIRST", "new").unwrap();
        assert_eq!(
            Input::open(&path, 1024).unwrap().read(1024).unwrap(),
            b"FIRST=new\nSECOND=kept\n"
        );
        assert!(write(&path, "FIRST", "bad\nINJECTED=yes").is_err());
    }

    #[test]
    fn hard_linked_credentials_are_rejected_without_modifying_either_name() {
        let root = Root::new();
        let path = root.0.join("shared.env");
        write(&path, "FIRST", "old").unwrap();
        let alias = root.0.join("alias.env");
        fs::hard_link(&path, &alias).unwrap();
        assert!(write(&path, "FIRST", "new").is_err());
        assert_eq!(fs::read(&alias).unwrap(), b"FIRST=old\n");
    }
}
