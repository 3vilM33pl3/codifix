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
        let result = self.synchronize_once(client, max_pages).await;
        let error = match &result {
            Ok(_) => None,
            Err(RuntimeError::Unauthorized) => Some("authority_denied"),
            Err(_) => Some("synchronization_incomplete"),
        };
        sqlx::query("UPDATE device_replica_projections SET last_sync_error=$2,last_sync_attempt_at=clock_timestamp() WHERE subscription=$1")
            .bind(self.binding.subscription).bind(error).execute(&self.pool).await.map_err(|_|RuntimeError::Unavailable)?;
        result
    }
    pub async fn last_error(&self) -> Result<Option<String>> {
        sqlx::query_scalar(
            "SELECT last_sync_error FROM device_replica_projections WHERE subscription=$1",
        )
        .bind(self.binding.subscription)
        .fetch_one(&self.pool)
        .await
        .map_err(|_| RuntimeError::Unavailable)
    }
    async fn synchronize_once(
        &self,
        client: &ScopedDeviceClient,
        max_pages: u32,
    ) -> Result<ProjectSyncProgress> {
        if !(1..=128).contains(&max_pages) {
            return Err(RuntimeError::Configuration);
        }
        // A contribution-specific failure must not stop receiving owner changes.
        // Report it only after the independent download has committed its cursor.
        let upload = self.upload(client).await;
        let resolutions = self.reconcile(client).await;
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
                upload?;
                resolutions?;
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
                        matches!(
                            error,
                            RuntimeError::Unauthorized
                                | RuntimeError::InvalidResponse
                                | RuntimeError::Configuration
                        ),
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
    async fn reconcile(&self, client: &ScopedDeviceClient) -> Result<()> {
        let Some(binding) = &self.contribution else {
            return Ok(());
        };
        let operations:Vec<Uuid>=sqlx::query_scalar("SELECT operation_id FROM laptop_curated_outbox WHERE binding=$1 AND state IN ('proposed','conflict') AND receipt->>'proposal_id' IS NOT NULL ORDER BY updated_at,operation_id LIMIT 32")
            .bind(sqlx::types::Json(binding)).fetch_all(&self.pool).await.map_err(|_|RuntimeError::Unavailable)?;
        for operation in operations {
            let response = client.curated_contribution_resolution(operation).await;
            // Persist fair polling even for a still-pending or failed response.
            // Otherwise the first 32 pending proposals starve later decisions.
            sqlx::query("UPDATE laptop_curated_outbox SET updated_at=clock_timestamp() WHERE operation_id=$1 AND binding=$2 AND state IN ('proposed','conflict')")
                .bind(operation).bind(sqlx::types::Json(binding)).execute(&self.pool).await.map_err(|_|RuntimeError::Unavailable)?;
            if let Some(status) = response? {
                if status.receipt.operation_id != operation {
                    return Err(RuntimeError::InvalidResponse);
                }
                let mut tx = self
                    .pool
                    .begin()
                    .await
                    .map_err(|_| RuntimeError::Unavailable)?;
                mem_store::device_contributions::reconcile_resolution(&mut tx, binding, &status)
                    .await
                    .map_err(|_| RuntimeError::InvalidResponse)?;
                tx.commit().await.map_err(|_| RuntimeError::Unavailable)?;
            }
        }
        Ok(())
    }
}

