// SPDX-License-Identifier: AGPL-3.0-or-later

use anyhow::Result;
use codifix_config::AppConfig;
use codifix_record::{
    ActivityListResponse, AgentWorkspaceFinishRequest, AgentWorkspaceHeartbeatRequest,
    AgentWorkspaceListResponse, AgentWorkspaceRecord, AgentWorkspaceStartRequest,
    ArchiveMemoryResponse, ArchiveRequest, ArchiveResponse, CaptureTaskRequest,
    CheckpointActivityRequest, CodeGraphStatusResponse, CommitDetailResponse, CommitSyncRequest,
    CommitSyncResponse, CurateRequest, CurateResponse, DeleteMemoryRequest, DeleteMemoryResponse,
    LoopApprovalDecisionRequest, LoopApprovalDecisionResponse, LoopApprovalStatus,
    LoopApprovalsResponse, LoopCancelRequest, LoopContextPackResponse, LoopDefinitionResponse,
    LoopDefinitionsResponse, LoopFeedbackRequest, LoopGlobalStateResponse,
    LoopGlobalStateUpdateRequest, LoopMemoryProposalCreateRequest,
    LoopMemoryProposalDecisionRequest, LoopMemoryProposalDecisionResponse,
    LoopMemoryProposalsResponse, LoopRunRequest, LoopRunResponse, LoopRunStatus, LoopRunsResponse,
    LoopSettingsUpdateRequest, MemoryEntryResponse, PlanActivityRequest, ProjectCommitsResponse,
    ProjectMemoriesResponse, ProjectMemoryBundlePreview, ProjectMemoryExportOptions,
    ProjectMemoryImportPreview, ProjectMemoryImportResponse, ProjectOverviewResponse,
    ProvenanceVerificationRequest, ProvenanceVerificationResponse, PruneEmbeddingsRequest,
    PruneEmbeddingsResponse, QueryRequest, QueryResponse, ReembedRequest, ReembedResponse,
    ReindexRequest, ReindexResponse, ReplacementPolicy, ResumeRequest, ResumeResponse,
    ScanActivityRequest, UpToSpeedRequest, UpToSpeedResponse,
};
use reqwest::Client;
use uuid::Uuid;

use crate::commands::output::{service_url, write_headers};

mod laptop_cache;
mod laptop_corrections;
mod local_session;
mod supervisor_questions;
#[cfg(any(feature = "tui", test))]
pub(crate) use laptop_cache::cache_scope_matches;
pub(super) use local_session::local_human_client;

fn jobs_path(project: &str, id: Option<Uuid>, retry: bool) -> Result<String> {
    let mut url = reqwest::Url::parse("http://localhost/")?;
    {
        let mut path = url
            .path_segments_mut()
            .map_err(|_| anyhow::anyhow!("invalid base URL"))?;
        path.extend(["v1", "projects", project, "jobs"]);
        if let Some(id) = id {
            path.push(&id.to_string());
        }
        if retry {
            path.push("retry");
        }
    }
    Ok(url.path().to_owned())
}

#[derive(Clone)]
pub(crate) struct ApiClient {
    pub(crate) client: Client,
    pub(crate) config: AppConfig,
    // A revocable loopback owner session, shared by cloned TUI clients, never persisted.
    question_session: std::sync::Arc<tokio::sync::Mutex<Option<reqwest::header::HeaderMap>>>,
}

