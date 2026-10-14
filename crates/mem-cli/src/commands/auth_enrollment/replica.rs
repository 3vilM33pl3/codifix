// SPDX-License-Identifier: AGPL-3.0-or-later
//! Explicit local subscriptions. Lean clients report a missing capability, not
//! a successful sync. Production credentials remain in macOS Keychain.
use anyhow::Result;
use clap::Args;
use std::path::PathBuf;

#[derive(Debug, Args)]
pub(in crate::commands) struct SyncArgs {
    /// Private JSON profile prepared by the native laptop onboarding operator.
    #[arg(long)]
    profile: PathBuf,
    /// Stay online: sync at startup and every five minutes; Ctrl-C stops work.
    #[arg(long)]
    watch: bool,
}
#[derive(Debug, Args)]
pub(in crate::commands) struct SubscriptionArgs {
    /// Select the private full curated project replica or existing shared publications.
    #[arg(long,default_value="shared_publications_v1",value_parser=["shared_publications_v1","curated_project_v1"])]
    replication_mode: String,
    #[command(flatten)]
    company: super::CompanyArgs,
    #[arg(long)]
    project: uuid::Uuid,
    #[arg(long)]
    area: String,
    #[arg(long)]
    owner_instance: uuid::Uuid,
    #[arg(long)]
    revision: i64,
    /// Create-only metadata in an existing owner-only directory, never a repo.
    #[arg(long)]
    output: PathBuf,
    #[arg(long, requires = "confirm")]
    bootstrap_sha256: Option<String>,
    #[arg(long, requires = "bootstrap_sha256")]
    confirm: bool,
}
pub(super) async fn subscription(args: &SubscriptionArgs) -> Result<()> {
    #[cfg(feature = "project-sync")]
    {
        native::subscription(args, &super::credentials::NativeKeychain).await
    }
    #[cfg(not(feature = "project-sync"))]
    {
        let _ = args;
        anyhow::bail!("this build has no project-sync support")
    }
}
#[derive(Debug, Args)]
pub(in crate::commands) struct SyncStatusArgs {
    #[arg(long)]
    profile: PathBuf,
}
#[derive(Debug, Args)]
pub(in crate::commands) struct OfflineSearchArgs {
    #[arg(long)]
    profile: PathBuf,
    #[arg(long)]
    question: String,
    #[arg(long, default_value_t = 8)]
    limit: u32,
}
pub(super) async fn sync(args: &SyncArgs) -> Result<()> {
    #[cfg(feature = "project-sync")]
    {
        native::sync(args, &super::credentials::NativeKeychain).await
    }
    #[cfg(not(feature = "project-sync"))]
    {
        let _ = args;
        anyhow::bail!(
            "this build has no project-sync support; install the native full Memory package"
        )
    }
}
pub(super) async fn status(args: &SyncStatusArgs) -> Result<()> {
    #[cfg(feature = "project-sync")]
    {
        native::status(args).await
    }
    #[cfg(not(feature = "project-sync"))]
    {
        let _ = args;
        anyhow::bail!("this build has no project-sync support")
    }
}
pub(super) async fn search(args: &OfflineSearchArgs) -> Result<()> {
    #[cfg(feature = "project-sync")]
    {
        native::search(args).await
    }
    #[cfg(not(feature = "project-sync"))]
    {
        let _ = args;
        anyhow::bail!("this build has no project-sync support")
    }
}

#[cfg(feature = "project-sync")]
mod native {
    use super::*;
    use crate::commands::auth_enrollment::{
        CompanyOrigin,
        credentials::{self, CredentialStore, Enrollment},
        transport::CompanyClient,
    };
    use anyhow::bail;
    use mem_federation_runtime::{
        Endpoint, LocalProjectReplicaProfile, PreparedProjectSubscription, ProjectReplica,
        ProjectSyncHealth, ScopedDeviceClient,
    };
    use mem_record::OnboardingDeviceReviewProject;

