// SPDX-License-Identifier: AGPL-3.0-or-later
//! Explicit, journaled local work. This is not agent registration or sync.
mod artifacts;
mod filesystem;
mod repository;
#[cfg(test)]
mod tests;

use super::{CompanyArgs, CompanyOrigin, credentials, transport};
use anyhow::{Result, bail};
use clap::Args;
use credentials::{CredentialStore, Enrollment};
use mem_record::{OnboardingDeviceReviewProject, OnboardingProject};
use serde::{Deserialize, Serialize};
use std::path::PathBuf;
use uuid::Uuid;

#[derive(Debug, Args)]
pub(in crate::commands) struct PrepareArgs {
    #[command(flatten)]
    pub(super) company: CompanyArgs,
    #[arg(long)]
    pub(super) project: Uuid,
    /// Stable reviewed area: workshop, homelab, trust or general.
    #[arg(long)]
    pub(super) area: String,
    #[arg(long)]
    pub(super) owner_instance: Uuid,
    #[arg(long)]
    pub(super) revision: i64,
    /// Absolute target with an existing parent; existing repositories are preserved.
    #[arg(long)]
    pub(super) workspace: PathBuf,
    /// Absolute local metadata journal outside the target repository.
    #[arg(long)]
    pub(super) journal: PathBuf,
}
#[derive(Debug, Args)]
pub(in crate::commands) struct ApplyArgs {
    #[command(flatten)]
    company: CompanyArgs,
    #[arg(long)]
    journal: PathBuf,
    /// Digest printed by prepare. Changed plans require fresh review.
    #[arg(long)]
    plan_sha256: String,
    /// Confirm the exact preview; no effects occur without this flag.
    #[arg(long)]
    confirm: bool,
}
#[derive(Debug, Args)]
pub(in crate::commands) struct StatusArgs {
    #[arg(long)]
    journal: PathBuf,
}

#[derive(Clone, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
struct PlannedFile {
    path: String,
    content: String,
}
#[derive(Clone, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
struct Plan {
    company: String,
    device_id: Uuid,
    workspace: PathBuf,
    manifest: OnboardingProject,
    manifest_sha256: String,
    repository_existing: bool,
    templates: Vec<PlannedFile>,
}
#[derive(Clone, Serialize, Deserialize, PartialEq, Eq)]
#[serde(rename_all = "snake_case")]
enum RepositoryStage {
    Pending,
    CloneIntent,
    Ready,
    ExistingPreserved,
}
#[derive(Clone, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
struct Journal {
    schema_version: u32,
    request_id: Uuid,
    plan: Plan,
    plan_sha256: String,
    repository: RepositoryStage,
    templates_done: bool,
    tools_done: bool,
    preserved_paths: Vec<String>,
}

trait Catalog {
    fn projects(
        &self,
        enrollment: &Enrollment,
    ) -> impl std::future::Future<Output = Result<transport::AvailableProjects>>;
}
impl Catalog for transport::CompanyClient {
    async fn projects(&self, enrollment: &Enrollment) -> Result<transport::AvailableProjects> {
        transport::CompanyClient::projects(self, enrollment).await
    }
}

pub(super) fn active(store: &impl CredentialStore, origin: &CompanyOrigin) -> Result<Enrollment> {
    let record = credentials::load(store, origin)?
        .ok_or_else(|| anyhow::anyhow!("sign in with memory auth login for this company first"))?;
    if !record.approved || record.pending_logout {
        bail!("workspace preparation requires an approved device without pending logout");
    }
    Ok(record)
}

/// Read-only path validation shared by the picker before entering its UI.
#[cfg(feature = "tui")]
pub(super) fn validate_selection_paths(workspace: &PathBuf, journal: &PathBuf) -> Result<()> {
    filesystem::separate(journal, workspace)?;
    filesystem::absolute_leaf(workspace)?;
    filesystem::absolute_leaf(journal)?;
    Ok(())
}