impl ApiClient {
    pub(crate) async fn supervisor_proposals(
        &self,
        project: &str,
        after: Option<Uuid>,
    ) -> Result<codifix_record::SupervisorProposalList> {
        self.supervisor_private(project, None, after, None).await
    }
    pub(crate) async fn supervisor_proposal(
        &self,
        project: &str,
        id: Uuid,
    ) -> Result<codifix_record::SupervisorProposalDetail> {
        self.supervisor_private(project, Some(id), None, None).await
    }
    pub(crate) async fn supervisor_review(
        &self,
        project: &str,
        id: Uuid,
        request: &codifix_record::SupervisorReviewRequest,
    ) -> Result<codifix_record::SupervisorReviewReceipt> {
        let receipt: codifix_record::SupervisorReviewReceipt = self
            .supervisor_private(project, Some(id), None, Some(request))
            .await?;
        anyhow::ensure!(
            receipt.action_id == id
                && receipt.request_id == request.request_id
                && receipt.proposal_digest == request.proposal_digest
                && receipt.decision == request.decision,
            "Review receipt mismatch; inspect before further changes"
        );
        Ok(receipt)
    }
    async fn supervisor_private<T: serde::de::DeserializeOwned>(
        &self,
        project: &str,
        id: Option<Uuid>,
        after: Option<Uuid>,
        request: Option<&codifix_record::SupervisorReviewRequest>,
    ) -> Result<T> {
        let mut url = reqwest::Url::parse("http://localhost/")?;
        {
            let mut path = url
                .path_segments_mut()
                .map_err(|_| anyhow::anyhow!("invalid URL"))?;
            path.extend(["v1", "projects", project, "supervisor", "proposals"]);
            if let Some(id) = id {
                path.push(&id.to_string());
            }
            if request.is_some() {
                path.push("review");
            }
        }
        let endpoint = service_url(&self.config, url.path());
        let builder = if let Some(body) = request {
            self.client.post(endpoint).json(body)
        } else {
            self.client.get(endpoint)
        };
        let builder = if let Some(after) = after {
            builder.query(&[("after", after.to_string())])
        } else {
            builder
        };
        let response = builder
            .headers(write_headers(&self.config)?)
            .header(reqwest::header::CACHE_CONTROL, "no-store")
            .send()
            .await
            .map_err(|_| {
                anyhow::anyhow!("Private operation unconfirmed; retry the same request ID")
            })?;
        match response.status().as_u16() {
            200 => response.json().await.map_err(|_| {
                anyhow::anyhow!("Unreadable private response; retry the same request ID")
            }),
            401 | 403 => {
                anyhow::bail!("Denied: current project permissions and authentication required")
            }
            404 => anyhow::bail!("Private interaction or this result is unavailable"),
            409 => anyhow::bail!("Conflict: profile, sources or review changed; refresh"),
            _ => anyhow::bail!(
                "Private operation unconfirmed; response details suppressed; retry the same request ID"
            ),
        }
    }
    pub(crate) async fn supervisor_policy(
        &self,
        request: &codifix_record::SupervisorPolicyRequest,
    ) -> Result<codifix_record::SupervisorPolicyResponse> {
        let response = self
            .client
            .post(service_url(&self.config, "/v1/supervisor/policy"))
            .headers(write_headers(&self.config)?)
            .json(request)
            .send()
            .await
            .map_err(|_| {
                anyhow::anyhow!(
                    "Supervisor outcome unconfirmed; inspect or retry the same request ID"
                )
            })?;
        match response.status().as_u16() {
            200 => {
                let receipt: codifix_record::SupervisorPolicyResponse =
                    response.json().await.map_err(|_| {
                        anyhow::anyhow!("Supervisor receipt unreadable; retry the same request ID")
                    })?;
                anyhow::ensure!(
                    receipt.request_id == request.request_id
                        && receipt.instance_id == request.instance_id
                        && receipt.policy_revision == request.policy_revision
                        && receipt.enabled == request.enabled,
                    "Supervisor receipt mismatch; inspect history before further changes"
                );
                Ok(receipt)
            }
            400 => anyhow::bail!("Invalid supervisor control request"),
            401 | 403 => anyhow::bail!(
                "Denied: current global system_admin, writable service and valid credential required"
            ),
            409 => anyhow::bail!(
                "Conflict: inspect current supervisor identity/policy; request IDs cannot change terms"
            ),
            _ => anyhow::bail!(
                "Supervisor outcome unconfirmed; inspect or retry the same request ID"
            ),
        }
    }
    pub(crate) async fn supervisor(
        &self,
        after: &str,
        limit: u32,
    ) -> Result<codifix_record::SupervisorResponse> {
        anyhow::ensure!(
            !after.is_empty()
                && after.bytes().all(|byte| byte.is_ascii_digit())
                && after.parse::<i64>().is_ok()
                && (1..=100).contains(&limit),
            "invalid supervisor page bounds"
        );
        let response = self
            .client
            .get(service_url(&self.config, "/v1/supervisor"))
            .query(&[("after", after.to_owned()), ("limit", limit.to_string())])
            .headers(write_headers(&self.config)?)
            .header(reqwest::header::CACHE_CONTROL, "no-store")
            .send()
            .await
            .map_err(|_| anyhow::anyhow!("Offline: supervisor request failed"))?;
        match response.status().as_u16() {
            200 => response
                .json()
                .await
                .map_err(|_| anyhow::anyhow!("Invalid supervisor metadata response")),
            401 => anyhow::bail!("Signed out: authenticate to inspect the supervisor"),
            403 => anyhow::bail!("Forbidden: instance-wide memory_read is required"),
            409 => {
                anyhow::bail!("Identity mismatch: configured supervisor and database need review")
            }
            503 => anyhow::bail!("Offline: supervisor storage unavailable"),
            _ => anyhow::bail!("Supervisor request rejected; response details suppressed"),
        }
    }
    pub(crate) async fn jobs(
        &self,
        project: &str,
        status: Option<codifix_record::JobStatus>,
        after: Option<Uuid>,
        limit: u32,
    ) -> Result<codifix_record::ProjectJobsResponse> {
        let path = jobs_path(project, None, false)?;
        let mut query = vec![("limit", limit.to_string())];
        if let Some(status) = status {
            query.push((
                "status",
                serde_json::to_value(status)?
                    .as_str()
                    .unwrap_or_default()
                    .into(),
            ));
        }
        if let Some(after) = after {
            query.push(("after", after.to_string()));
        }
        get_json(
            self.client
                .get(service_url(&self.config, &path))
                .query(&query)
                .headers(write_headers(&self.config)?)
                .send()
                .await?,
        )
        .await
    }
    pub(crate) async fn job(&self, project: &str, id: Uuid) -> Result<codifix_record::JobDetail> {
        let path = jobs_path(project, Some(id), false)?;
        get_json(
            self.client
                .get(service_url(&self.config, &path))
                .headers(write_headers(&self.config)?)
                .send()
                .await?,
        )
        .await
    }
    pub(crate) async fn retry_job(
        &self,
        project: &str,
        id: Uuid,
        request_id: Uuid,
    ) -> Result<codifix_record::JobRetryResponse> {
        let path = jobs_path(project, Some(id), true)?;
        get_json(
            self.client
                .post(service_url(&self.config, &path))
                .headers(write_headers(&self.config)?)
                .json(&codifix_record::JobRetryRequest { request_id })
                .send()
                .await?,
        )
        .await
    }
    pub(crate) fn new(client: Client, config: AppConfig) -> Self {
        Self {
            client,
            config,
            question_session: Default::default(),
        }
    }

    pub(crate) async fn health(&self) -> Result<serde_json::Value> {
        get_json(
            self.client
                .get(service_url(&self.config, "/healthz"))
                .send()
                .await?,
        )
        .await
    }

