// SPDX-License-Identifier: AGPL-3.0-or-later
use super::*;
use std::{
    cell::{Cell, RefCell},
    collections::VecDeque,
    fs,
    io::{Cursor, Write},
    path::Path,
};
use zip::{ZipWriter, write::SimpleFileOptions};

struct Fixture(PathBuf);
impl Fixture {
    fn new() -> Self {
        let path = std::env::temp_dir().join(format!("memory-workspace-test-{}", Uuid::new_v4()));
        fs::create_dir(&path).unwrap();
        Self(path)
    }
    fn prepare(&self, manifest: &OnboardingProject) -> PrepareArgs {
        PrepareArgs {
            company: CompanyArgs {
                company: "https://company.example.test".into(),
                allow_loopback: false,
            },
            project: manifest.project_id,
            area: manifest.entry.area.as_str().into(),
            owner_instance: manifest.entry.owner_instance_id,
            revision: manifest.revision,
            workspace: self.0.join("repo"),
            journal: self.0.join("plan.json"),
        }
    }
    fn apply(&self, journal: &Journal) -> ApplyArgs {
        ApplyArgs {
            company: CompanyArgs {
                company: journal.plan.company.clone(),
                allow_loopback: false,
            },
            journal: self.0.join("plan.json"),
            plan_sha256: journal.plan_sha256.clone(),
            confirm: true,
        }
    }
}
impl Drop for Fixture {
    fn drop(&mut self) {
        let _ = fs::remove_dir_all(&self.0);
    }
}
fn identity() -> (CompanyOrigin, Enrollment) {
    let origin = CompanyOrigin::parse("https://company.example.test", false).unwrap();
    let record = Enrollment {
        company: origin.0.clone(),
        device_id: Uuid::new_v4(),
        request_id: Uuid::new_v4(),
        name: "Fake Mac".into(),
        platform: "macos".into(),
        credential: format!("mlt_{}", "a".repeat(64)),
        poll_secret: "b".repeat(64),
        approved: true,
        pending_logout: false,
    };
    (origin, record)
}
fn project() -> OnboardingDeviceReviewProject {
    OnboardingDeviceReviewProject {
        manifest: OnboardingProject {
            project_id: Uuid::new_v4(),
            project: "not-an-authority-key".into(),
            revision: 1,
            reviewed_by: Uuid::new_v4(),
            reviewed_at: chrono::Utc::now(),
            entry: mem_record::OnboardingProjectEntry {
                area: mem_record::OnboardingArea::Workshop,
                owner_instance_id: Uuid::new_v4(),
                repository_url: "https://git.example.test/company/project.git".into(),
                default_branch: "main".into(),
                offline_allowed: true,
                tools: vec![],
                destinations: vec![],
            },
        },
        allowed_roles: vec![mem_record::AuthRole::Reader],
        offline_available: true,
    }
}
struct FakeCatalog {
    project: OnboardingDeviceReviewProject,
    queued: RefCell<VecDeque<Vec<OnboardingDeviceReviewProject>>>,
    calls: Cell<usize>,
}
impl FakeCatalog {
    fn new(project: OnboardingDeviceReviewProject) -> Self {
        Self {
            project,
            queued: RefCell::new(VecDeque::new()),
            calls: Cell::new(0),
        }
    }
}
impl Catalog for FakeCatalog {
    async fn projects(&self, _: &Enrollment) -> Result<transport::AvailableProjects> {
        self.calls.set(self.calls.get() + 1);
        Ok(transport::AvailableProjects {
            schema_version: 1,
            projects: self
                .queued
                .borrow_mut()
                .pop_front()
                .unwrap_or_else(|| vec![self.project.clone()]),
        })
    }
}
#[derive(Default)]
struct FakeGit {
    clones: Cell<usize>,
    finishes: Cell<usize>,
    existing: Cell<usize>,
    lose_clone_reply: Cell<bool>,
    reject_existing: Cell<bool>,
}
impl repository::Git for FakeGit {
    async fn clone_to(&self, _: &OnboardingProject, path: &Path) -> Result<()> {
        self.clones.set(self.clones.get() + 1);
        fs::create_dir(path.join(".git"))?;
        fs::write(path.join(".git/HEAD"), "ref: refs/heads/main\n")?;
        if self.lose_clone_reply.replace(false) {
            bail!("synthetic lost clone confirmation");
        }
        Ok(())
    }
    async fn finish(&self, _: &OnboardingProject, path: &Path) -> Result<()> {
        self.finishes.set(self.finishes.get() + 1);
        fs::write(path.join("README.md"), "synthetic repository\n")?;
        Ok(())
    }
    async fn existing(&self, _: &OnboardingProject, _: &Path) -> Result<()> {
        self.existing.set(self.existing.get() + 1);
        if self.reject_existing.get() {
            bail!("synthetic wrong origin");
        }
        Ok(())
    }
}
#[derive(Default)]
struct FakeDownloads {
    bytes: Vec<u8>,
    calls: Cell<usize>,
}
impl artifacts::Downloads for FakeDownloads {
    async fn fetch(&self, _: &str) -> Result<Vec<u8>> {
        self.calls.set(self.calls.get() + 1);
        Ok(self.bytes.clone())
    }
}
fn bundle(version: &str) -> Vec<u8> {
    let mut writer = ZipWriter::new(Cursor::new(Vec::new()));
    for name in mem_skills::MEMORY_SKILL_NAMES {
        writer
            .start_file(
                format!("fixture/.agents/skills/{name}/SKILL.md"),
                SimpleFileOptions::default(),
            )
            .unwrap();
        write!(
            writer,
            "---\nname: {name}\nversion: {version}\n---\nReviewed fixture only.\n"
        )
        .unwrap();
        writer
            .start_file(
                format!("fixture/.agents/skills/{name}/scripts/helper.sh"),
                SimpleFileOptions::default(),
            )
            .unwrap();
        writer.write_all(b"#!/bin/sh\nexit 99\n").unwrap();
    }
    writer.finish().unwrap().into_inner()
}
fn pin(bytes: &[u8], version: &str) -> mem_record::OnboardingPinnedTool {
    mem_record::OnboardingPinnedTool {
        name: "memory-skills".into(),
        version: version.into(),
        url: "https://tools.example.test/reviewed.zip".into(),
        sha256: filesystem::bytes_digest(bytes),
    }
}

