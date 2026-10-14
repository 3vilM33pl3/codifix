// SPDX-License-Identifier: AGPL-3.0-or-later
//! Company device enrollment is isolated from local-owner configuration.
mod agents;
mod credentials;
mod project_picker;
mod replica;
#[cfg(test)]
mod tests;
mod transport;
mod workspace;

use anyhow::{Result, bail};
use clap::{Args, Subcommand};
use reqwest::Url;
use std::time::Duration;

use super::runtime::{AuthCommand, Command};
use credentials::{CredentialStore, NativeKeychain};

pub(in crate::commands) const HELP: &str = "Agent notes:\nDocs: docs/user/cli/projects.md\nCompany credentials stay in Keychain. Review exact project, owner and disclosure policy before confirmation. Local pending work is not owner acceptance; Git pushes remain separate.";

#[derive(Debug, Args)]
pub(super) struct CompanyArgs {
    /// Exact company HTTPS origin, without a path, credentials, query or fragment.
    #[arg(long)]
    pub(super) company: String,
    /// Permit a loopback origin (including HTTP) for explicit local development.
    #[arg(long)]
    pub(super) allow_loopback: bool,
}

#[derive(Debug, Args)]
#[command(
    after_help = "Agent notes:\nRequires human browser approval on the exact company origin. Secrets stay in Keychain. This does not grant Git access or model disclosure."
)]
pub(super) struct LoginArgs {
    #[command(flatten)]
    pub(super) company: CompanyArgs,
    /// Device name shown to the approving human (1-128 bytes).
    #[arg(long)]
    pub(super) name: String,
    /// Print the public approval URL without opening a browser.
    #[arg(long)]
    pub(super) no_browser: bool,
}

#[derive(Debug, Args)]
#[command(
    after_help = "Agent notes:\nNormally revoke remotely before deleting the Keychain credential. Explicit local-only deletion does not revoke server access or recall downloaded bytes."
)]
pub(super) struct LogoutArgs {
    #[command(flatten)]
    pub(super) company: CompanyArgs,
    /// Forget this local credential without claiming server cancellation or revocation.
    #[arg(long, requires = "confirm")]
    pub(super) forget_local_only: bool,
    /// Confirm local-only deletion; remote device access may remain valid.
    #[arg(long, requires = "forget_local_only")]
    pub(super) confirm: bool,
}

#[derive(Debug, Args)]
#[command(after_help = HELP)]
pub(super) struct ProjectsArgs {
    #[command(subcommand)]
    pub(super) command: ProjectsCommand,
}
#[derive(Debug, Subcommand)]
pub(super) enum ProjectsCommand {
    /// Preview or queue a correction to downloaded knowledge for owner review.
    #[command(after_help = HELP)]
    ProposeCorrection(super::laptop_corrections::CorrectionArgs),
    /// Prepare independent, browser-approved Codex/Claude disclosure identities.
    #[command(after_help = HELP)]
    Agents(agents::AgentArgs),
    /// List reviewed projects currently available to this enrolled device.
    #[command(after_help = HELP)]
    Available(CompanyArgs),
    /// Interactively review one approved project and prepare metadata only.
    #[command(after_help = HELP)]
    Select(project_picker::SelectArgs),
    /// Preview an exact reviewed project and persist a resumable local plan.
    #[command(after_help = HELP)]
    Prepare(workspace::PrepareArgs),
    /// Apply the confirmed immutable plan without replacing existing settings.
    #[command(after_help = HELP)]
    Apply(workspace::ApplyArgs),
    /// Inspect local preparation stages (does not assert current remote access).
    #[command(after_help = HELP)]
    Status(workspace::StatusArgs),
    /// Apply approved, attested pages to a separate local read-only projection.
    #[command(after_help = HELP)]
    Sync(replica::SyncArgs),
    /// Review and prepare credential-free metadata for the native provisioner.
    #[command(after_help = HELP)]
    Subscription(replica::SubscriptionArgs),
    /// Show committed cursor/freshness without claiming current remote authority.
    #[command(after_help = HELP)]
    SyncStatus(replica::SyncStatusArgs),
    /// Search only the selected project's cached, cited read-only projection.
    #[command(after_help = HELP)]
    OfflineSearch(replica::OfflineSearchArgs),
}

