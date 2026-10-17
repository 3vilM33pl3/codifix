// SPDX-License-Identifier: AGPL-3.0-or-later
use super::{Journal, RepositoryStage, filesystem};
use anyhow::{Result, bail};
use mem_record::OnboardingProject;
use serde::{Deserialize, Serialize};
use std::{
    fs,
    path::{Path, PathBuf},
    process::Stdio,
    time::Duration,
};

pub(super) trait Git {
    fn clone_to(
        &self,
        manifest: &OnboardingProject,
        path: &Path,
    ) -> impl std::future::Future<Output = Result<()>>;
    fn finish(
        &self,
        manifest: &OnboardingProject,
        path: &Path,
    ) -> impl std::future::Future<Output = Result<()>>;
    fn existing(
        &self,
        manifest: &OnboardingProject,
        path: &Path,
    ) -> impl std::future::Future<Output = Result<()>>;
}
#[derive(Default)]
pub(super) struct NativeGit {
    #[cfg(test)]
    home: Option<PathBuf>,
    #[cfg(test)]
    system_config: Option<PathBuf>,
}

const SAFETY: &[&str] = &[
    "-c",
    "core.hooksPath=/dev/null",
    "-c",
    "core.fsmonitor=false",
    "-c",
    "protocol.allow=never",
    "-c",
    "protocol.https.allow=always",
    "-c",
    "protocol.file.allow=never",
    "-c",
    "protocol.ext.allow=never",
    "-c",
    "http.followRedirects=false",
    "-c",
    "submodule.recurse=false",
    "-c",
    "gc.auto=0",
    "-c",
    "maintenance.auto=false",
];
pub(super) fn branch(value: &str) -> Result<()> {
    if value.is_empty()
        || value.len() > 256
        || value.starts_with('-')
        || value.starts_with('/')
        || value.ends_with('/')
        || value.ends_with('.')
        || value.contains("..")
        || value.contains("@{")
        || value
            .split('/')
            .any(|part| part.is_empty() || part.starts_with('.') || part.ends_with(".lock"))
        || value
            .chars()
            .any(|c| c.is_control() || c.is_whitespace() || "~^:?*[\\".contains(c))
    {
        bail!("reviewed branch name is invalid");
    }
    Ok(())
}
fn command() -> tokio::process::Command {
    let mut command = tokio::process::Command::new("git");
    command
        .args(SAFETY)
        .stdin(Stdio::null())
        .stdout(Stdio::null())
        .stderr(Stdio::null())
        .kill_on_drop(true);
    // Do not inherit a caller's repository routing or injected per-command config.
    // Normal user gitconfig and their credential helper remain available. No
    // company device credential is sent to git or any artifact origin.
    for (key, _) in std::env::vars_os() {
        if key.to_string_lossy().starts_with("GIT_") && key != "GIT_ASKPASS" {
            command.env_remove(key);
        }
    }
    command.env("GIT_TERMINAL_PROMPT", "0");
    command
}
async fn run(mut command: tokio::process::Command) -> Result<()> {
    let status = tokio::time::timeout(Duration::from_secs(120), command.status())
        .await
        .map_err(|_| {
            anyhow::anyhow!(
                "git operation timed out; journal/staging retained, no reset or automatic reclone"
            )
        })?
        .map_err(|_| {
            anyhow::anyhow!("git unavailable; configure your own repository credentials and retry")
        })?;
    if !status.success() {
        bail!(
            "git operation unconfirmed; output suppressed, inspect retained staging and your repository credentials"
        );
    }
    Ok(())
}
impl Git for NativeGit {
    async fn existing(&self, manifest: &OnboardingProject, path: &Path) -> Result<()> {
        use tokio::io::AsyncReadExt;
        let mut command = self.command();
        let git = path.join(".git");
        if git.is_dir() {
            let config = filesystem::absolute_leaf(&git.join("config"))?;
            command.args(["config", "--file"]).arg(config);
        } else {
            command.arg("-C").arg(path).args(["config", "--local"]);
        }
        command
            .args(["--no-includes", "--get", "remote.origin.url"])
            .stdout(Stdio::piped());
        let mut child = command
            .spawn()
            .map_err(|_| anyhow::anyhow!("git repository metadata unavailable"))?;
        tokio::time::timeout(Duration::from_secs(10), async {
            let mut bytes = Vec::new();
            child
                .stdout
                .take()
                .ok_or_else(|| anyhow::anyhow!("git metadata stream unavailable"))?
                .take(2049)
                .read_to_end(&mut bytes)
                .await?;
            if bytes.len() > 2048 {
                bail!("existing repository origin exceeds safe limit");
            }
            let status = child.wait().await?;
            if !status.success()
                || std::str::from_utf8(&bytes).ok().map(str::trim)
                    != Some(manifest.entry.repository_url.as_str())
            {
                bail!("existing repository origin differs from reviewed URL; no changes performed");
            }
            Ok(())
        })
        .await
        .map_err(|_| anyhow::anyhow!("existing repository metadata timed out"))?
    }
    async fn clone_to(&self, manifest: &OnboardingProject, path: &Path) -> Result<()> {
        let mut command = self.command();
        command
            .args([
                "clone",
                "--no-checkout",
                "--no-recurse-submodules",
                "--single-branch",
                "--template=",
                "--branch",
            ])
            .arg(&manifest.entry.default_branch)
            .arg("--")
            .arg(&manifest.entry.repository_url)
            .arg(path);
        run(command).await
    }
    async fn finish(&self, manifest: &OnboardingProject, path: &Path) -> Result<()> {
        if !filesystem::is_repository(path)? || !path.join(".git").is_dir() {
            bail!(
                "interrupted clone has no usable repository; inspect it explicitly, no automatic duplicate clone"
            );
        }
        let mut verify = self.checkout_command();
        verify
            .arg("-C")
            .arg(path)
            .args(["rev-parse", "--verify", "HEAD"]);
        run(verify).await?;
        self.materialization_policy(manifest, path).await?;
        let mut checkout = self.checkout_command();
        checkout
            .arg("-C")
            .arg(path)
            .args(["checkout", "--no-recurse-submodules"])
            .arg(&manifest.entry.default_branch);
        // Only our new private clone is checked out, without --force/reset.
        run(checkout).await
    }
}