#[tokio::test]
async fn prepare_is_exact_preview_only_and_journal_has_no_credentials() {
    let fixture = Fixture::new();
    let project = project();
    let args = fixture.prepare(&project.manifest);
    let catalog = FakeCatalog::new(project);
    let (origin, record) = identity();
    let journal = prepare_with(&args, &origin, &record, &catalog)
        .await
        .unwrap();
    assert!(!args.workspace.exists());
    assert_eq!(catalog.calls.get(), 1);
    let raw = fs::read_to_string(&args.journal).unwrap();
    assert!(!raw.contains(&record.credential));
    assert!(!raw.contains(&record.poll_secret));
    assert_eq!(
        journal.plan.manifest.entry.owner_instance_id,
        args.owner_instance
    );
    assert_eq!(
        journal.plan.templates[0].content,
        include_str!("../../../../../../.codex/config.toml")
    );
    assert_eq!(
        journal.plan.templates[1].content,
        include_str!("../../../../../../.codex/agents/astra_adviser.toml")
    );
}
#[tokio::test]
async fn denied_or_wrong_stable_identity_never_prepares_repository() {
    let fixture = Fixture::new();
    let project = project();
    let (origin, record) = identity();
    let mut args = fixture.prepare(&project.manifest);
    args.owner_instance = Uuid::new_v4();
    let catalog = FakeCatalog::new(project);
    assert!(
        prepare_with(&args, &origin, &record, &catalog)
            .await
            .is_err()
    );
    assert!(!args.journal.exists());
    assert!(!args.workspace.exists());
    args.owner_instance = catalog.project.manifest.entry.owner_instance_id;
    catalog.queued.borrow_mut().push_back(vec![]);
    assert!(
        prepare_with(&args, &origin, &record, &catalog)
            .await
            .is_err()
    );
    assert!(!args.journal.exists());
}
#[tokio::test]
async fn changed_prepare_inputs_conflict_without_replacing_journal() {
    let fixture = Fixture::new();
    let project = project();
    let (origin, record) = identity();
    let catalog = FakeCatalog::new(project);
    let mut args = fixture.prepare(&catalog.project.manifest);
    let first = prepare_with(&args, &origin, &record, &catalog)
        .await
        .unwrap();
    let original = fs::read(&args.journal).unwrap();
    args.workspace = fixture.0.join("other");
    assert!(
        prepare_with(&args, &origin, &record, &catalog)
            .await
            .is_err()
    );
    assert_eq!(fs::read(&args.journal).unwrap(), original);
    args.workspace = fixture.0.join("repo");
    assert_eq!(
        prepare_with(&args, &origin, &record, &catalog)
            .await
            .unwrap()
            .request_id,
        first.request_id
    );
}
#[tokio::test]
async fn apply_requires_exact_confirmation_before_any_effect() {
    let fixture = Fixture::new();
    let project = project();
    let (origin, record) = identity();
    let catalog = FakeCatalog::new(project);
    let args = fixture.prepare(&catalog.project.manifest);
    let journal = prepare_with(&args, &origin, &record, &catalog)
        .await
        .unwrap();
    let mut apply = fixture.apply(&journal);
    let git = FakeGit::default();
    let downloads = FakeDownloads::default();
    apply.confirm = false;
    assert!(
        apply_with(&apply, &origin, &record, &catalog, &git, &downloads)
            .await
            .is_err()
    );
    apply.confirm = true;
    apply.plan_sha256 = "0".repeat(64);
    assert!(
        apply_with(&apply, &origin, &record, &catalog, &git, &downloads)
            .await
            .is_err()
    );
    assert_eq!(catalog.calls.get(), 1);
    assert_eq!(git.clones.get(), 0);
    assert!(!args.workspace.exists());
}
#[tokio::test]
async fn altered_manifest_or_revoked_project_stops_before_clone() {
    let fixture = Fixture::new();
    let project = project();
    let (origin, record) = identity();
    let catalog = FakeCatalog::new(project);
    let args = fixture.prepare(&catalog.project.manifest);
    let journal = prepare_with(&args, &origin, &record, &catalog)
        .await
        .unwrap();
    let mut altered = catalog.project.clone();
    altered.manifest.revision += 1;
    catalog.queued.borrow_mut().push_back(vec![altered]);
    let git = FakeGit::default();
    assert!(
        apply_with(
            &fixture.apply(&journal),
            &origin,
            &record,
            &catalog,
            &git,
            &FakeDownloads::default()
        )
        .await
        .is_err()
    );
    catalog.queued.borrow_mut().push_back(vec![]);
    assert!(
        apply_with(
            &fixture.apply(&journal),
            &origin,
            &record,
            &catalog,
            &git,
            &FakeDownloads::default()
        )
        .await
        .is_err()
    );
    assert_eq!(git.clones.get(), 0);
    assert!(!args.workspace.exists());
}
#[tokio::test]
async fn lost_clone_confirmation_resumes_same_request_without_second_clone() {
    let fixture = Fixture::new();
    let project = project();
    let (origin, record) = identity();
    let catalog = FakeCatalog::new(project);
    let args = fixture.prepare(&catalog.project.manifest);
    let journal = prepare_with(&args, &origin, &record, &catalog)
        .await
        .unwrap();
    let git = FakeGit::default();
    git.lose_clone_reply.set(true);
    let downloads = FakeDownloads::default();
    assert!(
        apply_with(
            &fixture.apply(&journal),
            &origin,
            &record,
            &catalog,
            &git,
            &downloads
        )
        .await
        .is_err()
    );
    let intent = filesystem::read(&args.journal).unwrap();
    assert!(intent.repository == RepositoryStage::CloneIntent);
    assert!(!args.workspace.exists());
    let done = apply_with(
        &fixture.apply(&journal),
        &origin,
        &record,
        &catalog,
        &git,
        &downloads,
    )
    .await
    .unwrap();
    assert_eq!(done.request_id, journal.request_id);
    assert_eq!(git.clones.get(), 1);
    assert_eq!(git.finishes.get(), 1);
    assert!(done.templates_done);
    assert!(done.tools_done);
    assert!(
        args.workspace
            .join(".codex/agents/astra_adviser.toml")
            .is_file()
    );
    apply_with(
        &fixture.apply(&journal),
        &origin,
        &record,
        &catalog,
        &git,
        &downloads,
    )
    .await
    .unwrap();
    assert_eq!(git.clones.get(), 1);
    assert_eq!(git.finishes.get(), 1);
}
#[tokio::test]
async fn existing_dirty_repo_and_custom_codex_claude_settings_are_preserved() {
    let fixture = Fixture::new();
    let project = project();
    let (origin, record) = identity();
    let catalog = FakeCatalog::new(project);
    let args = fixture.prepare(&catalog.project.manifest);
    fs::create_dir(&args.workspace).unwrap();
    fs::create_dir(args.workspace.join(".git")).unwrap();
    fs::create_dir_all(args.workspace.join(".codex/agents")).unwrap();
    fs::write(args.workspace.join("dirty.txt"), "uncommitted changes").unwrap();
    fs::write(
        args.workspace.join(".codex/config.toml"),
        "custom model and MCP",
    )
    .unwrap();
    fs::write(
        args.workspace.join(".codex/agents/astra_adviser.toml"),
        "custom adviser",
    )
    .unwrap();
    fs::write(
        args.workspace.join("CLAUDE.md"),
        "custom Anthropic settings",
    )
    .unwrap();
    let journal = prepare_with(&args, &origin, &record, &catalog)
        .await
        .unwrap();
    let git = FakeGit::default();
    let done = apply_with(
        &fixture.apply(&journal),
        &origin,
        &record,
        &catalog,
        &git,
        &FakeDownloads::default(),
    )
    .await
    .unwrap();
    assert!(done.repository == RepositoryStage::ExistingPreserved);
    assert_eq!(git.clones.get(), 0);
    assert_eq!(git.finishes.get(), 0);
    assert_eq!(git.existing.get(), 1);
    assert_eq!(
        fs::read_to_string(args.workspace.join("dirty.txt")).unwrap(),
        "uncommitted changes"
    );
    assert_eq!(
        fs::read_to_string(args.workspace.join(".codex/config.toml")).unwrap(),
        "custom model and MCP"
    );
    assert_eq!(
        fs::read_to_string(args.workspace.join(".codex/agents/astra_adviser.toml")).unwrap(),
        "custom adviser"
    );
    assert_eq!(
        fs::read_to_string(args.workspace.join("CLAUDE.md")).unwrap(),
        "custom Anthropic settings"
    );
    assert_eq!(done.preserved_paths.len(), 2);
}
#[tokio::test]
async fn wrong_existing_origin_and_target_appearing_after_preview_are_not_modified() {
    let fixture = Fixture::new();
    let project = project();
    let (origin, record) = identity();
    let catalog = FakeCatalog::new(project);
    let args = fixture.prepare(&catalog.project.manifest);
    fs::create_dir(&args.workspace).unwrap();
    fs::create_dir(args.workspace.join(".git")).unwrap();
    let journal = prepare_with(&args, &origin, &record, &catalog)
        .await
        .unwrap();
    let git = FakeGit::default();
    git.reject_existing.set(true);
    assert!(
        apply_with(
            &fixture.apply(&journal),
            &origin,
            &record,
            &catalog,
            &git,
            &FakeDownloads::default()
        )
        .await
        .is_err()
    );
    assert!(!args.workspace.join(".codex").exists());
    assert_eq!(git.finishes.get(), 0);
    let second = Fixture::new();
    let args = second.prepare(&catalog.project.manifest);
    let journal = prepare_with(&args, &origin, &record, &catalog)
        .await
        .unwrap();
    fs::create_dir(&args.workspace).unwrap();
    fs::write(args.workspace.join("sentinel"), "preserve").unwrap();
    assert!(
        apply_with(
            &second.apply(&journal),
            &origin,
            &record,
            &catalog,
            &git,
            &FakeDownloads::default()
        )
        .await
        .is_err()
    );
    assert_eq!(
        fs::read_to_string(args.workspace.join("sentinel")).unwrap(),
        "preserve"
    );
    assert_eq!(git.clones.get(), 0);
}
#[tokio::test]
async fn trust_remote_only_and_unsafe_urls_are_rejected() {
    let fixture = Fixture::new();
    let (origin, record) = identity();
    let mut reviewed = project();
    reviewed.manifest.entry.area = mem_record::OnboardingArea::Trust;
    reviewed.offline_available = false;
    reviewed.manifest.entry.offline_allowed = false;
    let args = fixture.prepare(&reviewed.manifest);
    let catalog = FakeCatalog::new(reviewed);
    assert!(
        prepare_with(&args, &origin, &record, &catalog)
            .await
            .is_err()
    );
    for url in [
        "https://user:secret@repo.test/x",
        "https://repo.test/x?bearer=secret",
        "file:///tmp/repo",
        "http://repo.test/x",
    ] {
        assert!(filesystem::https_url(url).is_err());
    }
    for branch in [
        "-option", "../other", "bad..ref", "bad ref", "x@{1}", "x.lock",
    ] {
        assert!(repository::branch(branch).is_err());
    }
}
#[tokio::test]
async fn pinned_skills_verify_versions_and_preserve_custom_skill_directories() {
    let fixture = Fixture::new();
    let (origin, record) = identity();
    let bytes = bundle("2.1.0");
    let mut reviewed = project();
    reviewed.manifest.entry.tools.push(pin(&bytes, "2.1.0"));
    let catalog = FakeCatalog::new(reviewed);
    let args = fixture.prepare(&catalog.project.manifest);
    fs::create_dir(&args.workspace).unwrap();
    fs::create_dir(args.workspace.join(".git")).unwrap();
    fs::create_dir_all(args.workspace.join(".agents/skills/memory-layer")).unwrap();
    fs::write(
        args.workspace.join(".agents/skills/memory-layer/SKILL.md"),
        "custom skill",
    )
    .unwrap();
    let journal = prepare_with(&args, &origin, &record, &catalog)
        .await
        .unwrap();
    let downloads = FakeDownloads {
        bytes,
        calls: Cell::new(0),
    };
    let git = FakeGit::default();
    let done = apply_with(
        &fixture.apply(&journal),
        &origin,
        &record,
        &catalog,
        &git,
        &downloads,
    )
    .await
    .unwrap();
    assert!(done.tools_done);
    assert_eq!(downloads.calls.get(), 1);
    assert_eq!(
        fs::read_to_string(args.workspace.join(".agents/skills/memory-layer/SKILL.md")).unwrap(),
        "custom skill"
    );
    assert!(
        args.workspace
            .join(".agents/skills/memory-remember/SKILL.md")
            .is_file()
    );
    #[cfg(unix)]
    {
        use std::os::unix::fs::PermissionsExt;
        assert_eq!(
            fs::metadata(
                args.workspace
                    .join(".agents/skills/memory-remember/scripts/helper.sh")
            )
            .unwrap()
            .permissions()
            .mode()
                & 0o111,
            0
        );
    }
    apply_with(
        &fixture.apply(&journal),
        &origin,
        &record,
        &catalog,
        &git,
        &downloads,
    )
    .await
    .unwrap();
    assert_eq!(downloads.calls.get(), 1);
}
#[tokio::test]
async fn hash_failure_unknown_plugin_and_wrong_version_are_explicitly_blocked() {
    for mode in 0..3 {
        let fixture = Fixture::new();
        let (origin, record) = identity();
        let bytes = bundle("2.1.0");
        let mut reviewed = project();
        let mut tool = pin(&bytes, "2.1.0");
        if mode == 0 {
            tool.sha256 = "0".repeat(64);
        }
        if mode == 1 {
            tool.name = "unknown-executable-plugin".into();
        }
        if mode == 2 {
            tool.version = "9.9.9".into();
        }
        reviewed.manifest.entry.tools.push(tool);
        let catalog = FakeCatalog::new(reviewed);
        let args = fixture.prepare(&catalog.project.manifest);
        let journal = prepare_with(&args, &origin, &record, &catalog)
            .await
            .unwrap();
        let downloads = FakeDownloads {
            bytes,
            calls: Cell::new(0),
        };
        assert!(
            apply_with(
                &fixture.apply(&journal),
                &origin,
                &record,
                &catalog,
                &FakeGit::default(),
                &downloads
            )
            .await
            .is_err()
        );
        assert!(!args.workspace.join(".agents").exists());
        assert!(!filesystem::read(&args.journal).unwrap().tools_done);
        assert_eq!(downloads.calls.get(), if mode == 1 { 0 } else { 1 });
    }
}
#[test]
fn unsafe_archive_paths_and_expansion_are_rejected() {
    let mut writer = ZipWriter::new(Cursor::new(Vec::new()));
    writer
        .start_file("../escape", SimpleFileOptions::default())
        .unwrap();
    writer.write_all(b"unsafe").unwrap();
    assert!(artifacts::bundle(&writer.finish().unwrap().into_inner()).is_err());
    assert!(artifacts::bundle(&vec![0; 16 * 1024 * 1024 + 1]).is_err());
}
#[test]
fn concurrent_apply_locks_fail_closed() {
    let fixture = Fixture::new();
    let path = fixture.0.join("plan.json");
    let first = filesystem::Lock::take(&path).unwrap();
    assert!(filesystem::Lock::take(&path).is_err());
    drop(first);
    assert!(filesystem::Lock::take(&path).is_ok());
}

