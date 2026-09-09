// SPDX-License-Identifier: AGPL-3.0-or-later

use crate::prelude::*;
use crate::*;
use mem_record::{CurateRequest, CurateResponse};
use std::collections::HashSet;

pub(crate) const QUERY_ACTIVITY_GRAPH_CONNECTION_LIMIT: usize = 5;

#[derive(Clone)]
pub(crate) struct AppState {
    pub(crate) role: ServiceRole,
    pub(crate) instance_id: String,
    pub(crate) startup_at: chrono::DateTime<chrono::Utc>,
    pub(crate) pool: Arc<RwLock<Option<PgPool>>>,
    pub(crate) offline: Option<OfflineRuntime>,
    pub(crate) api_token: String,
    pub(crate) config: AppConfig,
    pub(crate) web_root: Option<PathBuf>,
    pub(crate) http_client: reqwest::Client,
    pub(crate) embedders: Arc<tokio::sync::RwLock<EmbeddingRegistry>>,
    pub(crate) automated_embedding_creation_enabled: Arc<AtomicBool>,
    pub(crate) llm_audit: Arc<RwLock<LlmAuditConfig>>,
    pub(crate) events: broadcast::Sender<ServiceEvent>,
    pub(crate) recent_activity: Arc<Mutex<VecDeque<ServiceEvent>>>,
    pub(crate) watchers: Arc<Mutex<HashMap<String, WatcherPresence>>>,
    pub(crate) provenance: Arc<Mutex<ProvenanceRuntimeState>>,
    pub(crate) post_curation: PostCurationRuntime,
    pub(crate) federation: crate::federation::Registry,
    pub(crate) reinforcement: Option<crate::reinforcement::ReinforcementRuntime>,
    pub(crate) cluster: ClusterRuntime,
    pub(crate) shutdown: Arc<Mutex<Option<oneshot::Sender<()>>>>,
}

/// Serializes advisory curation work per project while allowing HTTP curation
/// requests to return as soon as their durable writes have committed.
#[derive(Clone, Default)]
pub(crate) struct PostCurationRuntime {
    state: Arc<Mutex<PostCurationQueue>>,
}

#[derive(Default)]
struct PostCurationQueue {
    pending: HashMap<String, VecDeque<PostCurationWork>>,
    running: HashSet<String>,
}

#[derive(Clone)]
pub(crate) struct PostCurationWork {
    pub(crate) request: CurateRequest,
    pub(crate) response: CurateResponse,
}

impl PostCurationRuntime {
    /// Queues work and returns true only for the caller that should start the
    /// project's background worker.
    pub(crate) fn enqueue(&self, work: PostCurationWork) -> bool {
        let project = work.request.project.clone();
        let mut state = self
            .state
            .lock()
            .expect("post-curation queue lock poisoned");
        state
            .pending
            .entry(project.clone())
            .or_default()
            .push_back(work);
        state.running.insert(project)
    }

    /// Takes every item currently queued for one running project. New work
    /// arriving after this call is handled by the next batch.
    pub(crate) fn take_batch(&self, project: &str) -> Vec<PostCurationWork> {
        let mut state = self
            .state
            .lock()
            .expect("post-curation queue lock poisoned");
        state
            .pending
            .get_mut(project)
            .map(|pending| pending.drain(..).collect())
            .unwrap_or_default()
    }

    /// Releases the worker only when no new work arrived while it was running.
    /// Returns true when the caller can exit its worker loop.
    pub(crate) fn finish_if_idle(&self, project: &str) -> bool {
        let mut state = self
            .state
            .lock()
            .expect("post-curation queue lock poisoned");
        if state
            .pending
            .get(project)
            .is_some_and(|pending| !pending.is_empty())
        {
            return false;
        }
        state.pending.remove(project);
        state.running.remove(project);
        true
    }
}

#[cfg(test)]
mod post_curation_tests {
    use super::*;

    fn work(project: &str, raw_capture_id: Uuid) -> PostCurationWork {
        PostCurationWork {
            request: CurateRequest {
                project: project.to_string(),
                batch_size: Some(1),
                raw_capture_id: Some(raw_capture_id),
                replacement_policy: None,
                dry_run: false,
            },
            response: CurateResponse {
                background_work: None,
                project_id: Uuid::new_v4(),
                run_id: Uuid::new_v4(),
                input_count: 1,
                output_count: 1,
                memory_ids: Vec::new(),
                replaced_count: 0,
                proposal_count: 0,
                replacements: Vec::new(),
                dry_run: false,
                warnings: Vec::new(),
                validation_due: Vec::new(),
                consolidation_due: Vec::new(),
            },
        }
    }

    #[test]
    fn post_curation_queue_starts_one_worker_and_preserves_fifo_batches() {
        let runtime = PostCurationRuntime::default();
        let first = Uuid::new_v4();
        let second = Uuid::new_v4();

        assert!(runtime.enqueue(work("memory", first)));
        assert!(!runtime.enqueue(work("memory", second)));

        let batch = runtime.take_batch("memory");
        assert_eq!(batch.len(), 2);
        assert_eq!(batch[0].request.raw_capture_id, Some(first));
        assert_eq!(batch[1].request.raw_capture_id, Some(second));
        assert!(runtime.finish_if_idle("memory"));

        assert!(runtime.enqueue(work("memory", Uuid::new_v4())));
    }

    #[test]
    fn post_curation_queue_keeps_worker_active_for_work_arriving_mid_batch() {
        let runtime = PostCurationRuntime::default();
        assert!(runtime.enqueue(work("memory", Uuid::new_v4())));
        assert_eq!(runtime.take_batch("memory").len(), 1);

        assert!(!runtime.enqueue(work("memory", Uuid::new_v4())));
        assert!(!runtime.finish_if_idle("memory"));
        assert_eq!(runtime.take_batch("memory").len(), 1);
        assert!(runtime.finish_if_idle("memory"));
    }

