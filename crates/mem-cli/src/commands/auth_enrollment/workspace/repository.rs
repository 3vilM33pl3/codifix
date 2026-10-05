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
pub(super) struct NativeGit;

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
        let mut command = command();
        command
            .arg("-C")
            .arg(path)
            .args([
                "config",
                "--local",
                "--no-includes",
                "--get",
                "remote.origin.url",
            ])
            .stdout(Stdio::piped());
        let mut child = command
            .spawn()
            .map_err(|_| anyhow::anyhow!("git repository metadata unavailable"))?;
        let result = tokio::time::timeout(Duration::from_secs(10), async {
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
        .map_err(|_| anyhow::anyhow!("existing repository metadata timed out"))?;
        result
    }
    async fn clone_to(&self, manifest: &OnboardingProject, path: &Path) -> Result<()> {
        let mut command = command();
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
        let mut verify = command();
        verify
            .arg("-C")
            .arg(path)
            .args(["rev-parse", "--verify", "HEAD"]);
        run(verify).await?;
        let mut checkout = command();
        checkout
            .arg("-C")
            .arg(path)
            .args(["checkout", "--no-recurse-submodules"])
            .arg(&manifest.entry.default_branch);
        // Only our new private clone is checked out, without --force/reset.
        run(checkout).await
    }
}

#[derive(Serialize, Deserialize, PartialEq, Eq)]
struct Receipt {
    request_id: uuid::Uuid,
    manifest_sha256: String,
}
fn receipt(journal: &Journal) -> Receipt {
    Receipt {
        request_id: journal.request_id,
        manifest_sha256: journal.plan.manifest_sha256.clone(),
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
    let stage = staging(journal)?;
    filesystem::absolute_leaf(&stage)?;
    if filesystem::exists(workspace)? {
        if journal.repository == RepositoryStage::CloneIntent && has_receipt(workspace, journal)? {
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