/// A picker confirmation binds the complete manifest, not just its display name
/// or revision. Current authorization/offline availability is checked again.
#[cfg(any(feature = "tui", test))]
struct ReviewedCatalog<'a, C> {
    remote: &'a C,
    expected: &'a OnboardingProject,
}
#[cfg(any(feature = "tui", test))]
impl<C: Catalog> Catalog for ReviewedCatalog<'_, C> {
    async fn projects(&self, enrollment: &Enrollment) -> Result<transport::AvailableProjects> {
        let scope = self.remote.projects(enrollment).await?;
        let expected = self.expected;
        let project = scope.projects.iter().find(|project| {
            project.manifest.project_id == expected.project_id
                && project.manifest.entry.area == expected.entry.area
                && project.manifest.entry.owner_instance_id == expected.entry.owner_instance_id
                && project.manifest.revision == expected.revision
        }).ok_or_else(|| anyhow::anyhow!("reviewed project is no longer in the current approved device scope; select again"))?;
        validate_project(project)?;
        if filesystem::digest(&project.manifest)? != filesystem::digest(expected)? {
            bail!("reviewed manifest changed; select and confirm the current manifest again");
        }
        Ok(scope)
    }
}

#[cfg(feature = "tui")]
pub(super) async fn prepare_reviewed(
    args: &PrepareArgs,
    expected: &OnboardingProject,
    device_id: Uuid,
    store: &impl CredentialStore,
) -> Result<String> {
    if args.project != expected.project_id
        || args.area != expected.entry.area.as_str()
        || args.owner_instance != expected.entry.owner_instance_id
        || args.revision != expected.revision
    {
        bail!("prepared selection does not match the confirmed area/owner/project/revision");
    }
    let origin = CompanyOrigin::parse(&args.company.company, args.company.allow_loopback)?;
    let enrollment = active(store, &origin)?;
    if enrollment.device_id != device_id {
        bail!("enrolled device changed during review; select again before preparing metadata");
    }
    let remote = transport::CompanyClient::new(origin.clone())?;
    let reviewed = ReviewedCatalog {
        remote: &remote,
        expected,
    };
    let journal = prepare_with(args, &origin, &enrollment, &reviewed).await?;
    print_preview(&journal)?;
    println!("Journal: {}", args.journal.display());
    Ok(journal.plan_sha256)
}

#[cfg(feature = "tui")]
pub(super) fn reviewed_template_preview() -> Result<String> {
    Ok(serde_json::to_string_pretty(&templates())?)
}

pub(super) async fn prepare(args: &PrepareArgs, store: &impl CredentialStore) -> Result<()> {
    let origin = CompanyOrigin::parse(&args.company.company, args.company.allow_loopback)?;
    let enrollment = active(store, &origin)?;
    let remote = transport::CompanyClient::new(origin.clone())?;
    let journal = prepare_with(args, &origin, &enrollment, &remote).await?;
    print_preview(&journal)?;
    println!("Journal: {}", args.journal.display());
    Ok(())
}