impl NativeGit {
    fn command(&self) -> tokio::process::Command {
        let command = command();
        #[cfg(test)]
        let mut command = command;
        #[cfg(test)]
        if let Some(home) = &self.home {
            command
                .env("HOME", home)
                .env("XDG_CONFIG_HOME", home.join(".config"));
        }
        #[cfg(test)]
        if let Some(config) = &self.system_config {
            command.env("GIT_CONFIG_SYSTEM", config);
        }
        command
    }
    fn checkout_command(&self) -> tokio::process::Command {
        let mut command = self.command();
        command
            .env("GIT_CONFIG_GLOBAL", "/dev/null")
            .env("GIT_CONFIG_NOSYSTEM", "1")
            .env("GIT_ATTR_NOSYSTEM", "1")
            .env_remove("GIT_ASKPASS")
            .args([
                "-c",
                "core.attributesFile=/dev/null",
                "-c",
                "core.sparseCheckout=false",
            ]);
        command
    }
    async fn materialization_policy(
        &self,
        manifest: &OnboardingProject,
        path: &Path,
    ) -> Result<()> {
        // Local include/worktree config and info attributes are outside the
        // reviewed tree. Do not let them reintroduce system/user configuration.
        for relative in [".git/config.worktree", ".git/info/attributes"] {
            let file = path.join(relative);
            if filesystem::exists(&file)?
                && (fs::symlink_metadata(&file)?.file_type().is_symlink()
                    || fs::metadata(&file)?.len() > 0)
            {
                bail!(
                    "external/local checkout attributes or worktree configuration are unsupported; no materialization performed"
                );
            }
        }
        let mut includes = self.checkout_command();
        includes.arg("-C").arg(path).args([
            "config",
            "--local",
            "--no-includes",
            "--name-only",
            "--get-regexp",
            "^include(if\\..*)?\\.path$",
        ]);
        if !inspect(includes, None, 64 * 1024, true).await?.is_empty() {
            bail!("local config includes are unsupported during isolated checkout");
        }
        let mut filters = self.checkout_command();
        filters.arg("-C").arg(path).args([
            "config",
            "--local",
            "--no-includes",
            "--name-only",
            "--get-regexp",
            "^filter\\.",
        ]);
        if !inspect(filters, None, 64 * 1024, true).await?.is_empty() {
            bail!(
                "local required/custom filter materialization unsupported; no checkout performed"
            );
        }
        let source = format!("refs/heads/{}", manifest.entry.default_branch);
        let mut tree = self.checkout_command();
        tree.arg("-C")
            .arg(path)
            .args(["ls-tree", "-r", "--name-only", "-z"])
            .arg(&source);
        let paths = inspect(tree, None, 8 * 1024 * 1024, false).await?;
        let mut attributes = self.checkout_command();
        attributes
            .arg("-C")
            .arg(path)
            .args(["check-attr", "--all", "--stdin", "-z"])
            .arg(format!("--source={source}"));
        let attributes = inspect(attributes, Some(&paths), 16 * 1024 * 1024, false).await?;
        let fields = attributes
            .split(|byte| *byte == 0)
            .filter(|field| !field.is_empty())
            .collect::<Vec<_>>();
        if fields.len() % 3 != 0 {
            bail!("isolated Git attribute inspection unsupported or unreadable");
        }
        if fields
            .as_chunks::<3>()
            .0
            .iter()
            .any(|entry| entry[1] == b"filter")
        {
            // Conservative first-slice policy: required/custom filters cannot be
            // silently replaced by raw files, and optional filters are not run.
            bail!(
                "repository filter materialization unsupported (including required filters); no checkout performed"
            );
        }
        Ok(())
    }
}