impl ScopedDeviceClient {
    pub async fn curated_contribution_resolution(
        &self,
        operation: Uuid,
    ) -> Result<Option<mem_record::DeviceContributionStatus>> {
        let scope = self.scope.as_ref().ok_or(RuntimeError::Configuration)?;
        let path = format!(
            "/v1/onboarding/areas/{}/owners/{}/projects/{}/contributions/resolution",
            scope.area,
            scope.owner.as_uuid(),
            scope.project
        );
        let response = self
            .endpoint
            .client
            .post(self.endpoint.url(&path)?)
            .bearer_auth(&self.credential)
            .json(&operation)
            .send()
            .await
            .map_err(|_| RuntimeError::Unavailable)?;
        match response.status().as_u16() {
            200..=299 => crate::http::json(response, 64 * 1024).await,
            401 | 403 => Err(RuntimeError::Unauthorized),
            _ => Err(RuntimeError::Unavailable),
        }
    }
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
            400 | 409 | 413 | 422 => Err(RuntimeError::InvalidResponse),
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

#[cfg(test)]
#[allow(clippy::unwrap_used)]
mod tests {
    use super::*;
    use axum::{Json, Router, routing::post};
    use mem_federation::{
        CanonicalJsonCodec,
        publication::{InstanceId, SpaceId},
        subscription::ProjectScope,
    };
    #[tokio::test]
    async fn failed_upload_does_not_block_verified_download() {
        let Some(db) = mem_test_support::IsolatedDatabase::new().await else {
            return;
        };
        let project = Uuid::new_v4();
        let scope = ProjectScope {
            company: Uuid::new_v4(),
            area: "workshop".into(),
            owner: InstanceId::new(Uuid::new_v4()).unwrap(),
            project,
            space: SpaceId::new(Uuid::new_v4()).unwrap(),
        };
        let binding = ProjectReplicaBinding {
            subscription: Uuid::new_v4(),
            device: Uuid::new_v4(),
            scope: scope.clone(),
            catalog_revision: 1,
            authority: scope.owner,
        };
        let contribution = mem_record::LaptopContributionBinding {
            scope: mem_record::LaptopContributionScope {
                company_id: scope.company,
                area: mem_record::OnboardingArea::Workshop,
                owner_instance_id: scope.owner.as_uuid(),
                project_id: project,
                space_id: scope.space.as_uuid(),
            },
            device_id: binding.device,
            manifest_revision: 1,
            manifest_sha256: "a".repeat(64),
        };
        sqlx::query("INSERT INTO projects(id,slug,name,root_path) VALUES($1,'local','local','/synthetic/local')").bind(project).execute(&db.pool).await.unwrap();
        let mut tx = db.pool.begin().await.unwrap();
        mem_store::device_contributions::bind_project(&mut tx, project, &contribution)
            .await
            .unwrap();
        tx.commit().await.unwrap();
        sqlx::query("INSERT INTO memory_entries(id,project_id,canonical_id,version_no,canonical_text,summary,memory_type,scope,importance,confidence,created_at,updated_at,search_document) VALUES($1,$2,$1,1,'local fact','fact','domain_fact','project',3,0.8,now(),now(),to_tsvector('simple','fact'))").bind(Uuid::new_v4()).bind(project).execute(&db.pool).await.unwrap();
        let security = crate::GatewaySecurity::initial(&[79; 32], Default::default()).unwrap();
        let trust = security.trust();
        let authority = scope.owner;
        let record = Some(DeviceReplicaRecord::CanonicalState(DeviceCanonicalState {
            canonical_id: Uuid::new_v4(),
            current_version_id: Uuid::new_v4(),
            status: mem_record::MemoryStatus::Active,
            confidence: mem_federation::publication::ConfidencePpm::from_confidence(0.8).unwrap(),
            importance: 3,
            updated_at: chrono::Utc::now(),
            archived_at: None,
        }));
        let entry = DeviceReplicaEntry {
            sequence: 1,
            key: record.as_ref().unwrap().key(),
            digest: CanonicalJsonCodec::digest(&record).unwrap(),
            record,
        };
        let path = format!(
            "/v1/onboarding/areas/{}/owners/{}/projects/{}/replica",
            scope.area,
            scope.owner.as_uuid(),
            project
        );
        let resolution_path = path.replace("/replica", "/contributions/resolution");
        let polled = std::sync::Arc::new(std::sync::Mutex::new(Vec::<Uuid>::new()));
        let observed = polled.clone();
        let app = Router::new()
            .route(
                &resolution_path,
                post(move |Json(operation): Json<Uuid>| {
                    let observed = observed.clone();
                    async move {
                        observed.lock().unwrap().push(operation);
                        Json(Option::<mem_record::DeviceContributionStatus>::None)
                    }
                }),
            )
            .route(
                &path,
                post(move |Json(request): Json<DeviceReplicaRequest>| {
                    let entry = entry.clone();
                    let security = security.clone();
                    async move {
                        Json(
                            security
                                .sign_device_replica_page(
                                    authority,
                                    DeviceReplicaPage {
                                        request,
                                        through: 1,
                                        next: 1,
                                        entries: vec![entry],
                                    },
                                )
                                .unwrap(),
                        )
                    }
                }),
            )
            .fallback(|| async { axum::http::StatusCode::SERVICE_UNAVAILABLE });
        let listener = tokio::net::TcpListener::bind("127.0.0.1:0").await.unwrap();
        let address = listener.local_addr().unwrap();
        let server = tokio::spawn(async move { axum::serve(listener, app).await.unwrap() });
        let client = ScopedDeviceClient::for_scope(
            crate::Endpoint::new(&format!("http://{address}"), true).unwrap(),
            format!("mlt_{}", "a".repeat(64)),
            scope.clone(),
        )
        .unwrap();
        let projection = DeviceProjectionStore::initialize(
            db.pool.clone(),
            binding.subscription,
            binding.device,
            scope,
        )
        .await
        .unwrap();
        let replica = DeviceProjectReplica {
            binding,
            projection,
            trust,
            pool: db.pool.clone(),
            contribution: Some(contribution),
        };
        assert!(
            replica.synchronize(&client, 8).await.is_err(),
            "upload failure remains visible"
        );
        assert_eq!(
            replica.status().await.unwrap().cursor,
            1,
            "download committed despite failed upload"
        );
        assert_eq!(replica.uploads().await.unwrap().get("queued"), Some(&1));
        assert_eq!(
            replica.last_error().await.unwrap().as_deref(),
            Some("synchronization_incomplete")
        );
        let mut last = Uuid::nil();
        for _ in 0..33 {
            last = Uuid::new_v4();
            sqlx::query("INSERT INTO memory_entries(id,project_id,canonical_id,version_no,canonical_text,summary,memory_type,scope,importance,confidence,created_at,updated_at,search_document) VALUES($1,$2,$1,1,'pending correction','pending','domain_fact','project',3,0.8,now(),now(),to_tsvector('simple','pending'))").bind(last).bind(project).execute(&db.pool).await.unwrap();
            sqlx::query("UPDATE laptop_curated_outbox SET state='proposed',receipt=$2 WHERE operation_id=$1").bind(last).bind(serde_json::json!({"proposal_id":Uuid::new_v4()})).execute(&db.pool).await.unwrap();
        }
        replica.reconcile(&client).await.unwrap();
        assert_eq!(polled.lock().unwrap().len(), 32);
        assert!(!polled.lock().unwrap().contains(&last));
        replica.reconcile(&client).await.unwrap();
        assert_eq!(
            polled.lock().unwrap()[32],
            last,
            "later decisions must not be starved by the first 32 pending proposals"
        );
        server.abort();
        drop(replica);
        db.cleanup().await.unwrap();
    }
}
