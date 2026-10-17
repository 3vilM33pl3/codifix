// SPDX-License-Identifier: AGPL-3.0-or-later
//! Protected Windows settings merge. Handles and a physical-workspace lock
//! fence content writers and ancestor replacement. Settings-only review shares
//! target deletion for atomic replacement; same-user renames are not an OS CAS.
use super::{MAX_BYTES, bytes_hash, changed};
use anyhow::{Context, Result, ensure};
use codifix_platform::private_windows::{self, Directory, Identity, Input};
use codifix_record::CompanyAgentCaller;
use std::{
    fs::File,
    io::{Read, Seek, SeekFrom, Write},
    path::{Path, PathBuf},
};
use uuid::Uuid;

fn read(input: &Input) -> Result<Vec<u8>> {
    let mut file = &input.file;
    file.seek(SeekFrom::Start(0))?;
    let mut bytes = Vec::new();
    Read::by_ref(&mut file)
        .take(MAX_BYTES as u64 + 1)
        .read_to_end(&mut bytes)?;
    ensure!(bytes.len() <= MAX_BYTES, "settings exceed bounded size");
    input.unchanged()?;
    Ok(bytes)
}
fn optional(path: &Path) -> Result<Option<Input>> {
    match Input::open_for_reviewed_replacement(path, MAX_BYTES as u64) {
        Ok(input) => Ok(Some(input)),
        Err(error) if error.kind() == std::io::ErrorKind::NotFound => Ok(None),
        Err(_) => Err(changed()),
    }
}