async fn prepare_with(
    args: &PrepareArgs,
    origin: &CompanyOrigin,
    enrollment: &Enrollment,
    remote: &impl Catalog,
) -> Result<Journal> {
    filesystem::separate(&args.journal, &args.workspace)?;
    let journal_path = filesystem::absolute_leaf(&args.journal)?;
    let workspace = filesystem::absolute_leaf(&args.workspace)?;
    let _lock = filesystem::Lock::take(&journal_path)?;
    let scope = remote.projects(enrollment).await?;
    let project = scope
        .projects
        .iter()
        .find(|project| {
            project.manifest.project_id == args.project
                && project.manifest.entry.area.as_str() == args.area
                && project.manifest.entry.owner_instance_id == args.owner_instance
                && project.manifest.revision == args.revision
        })
        .ok_or_else(|| {
            anyhow::anyhow!(
                "exact area/owner/project/revision is not currently approved for this device"
            )
        })?;
    validate_project(project)?;
    let plan = Plan {
        company: origin.0.clone(),
        device_id: enrollment.device_id,
        workspace: workspace.clone(),
        manifest: project.manifest.clone(),
        manifest_sha256: filesystem::digest(&project.manifest)?,
        repository_existing: filesystem::exists(&workspace)?,
        templates: templates(),
    };
    if plan.repository_existing && !filesystem::is_repository(&workspace)? {
        bail!("workspace exists but is not a repository; no files were changed");
    }
    let digest = filesystem::digest(&plan)?;
    if filesystem::exists(&journal_path)? {
        let existing = filesystem::read(&journal_path)?;
        // repository_existing is immutable after prepare, including a resumed clone.
        let mut resumed_plan = plan;
        resumed_plan.repository_existing = existing.plan.repository_existing;
        if existing.plan_sha256 != filesystem::digest(&resumed_plan)? {
            bail!("journal inputs changed; preserve this journal and review a new plan separately");
        }
        return Ok(existing);
    }
    let journal = Journal {
        schema_version: 1,
        request_id: Uuid::new_v4(),
        plan,
        plan_sha256: digest,
        repository: RepositoryStage::Pending,
        templates_done: false,
        tools_done: false,
        preserved_paths: Vec::new(),
    };
    filesystem::create_journal(&journal_path, &journal)?;
    Ok(journal)
}

pub(super) async fn apply(args: &ApplyArgs, store: &impl CredentialStore) -> Result<()> {
    if !args.confirm {
        bail!("apply requires --confirm and the exact --plan-sha256 from prepare");
    }
    let origin = CompanyOrigin::parse(&args.company.company, args.company.allow_loopback)?;
    let enrollment = active(store, &origin)?;
    let remote = transport::CompanyClient::new(origin.clone())?;
    let journal = apply_with(
        args,
        &origin,
        &enrollment,
        &remote,
        &repository::NativeGit::default(),
        &artifacts::HttpsArtifacts::new()?,
    )
    .await?;
    print_preview(&journal)?;
    println!(
        "Local preparation stages recorded. Agent registration is BLOCKED: separate Codex/OpenAI and Claude/Anthropic identities have not been provisioned."
    );
    println!("No local database, service activation, model calls or selective sync was performed.");
    Ok(())
}

async fn apply_with(
    args: &ApplyArgs,
    origin: &CompanyOrigin,
    enrollment: &Enrollment,
    remote: &impl Catalog,
    git: &impl repository::Git,
    downloads: &impl artifacts::Downloads,
) -> Result<Journal> {
    if !args.confirm {
        bail!("apply requires --confirm; no effects performed");
    }
    let path = filesystem::absolute_leaf(&args.journal)?;
    // Read-only physical validation comes before even creating lock sidecars.
    let preview = filesystem::read(&path)?;
    filesystem::separate(&path, &preview.plan.workspace)?;
    let _lock = filesystem::Lock::take(&path)?;
    let mut journal = filesystem::read(&path)?;
    filesystem::separate(&path, &journal.plan.workspace)?;
    if journal.plan.company != origin.0
        || journal.plan.device_id != enrollment.device_id
        || journal.plan_sha256 != args.plan_sha256
    {
        bail!("confirmed plan/company/device identity mismatch; no effects performed");
    }
    let _workspace_lock = filesystem::Lock::take(&journal.plan.workspace)?;
    // Recheck authority even when resuming completed local stages.
    current(&journal, enrollment, remote).await?;
    // Completed stages are not authority: reverify repository and origin on
    // every invocation before templates/downloads or successful status.
    repository::apply(&path, &mut journal, git).await?;
    current(&journal, enrollment, remote).await?;
    if !journal.templates_done {
        for template in &journal.plan.templates {
            if !filesystem::install_missing(
                &journal.plan.workspace,
                &template.path,
                template.content.as_bytes(),
            )? {
                journal.preserved_paths.push(template.path.clone());
            }
        }
        journal.templates_done = true;
        filesystem::save(&path, &journal)?;
    }
    if !journal.tools_done {
        for tool in journal.plan.manifest.entry.tools.clone() {
            current(&journal, enrollment, remote).await?;
            if tool.name != "memory-skills" {
                bail!(
                    "tool format unsupported: {}; repository/templates stages retained, no generic plugin was installed",
                    tool.name
                );
            }
            artifacts::apply(&path, &mut journal, &tool, downloads).await?;
        }
        journal.tools_done = true;
        filesystem::save(&path, &journal)?;
    }
    Ok(journal)
}