    #[cfg(feature = "tui")]
    pub(crate) async fn project_memories_for_browsing(
        &self,
        project: &str,
    ) -> Result<ProjectMemoriesResponse> {
        if let Some(cache) = &self.config.laptop_cache {
            let mut combined = ProjectMemoriesResponse {
                project: project.into(),
                total: 0,
                items: vec![],
            };
            let mut revision = None;
            let mut seen = std::collections::HashSet::new();
            loop {
                let snapshot: codifix_record::ProjectMemoryBrowseResponse = self.cache_operation(
                    &format!("/v1/projects/{project}/memories?include_synchronized=true&limit=100&offset={}", combined.items.len()), None).await?;
                anyhow::ensure!(
                    revision
                        .as_ref()
                        .is_none_or(|r| r == &snapshot.browse_revision),
                    "Synchronized view changed during pagination; refresh again"
                );
                revision = Some(snapshot.browse_revision);
                let page = snapshot.memories;
                anyhow::ensure!(
                    page.project == project
                        && (0..=100_000).contains(&page.total)
                        && page.items.len() <= 100,
                    "Unexpected synchronized project or browse bound"
                );
                for row in &page.items {
                    anyhow::ensure!(
                        seen.insert((row.id, row.origin.as_ref().map(|o| o.subscription_id))),
                        "Synchronized view repeated a record; refresh again"
                    );
                    if let Some(origin) = &row.origin {
                        anyhow::ensure!(
                            origin.read_only
                                && cache
                                    .subscriptions
                                    .iter()
                                    .any(|s| s.subscription_id == origin.subscription_id
                                        && s.project_id == origin.project_id
                                        && s.owner_instance_id == origin.owner_instance_id),
                            "Synchronized browse scope changed"
                        );
                    }
                }
                let empty = page.items.is_empty();
                combined.total = page.total;
                combined.items.extend(page.items);
                if combined.items.len() as i64 >= combined.total {
                    return Ok(combined);
                }
                anyhow::ensure!(
                    !empty,
                    "Synchronized browse changed during pagination; refresh again"
                );
            }
        }
        self.project_memories(project).await
    }

    pub(crate) async fn project_memories(&self, project: &str) -> Result<ProjectMemoriesResponse> {
        get_json(
            self.client
                .get(service_url(
                    &self.config,
                    &format!("/v1/projects/{project}/memories"),
                ))
                .send()
                .await?,
        )
        .await
    }

    pub(crate) async fn project_overview(&self, project: &str) -> Result<ProjectOverviewResponse> {
        get_json(
            self.client
                .get(service_url(
                    &self.config,
                    &format!("/v1/projects/{project}/overview"),
                ))
                .send()
                .await?,
        )
        .await
    }

    pub(crate) async fn replacement_proposals(
        &self,
        project: &str,
    ) -> Result<codifix_record::ReplacementProposalListResponse> {
        get_json(
            self.client
                .get(service_url(
                    &self.config,
                    &format!("/v1/projects/{project}/replacement-proposals"),
                ))
                .send()
                .await?,
        )
        .await
    }

    pub(crate) async fn memory_scores(
        &self,
        project: &str,
        needs_review: bool,
        limit: i64,
    ) -> Result<codifix_record::MemoryScoresResponse> {
        get_json(
            self.client
                .get(service_url(
                    &self.config,
                    &format!(
                        "/v1/projects/{project}/memory-scores?needs_review={needs_review}&limit={limit}"
                    ),
                ))
                .send()
                .await?,
        )
        .await
    }

    pub(crate) async fn validate_memory(
        &self,
        memory_id: Uuid,
        dry_run: Option<bool>,
        proof_scope: Option<codifix_record::ValidationProofScope>,
    ) -> Result<codifix_record::ValidationRunInfo> {
        get_json(
            self.client
                .post(service_url(
                    &self.config,
                    &format!("/v1/memory/{memory_id}/validate"),
                ))
                .headers(write_headers(&self.config)?)
                .json(&codifix_record::ValidateMemoryRequest {
                    dry_run,
                    proof_scope,
                })
                .send()
                .await?,
        )
        .await
    }

    pub(crate) async fn validation_runs(
        &self,
        project: &str,
        pending_only: bool,
        limit: i64,
    ) -> Result<codifix_record::ValidationRunsResponse> {
        let review = if pending_only { "pending" } else { "" };
        get_json(
            self.client
                .get(service_url(
                    &self.config,
                    &format!(
                        "/v1/projects/{project}/validation-runs?review={review}&limit={limit}"
                    ),
                ))
                .send()
                .await?,
        )
        .await
    }

    pub(crate) async fn review_validation_run(
        &self,
        run_id: Uuid,
        action: &str,
    ) -> Result<codifix_record::ReviewValidationResponse> {
        get_json(
            self.client
                .post(service_url(
                    &self.config,
                    &format!("/v1/validation-runs/{run_id}/review"),
                ))
                .headers(write_headers(&self.config)?)
                .json(&codifix_record::ReviewValidationRequest {
                    action: action.to_string(),
                })
                .send()
                .await?,
        )
        .await
    }

    pub(crate) async fn approve_replacement_proposal(
        &self,
        project: &str,
        proposal_id: Uuid,
    ) -> Result<codifix_record::ReplacementProposalResolutionResponse> {
        get_json(
            self.client
                .post(service_url(
                    &self.config,
                    &format!("/v1/projects/{project}/replacement-proposals/{proposal_id}/approve"),
                ))
                .headers(write_headers(&self.config)?)
                .json(&serde_json::json!({}))
                .send()
                .await?,
        )
        .await
    }

    pub(crate) async fn reject_replacement_proposal(
        &self,
        project: &str,
        proposal_id: Uuid,
    ) -> Result<codifix_record::ReplacementProposalResolutionResponse> {
        get_json(
            self.client
                .post(service_url(
                    &self.config,
                    &format!("/v1/projects/{project}/replacement-proposals/{proposal_id}/reject"),
                ))
                .headers(write_headers(&self.config)?)
                .json(&serde_json::json!({}))
                .send()
                .await?,
        )
        .await
    }

    pub(crate) async fn resume(&self, request: &ResumeRequest) -> Result<ResumeResponse> {
        get_json(
            self.client
                .post(service_url(
                    &self.config,
                    &format!("/v1/projects/{}/resume", request.project),
                ))
                .json(request)
                .send()
                .await?,
        )
        .await
    }