#[tokio::test]
async fn modified_journal_inputs_are_rejected_without_rewriting_user_files() {
    let fixture = Fixture::new();
    let catalog = FakeCatalog::new(project());
    let (origin, record) = identity();
    let args = fixture.prepare(&catalog.project.manifest);
    let journal = prepare_with(&args, &origin, &record, &catalog)
        .await
        .unwrap();
    let mut raw: serde_json::Value =
        serde_json::from_slice(&fs::read(&args.journal).unwrap()).unwrap();
    raw["plan"]["manifest"]["repository_url"] = "https://attacker.example.test/other.git".into();
    fs::write(&args.journal, serde_json::to_vec(&raw).unwrap()).unwrap();
    let git = FakeGit::default();
    assert!(
        apply_with(
            &fixture.apply(&journal),
            &origin,
            &record,
            &catalog,
            &git,
            &FakeDownloads::default()
        )
        .await
        .is_err()
    );
    assert_eq!(git.clones.get(), 0);
    assert!(!args.workspace.exists());
}

#[tokio::test]
async fn current_scope_revoked_after_clone_stops_templates_and_downloads() {
    let fixture = Fixture::new();
    let mut reviewed = project();
    let bytes = bundle("2.1.0");
    reviewed.manifest.entry.tools.push(pin(&bytes, "2.1.0"));
    let catalog = FakeCatalog::new(reviewed);
    let (origin, record) = identity();
    let args = fixture.prepare(&catalog.project.manifest);
    let journal = prepare_with(&args, &origin, &record, &catalog)
        .await
        .unwrap();
    catalog
        .queued
        .borrow_mut()
        .extend([vec![catalog.project.clone()], vec![]]);
    let git = FakeGit::default();
    let downloads = FakeDownloads {
        bytes,
        calls: Cell::new(0),
    };
    assert!(
        apply_with(
            &fixture.apply(&journal),
            &origin,
            &record,
            &catalog,
            &git,
            &downloads
        )
        .await
        .is_err()
    );
    assert_eq!(git.clones.get(), 1);
    assert!(args.workspace.join(".git").is_dir());
    assert!(!args.workspace.join(".codex").exists());
    assert_eq!(downloads.calls.get(), 0);
}

