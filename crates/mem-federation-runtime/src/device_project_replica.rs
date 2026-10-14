// SPDX-License-Identifier: AGPL-3.0-or-later
//! Device-private curated projection; deliberately separate from published AT records.
use crate::{
    LocalProjectReplicaProfile, ProjectReplicaBinding, ProjectSyncProgress, ReceiptTrust, Result,
    RuntimeError, ScopedDeviceClient, SignedDeviceReplicaPage,
};
use mem_federation::device_replica::*;
use mem_federation_store::{DeviceMemoryHit, DeviceProjectionStore, ProjectProjectionStatus};
use uuid::Uuid;

pub struct DeviceProjectReplica {
    binding: ProjectReplicaBinding,
    projection: DeviceProjectionStore,
    trust: ReceiptTrust,
    pool: sqlx::PgPool,
    contribution: Option<mem_record::LaptopContributionBinding>,
}
impl DeviceProjectReplica {
    pub async fn connect(profile: &LocalProjectReplicaProfile, initialize: bool) -> Result<Self> {
        profile.validate()?;
        if profile.replication_mode.is_shared() {
            return Err(RuntimeError::Configuration);
        }
        let pool = sqlx::postgres::PgPoolOptions::new()
            .max_connections(3)
            .acquire_timeout(std::time::Duration::from_secs(5))
            .connect(&profile.local_database_url)
            .await
            .map_err(|_| RuntimeError::Unavailable)?;
        let binding = profile.bootstrap.binding(profile.subscription);
        let projection = if initialize {
            DeviceProjectionStore::initialize(
                pool.clone(),
                binding.subscription,
                binding.device,
                binding.scope.clone(),
            )
            .await?
        } else {
            DeviceProjectionStore::open(
                pool.clone(),
                binding.subscription,
                binding.device,
                binding.scope.clone(),
            )
            .await?
        };
        let contribution = if let Some(local) = &profile.local_project {
            let scope = &binding.scope;
            let contribution = mem_record::LaptopContributionBinding {
                scope: mem_record::LaptopContributionScope {
                    company_id: scope.company,
                    area: serde_json::from_value(serde_json::Value::String(scope.area.clone()))
                        .map_err(|_| RuntimeError::Configuration)?,
                    owner_instance_id: scope.owner.as_uuid(),
                    project_id: scope.project,
                    space_id: scope.space.as_uuid(),
                },
                device_id: binding.device,
                manifest_revision: i64::try_from(binding.catalog_revision)
                    .map_err(|_| RuntimeError::Configuration)?,
                manifest_sha256: profile
                    .manifest_sha256
                    .clone()
                    .ok_or(RuntimeError::Configuration)?,
            };
            if initialize {
                let mut tx = pool.begin().await.map_err(|_| RuntimeError::Unavailable)?;
                // Exact project identity/path only. Never repoint an existing repository.
                let result=sqlx::query("INSERT INTO projects(id,slug,name,root_path) VALUES($1,$2,$2,$3) ON CONFLICT(id) DO UPDATE SET name=projects.name WHERE projects.slug=EXCLUDED.slug AND projects.root_path=EXCLUDED.root_path")
                    .bind(scope.project).bind(&local.slug).bind(local.repository.to_string_lossy().as_ref()).execute(&mut *tx).await.map_err(|_|RuntimeError::Configuration)?;
                if result.rows_affected() != 1 {
                    return Err(RuntimeError::Configuration);
                }
                mem_store::device_contributions::bind_project(
                    &mut tx,
                    scope.project,
                    &contribution,
                )
                .await
                .map_err(|_| RuntimeError::Configuration)?;
                tx.commit().await.map_err(|_| RuntimeError::Unavailable)?;
            }
            Some(contribution)
        } else {
            None
        };
        Ok(Self {
            binding,
            projection,
            trust: profile.bootstrap.validate()?,
            pool,
            contribution,
        })
    }
    pub async fn status(&self) -> Result<ProjectProjectionStatus> {
        Ok(self.projection.status().await?)
    }
    /// Metadata only; frozen payloads and provider content never enter status.
    pub async fn uploads(&self) -> Result<std::collections::BTreeMap<String, i64>> {
        let Some(binding) = &self.contribution else {
            return Ok(Default::default());
        };
        let counts: Vec<(String, i64)> = sqlx::query_as(
            "SELECT state,count(*) FROM laptop_curated_outbox WHERE binding=$1 GROUP BY state",
        )
        .bind(sqlx::types::Json(binding))
        .fetch_all(&self.pool)
        .await
        .map_err(|_| RuntimeError::Unavailable)?;
        Ok(counts.into_iter().collect())
    }
    /// Poll persisted intent so curation in another process wakes synchronization.
    pub async fn wait_for_work(&self, maximum: std::time::Duration) -> Result<()> {
        let end = tokio::time::Instant::now() + maximum;
        loop {
            tokio::time::sleep_until(
                end.min(tokio::time::Instant::now() + std::time::Duration::from_secs(2)),
            )
            .await;
            if tokio::time::Instant::now() >= end {
                return Ok(());
            }
            if self.uploads().await?.get("queued").copied().unwrap_or(0) > 0 {
                return Ok(());
            }
        }
    }
    pub async fn search(&self, query: &str, limit: u32) -> Result<Vec<DeviceMemoryHit>> {
        Ok(self.projection.search(query, limit).await?)
    }
    pub async fn synchronize(
        &self,
        client: &ScopedDeviceClient,
        max_pages: u32,
    ) -> Result<ProjectSyncProgress> {
        if !(1..=128).contains(&max_pages) {
            return Err(RuntimeError::Configuration);
        }
        self.upload(client).await?;
        let mut cursor = self.status().await?.cursor;
        let mut through = None;
        for index in 0..max_pages {
            let request = DeviceReplicaRequest {
                schema_version: DEVICE_REPLICA_SCHEMA,
                nonce: Uuid::new_v4(),
                device: self.binding.device,
                scope: self.binding.scope.clone(),
                catalog_revision: self.binding.catalog_revision,
                after: cursor,
                through,
                limit: 8,
            };
            let signed = client.device_replica(&request).await?;
            self.trust
                .verify_device_replica_page(&signed, &request, self.binding.authority)?;
            let page = &signed.claims.page;
            cursor = self.projection.apply_verified(page).await?;
            through = Some(page.through);
            if cursor == page.through || index + 1 == max_pages {
                return Ok(ProjectSyncProgress {
                    cursor,
                    through: page.through,
                    pages: index + 1,
                    caught_up: cursor == page.through,
                    observed_at: chrono::Utc::now(),
                });
            }
        }
        Err(RuntimeError::InvalidResponse)
    }
    async fn upload(&self, client: &ScopedDeviceClient) -> Result<()> {
        let Some(binding) = &self.contribution else {
            return Ok(());
        };
        for _ in 0..32 {
            let mut tx = self
                .pool
                .begin()
                .await
                .map_err(|_| RuntimeError::Unavailable)?;
            let claim = mem_store::device_contributions::claim(&mut tx, binding)
                .await
                .map_err(|_| RuntimeError::InvalidResponse)?;
            tx.commit().await.map_err(|_| RuntimeError::Unavailable)?;
            let Some(claim) = claim else { break };
            let result = client.curated_contribution(&claim.contribution).await;
            let mut tx = self
                .pool
                .begin()
                .await
                .map_err(|_| RuntimeError::Unavailable)?;
            match result {
                Ok(receipt) => {
                    mem_store::device_contributions::acknowledge(&mut tx, &claim, &receipt)
                        .await
                        .map_err(|_| RuntimeError::InvalidResponse)?;
                    tx.commit().await.map_err(|_| RuntimeError::Unavailable)?;
                }
                Err(error) => {
                    mem_store::device_contributions::release(
                        &mut tx,
                        &claim,
                        matches!(error, RuntimeError::Unauthorized),
                    )
                    .await
                    .map_err(|_| RuntimeError::InvalidResponse)?;
                    tx.commit().await.map_err(|_| RuntimeError::Unavailable)?;
                    return Err(error);
                }
            }
        }
        Ok(())
    }
}