async fn inspect(
    mut command: tokio::process::Command,
    input: Option<&[u8]>,
    limit: usize,
    no_matches: bool,
) -> Result<Vec<u8>> {
    use tokio::io::{AsyncReadExt, AsyncWriteExt};
    command.stdout(Stdio::piped());
    if input.is_some() {
        command.stdin(Stdio::piped());
    }
    let mut child = command
        .spawn()
        .map_err(|_| anyhow::anyhow!("isolated Git inspection unavailable"))?;
    tokio::time::timeout(Duration::from_secs(15), async {
        let stdout = child
            .stdout
            .take()
            .ok_or_else(|| anyhow::anyhow!("Git inspection stream unavailable"))?;
        let stdin = child.stdin.take();
        let writer = async {
            if let (Some(mut stdin), Some(input)) = (stdin, input) {
                stdin.write_all(input).await?;
                stdin.shutdown().await?;
            }
            Ok::<(), anyhow::Error>(())
        };
        let reader = async {
            let mut bytes = Vec::new();
            stdout
                .take(limit as u64 + 1)
                .read_to_end(&mut bytes)
                .await?;
            if bytes.len() > limit {
                bail!("Git inspection exceeds bounded metadata limit");
            }
            Ok::<_, anyhow::Error>(bytes)
        };
        let (_, bytes) = tokio::try_join!(writer, reader)?;
        let status = child.wait().await?;
        if !status.success() && !(no_matches && status.code() == Some(1) && bytes.is_empty()) {
            bail!(
                "isolated Git inspection failed; materialization unsupported, response suppressed"
            );
        }
        Ok(bytes)
    })
    .await
    .map_err(|_| anyhow::anyhow!("isolated Git inspection timed out; no checkout performed"))?
}

#[derive(Serialize, Deserialize, PartialEq, Eq)]
struct Receipt {
    request_id: uuid::Uuid,
    manifest_sha256: String,
    plan_sha256: String,
}
fn receipt(journal: &Journal) -> Receipt {
    Receipt {
        request_id: journal.request_id,
        manifest_sha256: journal.plan.manifest_sha256.clone(),
        plan_sha256: journal.plan_sha256.clone(),
    }
}
fn has_receipt(path: &Path, journal: &Journal) -> Result<bool> {
    let path = path.join(".git/memory-onboarding-receipt.json");
    if !filesystem::exists(&path)? {
        return Ok(false);
    }
    if fs::symlink_metadata(&path)?.file_type().is_symlink() || fs::metadata(&path)?.len() > 1024 {
        bail!("clone receipt path conflict");
    }
    let saved: Receipt = serde_json::from_slice(&fs::read(path)?)
        .map_err(|_| anyhow::anyhow!("clone receipt invalid"))?;
    if saved != receipt(journal) {
        bail!("clone receipt belongs to a different request/manifest");
    }
    Ok(true)
}
#[cfg(feature = "project-sync")]
pub(super) fn ready_for_agent_settings(journal: &Journal) -> Result<bool> {
    if !filesystem::is_repository(&journal.plan.workspace)? {
        return Ok(false);
    }
    match journal.repository {
        RepositoryStage::Ready => has_receipt(&journal.plan.workspace, journal),
        RepositoryStage::ExistingPreserved => Ok(true),
        _ => Ok(false),
    }
}
fn staging(journal: &Journal) -> Result<PathBuf> {
    let parent = journal
        .plan
        .workspace
        .parent()
        .ok_or_else(|| anyhow::anyhow!("workspace parent unavailable"))?;
    Ok(parent.join(format!(".memory-clone-{}", journal.request_id)))
}