    pub(crate) async fn project_activities(
        &self,
        project: &str,
        limit: usize,
        kind: Option<&str>,
    ) -> Result<ActivityListResponse> {
        let mut path = format!("/v1/projects/{project}/activities?limit={limit}");
        if let Some(kind) = kind {
            path.push_str("&kind=");
            path.push_str(kind);
        }
        get_json(
            self.client
                .get(service_url(&self.config, &path))
                .send()
                .await?,
        )
        .await
    }

    pub(crate) async fn up_to_speed(
        &self,
        request: &UpToSpeedRequest,
    ) -> Result<UpToSpeedResponse> {
        get_json(
            self.client
                .post(service_url(
                    &self.config,
                    &format!("/v1/projects/{}/up-to-speed", request.project),
                ))
                .json(request)
                .send()
                .await?,
        )
        .await
    }

    pub(crate) async fn project_commits(
        &self,
        project: &str,
        limit: i64,
        offset: i64,
    ) -> Result<ProjectCommitsResponse> {
        get_json(
            self.client
                .get(service_url(
                    &self.config,
                    &format!("/v1/projects/{project}/commits?limit={limit}&offset={offset}"),
                ))
                .send()
                .await?,
        )
        .await
    }

    pub(crate) async fn project_commit(
        &self,
        project: &str,
        commit: &str,
    ) -> Result<CommitDetailResponse> {
        get_json(
            self.client
                .get(service_url(
                    &self.config,
                    &format!("/v1/projects/{project}/commits/{commit}"),
                ))
                .send()
                .await?,
        )
        .await
    }

    pub(crate) async fn export_bundle_preview(
        &self,
        project: &str,
        options: &ProjectMemoryExportOptions,
    ) -> Result<ProjectMemoryBundlePreview> {
        get_json(
            self.client
                .post(service_url(
                    &self.config,
                    &format!("/v1/projects/{project}/bundle/export/preview"),
                ))
                .headers(write_headers(&self.config)?)
                .json(options)
                .send()
                .await?,
        )
        .await
    }

    pub(crate) async fn export_bundle(
        &self,
        project: &str,
        options: &ProjectMemoryExportOptions,
    ) -> Result<Vec<u8>> {
        let response = self
            .client
            .post(service_url(
                &self.config,
                &format!("/v1/projects/{project}/bundle/export"),
            ))
            .headers(write_headers(&self.config)?)
            .json(options)
            .send()
            .await?;
        let status = response.status();
        let bytes = response.bytes().await?;
        if !status.is_success() {
            anyhow::bail!("{status} {}", String::from_utf8_lossy(&bytes));
        }
        Ok(bytes.to_vec())
    }

    pub(crate) async fn import_bundle_preview(
        &self,
        project: &str,
        bytes: Vec<u8>,
    ) -> Result<ProjectMemoryImportPreview> {
        get_json(
            self.client
                .post(service_url(
                    &self.config,
                    &format!("/v1/projects/{project}/bundle/import/preview"),
                ))
                .headers(write_headers(&self.config)?)
                .header("content-type", "application/octet-stream")
                .body(bytes)
                .send()
                .await?,
        )
        .await
    }

    pub(crate) async fn import_bundle(
        &self,
        project: &str,
        bytes: Vec<u8>,
    ) -> Result<ProjectMemoryImportResponse> {
        get_json(
            self.client
                .post(service_url(
                    &self.config,
                    &format!("/v1/projects/{project}/bundle/import"),
                ))
                .headers(write_headers(&self.config)?)
                .header("content-type", "application/octet-stream")
                .body(bytes)
                .send()
                .await?,
        )
        .await
    }

    pub(crate) async fn query(&self, request: &QueryRequest) -> Result<QueryResponse> {
        get_json(
            self.client
                .post(service_url(&self.config, "/v1/query"))
                .json(request)
                .send()
                .await?,
        )
        .await
    }

    pub(crate) async fn agent_workspaces(
        &self,
        project: &str,
        include_finished: bool,
    ) -> Result<AgentWorkspaceListResponse> {
        get_json(
            self.client
                .get(service_url(&self.config, "/v1/agents/workspaces"))
                .query(&[
                    ("project", project.to_string()),
                    ("include_finished", include_finished.to_string()),
                ])
                .send()
                .await?,
        )
        .await
    }

    pub(crate) async fn start_agent_workspace(
        &self,
        request: &AgentWorkspaceStartRequest,
    ) -> Result<AgentWorkspaceRecord> {
        get_json(
            self.client
                .post(service_url(&self.config, "/v1/agents/workspaces/start"))
                .headers(write_headers(&self.config)?)
                .json(request)
                .send()
                .await?,
        )
        .await
    }

    pub(crate) async fn heartbeat_agent_workspace(
        &self,
        workspace_id: Uuid,
        request: &AgentWorkspaceHeartbeatRequest,
    ) -> Result<AgentWorkspaceRecord> {
        get_json(
            self.client
                .post(service_url(
                    &self.config,
                    &format!("/v1/agents/workspaces/{workspace_id}/heartbeat"),
                ))
                .headers(write_headers(&self.config)?)
                .json(request)
                .send()
                .await?,
        )
        .await
    }

    pub(crate) async fn finish_agent_workspace(
        &self,
        workspace_id: Uuid,
        request: &AgentWorkspaceFinishRequest,
    ) -> Result<AgentWorkspaceRecord> {
        get_json(
            self.client
                .post(service_url(
                    &self.config,
                    &format!("/v1/agents/workspaces/{workspace_id}/finish"),
                ))
                .headers(write_headers(&self.config)?)
                .json(request)
                .send()
                .await?,
        )
        .await
    }

    pub(crate) async fn verify_provenance(
        &self,
        request: &ProvenanceVerificationRequest,
    ) -> Result<ProvenanceVerificationResponse> {
        get_json(
            self.client
                .post(service_url(&self.config, "/v1/provenance/verify"))
                .headers(write_headers(&self.config)?)
                .json(request)
                .send()
                .await?,
        )
        .await
    }