pub(super) struct Settings {
    workspace: PathBuf,
    root: Directory,
    target: Option<Directory>,
    stage: Directory,
    stage_path: PathBuf,
    _parent: Directory,
    _lock: File,
    current: Option<Input>,
    caller: CompanyAgentCaller,
}
impl Settings {
    pub(super) fn open(
        workspace: &Path,
        caller: CompanyAgentCaller,
        journal: &Path,
    ) -> Result<Self> {
        let root = Directory::open(workspace)?;
        let parent_path = workspace.parent().ok_or_else(changed)?;
        let parent = Directory::open(parent_path)?;
        let stage_path = journal.parent().ok_or_else(changed)?;
        let stage = Directory::open(stage_path)?;
        ensure!(
            !stage.is_beneath(&root)?,
            "private settings journal/staging must remain physically outside workspace"
        );
        ensure!(
            stage.identity()?.stable_key().0 == root.identity()?.stable_key().0,
            "settings staging and workspace must share one local volume"
        );
        let identity = root.identity()?.stable_key();
        let lock_name = format!(
            ".memory-agent-settings-{:x}-{:x}.lock",
            identity.0, identity.1
        );
        let lock = private_windows::open_private_lock(&parent_path.join(lock_name))?;
        lock.try_lock().map_err(|_| {
            anyhow::anyhow!("another Memory settings operation holds this workspace lock")
        })?;
        let target = match caller {
            CompanyAgentCaller::Claude => Some(Directory::open(workspace)?),
            CompanyAgentCaller::Codex => {
                match std::fs::symlink_metadata(workspace.join(".codex")) {
                    Ok(_) => Some(Directory::open(&workspace.join(".codex"))?),
                    Err(error) if error.kind() == std::io::ErrorKind::NotFound => None,
                    Err(_) => return Err(changed()),
                }
            }
        };
        let path = workspace.join(super::relative(caller));
        let current = if target.is_some() {
            optional(&path)?
        } else {
            None
        };
        Ok(Self {
            workspace: workspace.into(),
            root,
            target,
            stage,
            stage_path: stage_path.into(),
            _parent: parent,
            _lock: lock,
            current,
            caller,
        })
    }
    pub(super) fn identity(&self) -> Option<(u64, u64)> {
        self.current
            .as_ref()
            .map(|input| input.identity().stable_key())
    }
    pub(super) fn read(&self) -> Result<Option<Vec<u8>>> {
        self.current.as_ref().map(read).transpose()
    }
    fn leaf(&self) -> &'static str {
        match self.caller {
            CompanyAgentCaller::Codex => "config.toml",
            CompanyAgentCaller::Claude => ".mcp.json",
        }
    }
    fn stage(&self, id: Uuid, result: &[u8]) -> Result<(String, Identity)> {
        let name = format!(".memory-agent-settings-{id}.tmp");
        let path = self.stage_path.join(&name);
        match self.stage.create(&name) {
            Ok(mut file) => {
                file.write_all(result)?;
                file.sync_all()?;
                self.stage.check_entry(&name, &file)?;
            }
            Err(error) if error.kind() == std::io::ErrorKind::AlreadyExists => {
                let mut file = self.stage.resume_private_file(&name)?;
                self.stage.check_entry(&name, &file)?;
                let mut saved = Vec::new();
                Read::by_ref(&mut file)
                    .take(MAX_BYTES as u64 + 1)
                    .read_to_end(&mut saved)?;
                ensure!(
                    result.starts_with(&saved),
                    "interrupted settings staging differs; unknown bytes preserved"
                );
                self.stage.check_entry(&name, &file)?;
                file.write_all(&result[saved.len()..])?;
                file.sync_all()?;
                self.stage.check_entry(&name, &file)?;
            }
            Err(error) => return Err(error.into()),
        }
        let staged = Input::open(&path, MAX_BYTES as u64)?;
        ensure!(read(&staged)? == result, "settings staging changed");
        Ok((name, staged.identity()))
    }
    pub(super) fn replace(
        &mut self,
        before: Option<&str>,
        after: &str,
        result: &[u8],
        id: Uuid,
    ) -> Result<()> {
        ensure!(
            !id.is_nil() && result.len() <= MAX_BYTES && bytes_hash(result) == after,
            "reviewed settings result changed"
        );
        let fresh_root = Directory::open(&self.workspace)?;
        ensure!(
            fresh_root.identity()?.stable_key() == self.root.identity()?.stable_key(),
            "workspace identity changed"
        );
        let original = self.read()?;
        if original
            .as_ref()
            .is_some_and(|bytes| bytes_hash(bytes) == after)
        {
            return Ok(());
        }
        ensure!(
            original.as_deref().map(bytes_hash).as_deref() == before,
            "settings changed since preview; preserved"
        );
        if self.target.is_none() {
            private_windows::create_private_directory(&self.workspace.join(".codex"))?;
            self.target = Some(Directory::open(&self.workspace.join(".codex"))?);
        }
        let target = self.target.as_ref().ok_or_else(changed)?;
        let target_path = match self.caller {
            CompanyAgentCaller::Codex => self.workspace.join(".codex"),
            CompanyAgentCaller::Claude => self.workspace.clone(),
        };
        let fresh_target = Directory::open(&target_path)?;
        ensure!(
            fresh_target.identity()?.stable_key() == target.identity()?.stable_key(),
            "settings directory identity changed"
        );
        let (name, identity) = self
            .stage(id, result)
            .context("native settings prepare staging")?;
        private_windows::publish_reviewed_settings(
            &self.stage,
            &name,
            identity,
            target,
            self.leaf(),
            self.current.as_ref().zip(original.as_deref()),
            result,
        )
        .context("native settings publish reviewed bytes")?;
        let installed = Input::open_for_reviewed_replacement(
            &self.workspace.join(super::relative(self.caller)),
            MAX_BYTES as u64,
        )
        .context("native settings reopen published bytes")?;
        ensure!(
            read(&installed)? == result,
            "published settings changed; inspect before retrying"
        );
        self.current = Some(installed);
        Ok(())
    }
}

#[cfg(test)]
#[allow(clippy::unwrap_used)]
mod tests {
    use super::super::{Inputs, prepare};
    use super::*;
    use crate::commands::auth_enrollment::{CompanyOrigin, credentials::CredentialStore};
    use std::{collections::BTreeMap, sync::Mutex};