pub(super) async fn apply(path: &Path, journal: &mut Journal, git: &impl Git) -> Result<()> {
    let workspace = &journal.plan.workspace;
    if journal.plan.repository_existing {
        if !filesystem::is_repository(workspace)? {
            bail!("existing repository changed; preserving workspace and stopping");
        }
        git.existing(&journal.plan.manifest, workspace).await?;
        // Never switch branches, checkout/reset, fetch or run hooks in a user's repo.
        journal.repository = RepositoryStage::ExistingPreserved;
        filesystem::save(path, journal)?;
        return Ok(());
    }
    if journal.repository == RepositoryStage::Ready {
        if !filesystem::is_repository(workspace)? || !has_receipt(workspace, journal)? {
            bail!(
                "prepared repository or immutable clone receipt changed; no further effects performed"
            );
        }
        git.existing(&journal.plan.manifest, workspace).await?;
        return Ok(());
    }
    let stage = staging(journal)?;
    filesystem::absolute_leaf(&stage)?;
    if filesystem::exists(workspace)? {
        if journal.repository == RepositoryStage::CloneIntent && has_receipt(workspace, journal)? {
            if workspace.join(".git/config").is_file() {
                git.existing(&journal.plan.manifest, workspace).await?;
            } else {
                git.existing(&journal.plan.manifest, &stage).await?;
            }
            filesystem::publish_directory(
                &stage,
                workspace,
                ".git/memory-onboarding-receipt.json",
                &serde_json::to_vec(&receipt(journal))?,
            )?;
            journal.repository = RepositoryStage::Ready;
            filesystem::save(path, journal)?;
            return Ok(());
        }
        bail!("target appeared after preview; no existing directory/repository was replaced");
    }
    if journal.repository == RepositoryStage::Pending {
        if filesystem::exists(&stage)? {
            bail!("private clone staging already exists; refusing replacement");
        }
        journal.repository = RepositoryStage::CloneIntent;
        filesystem::save(path, journal)?;
        fs::create_dir(&stage)?;
        git.clone_to(&journal.plan.manifest, &stage).await?;
    } else if !filesystem::exists(&stage)? {
        // Interrupted before creating private staging: no local clone exists.
        fs::create_dir(&stage)?;
        git.clone_to(&journal.plan.manifest, &stage).await?;
    } else if fs::read_dir(&stage)?.next().is_none() {
        // Retry only a provably empty private target, not a partial/existing repo.
        git.clone_to(&journal.plan.manifest, &stage).await?;
    }
    git.existing(&journal.plan.manifest, &stage).await?;
    if !has_receipt(&stage, journal)? {
        git.finish(&journal.plan.manifest, &stage).await?;
        if !filesystem::install_missing(
            &stage,
            ".git/memory-onboarding-receipt.json",
            &serde_json::to_vec(&receipt(journal))?,
        )? {
            bail!("clone receipt collision; no workspace published");
        }
    }
    // Final user target is never cleared, reset, or recursively removed.
    if filesystem::exists(workspace)? {
        bail!("target appeared during clone; staging retained, nothing replaced");
    }
    filesystem::publish_directory(
        &stage,
        workspace,
        ".git/memory-onboarding-receipt.json",
        &serde_json::to_vec(&receipt(journal))?,
    )?;
    journal.repository = RepositoryStage::Ready;
    filesystem::save(path, journal)?;
    Ok(())
}