    pub(crate) async fn log_scan_activity(&self, request: &ScanActivityRequest) -> Result<()> {
        let response = self
            .client
            .post(service_url(&self.config, "/v1/activity"))
            .headers(write_headers(&self.config)?)
            .json(&codifix_record::ActivityIngestRequest::Scan(
                request.clone(),
            ))
            .send()
            .await?;
        let status = response.status();
        let body = response.text().await?;
        if !status.is_success() {
            anyhow::bail!("{status} {body}");
        }
        Ok(())
    }

    pub(crate) async fn graph_extract(
        &self,
        project: &str,
        request: &codifix_graph::GraphExtractionRequest,
    ) -> Result<codifix_graph::GraphExtractionReport> {
        get_json(
            self.client
                .post(service_url(
                    &self.config,
                    &format!("/v1/projects/{project}/graph/extract"),
                ))
                .headers(write_headers(&self.config)?)
                .json(request)
                .send()
                .await?,
        )
        .await
    }

    pub(crate) async fn graph_status(&self, project: &str) -> Result<CodeGraphStatusResponse> {
        get_json(
            self.client
                .get(service_url(
                    &self.config,
                    &format!("/v1/projects/{project}/graph/status"),
                ))
                .send()
                .await?,
        )
        .await
    }

    pub(crate) async fn log_checkpoint_activity(
        &self,
        request: &CheckpointActivityRequest,
    ) -> Result<()> {
        let response = self
            .client
            .post(service_url(&self.config, "/v1/activity"))
            .headers(write_headers(&self.config)?)
            .json(&codifix_record::ActivityIngestRequest::Checkpoint(
                request.clone(),
            ))
            .send()
            .await?;
        let status = response.status();
        let body = response.text().await?;
        if !status.is_success() {
            anyhow::bail!("{status} {body}");
        }
        Ok(())
    }

    pub(crate) async fn log_plan_activity(&self, request: &PlanActivityRequest) -> Result<()> {
        let root = codifix_platform::preferred_user_state_dir()
            .ok_or_else(|| anyhow::anyhow!("cannot determine private local journal directory"))?
            .join("operation-journal");
        codifix_platform::journal::reconcile(&root, 100)?;
        let operation = codifix_platform::journal::Operation::begin(
            &root,
            "plan_activity",
            std::path::Path::new(&request.project),
            &serde_json::to_vec(request)?,
        )?;
        let response = self
            .client
            .post(service_url(&self.config, "/v1/activity"))
            .headers(write_headers(&self.config)?)
            .json(&codifix_record::ActivityIngestRequest::Plan(
                request.clone(),
            ))
            .send()
            .await?;
        let status = response.status();
        let body = response.text().await?;
        operation.observed(status.is_success())?;
        if !status.is_success() {
            anyhow::bail!(
                "plan activity {} is unconfirmed ({status}); inspect the timeline before replay: {body}",
                operation.intent.id
            );
        }
        Ok(())
    }

    pub(crate) async fn memory_detail(&self, memory_id: &str) -> Result<MemoryEntryResponse> {
        get_json(
            self.client
                .get(service_url(
                    &self.config,
                    &format!("/v1/memory/{memory_id}"),
                ))
                .send()
                .await?,
        )
        .await
    }

    pub(crate) async fn list_embedding_backends(
        &self,
        project: Option<&str>,
    ) -> Result<codifix_record::EmbeddingBackendsResponse> {
        let mut request = self
            .client
            .get(service_url(&self.config, "/v1/embeddings/backends"));
        if let Some(slug) = project {
            request = request.query(&[("project", slug)]);
        }
        get_json(request.send().await?).await
    }

    pub(crate) async fn activate_embedding_backend(
        &self,
        name: &str,
    ) -> Result<codifix_record::EmbeddingBackendsResponse> {
        get_json(
            self.client
                .post(service_url(&self.config, "/v1/embeddings/activate"))
                .headers(write_headers(&self.config)?)
                .json(&codifix_record::ActivateEmbeddingBackendRequest {
                    name: name.to_string(),
                })
                .send()
                .await?,
        )
        .await
    }

    pub(crate) async fn deactivate_embedding_backend(
        &self,
    ) -> Result<codifix_record::EmbeddingBackendsResponse> {
        let response = self
            .client
            .post(service_url(&self.config, "/v1/embeddings/deactivate"))
            .headers(write_headers(&self.config)?)
            .json(&codifix_record::DeactivateEmbeddingBackendRequest::default())
            .send()
            .await?;
        if response.status() == reqwest::StatusCode::METHOD_NOT_ALLOWED {
            anyhow::bail!(
                "service does not support turning embeddings off yet; restart or upgrade memory-service so /v1/embeddings/deactivate is available"
            );
        }
        get_json(response).await
    }

    pub(crate) async fn set_embedding_creation_enabled(
        &self,
        name: &str,
        enabled: bool,
    ) -> Result<codifix_record::EmbeddingBackendsResponse> {
        let response = self
            .client
            .post(service_url(&self.config, "/v1/embeddings/create-enabled"))
            .headers(write_headers(&self.config)?)
            .json(&codifix_record::SetEmbeddingCreationRequest {
                name: name.to_string(),
                enabled,
            })
            .send()
            .await?;
        if response.status() == reqwest::StatusCode::METHOD_NOT_ALLOWED {
            anyhow::bail!(
                "service does not support toggling automatic embedding creation yet; restart or upgrade memory-service so /v1/embeddings/create-enabled is available"
            );
        }
        get_json(response).await
    }

    pub(crate) async fn llm_audit_status(&self) -> Result<codifix_record::LlmAuditStatusResponse> {
        get_json(
            self.client
                .get(service_url(&self.config, "/v1/config/llm-audit"))
                .send()
                .await?,
        )
        .await
    }

