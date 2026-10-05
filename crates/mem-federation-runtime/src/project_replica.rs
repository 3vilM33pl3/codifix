// SPDX-License-Identifier: AGPL-3.0-or-later
//! Laptop selective replica: device credentials contact only a configured owner,
//! never the gateway/PDS. Trust and scope are installed by a trusted operator,
//! not learned from a response. No local authoritative memory is overwritten.
use crate::{Endpoint, ReceiptTrust, Result, RuntimeError, SignedProjectPage};
use chrono::{DateTime, Utc};
use mem_federation::{publication::InstanceId, subscription::ProjectScope};
use mem_federation_store::{
    ProjectProjectionStatus, ProjectProjectionStore, ProjectSearchHit, ReceiptKey,
};
use mem_record::OnboardingProjectSnapshotRequest;
use serde::{Deserialize, Serialize};
use sqlx::PgPool;
use std::{future::Future, pin::Pin, time::Duration};
use tokio::sync::watch;
use uuid::Uuid;

pub type ProjectDeviceFuture<'a> =
    Pin<Box<dyn Future<Output = Result<SignedProjectPage>> + Send + 'a>>;
pub trait ProjectDeviceTransport: Send + Sync {
    fn snapshot<'a>(
        &'a self,
        project: Uuid,
        request: &'a OnboardingProjectSnapshotRequest,
    ) -> ProjectDeviceFuture<'a>;
}

/// The enrolled credential comes from Keychain; this type has no Debug and
/// never prints credentials or provider/server response bodies.
pub struct ScopedDeviceClient {
    endpoint: Endpoint,
    credential: String,
    scope: Option<ProjectScope>,
}

#[cfg(test)]
mod scoped_client_tests {
    use super::*;
    #[test]
    fn scoped_address_is_exact_and_never_falls_back_to_uuid_only() {
        use mem_federation::publication::SpaceId;
        let scope = ProjectScope {
            company: Uuid::new_v4(),
            area: "workshop".into(),
            owner: InstanceId::new(Uuid::new_v4()).expect("non-nil fixture owner"),
            project: Uuid::new_v4(),
            space: SpaceId::new(Uuid::new_v4()).expect("non-nil fixture space"),
        };
        let token = format!("mlt_{}", "a".repeat(64));
        let client = ScopedDeviceClient::for_scope(
            Endpoint::new("http://127.0.0.1:4250", true).expect("explicit loopback fixture"),
            token.clone(),
            scope.clone(),
        )
        .expect("valid exact scoped client");
        assert_eq!(
            client
                .snapshot_path(scope.project)
                .expect("selected project"),
            format!(
                "/v1/onboarding/areas/workshop/owners/{}/projects/{}/snapshot",
                scope.owner.as_uuid(),
                scope.project
            )
        );
        assert!(client.snapshot_path(Uuid::new_v4()).is_err());
        let mut invalid = scope.clone();
        invalid.area = "trust".into();
        assert!(
            ScopedDeviceClient::for_scope(
                Endpoint::new("http://127.0.0.1:4250", true).expect("explicit loopback fixture"),
                token.clone(),
                invalid
            )
            .is_err()
        );
        let legacy = ScopedDeviceClient::new(
            Endpoint::new("http://127.0.0.1:4250", true).expect("explicit loopback fixture"),
            token,
        )
        .expect("valid legacy client");
        assert_eq!(
            legacy.snapshot_path(scope.project).expect("legacy project"),
            format!("/v1/onboarding/projects/{}/snapshot", scope.project)
        );
    }
}
impl ScopedDeviceClient {
    pub fn new(endpoint: Endpoint, credential: String) -> Result<Self> {
        if credential.len() != 68
            || !credential.starts_with("mlt_")
            || !credential.as_bytes()[4..].iter().all(u8::is_ascii_hexdigit)
        {
            return Err(RuntimeError::Configuration);
        }
        Ok(Self {
            endpoint,
            credential,
            scope: None,
        })
    }
    /// New clients address the complete reviewed identity. Preserve the old
    /// single-owner transport for existing profiles/callers, never infer an
    /// owner from a project name or silently fall back after a scoped failure.
    pub fn for_scope(endpoint: Endpoint, credential: String, scope: ProjectScope) -> Result<Self> {
        if !scope.valid() {
            return Err(RuntimeError::Configuration);
        }
        let mut client = Self::new(endpoint, credential)?;
        client.scope = Some(scope);
        Ok(client)
    }
    fn snapshot_path(&self, project: Uuid) -> Result<String> {
        match &self.scope {
            Some(scope) if scope.project == project => Ok(format!(
                "/v1/onboarding/areas/{}/owners/{}/projects/{project}/snapshot",
                scope.area,
                scope.owner.as_uuid()
            )),
            Some(_) => Err(RuntimeError::Configuration),
            None => Ok(format!("/v1/onboarding/projects/{project}/snapshot")),
        }
    }
}
impl ProjectDeviceTransport for ScopedDeviceClient {
    fn snapshot<'a>(
        &'a self,
        project: Uuid,
        request: &'a OnboardingProjectSnapshotRequest,
    ) -> ProjectDeviceFuture<'a> {
        Box::pin(async move {
            if project.is_nil()
                || request.nonce.is_nil()
                || !(1..=32).contains(&request.limit)
                || request.after > i64::MAX as u64
                || request
                    .through
                    .is_some_and(|n| n < request.after || n > i64::MAX as u64)
            {
                return Err(RuntimeError::Configuration);
            }
            let response = self
                .endpoint
                .client
                .post(self.endpoint.url(&self.snapshot_path(project)?)?)
                .bearer_auth(&self.credential)
                .json(request)
                .send()
                .await
                .map_err(|_| RuntimeError::Unavailable)?;
            match response.status().as_u16() {
                200..=299 => crate::http::json(response, 4 * 1024 * 1024).await,
                401 | 403 => Err(RuntimeError::Unauthorized),
                409 => Err(mem_federation_store::StoreError::Resync.into()),
                _ => Err(RuntimeError::Unavailable),
            }
        })
    }
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct ProjectReplicaBinding {
    pub subscription: Uuid,
    pub device: Uuid,
    pub scope: ProjectScope,
    pub catalog_revision: u64,
    pub authority: InstanceId,
}
/// Public trust bootstrap returned only by the authenticated, operator-pinned
/// company/owner control plane. Never accept this object from a content page.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct DeviceProjectSubscription {
    pub schema_version: u32,
    pub device: Uuid,
    pub scope: ProjectScope,
    pub catalog_revision: u64,
    pub authority: InstanceId,
    pub receipt_keys: Vec<ReceiptKey>,
}
impl DeviceProjectSubscription {
    pub fn validate(&self) -> Result<ReceiptTrust> {
        if self.schema_version != 1
            || !self.scope.valid()
            || self.device.is_nil()
            || self.catalog_revision == 0
            || self.receipt_keys.len() > 32
        {
            return Err(RuntimeError::Configuration);
        }
        ReceiptTrust::new(self.receipt_keys.clone())
    }
    pub fn binding(&self, subscription: Uuid) -> ProjectReplicaBinding {
        ProjectReplicaBinding {
            subscription,
            device: self.device,
            scope: self.scope.clone(),
            catalog_revision: self.catalog_revision,
            authority: self.authority,
        }
    }
}
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct ProjectSyncProgress {
    pub cursor: u64,
    pub through: u64,
    pub pages: u32,
    pub caught_up: bool,
    pub observed_at: DateTime<Utc>,
}
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum ProjectSyncFailure {
    AccessDenied,
    ResynchronizationRequired,
    InvalidAttestation,
    Unavailable,
    LocalStorage,
}
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(tag = "status", rename_all = "snake_case")]
pub enum ProjectSyncHealth {
    Unknown,
    Syncing,
    Observed {
        progress: ProjectSyncProgress,
    },
    Failed {
        reason: ProjectSyncFailure,
        observed_at: DateTime<Utc>,
    },
}