    pub(super) async fn subscription(
        args: &SubscriptionArgs,
        store: &impl CredentialStore,
    ) -> Result<()> {
        let mode = if args.replication_mode == "curated_project_v1" {
            mem_federation_runtime::ProjectReplicationMode::CuratedProjectV1
        } else {
            mem_federation_runtime::ProjectReplicationMode::SharedPublicationsV1
        };
        if args.project.is_nil()
            || args.owner_instance.is_nil()
            || args.revision <= 0
            || !matches!(args.area.as_str(), "workshop" | "homelab" | "general")
            || !args.output.is_absolute()
            || args.output.file_name().is_none()
        {
            bail!(
                "exact offline area/project/owner/revision and absolute private metadata output required"
            );
        }
        let origin = CompanyOrigin::parse(&args.company.company, args.company.allow_loopback)?;
        let enrollment = credentials::load(store, &origin)?
            .ok_or_else(|| anyhow::anyhow!("sign in to this company first"))?;
        if !enrollment.approved || enrollment.pending_logout {
            bail!("approved device without pending logout required");
        }
        let client = CompanyClient::new(origin.clone())?;
        let catalog = client.projects(&enrollment).await?;
        let matching: Vec<_> = catalog
            .projects
            .iter()
            .filter(|p| {
                p.manifest.project_id == args.project
                    && p.manifest.entry.area.as_str() == args.area
                    && p.manifest.entry.owner_instance_id == args.owner_instance
            })
            .collect();
        if matching.len() != 1
            || matching[0].manifest.revision != args.revision
            || !matching[0].offline_available
            || !matching[0].manifest.entry.offline_allowed
        {
            bail!("exact currently approved offline manifest required");
        }
        let bootstrap = client
            .subscription(&enrollment, args.project, &args.area, args.owner_instance)
            .await?;
        bootstrap.validate()?;
        if bootstrap.device != enrollment.device_id
            || bootstrap.scope.project != args.project
            || bootstrap.scope.area != args.area
            || bootstrap.scope.owner.as_uuid() != args.owner_instance
            || u64::try_from(args.revision).ok() != Some(bootstrap.catalog_revision)
        {
            bail!("control-plane subscription does not match the selected device/manifest");
        }
        use sha2::{Digest, Sha256};
        let digest = format!(
            "{:x}",
            Sha256::digest(if mode.is_shared() {
                serde_json::to_vec(&(&origin.0, &bootstrap))?
            } else {
                serde_json::to_vec(&(&origin.0, &bootstrap, mode))?
            })
        );
        let manifest_sha256 = (!mode.is_shared())
            .then(|| mem_federation_runtime::onboarding_manifest_digest(&matching[0].manifest))
            .transpose()?;
        let mut preview = serde_json::json!({"company":origin.0,
            "bootstrap":bootstrap,"bootstrap_sha256":digest,"output":args.output,
            "contains_database_credentials":false,"creates_services":false,"downloads_knowledge":false});
        if !mode.is_shared() {
            preview["replication_mode"] = serde_json::to_value(mode)?;
            preview["manifest_sha256"] = serde_json::to_value(&manifest_sha256)?;
        }
        println!("{}", serde_json::to_string_pretty(&preview)?);
        if !args.confirm {
            return Ok(());
        }
        if args.bootstrap_sha256.as_deref() != Some(digest.as_str()) {
            bail!("bootstrap confirmation differs; no metadata written");
        }
        let existing = match std::fs::symlink_metadata(&args.output) {
            Ok(_) => Some(PreparedProjectSubscription::read(&args.output)?),
            Err(error) if error.kind() == std::io::ErrorKind::NotFound => None,
            Err(_) => bail!("subscription metadata unavailable; existing files preserved"),
        };
        if let Some(existing) = existing {
            if existing.company != origin.0
                || existing.replication_mode != mode
                || existing.manifest_sha256 != manifest_sha256
                || existing.bootstrap != bootstrap
                || existing.allow_loopback_development != args.company.allow_loopback
            {
                bail!(
                    "existing subscription metadata differs; review a new output rather than replacing it"
                );
            }
            println!(
                "Subscription {} already prepared; unchanged.",
                existing.subscription
            );
        } else {
            let prepared = PreparedProjectSubscription {
                manifest_sha256,
                replication_mode: mode,
                schema_version: 1,
                company: origin.0,
                subscription: uuid::Uuid::new_v4(),
                bootstrap,
                allow_loopback_development: args.company.allow_loopback,
            };
            prepared.create(&args.output)?;
            println!(
                "Prepared subscription {}. No services or database credentials created.",
                prepared.subscription
            );
        }
        Ok(())
    }