    struct Fixture {
        root: PathBuf,
        workspace: PathBuf,
        journal: PathBuf,
    }
    impl Fixture {
        fn new() -> Self {
            let root =
                std::env::temp_dir().join(format!("memory-settings-windows-{}", Uuid::new_v4()));
            private_windows::create_private_directory(&root).unwrap();
            let workspace = root.join("Unicode 项目 space");
            private_windows::create_private_directory(&workspace).unwrap();
            Self {
                journal: root.join("journal.json"),
                root,
                workspace,
            }
        }
    }
    impl Drop for Fixture {
        fn drop(&mut self) {
            std::fs::remove_dir_all(&self.root).unwrap();
        }
    }
    #[derive(Default)]
    struct Store(Mutex<BTreeMap<(String, String), Vec<u8>>>);
    impl CredentialStore for Store {
        fn get(&self, service: &str, account: &str) -> Result<Option<Vec<u8>>> {
            Ok(self
                .0
                .lock()
                .unwrap()
                .get(&(service.into(), account.into()))
                .cloned())
        }
        fn put(&self, service: &str, account: &str, value: &[u8]) -> Result<()> {
            self.0
                .lock()
                .unwrap()
                .insert((service.into(), account.into()), value.to_vec());
            Ok(())
        }
        fn delete(&self, service: &str, account: &str) -> Result<()> {
            self.0
                .lock()
                .unwrap()
                .remove(&(service.into(), account.into()));
            Ok(())
        }
    }
    fn inputs(f: &Fixture, caller: CompanyAgentCaller) -> Inputs {
        Inputs {
            company: "https://company.example.invalid".into(),
            agent_id: Uuid::new_v4(),
            caller,
            provider_id: caller.provider_id().into(),
            device_id: Uuid::new_v4(),
            scope: serde_json::json!({"project":Uuid::new_v4()}),
            workspace: f.workspace.clone(),
            profile: f.root.join("profile.json"),
            workspace_journal_sha256: "a".repeat(64),
            profile_sha256: "b".repeat(64),
            bootstrap_sha256: "c".repeat(64),
            manifest_sha256: "d".repeat(64),
            receipt_sha256: "e".repeat(64),
            command: f.root.join("codifix.exe").to_string_lossy().into_owned(),
            allow_loopback: false,
            allow_offline: false,
        }
    }
    #[test]
    fn native_repeated_replace_retains_review_mode_and_rejects_path_substitution() {
        let f = Fixture::new();
        let mut settings =
            Settings::open(&f.workspace, CompanyAgentCaller::Claude, &f.journal).unwrap();
        let first = b"{\"first\":true}";
        let second = b"{\"second\":true}";
        settings
            .replace(None, &bytes_hash(first), first, Uuid::new_v4())
            .unwrap();
        settings
            .replace(
                Some(&bytes_hash(first)),
                &bytes_hash(second),
                second,
                Uuid::new_v4(),
            )
            .unwrap();
        assert_eq!(settings.read().unwrap().unwrap(), second);
        let original = f.workspace.join(".mcp.json");
        let moved = f.workspace.join("moved.json");
        std::fs::rename(&original, &moved).unwrap();
        std::fs::write(&original, b"{\"unreviewed\":true}").unwrap();
        assert!(
            settings
                .replace(
                    Some(&bytes_hash(second)),
                    &bytes_hash(first),
                    first,
                    Uuid::new_v4()
                )
                .is_err()
        );
        assert_eq!(std::fs::read(&original).unwrap(), b"{\"unreviewed\":true}");
    }
    #[test]
    fn native_codex_and_claude_merge_and_replay_preserve_custom_settings() {
        for caller in [CompanyAgentCaller::Codex, CompanyAgentCaller::Claude] {
            let f = Fixture::new();
            if caller == CompanyAgentCaller::Codex {
                private_windows::create_private_directory(&f.workspace.join(".codex")).unwrap();
            }
            let path = f.workspace.join(super::super::relative(caller));
            let original: &[u8] = match caller {
                CompanyAgentCaller::Codex => b"# preserve-secret-comment\nmodel = 'custom'\n",
                CompanyAgentCaller::Claude => br#"{"custom":"preserve-secret-comment"}"#,
            };
            std::fs::write(&path, original).unwrap();
            let store = Store::default();
            let i = inputs(&f, caller);
            let origin = CompanyOrigin::parse(&i.company, false).unwrap();
            let prepared = prepare(&store, &origin, i.clone(), &f.journal, None).unwrap();
            let digest = prepared.preview(false, false)["plan_sha256"]
                .as_str()
                .unwrap()
                .to_owned();
            prepared.apply(&store, &origin).unwrap();
            let installed = std::fs::read(&path).unwrap();
            assert!(String::from_utf8_lossy(&installed).contains("preserve-secret-comment"));
            assert!(
                prepare(&store, &origin, i, &f.journal, Some(&digest))
                    .unwrap()
                    .apply(&store, &origin)
                    .unwrap()["replayed"]
                    .as_bool()
                    .unwrap()
            );
            assert_eq!(std::fs::read(&path).unwrap(), installed);
            for value in store.0.lock().unwrap().values() {
                assert!(!String::from_utf8_lossy(value).contains("preserve-secret-comment"));
            }
        }
    }
    #[test]
    fn native_normal_editor_is_fenced_and_same_bytes_new_identity_require_preview() {
        let f = Fixture::new();
        let path = f.workspace.join(".mcp.json");
        std::fs::write(&path, b"{}").unwrap();
        let store = Store::default();
        let i = inputs(&f, CompanyAgentCaller::Claude);
        let origin = CompanyOrigin::parse(&i.company, false).unwrap();
        let prepared = prepare(&store, &origin, i.clone(), &f.journal, None).unwrap();
        let digest = prepared.preview(false, false)["plan_sha256"]
            .as_str()
            .unwrap()
            .to_owned();
        assert!(std::fs::write(&path, b"changed").is_err());
        assert!(Settings::open(&f.workspace, CompanyAgentCaller::Codex, &f.journal).is_err());
        let replacement = f.root.join("replacement.json");
        std::fs::write(&replacement, b"{}").unwrap();
        drop(prepared);
        let source_identity = {
            let source = Input::open(&replacement, 2).unwrap();
            source.identity()
        };
        private_windows::publish_reviewed_settings(
            &Directory::open(&f.root).unwrap(),
            "replacement.json",
            source_identity,
            &Directory::open(&f.workspace).unwrap(),
            ".mcp.json",
            Some((
                &Input::open_for_reviewed_replacement(&path, 2).unwrap(),
                b"{}",
            )),
            b"{}",
        )
        .unwrap();
        assert!(prepare(&store, &origin, i, &f.journal, Some(&digest)).is_err());
        assert_eq!(std::fs::read(&path).unwrap(), b"{}");
    }
    #[test]
    fn native_prefix_recovery_and_unknown_staging_are_preserved() {
        let f = Fixture::new();
        let id = Uuid::new_v4();
        let name = format!(".memory-agent-settings-{id}.tmp");
        let stage = f.root.join(&name);
        std::fs::write(&stage, b"approved").unwrap();
        let mut io = Settings::open(&f.workspace, CompanyAgentCaller::Claude, &f.journal).unwrap();
        let result = b"approved suffix";
        io.replace(None, &bytes_hash(result), result, id).unwrap();
        assert_eq!(
            std::fs::read(f.workspace.join(".mcp.json")).unwrap(),
            result
        );
        drop(io);
        let f = Fixture::new();
        let stage = f.root.join(&name);
        std::fs::write(&stage, b"unknown secret").unwrap();
        let mut io = Settings::open(&f.workspace, CompanyAgentCaller::Claude, &f.journal).unwrap();
        assert!(io.replace(None, &bytes_hash(result), result, id).is_err());
        assert_eq!(std::fs::read(&stage).unwrap(), b"unknown secret");
        assert!(!f.workspace.join(".mcp.json").exists());
    }
    #[test]
    fn native_hardlinks_and_inside_workspace_staging_are_rejected() {
        let f = Fixture::new();
        let path = f.workspace.join(".mcp.json");
        std::fs::write(&path, b"{}").unwrap();
        std::fs::hard_link(&path, f.root.join("alias")).unwrap();
        assert!(Settings::open(&f.workspace, CompanyAgentCaller::Claude, &f.journal).is_err());
        assert!(
            Settings::open(
                &f.workspace,
                CompanyAgentCaller::Claude,
                &f.workspace.join("journal.json")
            )
            .is_err()
        );
        let alias =
            PathBuf::from(f.workspace.to_string_lossy().to_uppercase()).join("journal.json");
        assert!(Settings::open(&f.workspace, CompanyAgentCaller::Claude, &alias).is_err());
    }
    #[test]
    fn native_new_editor_file_and_shared_acl_are_never_overwritten() {
        let f = Fixture::new();
        let path = f.workspace.join(".mcp.json");
        let mut io = Settings::open(&f.workspace, CompanyAgentCaller::Claude, &f.journal).unwrap();
        std::fs::write(&path, b"custom settings appeared").unwrap();
        let result = b"reviewed result";
        assert!(
            io.replace(None, &bytes_hash(result), result, Uuid::new_v4())
                .is_err()
        );
        assert_eq!(std::fs::read(&path).unwrap(), b"custom settings appeared");
        drop(io);
        let output = std::process::Command::new("icacls.exe")
            .arg(&path)
            .args(["/grant", "*S-1-1-0:(R)"])
            .output()
            .unwrap();
        assert!(output.status.success(), "synthetic test ACL setup failed");
        assert!(Settings::open(&f.workspace, CompanyAgentCaller::Claude, &f.journal).is_err());
        assert_eq!(std::fs::read(&path).unwrap(), b"custom settings appeared");
    }
}