#[derive(Clone)]
struct CompanyOrigin(String);
impl CompanyOrigin {
    fn parse(value: &str, allow_loopback: bool) -> Result<Self> {
        if value.len() > 2048 || value.trim() != value || value.chars().any(char::is_control) {
            bail!("--company requires an exact origin without whitespace");
        }
        let tail = value
            .split_once("://")
            .map(|(_, tail)| tail)
            .ok_or_else(|| anyhow::anyhow!("--company requires an HTTPS origin"))?;
        if tail.find('/').is_some_and(|index| &tail[index..] != "/") {
            bail!("--company must not contain a path");
        }
        let url =
            Url::parse(value).map_err(|_| anyhow::anyhow!("--company is not a valid origin"))?;
        if url.host_str().is_none()
            || !url.username().is_empty()
            || url.password().is_some()
            || url.query().is_some()
            || url.fragment().is_some()
            || url.path() != "/"
        {
            bail!("--company must be an origin without credentials, path, query or fragment");
        }
        let loopback = url.host_str().is_some_and(|name| {
            matches!(name, "localhost" | "localhost.")
                || name
                    .trim_matches(['[', ']'])
                    .parse::<std::net::IpAddr>()
                    .is_ok_and(|ip| ip.is_loopback())
        });
        if loopback && !allow_loopback {
            bail!(
                "loopback company origins require --allow-loopback for explicit local development"
            );
        }
        if url.scheme() != "https" && !(allow_loopback && loopback && url.scheme() == "http") {
            bail!(
                "--company requires HTTPS; HTTP is permitted only for explicit loopback development"
            );
        }
        Ok(Self(url.origin().ascii_serialization()))
    }
    fn endpoint(&self, path: &str) -> String {
        format!("{}{path}", self.0)
    }
}

pub(super) async fn handle_command(command: &Command) -> Result<bool> {
    let store = NativeKeychain;
    match command {
        Command::Auth(args) => match &args.command {
            AuthCommand::Login(args) => {
                login(args, &store).await?;
            }
            AuthCommand::Logout(args) => {
                logout(args, &store).await?;
            }
            _ => return Ok(false),
        },
        Command::Projects(args) => match &args.command {
            ProjectsCommand::ProposeCorrection(_) => return Ok(false),
            ProjectsCommand::Agents(args) => agents::handle(args).await?,
            ProjectsCommand::Available(args) => {
                available(args, &store).await?;
            }
            ProjectsCommand::Select(args) => project_picker::run(args, &store).await?,
            ProjectsCommand::Prepare(args) => workspace::prepare(args, &store).await?,
            ProjectsCommand::Apply(args) => workspace::apply(args, &store).await?,
            ProjectsCommand::Status(args) => workspace::status(args)?,
            ProjectsCommand::Sync(args) => replica::sync(args).await?,
            ProjectsCommand::Subscription(args) => replica::subscription(args).await?,
            ProjectsCommand::SyncStatus(args) => replica::status(args).await?,
            ProjectsCommand::OfflineSearch(args) => replica::search(args).await?,
        },
        _ => return Ok(false),
    }
    Ok(true)
}