    async fn replica(
        profile: &LocalProjectReplicaProfile,
        initialize: bool,
    ) -> Result<ProjectReplica> {
        ProjectReplica::connect(profile, initialize).await.map_err(|_|anyhow::anyhow!("dedicated local subscription unavailable or binding changed; provision the laptop schema and inspect the profile. No fallback or migration performed"))
    }
    fn current_project(
        profile: &LocalProjectReplicaProfile,
        enrollment: &Enrollment,
        projects: &[OnboardingDeviceReviewProject],
    ) -> Result<()> {
        if !enrollment.approved
            || enrollment.pending_logout
            || enrollment.device_id != profile.bootstrap.device
        {
            bail!(
                "approved Keychain device does not match this subscription; no network transfer permitted"
            );
        }
        let matching: Vec<_> = projects
            .iter()
            .filter(|p| {
                p.manifest.project_id == profile.bootstrap.scope.project
                    && p.manifest.entry.area.as_str() == profile.bootstrap.scope.area
                    && p.manifest.entry.owner_instance_id == profile.bootstrap.scope.owner.as_uuid()
            })
            .collect();
        if matching.len() != 1
            || !matching[0].offline_available
            || !matching[0].manifest.entry.offline_allowed
            || u64::try_from(matching[0].manifest.revision).ok()
                != Some(profile.bootstrap.catalog_revision)
        {
            bail!(
                "project's current approved offline scope differs; refresh the reviewed subscription before retrying"
            );
        }
        Ok(())
    }
    fn current_bootstrap(
        profile: &LocalProjectReplicaProfile,
        current: &mem_federation_runtime::DeviceProjectSubscription,
    ) -> Result<()> {
        current.validate()?;
        if current != &profile.bootstrap {
            bail!(
                "subscription authority, review or receipt keys changed; explicit profile renewal required"
            );
        }
        Ok(())
    }
    pub(super) async fn sync(args: &SyncArgs, store: &impl CredentialStore) -> Result<()> {
        let profile = LocalProjectReplicaProfile::read(&args.profile)?;
        let origin = CompanyOrigin::parse(&profile.company, profile.allow_loopback_development)?;
        let enrollment = credentials::load(store, &origin)?
            .ok_or_else(|| anyhow::anyhow!("run memory auth login for this company first"))?;
        // Reject a wrong/unapproved local device before even inventory/bootstrap.
        if enrollment.device_id != profile.bootstrap.device
            || !enrollment.approved
            || enrollment.pending_logout
        {
            bail!("Keychain device does not match this approved subscription");
        }
        let company = CompanyClient::new(origin.clone())?;
        let projects = company.projects(&enrollment).await?;
        current_project(&profile, &enrollment, &projects.projects)?;
        let current = company
            .subscription(
                &enrollment,
                profile.bootstrap.scope.project,
                &profile.bootstrap.scope.area,
                profile.bootstrap.scope.owner.as_uuid(),
            )
            .await?;
        current_bootstrap(&profile, &current)?;
        let client = ScopedDeviceClient::for_scope(
            Endpoint::new(&origin.0, profile.allow_loopback_development)?,
            enrollment.credential,
            profile.bootstrap.scope.clone(),
        )?;
        if !profile.replication_mode.is_shared() {
            let replica =
                mem_federation_runtime::DeviceProjectReplica::connect(&profile, true).await?;
            loop {
                let result = replica.synchronize(&client, 128).await;
                let failed = result.is_err();
                let delay = match result {
                    Ok(progress) => {
                        let caught_up = progress.caught_up;
                        println!("{}", serde_json::to_string(&progress)?);
                        if caught_up { 300 } else { 1 }
                    }
                    Err(error) => {
                        if !args.watch {
                            return Err(error.into());
                        }
                        eprintln!(
                            "curated project synchronization unavailable; committed cursor retained"
                        );
                        30
                    }
                };
                if !args.watch {
                    return Ok(());
                }
                // Queued work accelerates healthy synchronization, not retry loops.
                let wait = async {
                    if failed {
                        tokio::time::sleep(std::time::Duration::from_secs(delay)).await;
                        Ok(())
                    } else {
                        replica
                            .wait_for_work(std::time::Duration::from_secs(delay))
                            .await
                    }
                };
                tokio::select! {result=wait=>{result?;},signal=tokio::signal::ctrl_c()=>{signal?;return Ok(());}}
            }
        }
        let replica = replica(&profile, true).await?;
        if !args.watch {
            let progress = replica.synchronize(&client, 128).await?;
            println!("{}", serde_json::to_string_pretty(&progress)?);
            return Ok(());
        }
        let (stop, stopping) = tokio::sync::watch::channel(false);
        let (health, mut observed) = tokio::sync::watch::channel(ProjectSyncHealth::Unknown);
        let wake = tokio::sync::Notify::new();
        let worker = replica.watch(&client, stopping, &wake, health);
        tokio::pin!(worker);
        loop {
            tokio::select! {
                _=&mut worker=>return Ok(()),
                changed=observed.changed()=>{
                    if changed.is_ok() {
                        let health=observed.borrow_and_update().clone();
                        println!("{}",serde_json::to_string(&health)?);
                    }
                },
                signal=tokio::signal::ctrl_c()=>{
                    signal.map_err(|_|anyhow::anyhow!("could not receive shutdown signal"))?;
                    stop.send_replace(true);
                    worker.await;
                    return Ok(());
                }
            }
        }
    }
    pub(super) async fn status(args: &SyncStatusArgs) -> Result<()> {
        let profile = LocalProjectReplicaProfile::read(&args.profile)?;
        let (status, uploads, sync_error) = if profile.replication_mode.is_shared() {
            (replica(&profile, false).await?.status().await?, None, None)
        } else {
            let replica =
                mem_federation_runtime::DeviceProjectReplica::connect(&profile, false).await?;
            (
                replica.status().await?,
                Some(replica.uploads().await?),
                replica.last_error().await?,
            )
        };
        println!(
            "{}",
            serde_json::to_string_pretty(&serde_json::json!({
                "area":profile.bootstrap.scope.area,"project":profile.bootstrap.scope.project,
                "owner":profile.bootstrap.scope.owner,"subscription":profile.subscription,
                "cursor":status.cursor,"last_committed_page":status.observed_at,
                "uploads":uploads,"sync_error":sync_error,"replication_mode":profile.replication_mode,
                "current_remote_access":"unknown_offline_observation_only",
                "stale":status.observed_at.is_none_or(|t|chrono::Utc::now()-t>chrono::Duration::minutes(5))
            }))?
        );
        Ok(())
    }
    pub(super) async fn search(args: &OfflineSearchArgs) -> Result<()> {
        if args.question.trim().is_empty()
            || args.question.len() > 4096
            || !(1..=32).contains(&args.limit)
        {
            bail!("offline search requires a nonempty question up to4096 bytes and limit1..32");
        }
        let profile = LocalProjectReplicaProfile::read(&args.profile)?;
        let results = if profile.replication_mode.is_shared() {
            serde_json::to_value(
                replica(&profile, false)
                    .await?
                    .search(&args.question, args.limit)
                    .await?,
            )?
        } else {
            serde_json::to_value(
                mem_federation_runtime::DeviceProjectReplica::connect(&profile, false)
                    .await?
                    .search(&args.question, args.limit)
                    .await?,
            )?
        };
        println!(
            "{}",
            serde_json::to_string_pretty(&serde_json::json!({
                "read_only":true,"remote_authority_currently_verified":false,
                "area":profile.bootstrap.scope.area,"project":profile.bootstrap.scope.project,"results":results
            }))?
        );
        Ok(())
    }
    #[cfg(test)]
    mod tests {
        use super::*;
        use mem_record::{AuthRole, OnboardingArea, OnboardingProject, OnboardingProjectEntry};
        use uuid::Uuid;
        fn fixture() -> (
            LocalProjectReplicaProfile,
            Enrollment,
            OnboardingDeviceReviewProject,
        ) {
            let security =
                mem_federation_runtime::GatewaySecurity::initial(&[21; 32], Default::default())
                    .unwrap();
            let device = Uuid::new_v4();
            let project = Uuid::new_v4();
            let owner = Uuid::new_v4();
            let bootstrap=serde_json::from_value(serde_json::json!({"schema_version":1,"device":device,"catalog_revision":1,
                "authority":Uuid::new_v4(),"receipt_keys":security.trust().keys(),
                "scope":{"company":Uuid::new_v4(),"area":"workshop","owner":owner,"project":project,"space":Uuid::new_v4()}})).unwrap();
            let profile = LocalProjectReplicaProfile {
                manifest_sha256: None,
                local_project: None,
                replication_mode:
                    mem_federation_runtime::ProjectReplicationMode::SharedPublicationsV1,
                schema_version: 1,
                company: "https://company.example.invalid".into(),
                subscription: Uuid::new_v4(),
                local_database_url: "postgresql://laptop@127.0.0.1:55432/memory_laptop_workshop"
                    .into(),
                bootstrap,
                allow_loopback_development: false,
            };
            let enrollment = Enrollment {
                company: profile.company.clone(),
                device_id: device,
                request_id: Uuid::new_v4(),
                name: "Synthetic Mac".into(),
                platform: "macos".into(),
                credential: format!("mlt_{}", "a".repeat(64)),
                poll_secret: "b".repeat(64),
                approved: true,
                pending_logout: false,
            };
            let entry = OnboardingDeviceReviewProject {
                manifest: OnboardingProject {
                    project_id: project,
                    project: "same-name".into(),
                    revision: 1,
                    reviewed_by: Uuid::new_v4(),
                    reviewed_at: chrono::Utc::now(),
                    entry: OnboardingProjectEntry {
                        area: OnboardingArea::Workshop,
                        owner_instance_id: owner,
                        repository_url: "https://git.example.invalid/project.git".into(),
                        default_branch: "main".into(),
                        offline_allowed: true,
                        tools: vec![],
                        destinations: vec![],
                    },
                },
                allowed_roles: vec![AuthRole::Reader],
                offline_available: true,
            };
            (profile, enrollment, entry)
        }
        #[test]
        fn subscription_preview_and_confirmation_require_exact_selection() {
            use crate::commands::runtime::Cli;
            use clap::Parser;
            let project = Uuid::new_v4().to_string();
            let owner = Uuid::new_v4().to_string();
            let mut args = vec![
                "memory",
                "projects",
                "subscription",
                "--company",
                "https://company.example.invalid",
                "--project",
                &project,
                "--area",
                "workshop",
                "--owner-instance",
                &owner,
                "--revision",
                "1",
                "--output",
                "/private/subscription.json",
            ];
            assert!(Cli::try_parse_from(args.clone()).is_ok());
            args.push("--confirm");
            assert!(Cli::try_parse_from(args.clone()).is_err());
            args.extend(["--bootstrap-sha256", "0123456789"]);
            assert!(Cli::try_parse_from(args).is_ok());
            assert!(
                Cli::try_parse_from([
                    "memory",
                    "projects",
                    "subscription",
                    "--company",
                    "https://company.example.invalid"
                ])
                .is_err()
            );
        }
        #[test]
        fn offline_scope_device_catalog_and_consent_are_checked_before_initialization() {
            let (profile, enrollment, entry) = fixture();
            assert!(current_project(&profile, &enrollment, std::slice::from_ref(&entry)).is_ok());
            for change in [
                (|p: &mut OnboardingDeviceReviewProject| {
                    p.manifest.entry.area = OnboardingArea::General
                }) as fn(&mut OnboardingDeviceReviewProject),
                |p| p.manifest.entry.owner_instance_id = Uuid::new_v4(),
                |p| p.manifest.project_id = Uuid::new_v4(),
                |p| p.manifest.revision += 1,
                |p| p.offline_available = false,
                |p| p.manifest.entry.offline_allowed = false,
            ] {
                let mut changed = entry.clone();
                change(&mut changed);
                assert!(current_project(&profile, &enrollment, &[changed]).is_err());
            }
            assert!(
                current_project(&profile, &enrollment, &[entry.clone(), entry.clone()]).is_err()
            );
            let mut device = enrollment;
            device.device_id = Uuid::new_v4();
            assert!(current_project(&profile, &device, &[entry]).is_err());
        }
        #[test]
        fn bootstrap_is_control_plane_pinned_and_not_silently_updated() {
            let (profile, _, _) = fixture();
            assert!(current_bootstrap(&profile, &profile.bootstrap).is_ok());
            for change in [
                (|p: &mut mem_federation_runtime::DeviceProjectSubscription| {
                    p.device = Uuid::new_v4()
                }) as fn(&mut mem_federation_runtime::DeviceProjectSubscription),
                |p| p.catalog_revision += 1,
                |p| p.scope.company = Uuid::new_v4(),
                |p| p.scope.project = Uuid::new_v4(),
                |p| p.scope.area = "general".into(),
                |p| {
                    p.receipt_keys = mem_federation_runtime::GatewaySecurity::initial(
                        &[22; 32],
                        Default::default(),
                    )
                    .unwrap()
                    .trust()
                    .keys()
                    .to_vec()
                },
            ] {
                let mut changed = profile.bootstrap.clone();
                change(&mut changed);
                assert!(current_bootstrap(&profile, &changed).is_err());
            }
        }
    }
}