#[cfg(test)]
mod tests {
    use super::*;
    struct Fixture(PathBuf);
    impl Drop for Fixture {
        fn drop(&mut self) {
            let _ = fs::remove_dir_all(&self.0);
        }
    }
    fn fixture(attributes: bool) -> (Fixture, NativeGit, OnboardingProject, Vec<u8>, Vec<u8>) {
        let root =
            std::env::temp_dir().join(format!("memory-git-filter-test-{}", uuid::Uuid::new_v4()));
        fs::create_dir_all(root.join("repo")).unwrap();
        fs::create_dir(root.join("home")).unwrap();
        let repo = root.join("repo");
        let setup = |args: &[&str]| {
            let status = std::process::Command::new("git")
                .arg("-C")
                .arg(&repo)
                .args([
                    "-c",
                    "core.hooksPath=/dev/null",
                    "-c",
                    "user.name=Fixture",
                    "-c",
                    "user.email=fixture@example.test",
                ])
                .args(args)
                .env("GIT_CONFIG_GLOBAL", "/dev/null")
                .env("GIT_CONFIG_NOSYSTEM", "1")
                .env("GIT_ATTR_NOSYSTEM", "1")
                .stdout(Stdio::null())
                .stderr(Stdio::null())
                .status()
                .unwrap();
            assert!(status.success(), "synthetic Git setup failed");
        };
        setup(&["init", "-b", "main"]);
        fs::write(repo.join("file.txt"), "reviewed bytes\n").unwrap();
        if attributes {
            fs::write(repo.join(".gitattributes"), "*.txt filter=trap\n").unwrap();
        }
        setup(&["add", "."]);
        setup(&["commit", "-m", "synthetic"]);
        let manifest = super::super::tests::project().manifest;
        setup(&["remote", "add", "origin", &manifest.entry.repository_url]);
        setup(&["read-tree", "--empty"]);
        // A real clone --no-checkout has no index yet; do not simulate tracked
        // user deletions, which checkout must correctly preserve.
        fs::remove_file(repo.join(".git/index")).unwrap();
        fs::remove_file(repo.join("file.txt")).unwrap();
        if attributes {
            fs::remove_file(repo.join(".gitattributes")).unwrap();
        }
        let sentinel = root.join("filter-ran");
        let global_attrs = root.join("global-attributes");
        fs::write(&global_attrs, "*.txt filter=trap\n").unwrap();
        let global = format!("[filter \"trap\"]\n smudge = touch {}\n process = touch {}\n required = true\n[core]\n attributesFile = {}\n", sentinel.display(), sentinel.display(), global_attrs.display()).into_bytes();
        let system = format!(
            "[filter \"trap\"]\n smudge = touch {}\n process = touch {}\n required = true\n",
            sentinel.display(),
            sentinel.display()
        )
        .into_bytes();
        fs::write(root.join("home/.gitconfig"), &global).unwrap();
        fs::write(root.join("system.gitconfig"), &system).unwrap();
        let git = NativeGit {
            home: Some(root.join("home")),
            system_config: Some(root.join("system.gitconfig")),
        };
        (Fixture(root), git, manifest, global, system)
    }
    #[tokio::test]
    async fn isolated_checkout_ignores_external_filters_and_preserves_user_config() {
        let (fixture, git, manifest, global, system) = fixture(false);
        git.finish(&manifest, &fixture.0.join("repo"))
            .await
            .unwrap();
        assert_eq!(
            fs::read_to_string(fixture.0.join("repo/file.txt")).unwrap(),
            "reviewed bytes\n"
        );
        assert!(!fixture.0.join("filter-ran").exists());
        assert_eq!(fs::read(fixture.0.join("home/.gitconfig")).unwrap(), global);
        assert_eq!(
            fs::read(fixture.0.join("system.gitconfig")).unwrap(),
            system
        );
        // Normal clone command keeps user/system config for their credentials.
        assert!(
            !git.command()
                .as_std()
                .get_envs()
                .any(|(key, _)| key == "GIT_CONFIG_GLOBAL" || key == "GIT_CONFIG_NOSYSTEM")
        );
    }
    #[tokio::test]
    async fn tracked_required_filters_fail_without_executing_smudge_or_process() {
        let (fixture, git, manifest, global, system) = fixture(true);
        let error = git
            .finish(&manifest, &fixture.0.join("repo"))
            .await
            .unwrap_err();
        assert!(
            error
                .to_string()
                .contains("filter materialization unsupported")
        );
        assert!(!fixture.0.join("filter-ran").exists());
        assert!(!fixture.0.join("repo/file.txt").exists());
        assert_eq!(fs::read(fixture.0.join("home/.gitconfig")).unwrap(), global);
        assert_eq!(
            fs::read(fixture.0.join("system.gitconfig")).unwrap(),
            system
        );
    }
    #[tokio::test]
    async fn local_filter_and_external_attributes_are_explicitly_unsupported() {
        let (fixture, git, manifest, _, _) = fixture(false);
        let repo = fixture.0.join("repo");
        fs::write(repo.join(".git/info/attributes"), "*.txt filter=trap\n").unwrap();
        assert!(git.finish(&manifest, &repo).await.is_err());
        fs::remove_file(repo.join(".git/info/attributes")).unwrap();
        let mut config = fs::OpenOptions::new()
            .append(true)
            .open(repo.join(".git/config"))
            .unwrap();
        use std::io::Write;
        writeln!(
            config,
            "[filter \"trap\"]\n process = touch {}\n required = true",
            // Git config treats raw Windows backslashes as escape sequences.
            // Keep this a valid custom filter on every platform so the test
            // reaches the materialization policy, not a config parse failure.
            fixture
                .0
                .join("filter-ran")
                .display()
                .to_string()
                .replace('\\', "/")
        )
        .unwrap();
        assert!(
            git.finish(&manifest, &repo)
                .await
                .unwrap_err()
                .to_string()
                .contains("filter materialization unsupported")
        );
        assert!(!fixture.0.join("filter-ran").exists());
    }
}