async fn current(journal: &Journal, enrollment: &Enrollment, remote: &impl Catalog) -> Result<()> {
    let scope = remote.projects(enrollment).await?;
    let project = scope
        .projects
        .iter()
        .find(|project| {
            project.manifest.project_id == journal.plan.manifest.project_id
                && project.manifest.entry.area == journal.plan.manifest.entry.area
                && project.manifest.entry.owner_instance_id
                    == journal.plan.manifest.entry.owner_instance_id
        })
        .ok_or_else(|| {
            anyhow::anyhow!(
                "project no longer approved; local effects stopped and journal retained"
            )
        })?;
    validate_project(project)?;
    if filesystem::digest(&project.manifest)? != journal.plan.manifest_sha256 {
        bail!("reviewed manifest changed; local effects stopped, create and confirm a new plan");
    }
    Ok(())
}

pub(super) fn validate_project(project: &OnboardingDeviceReviewProject) -> Result<()> {
    let manifest = &project.manifest;
    if manifest.project_id.is_nil()
        || manifest.entry.owner_instance_id.is_nil()
        || manifest.revision < 1
        || project.allowed_roles.is_empty()
        || project.allowed_roles.iter().any(|role| {
            !matches!(
                role,
                mem_record::AuthRole::Reader | mem_record::AuthRole::Writer
            )
        })
    {
        bail!("project catalog has invalid identity or device scope");
    }
    filesystem::https_url(&manifest.entry.repository_url)?;
    repository::branch(&manifest.entry.default_branch)?;
    if manifest.entry.area == mem_record::OnboardingArea::Trust {
        bail!("Trust is remote-only; local repository/tool preparation is not allowed");
    }
    // Workspace journals retain offline repository/settings/tool copies. This
    // condition must hold at prepare AND every apply/resume, not just in the UI.
    if !project
        .allowed_roles
        .contains(&mem_record::AuthRole::Reader)
        || !project.offline_available
        || !manifest.entry.offline_allowed
    {
        bail!("workspace preparation requires current Reader and reviewed offline scope");
    }
    if manifest.entry.tools.len() > 32 {
        bail!("too many pinned tools");
    }
    for tool in &manifest.entry.tools {
        if tool.name.is_empty()
            || tool.name.len() > 128
            || tool.name.chars().any(char::is_control)
            || tool.version.is_empty()
            || tool.version.len() > 128
            || tool.sha256.len() != 64
            || !tool.sha256.bytes().all(|value| value.is_ascii_hexdigit())
        {
            bail!("invalid reviewed tool pin");
        }
        filesystem::https_url(&tool.url)?;
    }
    Ok(())
}

fn templates() -> Vec<PlannedFile> {
    vec![
        PlannedFile {
            path: ".codex/config.toml".into(),
            content: include_str!("../../../../../.codex/config.toml").into(),
        },
        PlannedFile {
            path: ".codex/agents/astra_adviser.toml".into(),
            content: include_str!("../../../../../.codex/agents/astra_adviser.toml").into(),
        },
    ]
}

pub(super) fn status(args: &StatusArgs) -> Result<()> {
    let path = filesystem::absolute_leaf(&args.journal)?;
    print_preview(&filesystem::read(&path)?)?;
    println!("Local metadata only; current company authorization was not checked.");
    Ok(())
}
fn print_preview(journal: &Journal) -> Result<()> {
    println!("{}", serde_json::to_string_pretty(journal)?);
    println!(
        "Existing repository branches, custom Codex/Claude settings and skills are preserved. Templates apply only to missing files; unsupported tools block completion."
    );
    println!(
        "Agent registration, local service/database setup and selective sync are not completed by this plan."
    );
    Ok(())
}