pub struct ProjectReplica {
    binding: ProjectReplicaBinding,
    projection: ProjectProjectionStore,
    trust: ReceiptTrust,
}
impl ProjectReplica {
    /// Native client connection stays in the runtime, not the CLI boundary.
    /// This opens only a validated loopback replica and never migrates it.
    pub async fn connect(
        profile: &crate::LocalProjectReplicaProfile,
        initialize: bool,
    ) -> Result<Self> {
        profile.validate()?;
        let pool = sqlx::postgres::PgPoolOptions::new()
            .max_connections(3)
            .acquire_timeout(Duration::from_secs(5))
            .connect(&profile.local_database_url)
            .await
            .map_err(|_| RuntimeError::Unavailable)?;
        let exists: bool =
            sqlx::query_scalar("SELECT to_regclass('onboarding_project_projections') IS NOT NULL")
                .fetch_one(&pool)
                .await
                .map_err(|_| RuntimeError::Unavailable)?;
        if !exists {
            return Err(RuntimeError::Configuration);
        }
        let binding = profile.bootstrap.binding(profile.subscription);
        let trust = profile.bootstrap.validate()?;
        if initialize {
            Self::initialize(pool, binding, trust).await
        } else {
            Self::open(pool, binding, trust).await
        }
    }

    pub async fn open(
        pool: PgPool,
        binding: ProjectReplicaBinding,
        trust: ReceiptTrust,
    ) -> Result<Self> {
        if binding.catalog_revision == 0 {
            return Err(RuntimeError::Configuration);
        }
        let projection = ProjectProjectionStore::open(
            pool,
            binding.subscription,
            binding.device,
            binding.scope.clone(),
        )
        .await?;
        Ok(Self {
            binding,
            projection,
            trust,
        })
    }
    pub async fn initialize(
        pool: PgPool,
        binding: ProjectReplicaBinding,
        trust: ReceiptTrust,
    ) -> Result<Self> {
        if !binding.scope.valid()
            || binding.device.is_nil()
            || binding.subscription.is_nil()
            || binding.catalog_revision == 0
        {
            return Err(RuntimeError::Configuration);
        }
        let projection = ProjectProjectionStore::initialize(
            pool,
            binding.subscription,
            binding.device,
            binding.scope.clone(),
        )
        .await?;
        Ok(Self {
            binding,
            projection,
            trust,
        })
    }
    pub async fn status(&self) -> Result<ProjectProjectionStatus> {
        Ok(self.projection.status().await?)
    }
    pub async fn search(&self, query: &str, limit: u32) -> Result<Vec<ProjectSearchHit>> {
        Ok(self.projection.search(query, limit).await?)
    }
    /// One bounded consistent snapshot/delta pass. A changed policy interrupts
    /// the pass; already committed pages remain recoverable. The next pass starts
    /// from the committed cursor and obtains a fresh high-water mark.
    pub async fn synchronize(
        &self,
        transport: &impl ProjectDeviceTransport,
        max_pages: u32,
    ) -> Result<ProjectSyncProgress> {
        if !(1..=128).contains(&max_pages) {
            return Err(RuntimeError::Configuration);
        }
        let mut cursor = self.projection.cursor().await?;
        let mut through = None;
        let mut revisions = None;
        for index in 0..max_pages {
            let request = OnboardingProjectSnapshotRequest {
                nonce: Uuid::new_v4(),
                after: cursor,
                through,
                limit: 32,
            };
            let signed = transport
                .snapshot(self.binding.scope.project, &request)
                .await?;
            let expected = &signed.claims.page.request;
            if expected.scope != self.binding.scope
                || expected.device != self.binding.device
                || expected.catalog_revision != self.binding.catalog_revision
                || expected.nonce != request.nonce
                || expected.after != request.after
                || expected.through != request.through
                || expected.limit != request.limit
                || revisions
                    .is_some_and(|r| r != (expected.policy_revision, expected.binding_revision))
            {
                return Err(RuntimeError::InvalidResponse);
            }
            let expected = expected.clone();
            let page = self
                .trust
                .verify_project_page(signed, &expected, self.binding.authority)?;
            revisions = Some((expected.policy_revision, expected.binding_revision));
            through = Some(page.through);
            cursor = self.projection.apply_verified(&page).await?;
            let caught_up = cursor == page.through;
            if caught_up || index + 1 == max_pages {
                return Ok(ProjectSyncProgress {
                    cursor,
                    through: page.through,
                    pages: index + 1,
                    caught_up,
                    observed_at: Utc::now(),
                });
            }
        }
        Err(RuntimeError::InvalidResponse)
    }