    #[test]
    fn post_curation_queue_recovers_after_a_failed_batch() {
        let runtime = PostCurationRuntime::default();
        assert!(runtime.enqueue(work("memory", Uuid::new_v4())));

        // The worker logs and discards a failed advisory job, then must release
        // its project slot so later durable captures are never stranded.
        let failed_batch = runtime.take_batch("memory");
        assert_eq!(failed_batch.len(), 1);
        assert!(runtime.finish_if_idle("memory"));

        assert!(runtime.enqueue(work("memory", Uuid::new_v4())));
        assert_eq!(runtime.take_batch("memory").len(), 1);
        assert!(runtime.finish_if_idle("memory"));
    }
}

#[derive(Clone, Debug)]
pub(crate) struct ServiceEvent {
    pub(crate) id: Uuid,
    /// Monotonic position in the durable timeline log; None until persisted
    /// (or when the event is ephemeral / the database is offline).
    pub(crate) seq: Option<i64>,
    pub(crate) project: String,
    pub(crate) memory_id: Option<Uuid>,
    pub(crate) kind: ActivityKind,
    pub(crate) summary: String,
    pub(crate) details: Option<ActivityDetails>,
    pub(crate) recorded_at: chrono::DateTime<chrono::Utc>,
    pub(crate) actor_id: Option<String>,
    pub(crate) actor_name: Option<String>,
    pub(crate) source: Option<String>,
    pub(crate) operation_id: Option<String>,
    pub(crate) duration_ms: Option<u64>,
    pub(crate) provider: Option<String>,
    pub(crate) model: Option<String>,
    pub(crate) token_usage: Option<TokenUsage>,
    pub(crate) include_activity: bool,
}

#[derive(Clone, Debug, Default, Serialize)]
pub(crate) struct ProvenanceRuntimeState {
    pub(crate) status: String,
    pub(crate) last_started_at: Option<chrono::DateTime<chrono::Utc>>,
    pub(crate) last_finished_at: Option<chrono::DateTime<chrono::Utc>>,
    pub(crate) last_project: Option<String>,
    pub(crate) checked_count: usize,
    pub(crate) stale_count: usize,
    pub(crate) error: Option<String>,
}

#[derive(Clone, Debug, Default, Serialize)]
pub(crate) struct ReinforcementRuntimeState {
    pub(crate) status: String,
    pub(crate) last_started_at: Option<chrono::DateTime<chrono::Utc>>,
    pub(crate) last_finished_at: Option<chrono::DateTime<chrono::Utc>>,
    pub(crate) pruned_access_events: u64,
    pub(crate) compacted_rows: u64,
    pub(crate) due_candidates: usize,
    pub(crate) validations_run: usize,
    pub(crate) daily_budget_remaining: Option<i64>,
    pub(crate) error: Option<String>,
}

#[derive(Clone, Debug)]
pub(crate) enum ServiceRole {
    Primary,
    Relay,
}

#[derive(Clone, Debug)]
pub(crate) struct ClusterRuntime {
    pub(crate) peers: Arc<Mutex<HashMap<String, ClusterPeer>>>,
}

#[derive(Clone, Debug, Serialize, SerdeDeserialize)]
#[serde(rename_all = "snake_case")]
pub(crate) enum DiscoveryKind {
    Discover,
    Announce,
}

#[derive(Clone, Debug, Serialize, SerdeDeserialize)]
pub(crate) struct DiscoveryPacket {
    pub(crate) kind: DiscoveryKind,
    pub(crate) service_id: String,
    pub(crate) advertise_addr: String,
    pub(crate) version: String,
    pub(crate) priority: i32,
    pub(crate) sent_at: chrono::DateTime<chrono::Utc>,
}

#[derive(Clone, Debug)]
pub(crate) struct ClusterPeer {
    pub(crate) service_id: String,
    pub(crate) advertise_addr: String,
    pub(crate) version: String,
    pub(crate) priority: i32,
    pub(crate) last_seen: chrono::DateTime<chrono::Utc>,
}

pub(crate) const WATCHER_STALE_AFTER_SECONDS: u64 = 90;
pub(crate) const WATCHER_RESTART_BACKOFF_SECONDS: u64 = 120;
pub(crate) const WATCHER_EXPIRY_AFTER_SECONDS: u64 = 600;
pub(crate) const WATCHER_MAX_RESTART_ATTEMPTS: u32 = 3;

impl AppState {
    pub(crate) fn is_primary(&self) -> bool {
        matches!(self.role, ServiceRole::Primary)
    }

    pub(crate) fn role_name(&self) -> &'static str {
        match self.role {
            ServiceRole::Primary => "primary",
            ServiceRole::Relay => "relay",
        }
    }

    pub(crate) fn pool(&self) -> Result<PgPool, ApiError> {
        self.pool
            .read()
            .expect("database pool lock poisoned")
            .clone()
            .ok_or_else(|| {
                if self.offline.is_some() {
                    ApiError::service_unavailable(
                        "PostgreSQL is unavailable; service is running in offline degraded mode",
                    )
                } else {
                    ApiError::service_unavailable("relay has no local database connection")
                }
            })
    }

    pub(crate) fn pool_available(&self) -> bool {
        self.pool
            .read()
            .expect("database pool lock poisoned")
            .is_some()
    }

    pub(crate) fn set_pool(&self, pool: PgPool) {
        *self.pool.write().expect("database pool lock poisoned") = Some(pool);
    }

    pub(crate) fn offline_store(&self) -> Option<OfflineStore> {
        self.offline.as_ref().map(|runtime| runtime.store.clone())
    }
}
