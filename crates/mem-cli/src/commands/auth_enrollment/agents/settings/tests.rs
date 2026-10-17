// SPDX-License-Identifier: AGPL-3.0-or-later
#![allow(clippy::unwrap_used)]
use super::*;
#[cfg(unix)]
use std::{
    collections::BTreeMap,
    sync::{
        Mutex,
        atomic::{AtomicBool, Ordering},
    },
};
#[cfg(unix)]
#[derive(Default)]
struct Store {
    values: Mutex<BTreeMap<(String, String), Vec<u8>>>,
    fail: AtomicBool,
}
#[cfg(unix)]
impl CredentialStore for Store {
    fn get(&self, s: &str, a: &str) -> Result<Option<Vec<u8>>> {
        Ok(self
            .values
            .lock()
            .unwrap()
            .get(&(s.into(), a.into()))
            .cloned())
    }
    fn put(&self, s: &str, a: &str, v: &[u8]) -> Result<()> {
        ensure!(
            !self.fail.load(Ordering::SeqCst),
            "synthetic Keychain failure"
        );
        self.values
            .lock()
            .unwrap()
            .insert((s.into(), a.into()), v.to_vec());
        Ok(())
    }
    fn delete(&self, s: &str, a: &str) -> Result<()> {
        self.values.lock().unwrap().remove(&(s.into(), a.into()));
        Ok(())
    }
}
fn inputs(workspace: PathBuf, caller: CompanyAgentCaller) -> Inputs {
    Inputs {
        company: "https://company.example.invalid".into(),
        agent_id: Uuid::new_v4(),
        caller,
        provider_id: caller.provider_id().into(),
        device_id: Uuid::new_v4(),
        scope: json!({"company":Uuid::new_v4(),"area":"workshop","owner":Uuid::new_v4(),"project":Uuid::new_v4(),"space":Uuid::new_v4()}),
        profile: workspace.parent().unwrap().join("native.json"),
        workspace,
        workspace_journal_sha256: "a".repeat(64),
        profile_sha256: "b".repeat(64),
        bootstrap_sha256: "c".repeat(64),
        manifest_sha256: "d".repeat(64),
        receipt_sha256: "e".repeat(64),
        command: "/usr/local/bin/memctl".into(),
        allow_loopback: false,
        allow_offline: false,
    }
}
#[test]
fn codex_preserves_exact_prefix_and_rejects_conflicting_key() {
    let original=b"# private comment\nmodel = 'custom-model'\n[mcp_servers.other]\ncommand = '/custom'\nargs = ['--private']\n";
    let merged = merge(
        CompanyAgentCaller::Codex,
        Some(original),
        "memory_project_test",
        "/installed",
        &["public".into()],
    )
    .unwrap();
    assert!(merged.starts_with(original));
    assert_eq!(
        merge(
            CompanyAgentCaller::Codex,
            Some(&merged),
            "memory_project_test",
            "/installed",
            &["public".into()]
        )
        .unwrap(),
        merged
    );
    assert!(
        merge(
            CompanyAgentCaller::Codex,
            Some(&merged),
            "memory_project_test",
            "/different",
            &[]
        )
        .is_err()
    );
    assert!(
        merge(
            CompanyAgentCaller::Codex,
            Some(b"mcp_servers = { other = { command = '/custom' } }"),
            "memory_project_test",
            "/installed",
            &[]
        )
        .is_err()
    );
}
#[test]
fn claude_preserves_all_other_values_and_generated_entry_is_secret_free() {
    let original=br#"{"model":"custom","custom":{"credential":"existing-user-secret"},"mcpServers":{"other":{"command":"custom","env":{"USER_SECRET":"keep"}}}}"#;
    let merged = merge(
        CompanyAgentCaller::Claude,
        Some(original),
        "memory_project_test",
        "/installed",
        &["public".into()],
    )
    .unwrap();
    let before: Value = serde_json::from_slice(original).unwrap();
    let after: Value = serde_json::from_slice(&merged).unwrap();
    assert_eq!(before["custom"], after["custom"]);
    assert_eq!(before["model"], after["model"]);
    assert_eq!(before["mcpServers"]["other"], after["mcpServers"]["other"]);
    assert_eq!(
        after["mcpServers"]["memory_project_test"],
        json!({"type":"stdio","command":"/installed","args":["public"]})
    );
    assert!(
        merge(
            CompanyAgentCaller::Claude,
            Some(br#"{"custom":{"x":1,"x":2}}"#),
            "memory_project_test",
            "/installed",
            &[]
        )
        .is_err()
    );
}
#[test]
fn args_and_keys_bind_exact_agent_and_explicit_offline_choice() {
    let root = std::env::temp_dir();
    let mut i = inputs(root.join("project"), CompanyAgentCaller::Codex);
    i.command = root.join("memory.exe").to_str().unwrap().to_owned();
    let a = arguments(&i).unwrap();
    assert_eq!(&a[..3], ["projects", "agents", "mcp"]);
    assert_eq!(a.len(), 9);
    let key = owned_key(&i).unwrap();
    i.agent_id = Uuid::new_v4();
    assert_ne!(key, owned_key(&i).unwrap());
    i.allow_loopback = true;
    i.allow_offline = true;
    assert_eq!(
        &arguments(&i).unwrap()[9..],
        ["--allow-loopback", "--allow-offline"]
    );
    assert!(
        !serde_json::to_string(&entry(i.caller, &i.command, &a))
            .unwrap()
            .contains("env")
    );
}
#[cfg(unix)]
struct Fixture {
    root: PathBuf,
    workspace: PathBuf,
    journal: PathBuf,
}
#[cfg(unix)]
fn private(path: &Path) {
    use std::os::unix::fs::PermissionsExt;
    std::fs::set_permissions(path, std::fs::Permissions::from_mode(0o600)).unwrap();
}
#[cfg(unix)]
impl Fixture {
    fn new() -> Self {
        use std::os::unix::fs::PermissionsExt;
        let root = std::env::temp_dir().join(format!("memory-settings-test-{}", Uuid::new_v4()));
        std::fs::create_dir(&root).unwrap();
        std::fs::set_permissions(&root, std::fs::Permissions::from_mode(0o700)).unwrap();
        let workspace = root.join("repo");
        std::fs::create_dir(&workspace).unwrap();
        std::fs::set_permissions(&workspace, std::fs::Permissions::from_mode(0o700)).unwrap();
        Self {
            journal: root.join("journal.json"),
            root,
            workspace,
        }
    }
    fn origin() -> CompanyOrigin {
        CompanyOrigin::parse("https://company.example.invalid", false).unwrap()
    }
}
#[cfg(unix)]
impl Drop for Fixture {
    fn drop(&mut self) {
        std::fs::remove_dir_all(&self.root).unwrap();
    }
}
#[cfg(unix)]
#[test]
fn create_apply_exact_replay_and_metadata_contains_no_custom_settings() {
    let f = Fixture::new();
    let store = Store::default();
    let i = inputs(f.workspace.clone(), CompanyAgentCaller::Codex);
    let p = prepare(&store, &Fixture::origin(), i.clone(), &f.journal, None).unwrap();
    let preview = p.preview(false, false);
    let digest = preview["plan_sha256"].as_str().unwrap().to_string();
    assert_eq!(preview["before_sha256"], Value::Null);
    let receipt = p.apply(&store, &Fixture::origin()).unwrap();
    assert_eq!(receipt["committed"], true);
    assert_eq!(receipt["already_configured"], true);
    assert_eq!(receipt["replayed"], false);
    let original = std::fs::read(f.workspace.join(".codex/config.toml")).unwrap();
    let p = prepare(&store, &Fixture::origin(), i, &f.journal, Some(&digest)).unwrap();
    assert_eq!(
        p.apply(&store, &Fixture::origin()).unwrap()["replayed"],
        true
    );
    assert_eq!(
        std::fs::read(f.workspace.join(".codex/config.toml")).unwrap(),
        original
    );
    let values = store.values.lock().unwrap();
    for bytes in values.values() {
        let s = String::from_utf8_lossy(bytes);
        assert!(!s.contains("local_database_url"));
        assert!(!s.contains("existing-user-secret"));
    }
}
#[cfg(unix)]
#[test]
fn changed_hash_is_preserved_and_lock_excludes_other_writer() {
    let f = Fixture::new();
    let store = Store::default();
    let i = inputs(f.workspace.clone(), CompanyAgentCaller::Claude);
    let p = prepare(&store, &Fixture::origin(), i.clone(), &f.journal, None).unwrap();
    assert!(prepare(&store, &Fixture::origin(), i, &f.journal, None).is_err());
    let alternate = f.root.join("another-journal.json");
    assert!(
        prepare(
            &store,
            &Fixture::origin(),
            p.inputs().clone(),
            &alternate,
            None
        )
        .is_err()
    );
    std::fs::write(f.workspace.join(".mcp.json"), b"{\"custom\":true}").unwrap();
    private(&f.workspace.join(".mcp.json"));
    assert!(p.apply(&store, &Fixture::origin()).is_err());
    assert_eq!(
        std::fs::read(f.workspace.join(".mcp.json")).unwrap(),
        b"{\"custom\":true}"
    );
}
#[cfg(unix)]
#[test]
fn symlinks_hardlinks_and_shared_writable_files_fail_closed() {
    use std::os::unix::fs::{PermissionsExt, symlink};
    let f = Fixture::new();
    let store = Store::default();
    let i = inputs(f.workspace.clone(), CompanyAgentCaller::Claude);
    let outside = f.root.join("other");
    std::fs::write(&outside, b"{}").unwrap();
    private(&outside);
    let target = f.workspace.join(".mcp.json");
    symlink(&outside, &target).unwrap();
    assert!(prepare(&store, &Fixture::origin(), i.clone(), &f.journal, None).is_err());
    std::fs::remove_file(&target).unwrap();
    std::fs::hard_link(&outside, &target).unwrap();
    assert!(prepare(&store, &Fixture::origin(), i.clone(), &f.journal, None).is_err());
    std::fs::remove_file(&target).unwrap();
    std::fs::write(&target, b"{}").unwrap();
    std::fs::set_permissions(&target, std::fs::Permissions::from_mode(0o666)).unwrap();
    assert!(prepare(&store, &Fixture::origin(), i, &f.journal, None).is_err());
}
#[cfg(unix)]
#[test]
fn interruption_after_replace_replays_without_custom_bytes_in_keychain() {
    let f = Fixture::new();
    std::fs::write(
        f.workspace.join(".mcp.json"),
        b"{\"private\":\"existing-user-secret\"}",
    )
    .unwrap();
    private(&f.workspace.join(".mcp.json"));
    let store = Store::default();
    let i = inputs(f.workspace.clone(), CompanyAgentCaller::Claude);
    let p = prepare(&store, &Fixture::origin(), i.clone(), &f.journal, None).unwrap();
    let digest = p.journal.plan_sha256.clone();
    store.fail.store(true, Ordering::SeqCst);
    assert!(p.apply(&store, &Fixture::origin()).is_err());
    store.fail.store(false, Ordering::SeqCst);
    let installed = std::fs::read(f.workspace.join(".mcp.json")).unwrap();
    let p = prepare(&store, &Fixture::origin(), i, &f.journal, Some(&digest)).unwrap();
    assert_eq!(
        p.apply(&store, &Fixture::origin()).unwrap()["replayed"],
        true
    );
    assert_eq!(
        std::fs::read(f.workspace.join(".mcp.json")).unwrap(),
        installed
    );
    for value in store.values.lock().unwrap().values() {
        assert!(!String::from_utf8_lossy(value).contains("existing-user-secret"));
    }
}
#[cfg(unix)]
#[test]
fn interrupted_partial_stage_recovers_exact_prefix_but_preserves_conflict() {
    let f = Fixture::new();
    let store = Store::default();
    let i = inputs(f.workspace.clone(), CompanyAgentCaller::Claude);
    let p = prepare(&store, &Fixture::origin(), i.clone(), &f.journal, None).unwrap();
    let digest = p.journal.plan_sha256.clone();
    let stage = f.root.join(format!(
        ".memory-agent-settings-{}.tmp",
        p.journal.plan.stage_id
    ));
    // Retained bytes model write_all interruption/ENOSPC, not a completed file.
    std::fs::write(&stage, &p.result[..p.result.len() / 2]).unwrap();
    private(&stage);
    drop(p);
    let p = prepare(&store, &Fixture::origin(), i, &f.journal, Some(&digest)).unwrap();
    let expected = p.result.clone();
    p.apply(&store, &Fixture::origin()).unwrap();
    assert_eq!(
        std::fs::read(f.workspace.join(".mcp.json")).unwrap(),
        expected
    );
    assert!(!stage.exists());
    let g = Fixture::new();
    let j = inputs(g.workspace.clone(), CompanyAgentCaller::Claude);
    let p = prepare(&store, &Fixture::origin(), j, &g.journal, None).unwrap();
    let stage = g.root.join(format!(
        ".memory-agent-settings-{}.tmp",
        p.journal.plan.stage_id
    ));
    std::fs::write(&stage, b"not-the-intent-prefix").unwrap();
    private(&stage);
    assert!(p.apply(&store, &Fixture::origin()).is_err());
    assert_eq!(std::fs::read(&stage).unwrap(), b"not-the-intent-prefix");
    assert!(!g.workspace.join(".mcp.json").exists());
}
#[cfg(unix)]
#[test]
fn symlinked_codex_parent_and_directory_substitution_are_denied() {
    use std::os::unix::fs::{PermissionsExt, symlink};
    let f = Fixture::new();
    let store = Store::default();
    let outside = f.root.join("outside");
    std::fs::create_dir(&outside).unwrap();
    std::fs::set_permissions(&outside, std::fs::Permissions::from_mode(0o700)).unwrap();
    symlink(&outside, f.workspace.join(".codex")).unwrap();
    let i = inputs(f.workspace.clone(), CompanyAgentCaller::Codex);
    assert!(prepare(&store, &Fixture::origin(), i, &f.journal, None).is_err());
    std::fs::remove_file(f.workspace.join(".codex")).unwrap();
    let i = inputs(f.workspace.clone(), CompanyAgentCaller::Claude);
    let p = prepare(&store, &Fixture::origin(), i, &f.journal, None).unwrap();
    std::fs::rename(&f.workspace, f.root.join("old-repo")).unwrap();
    std::fs::create_dir(&f.workspace).unwrap();
    std::fs::set_permissions(&f.workspace, std::fs::Permissions::from_mode(0o700)).unwrap();
    assert!(p.apply(&store, &Fixture::origin()).is_err());
    assert!(!f.workspace.join(".mcp.json").exists());
}