    /// Startup + five-minute polling; transient failure retries after 30 seconds.
    /// The host may request an immediate reconnect pass through `wake`. Shutdown
    /// cancels network work; only already committed pages survive cancellation.
    /// Health is observation-time metadata, not durable entitlement or consent.
    pub async fn watch(
        &self,
        transport: &impl ProjectDeviceTransport,
        mut shutdown: watch::Receiver<bool>,
        wake: &tokio::sync::Notify,
        health: watch::Sender<ProjectSyncHealth>,
    ) {
        loop {
            if *shutdown.borrow() {
                return;
            }
            health.send_replace(ProjectSyncHealth::Syncing);
            let result = tokio::select! {
                result = self.synchronize(transport, 128) => result,
                _ = shutdown.changed() => {
                    if *shutdown.borrow() || shutdown.has_changed().is_err() { return; }
                    continue;
                }
            };
            let delay = match result {
                Ok(progress) => {
                    let caught_up = progress.caught_up;
                    health.send_replace(ProjectSyncHealth::Observed { progress });
                    if caught_up {
                        Duration::from_secs(300)
                    } else {
                        Duration::from_secs(1)
                    }
                }
                Err(error) => {
                    let reason = classify(&error);
                    let denied = reason == ProjectSyncFailure::AccessDenied;
                    health.send_replace(ProjectSyncHealth::Failed {
                        reason,
                        observed_at: Utc::now(),
                    });
                    if denied {
                        Duration::from_secs(300)
                    } else {
                        Duration::from_secs(30)
                    }
                }
            };
            tokio::select! {
                _ = tokio::time::sleep(delay) => {},
                _ = wake.notified() => {},
                _ = shutdown.changed() => {
                    if *shutdown.borrow() || shutdown.has_changed().is_err() { return; }
                }
            }
        }
    }
}
fn classify(error: &RuntimeError) -> ProjectSyncFailure {
    match error {
        RuntimeError::Unauthorized => ProjectSyncFailure::AccessDenied,
        RuntimeError::Store(mem_federation_store::StoreError::Resync) => {
            ProjectSyncFailure::ResynchronizationRequired
        }
        RuntimeError::InvalidResponse | RuntimeError::Configuration => {
            ProjectSyncFailure::InvalidAttestation
        }
        RuntimeError::Unavailable => ProjectSyncFailure::Unavailable,
        RuntimeError::Store(_) => ProjectSyncFailure::LocalStorage,
    }
}