async fn login(args: &LoginArgs, store: &impl CredentialStore) -> Result<()> {
    let origin = CompanyOrigin::parse(&args.company.company, args.company.allow_loopback)?;
    let name = args.name.trim();
    if name.is_empty() || name.len() > 128 || args.name.chars().any(char::is_control) {
        bail!("--name requires 1-128 bytes without control characters");
    }
    let mut enrollment = credentials::load_or_create(store, &origin, name)?;
    if enrollment.pending_logout {
        bail!(
            "this device has a pending logout; run auth logout for the same --company before enrolling again"
        );
    }
    let remote = transport::CompanyClient::new(origin.clone())?;
    if enrollment.approved {
        println!(
            "Device {} is already enrolled for {}.",
            enrollment.device_id, origin.0
        );
        return print_projects(remote.projects(&enrollment).await?);
    }
    let started = remote.start(&enrollment).await?;
    if started.request_id != enrollment.request_id || started.poll_interval_seconds != 2 {
        bail!(
            "enrollment start receipt mismatch; rerun the same login to inspect this saved request"
        );
    }
    let approval_url = transport::validated_approval_url(
        &origin,
        &started.user_code,
        &started.verification_uri_complete,
    )?;
    println!("Device: {} ({})", enrollment.name, enrollment.device_id);
    println!(
        "Credential SHA-256: {}",
        credentials::sha256(&enrollment.credential)
    );
    println!("User code: {}", started.user_code);
    println!("Approve at: {approval_url}");
    if !args.no_browser {
        transport::open_browser(&approval_url)?;
    }
    let remaining = (started.expires_at - chrono::Utc::now())
        .to_std()
        .unwrap_or_default()
        .min(Duration::from_secs(600));
    let deadline = tokio::time::Instant::now() + remaining;
    loop {
        if tokio::time::Instant::now() >= deadline {
            bail!(
                "enrollment deadline reached; the same saved request and credentials are retained for inspection"
            );
        }
        let receipt = tokio::time::timeout_at(deadline, remote.poll(&enrollment))
            .await
            .map_err(|_| {
                anyhow::anyhow!(
                    "enrollment deadline reached; saved request and credentials retained"
                )
            })??;
        match receipt.status.as_str() {
            "approved" => {
                if receipt.device_id != Some(enrollment.device_id)
                    || receipt.principal_id.is_none_or(|id| id.is_nil())
                    || receipt.projects.as_ref().is_none_or(|projects| {
                        projects.is_empty()
                            || projects.iter().any(|project| {
                                !matches!(
                                    project.role,
                                    mem_record::AuthRole::Reader | mem_record::AuthRole::Writer
                                ) || (project.manifest.entry.area
                                    == mem_record::OnboardingArea::Trust
                                    && project.manifest.entry.offline_allowed)
                            })
                    })
                {
                    bail!(
                        "approved device identity mismatch; saved credentials retained, no local activation"
                    );
                }
                enrollment.approved = true;
                credentials::save(store, &origin, &enrollment)?;
                println!(
                    "Device approval recorded for {}. No service activation, repository clone or selective sync was performed.",
                    enrollment.device_id
                );
                return print_projects(remote.projects(&enrollment).await?);
            }
            "pending" => {
                tokio::time::sleep_until(
                    (tokio::time::Instant::now() + Duration::from_secs(2)).min(deadline),
                )
                .await;
            }
            "denied" | "expired" => bail!(
                "device enrollment {}. Saved request retained; run auth logout to clear or inspect the device request before creating another.",
                receipt.status
            ),
            _ => bail!(
                "unrecognized enrollment outcome; saved request retained, no local activation"
            ),
        }
    }
}

async fn available(args: &CompanyArgs, store: &impl CredentialStore) -> Result<()> {
    let origin = CompanyOrigin::parse(&args.company, args.allow_loopback)?;
    let enrollment = credentials::load(store, &origin)?
        .ok_or_else(|| anyhow::anyhow!("no saved company device; run memory auth login first"))?;
    if !enrollment.approved || enrollment.pending_logout {
        bail!(
            "device credential is not locally usable; complete login or resolve pending logout first"
        );
    }
    print_projects(
        transport::CompanyClient::new(origin)?
            .projects(&enrollment)
            .await?,
    )
}

async fn logout(args: &LogoutArgs, store: &impl CredentialStore) -> Result<()> {
    let origin = CompanyOrigin::parse(&args.company.company, args.company.allow_loopback)?;
    let Some(mut enrollment) = credentials::load(store, &origin)? else {
        println!("No saved company device.");
        return Ok(());
    };
    if args.forget_local_only {
        if !args.confirm {
            bail!(
                "local-only credential removal requires --forget-local-only --confirm; server revocation remains unknown"
            );
        }
        println!(
            "Warning: server cancellation and revocation are UNKNOWN. Remote device access may remain valid. An administrator must inspect and revoke the device separately if it exists."
        );
        credentials::remove(store, &origin, &enrollment)?;
        println!(
            "Forgot only the local Keychain credential for device {}. No server revocation was performed or confirmed.",
            enrollment.device_id
        );
        return Ok(());
    }
    // Disable local use durably before sending the revoke. A lost response must
    // retain the exact credential for a later server-reconciliation attempt.
    enrollment.pending_logout = true;
    credentials::save(store, &origin, &enrollment)?;
    let remote = transport::CompanyClient::new(origin.clone())?;
    let acknowledged = if enrollment.approved {
        remote.logout(&enrollment).await
    } else {
        match remote.cancel(&enrollment).await {
            Ok(true) => Ok(()),
            Ok(false) => remote.logout(&enrollment).await,
            Err(error) => Err(error),
        }
    };
    if acknowledged.is_err() {
        bail!(
            "logout is pending: local use is disabled, server revocation is unconfirmed. Keychain credential retained; retry memory auth logout with the same --company when the server is available"
        );
    }
    credentials::remove(store, &origin, &enrollment)?;
    println!(
        "Server confirmed cancellation or revocation for device {}; its Keychain credential was removed.",
        enrollment.device_id
    );
    Ok(())
}

fn print_projects(projects: transport::AvailableProjects) -> Result<()> {
    println!("{}", serde_json::to_string_pretty(&projects)?);
    println!(
        "Reviewed catalog metadata only; remote grants, installation and selective sync remain separate."
    );
    Ok(())
}