#[tokio::test]
async fn lost_skill_completion_reuses_pinned_cache_without_overwrite_or_download() {
    let fixture = Fixture::new();
    let bytes = bundle("2.1.0");
    let mut reviewed = project();
    reviewed.manifest.entry.tools.push(pin(&bytes, "2.1.0"));
    let catalog = FakeCatalog::new(reviewed);
    let (origin, record) = identity();
    let args = fixture.prepare(&catalog.project.manifest);
    let journal = prepare_with(&args, &origin, &record, &catalog)
        .await
        .unwrap();
    let downloads = FakeDownloads {
        bytes,
        calls: Cell::new(0),
    };
    let git = FakeGit::default();
    let mut done = apply_with(
        &fixture.apply(&journal),
        &origin,
        &record,
        &catalog,
        &git,
        &downloads,
    )
    .await
    .unwrap();
    done.tools_done = false; // Simulate completed effects but lost stage acknowledgement.
    filesystem::save(&args.journal, &done).unwrap();
    let recovered = apply_with(
        &fixture.apply(&journal),
        &origin,
        &record,
        &catalog,
        &git,
        &downloads,
    )
    .await
    .unwrap();
    assert!(recovered.tools_done);
    assert_eq!(downloads.calls.get(), 1);
    assert_eq!(git.clones.get(), 1);
    let skill = args.workspace.join(".agents/skills/memory-layer/SKILL.md");
    fs::write(&skill, "custom change after installation").unwrap();
    let mut pending = recovered;
    pending.tools_done = false;
    filesystem::save(&args.journal, &pending).unwrap();
    assert!(
        apply_with(
            &fixture.apply(&journal),
            &origin,
            &record,
            &catalog,
            &git,
            &downloads
        )
        .await
        .is_err()
    );
    assert_eq!(
        fs::read_to_string(skill).unwrap(),
        "custom change after installation"
    );
    assert_eq!(downloads.calls.get(), 1);
}

