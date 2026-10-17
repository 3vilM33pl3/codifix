// SPDX-License-Identifier: AGPL-3.0-or-later
//! Unprivileged, explicit trusted-host journal I/O. Never an agent endpoint.
//! Existing ACL/handle validation and write-through publication are reused;
//! no directory FlushFileBuffers or power-loss guarantee is invented.
use crate::private_windows::{Directory, Input, create_private_directory};
use anyhow::{Result, ensure};
use serde::Serialize;
use serde_json::Value;
use sha2::{Digest, Sha256};
use std::{io::Write, path::Path};

pub const LIMIT: u64 = 16 * 1024 * 1024;
#[derive(Serialize)]
pub struct Receipt {
    format: &'static str,
    operation: String,
    name: String,
    directory_identity: (String, String),
    #[serde(skip_serializing_if = "Option::is_none")]
    sha256: Option<String>,
    #[serde(skip_serializing_if = "Option::is_none")]
    value: Option<Value>,
}

pub fn run(operation: &str, parent: &Path, name: &str, bytes: &[u8]) -> Result<Receipt> {
    ensure!(
        !name.is_empty()
            && name.len() <= 128
            && name
                .bytes()
                .all(|b| b.is_ascii_lowercase() || b.is_ascii_digit() || b".-".contains(&b))
            && !name.starts_with('.')
            && !name.ends_with('.')
            && !name.contains(".."),
        "explicit journal component required"
    );
    let directory = Directory::open(parent)?;
    let mut receipt = Receipt {
        format: "memory-windows-host-journal-v1",
        operation: operation.to_owned(),
        name: name.to_owned(),
        directory_identity: identity(&directory)?,
        sha256: None,
        value: None,
    };
    match operation {
        "create" => {
            ensure!(bytes.is_empty(), "directory creation has no payload");
            directory.ensure_absent(&[name])?;
            let path = parent.join(name);
            create_private_directory(&path)?;
            let child = Directory::open(&path)?;
            ensure!(child.is_beneath(&directory)?, "journal directory changed");
            receipt.directory_identity = identity(&child)?;
        }
        "write" => {
            ensure!(bytes.len() as u64 <= LIMIT, "bounded journal required");
            let _: Value = serde_json::from_slice(bytes)?;
            directory.ensure_absent(&[name])?;
            let partial = format!("partial-{}.json", uuid::Uuid::new_v4());
            let mut file = directory.create(&partial)?;
            file.write_all(bytes)?;
            directory.publish(&partial, name, &file)?;
            receipt.sha256 = Some(format!("{:x}", Sha256::digest(bytes)));
        }
        "read" => {
            ensure!(bytes.is_empty(), "journal reads have no input payload");
            let bytes = Input::open(&parent.join(name), LIMIT)?.read(LIMIT)?;
            receipt.value = Some(serde_json::from_slice(&bytes)?);
            receipt.sha256 = Some(format!("{:x}", Sha256::digest(&bytes)));
        }
        _ => anyhow::bail!("unknown host journal operation"),
    }
    Ok(receipt)
}
fn identity(directory: &Directory) -> Result<(String, String)> {
    let (volume, index) = directory.identity()?.stable_key();
    Ok((format!("{volume:x}"), format!("{index:x}")))
}

#[cfg(test)]
mod tests {
    use super::*;
    #[test]
    fn private_host_journal_publishes_once_and_rejects_redirected_input() {
        let root = std::env::temp_dir().join(format!("host-journal-{}", uuid::Uuid::new_v4()));
        create_private_directory(&root).unwrap();
        run("create", &root, "dispatch", &[]).unwrap();
        let journal = root.join("dispatch");
        let bytes = br#"{"synthetic":true}"#;
        let written = run("write", &journal, "identity.json", bytes).unwrap();
        let read = run("read", &journal, "identity.json", &[]).unwrap();
        assert_eq!(written.sha256, read.sha256);
        assert_eq!(read.value, Some(serde_json::json!({"synthetic":true})));
        assert!(run("write", &journal, "identity.json", bytes).is_err());
        assert!(run("write", &journal, "other.json", b"not json").is_err());
        for name in [
            "..",
            "../outside",
            "a:stream",
            "C:\\secret",
            "a..json",
            "CON",
        ] {
            assert!(run("read", &journal, name, &[]).is_err());
        }
        std::fs::hard_link(journal.join("identity.json"), journal.join("alias.json")).unwrap();
        assert!(run("read", &journal, "identity.json", &[]).is_err());
        assert!(run("read", &journal, "alias.json", &[]).is_err());
        std::fs::remove_dir_all(root).unwrap();
    }
}