impl ScopedDeviceClient {
    pub async fn curated_contribution(
        &self,
        request: &mem_record::CuratedDeviceContribution,
    ) -> Result<mem_record::DeviceContributionReceipt> {
        request
            .validate()
            .map_err(|_| RuntimeError::Configuration)?;
        let scope = self.scope.as_ref().ok_or(RuntimeError::Configuration)?;
        let binding = &request.binding.scope;
        if binding.company_id != scope.company
            || binding.area.as_str() != scope.area
            || binding.owner_instance_id != scope.owner.as_uuid()
            || binding.project_id != scope.project
            || binding.space_id != scope.space.as_uuid()
        {
            return Err(RuntimeError::Configuration);
        }
        let path = format!(
            "/v1/onboarding/areas/{}/owners/{}/projects/{}/contributions",
            scope.area,
            scope.owner.as_uuid(),
            scope.project
        );
        let response = self
            .endpoint
            .client
            .post(self.endpoint.url(&path)?)
            .bearer_auth(&self.credential)
            .json(request)
            .send()
            .await
            .map_err(|_| RuntimeError::Unavailable)?;
        match response.status().as_u16() {
            200..=299 => crate::http::json(response, 64 * 1024).await,
            401 | 403 => Err(RuntimeError::Unauthorized),
            _ => Err(RuntimeError::Unavailable),
        }
    }
    pub async fn device_replica(
        &self,
        request: &DeviceReplicaRequest,
    ) -> Result<SignedDeviceReplicaPage> {
        if !request.valid() || self.scope.as_ref() != Some(&request.scope) {
            return Err(RuntimeError::Configuration);
        }
        let path = format!(
            "/v1/onboarding/areas/{}/owners/{}/projects/{}/replica",
            request.scope.area,
            request.scope.owner.as_uuid(),
            request.scope.project
        );
        let response = self
            .endpoint
            .client
            .post(self.endpoint.url(&path)?)
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
    }
}