    pub(crate) async fn set_llm_audit_enabled(
        &self,
        enabled: bool,
    ) -> Result<codifix_record::LlmAuditStatusResponse> {
        let response = self
            .client
            .post(service_url(&self.config, "/v1/config/llm-audit"))
            .headers(write_headers(&self.config)?)
            .json(&codifix_record::SetLlmAuditRequest { enabled })
            .send()
            .await?;
        if response.status() == reqwest::StatusCode::METHOD_NOT_ALLOWED {
            anyhow::bail!(
                "service does not support toggling LLM audit yet; restart or upgrade memory-service so /v1/config/llm-audit is available"
            );
        }
        get_json(response).await
    }

    pub(crate) async fn memory_history(
        &self,
        memory_id: &str,
    ) -> Result<codifix_record::MemoryHistoryResponse> {
        get_json(
            self.client
                .get(service_url(
                    &self.config,
                    &format!("/v1/memory/{memory_id}/history"),
                ))
                .send()
                .await?,
        )
        .await
    }

    pub(crate) async fn sync_commits(
        &self,
        request: &CommitSyncRequest,
    ) -> Result<CommitSyncResponse> {
        get_json(
            self.client
                .post(service_url(&self.config, "/v1/commits/sync"))
                .headers(write_headers(&self.config)?)
                .json(request)
                .send()
                .await?,
        )
        .await
    }

    pub(crate) async fn capture_task(
        &self,
        request: &CaptureTaskRequest,
    ) -> Result<codifix_record::CaptureTaskResponse> {
        get_json(
            self.client
                .post(service_url(&self.config, "/v1/capture/task"))
                .headers(write_headers(&self.config)?)
                .json(request)
                .send()
                .await?,
        )
        .await
    }

    pub(crate) async fn curate(
        &self,
        project: &str,
        replacement_policy: ReplacementPolicy,
        dry_run: bool,
    ) -> Result<CurateResponse> {
        get_json(
            self.client
                .post(service_url(&self.config, "/v1/curate"))
                .headers(write_headers(&self.config)?)
                .json(&CurateRequest {
                    project: project.to_string(),
                    batch_size: None,
                    replacement_policy: Some(replacement_policy),
                    raw_capture_id: None,
                    dry_run,
                })
                .send()
                .await?,
        )
        .await
    }

    pub(crate) async fn curate_capture(
        &self,
        project: &str,
        raw_capture_id: Uuid,
        replacement_policy: ReplacementPolicy,
        dry_run: bool,
    ) -> Result<CurateResponse> {
        get_json(
            self.client
                .post(service_url(&self.config, "/v1/curate"))
                .headers(write_headers(&self.config)?)
                .json(&CurateRequest {
                    project: project.to_string(),
                    batch_size: Some(1),
                    raw_capture_id: Some(raw_capture_id),
                    replacement_policy: Some(replacement_policy),
                    dry_run,
                })
                .send()
                .await?,
        )
        .await
    }

    pub(crate) async fn reindex(
        &self,
        project: &str,
        dry_run: bool,
        backend: Option<&str>,
    ) -> Result<ReindexResponse> {
        get_json(
            self.client
                .post(service_url(&self.config, "/v1/reindex"))
                .headers(write_headers(&self.config)?)
                .json(&ReindexRequest {
                    project: project.to_string(),
                    dry_run,
                    backend: backend.map(str::to_string),
                })
                .send()
                .await?,
        )
        .await
    }

    pub(crate) async fn reembed(
        &self,
        project: &str,
        dry_run: bool,
        backend: Option<&str>,
    ) -> Result<ReembedResponse> {
        get_json(
            self.client
                .post(service_url(&self.config, "/v1/reembed"))
                .headers(write_headers(&self.config)?)
                .json(&ReembedRequest {
                    project: project.to_string(),
                    dry_run,
                    backend: backend.map(str::to_string),
                })
                .send()
                .await?,
        )
        .await
    }

    pub(crate) async fn prune_embeddings(
        &self,
        project: &str,
        dry_run: bool,
    ) -> Result<PruneEmbeddingsResponse> {
        get_json(
            self.client
                .post(service_url(&self.config, "/v1/prune-embeddings"))
                .headers(write_headers(&self.config)?)
                .json(&PruneEmbeddingsRequest {
                    project: project.to_string(),
                    dry_run,
                })
                .send()
                .await?,
        )
        .await
    }

    pub(crate) async fn archive_low_value(
        &self,
        project: &str,
        dry_run: bool,
    ) -> Result<ArchiveResponse> {
        get_json(
            self.client
                .post(service_url(&self.config, "/v1/archive"))
                .headers(write_headers(&self.config)?)
                .json(&ArchiveRequest {
                    project: project.to_string(),
                    max_confidence: 0.3,
                    max_importance: 1,
                    dry_run,
                })
                .send()
                .await?,
        )
        .await
    }

    pub(crate) async fn archive_memory(&self, memory_id: Uuid) -> Result<ArchiveMemoryResponse> {
        get_json(
            self.client
                .post(service_url(
                    &self.config,
                    &format!("/v1/memory/{memory_id}/archive"),
                ))
                .headers(write_headers(&self.config)?)
                .json(&serde_json::json!({}))
                .send()
                .await?,
        )
        .await
    }

    pub(crate) async fn delete_memory(&self, memory_id: Uuid) -> Result<DeleteMemoryResponse> {
        get_json(
            self.client
                .delete(service_url(&self.config, "/v1/memory"))
                .headers(write_headers(&self.config)?)
                .json(&DeleteMemoryRequest { memory_id })
                .send()
                .await?,
        )
        .await
    }

    pub(crate) async fn loop_definitions(
        &self,
        project: Option<&str>,
    ) -> Result<LoopDefinitionsResponse> {
        let mut request = self.client.get(service_url(&self.config, "/v1/loops"));
        if let Some(project) = project {
            request = request.query(&[("project", project)]);
        }
        get_json(request.send().await?).await
    }