#[test]
fn directory_publication_never_replaces_even_empty_existing_target() {
    let fixture = Fixture::new();
    let source = fixture.0.join("private");
    let target = fixture.0.join("user-directory");
    fs::create_dir(&source).unwrap();
    fs::write(source.join("payload"), "private contents").unwrap();
    fs::create_dir(&target).unwrap();
    #[cfg(unix)]
    {
        use std::os::unix::fs::MetadataExt;
        let inode = fs::metadata(&target).unwrap().ino();
        assert!(
            filesystem::publish_directory(&source, &target, "receipt.json", b"receipt").is_err()
        );
        assert_eq!(fs::metadata(&target).unwrap().ino(), inode);
    }
    #[cfg(not(unix))]
    assert!(filesystem::publish_directory(&source, &target, "receipt.json", b"receipt").is_err());
    assert!(target.read_dir().unwrap().next().is_none());
    assert_eq!(
        fs::read_to_string(source.join("payload")).unwrap(),
        "private contents"
    );
}

#[cfg(unix)]
#[test]
fn symlinked_settings_are_preserved_and_traversal_paths_fail() {
    use std::os::unix::fs::symlink;
    let fixture = Fixture::new();
    let workspace = fixture.0.join("repo");
    fs::create_dir(&workspace).unwrap();
    let outside = fixture.0.join("outside");
    fs::create_dir(&outside).unwrap();
    symlink(&outside, workspace.join(".codex")).unwrap();
    assert!(!filesystem::install_missing(&workspace, ".codex/config.toml", b"defaults").unwrap());
    assert!(!outside.join("config.toml").exists());
    assert!(filesystem::install_missing(&workspace, "../outside/config.toml", b"bad").is_err());
    assert!(filesystem::absolute_leaf(&fixture.0.join("repo/../outside/file")).is_err());
}
#[test]
fn cli_workspace_commands_require_selection_and_confirmation_digest() {
    use crate::commands::runtime::Cli;
    use clap::Parser;
    assert!(
        Cli::try_parse_from([
            "memory",
            "projects",
            "prepare",
            "--company",
            "https://example.test"
        ])
        .is_err()
    );
    assert!(
        Cli::try_parse_from([
            "memory",
            "projects",
            "apply",
            "--company",
            "https://example.test",
            "--journal",
            "/tmp/plan.json"
        ])
        .is_err()
    );
    assert!(
        Cli::try_parse_from([
            "memory",
            "projects",
            "status",
            "--journal",
            "/tmp/plan.json"
        ])
        .is_ok()
    );
}