    pub(crate) async fn loop_definition(
        &self,
        loop_id: &str,
        project: Option<&str>,
        repo_root: Option<&str>,
    ) -> Result<LoopDefinitionResponse> {
        let mut request = self
            .client
            .get(service_url(&self.config, &format!("/v1/loops/{loop_id}")));
        let mut query = Vec::new();
        if let Some(project) = project {
            query.push(("project", project));
        }
        if let Some(repo_root) = repo_root {
            query.push(("repo_root", repo_root));
        }
        if !query.is_empty() {
            request = request.query(&query);
        }
        get_json(request.send().await?).await
    }

    pub(crate) async fn loop_update_settings(
        &self,
        loop_id: &str,
        request: &LoopSettingsUpdateRequest,
    ) -> Result<codifix_record::LoopSettingResponse> {
        get_json(
            self.client
                .post(service_url(
                    &self.config,
                    &format!("/v1/loops/{loop_id}/settings"),
                ))
                .headers(write_headers(&self.config)?)
                .json(request)
                .send()
                .await?,
        )
        .await
    }
    pub(crate) async fn loop_run(
        &self,
        loop_id: &str,
        request: &LoopRunRequest,
    ) -> Result<LoopRunResponse> {
        self.loop_run_with_timeout(loop_id, request, None).await
    }

    /// `timeout` overrides the client-wide request timeout for runs that do
    /// synchronous LLM work server-side (e.g. consolidation synthesis).
    pub(crate) async fn loop_run_with_timeout(
        &self,
        loop_id: &str,
        request: &LoopRunRequest,
        timeout: Option<std::time::Duration>,
    ) -> Result<LoopRunResponse> {
        let mut builder = self
            .client
            .post(service_url(
                &self.config,
                &format!("/v1/loops/{loop_id}/run"),
            ))
            .headers(write_headers(&self.config)?)
            .json(request);
        if let Some(timeout) = timeout {
            builder = builder.timeout(timeout);
        }
        get_json(builder.send().await?).await
    }

    pub(crate) async fn loop_runs(
        &self,
        project: Option<&str>,
        loop_id: Option<&str>,
        status: Option<LoopRunStatus>,
        limit: i64,
    ) -> Result<LoopRunsResponse> {
        let mut query = vec![("limit", limit.to_string())];
        if let Some(project) = project {
            query.push(("project", project.to_string()));
        }
        if let Some(loop_id) = loop_id {
            query.push(("loop_id", loop_id.to_string()));
        }
        if let Some(status) = status {
            query.push(("status", status.as_str().to_string()));
        }
        get_json(
            self.client
                .get(service_url(&self.config, "/v1/loops/runs"))
                .query(&query)
                .send()
                .await?,
        )
        .await
    }

    pub(crate) async fn loop_run_detail(&self, run_id: Uuid) -> Result<LoopRunResponse> {
        get_json(
            self.client
                .get(service_url(
                    &self.config,
                    &format!("/v1/loops/runs/{run_id}"),
                ))
                .send()
                .await?,
        )
        .await
    }

    pub(crate) async fn loop_context_pack(
        &self,
        loop_id: &str,
        project: Option<&str>,
        repo_root: Option<&str>,
        run_id: Option<Uuid>,
        token_budget: usize,
        limit: usize,
    ) -> Result<LoopContextPackResponse> {
        let mut query = vec![
            ("token_budget", token_budget.to_string()),
            ("limit", limit.to_string()),
        ];
        if let Some(project) = project {
            query.push(("project", project.to_string()));
        }
        if let Some(repo_root) = repo_root {
            query.push(("repo_root", repo_root.to_string()));
        }
        if let Some(run_id) = run_id {
            query.push(("run_id", run_id.to_string()));
        }
        get_json(
            self.client
                .get(service_url(
                    &self.config,
                    &format!("/v1/loops/{loop_id}/context-pack"),
                ))
                .query(&query)
                .send()
                .await?,
        )
        .await
    }

    pub(crate) async fn loop_run_context_pack(
        &self,
        run_id: Uuid,
    ) -> Result<LoopContextPackResponse> {
        get_json(
            self.client
                .get(service_url(
                    &self.config,
                    &format!("/v1/loops/runs/{run_id}/context-pack"),
                ))
                .send()
                .await?,
        )
        .await
    }

    pub(crate) async fn loop_cancel(
        &self,
        run_id: Uuid,
        request: &LoopCancelRequest,
    ) -> Result<LoopRunResponse> {
        get_json(
            self.client
                .post(service_url(
                    &self.config,
                    &format!("/v1/loops/runs/{run_id}/cancel"),
                ))
                .headers(write_headers(&self.config)?)
                .json(request)
                .send()
                .await?,
        )
        .await
    }

    pub(crate) async fn loop_feedback(
        &self,
        run_id: Uuid,
        request: &LoopFeedbackRequest,
    ) -> Result<LoopRunResponse> {
        get_json(
            self.client
                .post(service_url(
                    &self.config,
                    &format!("/v1/loops/runs/{run_id}/feedback"),
                ))
                .headers(write_headers(&self.config)?)
                .json(request)
                .send()
                .await?,
        )
        .await
    }

    pub(crate) async fn loop_approvals(
        &self,
        project: Option<&str>,
        run_id: Option<Uuid>,
        loop_id: Option<&str>,
        status: Option<LoopApprovalStatus>,
        limit: i64,
    ) -> Result<LoopApprovalsResponse> {
        let mut query = vec![("limit", limit.to_string())];
        if let Some(project) = project {
            query.push(("project", project.to_string()));
        }
        if let Some(run_id) = run_id {
            query.push(("run_id", run_id.to_string()));
        }
        if let Some(loop_id) = loop_id {
            query.push(("loop_id", loop_id.to_string()));
        }
        if let Some(status) = status {
            query.push(("status", status.as_str().to_string()));
        }
        get_json(
            self.client
                .get(service_url(&self.config, "/v1/loops/approvals"))
                .query(&query)
                .send()
                .await?,
        )
        .await
    }

    pub(crate) async fn loop_approval_edit(
        &self,
        approval_id: Uuid,
        request: &LoopApprovalDecisionRequest,
    ) -> Result<LoopApprovalDecisionResponse> {
        get_json(
            self.client
                .post(service_url(
                    &self.config,
                    &format!("/v1/loops/approvals/{approval_id}/edit"),
                ))
                .headers(write_headers(&self.config)?)
                .json(request)
                .send()
                .await?,
        )
        .await
    }

    pub(crate) async fn loop_approval_decision(
        &self,
        approval_id: Uuid,
        approved: bool,
        request: &LoopApprovalDecisionRequest,
    ) -> Result<LoopApprovalDecisionResponse> {
        let action = if approved { "approve" } else { "reject" };
        get_json(
            self.client
                .post(service_url(
                    &self.config,
                    &format!("/v1/loops/approvals/{approval_id}/{action}"),
                ))
                .headers(write_headers(&self.config)?)
                .json(request)
                .send()
                .await?,
        )
        .await
    }

    pub(crate) async fn project_structure(
        &self,
        project: &str,
    ) -> Result<codifix_record::ProjectStructureResponse> {
        get_json(
            self.client
                .get(service_url(
                    &self.config,
                    &format!("/v1/projects/{project}/structure"),
                ))
                .send()
                .await?,
        )
        .await
    }

    pub(crate) async fn loop_memory_proposals(
        &self,
        project: Option<&str>,
        run_id: Option<Uuid>,
        loop_id: Option<&str>,
        status: Option<&str>,
        limit: i64,
    ) -> Result<LoopMemoryProposalsResponse> {
        let mut query = vec![("limit", limit.to_string())];
        if let Some(project) = project {
            query.push(("project", project.to_string()));
        }
        if let Some(run_id) = run_id {
            query.push(("run_id", run_id.to_string()));
        }
        if let Some(loop_id) = loop_id {
            query.push(("loop_id", loop_id.to_string()));
        }
        if let Some(status) = status {
            query.push(("status", status.to_string()));
        }
        get_json(
            self.client
                .get(service_url(&self.config, "/v1/loops/memory-proposals"))
                .query(&query)
                .send()
                .await?,
        )
        .await
    }

    pub(crate) async fn create_loop_memory_proposal(
        &self,
        request: &LoopMemoryProposalCreateRequest,
    ) -> Result<LoopMemoryProposalDecisionResponse> {
        get_json(
            self.client
                .post(service_url(&self.config, "/v1/loops/memory-proposals"))
                .headers(write_headers(&self.config)?)
                .json(request)
                .send()
                .await?,
        )
        .await
    }

    pub(crate) async fn loop_memory_proposal_decision(
        &self,
        proposal_id: Uuid,
        action: &str,
        request: &LoopMemoryProposalDecisionRequest,
    ) -> Result<LoopMemoryProposalDecisionResponse> {
        get_json(
            self.client
                .post(service_url(
                    &self.config,
                    &format!("/v1/loops/memory-proposals/{proposal_id}/{action}"),
                ))
                .headers(write_headers(&self.config)?)
                .json(request)
                .send()
                .await?,
        )
        .await
    }

    pub(crate) async fn loop_global_state(&self) -> Result<LoopGlobalStateResponse> {
        get_json(
            self.client
                .get(service_url(&self.config, "/v1/loops/global-kill-switch"))
                .send()
                .await?,
        )
        .await
    }

    pub(crate) async fn loop_set_global_state(
        &self,
        request: &LoopGlobalStateUpdateRequest,
    ) -> Result<LoopGlobalStateResponse> {
        get_json(
            self.client
                .post(service_url(&self.config, "/v1/loops/global-kill-switch"))
                .headers(write_headers(&self.config)?)
                .json(request)
                .send()
                .await?,
        )
        .await
    }
}

pub(crate) async fn get_json<T: serde::de::DeserializeOwned>(
    response: reqwest::Response,
) -> Result<T> {
    let status = response.status();
    let body = response.text().await?;
    if !status.is_success() {
        anyhow::bail!("{}", format_api_error(status, &body));
    }
    Ok(serde_json::from_str(&body)?)
}

pub(crate) async fn print_json_response(response: reqwest::Response) -> Result<()> {
    let status = response.status();
    let body = response.text().await?;
    if !status.is_success() {
        anyhow::bail!("{}", format_api_error(status, &body));
    }
    println!("{body}");
    Ok(())
}

pub(crate) fn format_api_error(status: reqwest::StatusCode, body: &str) -> String {
    let Ok(value) = serde_json::from_str::<serde_json::Value>(body) else {
        return format!("{status} {body}");
    };
    let mut parts = vec![format!(
        "{status} {}",
        value
            .get("error")
            .and_then(serde_json::Value::as_str)
            .unwrap_or(body)
    )];
    for (label, key) in [
        ("code", "code"),
        ("explanation", "explanation"),
        ("fix", "fix_hint"),
        ("doctor", "doctor_hint"),
        ("command", "command_hint"),
    ] {
        if let Some(text) = value.get(key).and_then(serde_json::Value::as_str) {
            parts.push(format!("{label}: {text}"));
        }
    }
    parts.join("\n")
}

pub(crate) fn diagnostic_severity_name(
    severity: &codifix_record::DiagnosticSeverity,
) -> &'static str {
    match severity {
        codifix_record::DiagnosticSeverity::Info => "info",
        codifix_record::DiagnosticSeverity::Warning => "warning",
        codifix_record::DiagnosticSeverity::Error => "error",
    }
}
