// SPDX-License-Identifier: AGPL-3.0-or-later

use crate::prelude::*;
use crate::*;

const HTTP_SHUTDOWN_GRACE_PERIOD: Duration = Duration::from_secs(2);

mod schema;

#[cfg(test)]
mod evaluation_tests {
    use super::*;
    mod http;

    fn config(read_only: bool) -> AppConfig {
        serde_json::from_value(serde_json::json!({
            "service": {"bind_addr":"127.0.0.1:0", "api_token":"synthetic", "read_only":read_only},
            "database": {"url": std::env::var("MEMORY_LAYER_TEST_DATABASE_URL").unwrap_or_default()},
            "evaluation": {"semantic_time":"2026-01-01T00:00:00Z"},
            "offline":{"enabled":false}, "cluster":{"enabled":false},
            "reinforcement":{"enabled":true}
        })).unwrap()
    }

    #[tokio::test]
    async fn cloned_evaluation_database_satisfies_service_seal() {
        let Some(db) = mem_test_support::IsolatedDatabase::new().await else {
            return;
        };
        let frozen = db.freeze().await.unwrap();
        let reader = frozen.fork_read_only().await.unwrap();
        verify_evaluation_seal(&reader.pool).await.unwrap();
        reader.cleanup().await.unwrap();
        frozen.cleanup().await.unwrap();
    }

    #[tokio::test]
    async fn real_query_on_sealed_clones_preserves_corpus_embeddings_and_reinforcement() {
        use mem_test_support::fingerprint::benchmark_fingerprint;
        use sqlx::ConnectOptions;
        let Some(db) = mem_test_support::IsolatedDatabase::new().await else {
            return;
        };
        let project = Uuid::new_v4();
        let other = Uuid::new_v4();
        let hot = Uuid::new_v4();
        let cold = Uuid::new_v4();
        let forbidden = Uuid::new_v4();
        for (id, slug) in [(project, "r3-corpus"), (other, "r3-other")] {
            sqlx::query(
                "INSERT INTO projects(id,slug,name,root_path) VALUES($1,$2,$2,'/synthetic')",
            )
            .bind(id)
            .bind(slug)
            .execute(&db.pool)
            .await
            .unwrap();
        }
        for (id, owner) in [(hot, project), (cold, project), (forbidden, other)] {
            sqlx::query("INSERT INTO memory_entries
                (id,project_id,canonical_id,version_no,is_tombstone,canonical_text,summary,memory_type,scope,importance,confidence,created_at,updated_at,search_document)
                VALUES($1,$2,$1,1,false,'Reconcile settlement acknowledgement before retry.',
                'Settlement reconciliation','implementation','project',3,0.9,
                '2025-12-01T00:00:00Z','2025-12-01T00:00:00Z',to_tsvector('english','Reconcile settlement acknowledgement before retry.'))")
                .bind(id).bind(owner).execute(&db.pool).await.unwrap();
        }
        sqlx::query("INSERT INTO memory_scores(canonical_id,project_id,activation,access_count,last_decay_at)
            VALUES($1,$2,10,7,'2025-12-31T00:00:00Z')")
            .bind(hot).bind(project).execute(&db.pool).await.unwrap();
        let mut registry = mem_search::test_support::static_embedding_registry(&[("r3-fake", 1.0)]);
        registry.set_active("r3-fake").unwrap();
        for slug in ["r3-corpus", "r3-other"] {
            mem_search::rebuild_chunks(&db.pool, slug, &registry, None)
                .await
                .unwrap();
        }
        let before = benchmark_fingerprint(&db.pool).await.unwrap();
        let frozen = db.freeze().await.unwrap();
        let mut previous = None;
        for _ in 0..2 {
            let reader = frozen.fork_read_only().await.unwrap();
            assert_eq!(benchmark_fingerprint(&reader.pool).await.unwrap(), before);
            let mut cfg = config(true);
            cfg.database.url = reader.pool.connect_options().to_url_lossy().to_string();
            let state = build_state(cfg, Arc::new(Mutex::new(None))).await.unwrap();
            *state.embedders.write().await = registry.clone(); // Fake provider, real retrieval path.
            let request: QueryRequest = serde_json::from_value(serde_json::json!({
                "project":"r3-corpus", "query":"settlement acknowledgement retry",
                "top_k":5, "answer_mode":"deterministic", "retrieval_mode":"full-memory"
            }))
            .unwrap();
            let Json(answer) = crate::handlers::query::query(State(state.clone()), Json(request))
                .await
                .unwrap();
            assert_eq!(answer.results.len(), 2);
            assert_eq!(answer.results[0].memory_id, hot);
            assert!(answer.results.iter().all(|r| r.memory_id != forbidden));
            assert!(
                answer
                    .results
                    .iter()
                    .any(|r| r.debug.semantic_similarity > 0.0)
            );
            assert!(
                answer.results[0]
                    .score_explanation
                    .iter()
                    .any(|s| s.starts_with("activation"))
            );
            let scores: Vec<_> = answer
                .results
                .iter()
                .map(|r| (r.memory_id, r.score, r.debug.recency_boost))
                .collect();
            if let Some(previous) = &previous {
                assert_eq!(&scores, previous);
            }
            previous = Some(scores);
            assert!(state.recent_activity.lock().unwrap().is_empty());
            assert_eq!(benchmark_fingerprint(&reader.pool).await.unwrap(), before);
            state.pool().unwrap().close().await;
            drop(state);
            reader.cleanup().await.unwrap();
        }
        frozen.cleanup().await.unwrap();
    }

    #[tokio::test]
    async fn evaluation_rejects_writable_service_before_connecting() {
        let result = build_state(config(false), Arc::new(Mutex::new(None))).await;
        assert!(result.err().unwrap().to_string().contains("read_only"));
    }

    #[tokio::test]
    async fn evaluation_keeps_scoring_but_has_no_learning_or_workers() {
        let Some(db) = mem_test_support::IsolatedDatabase::new().await else {
            return;
        };
        assert!(verify_evaluation_seal(&db.pool).await.is_err());
        let role = format!("r3_reader_{}", Uuid::new_v4().simple());
        sqlx::raw_sql(&format!(
            "CREATE ROLE {role} LOGIN PASSWORD 'disposable-only';
            GRANT USAGE ON SCHEMA public TO {role};
            GRANT SELECT ON ALL TABLES IN SCHEMA public TO {role};
            GRANT INSERT,UPDATE ON external_effects,external_effect_outcomes,
                foreground_executions,auth_audit_events TO {role};"
        ))
        .execute(&db.pool)
        .await
        .unwrap();
        let mut cfg = config(true);
        let mut url = reqwest::Url::parse(&cfg.database.url).unwrap();
        url.set_username(&role).unwrap();
        url.set_password(Some("disposable-only")).unwrap();
        url.set_path(db.pool.connect_options().get_database().unwrap());
        cfg.database.url = url.to_string();
        let state = build_state(cfg, Arc::new(Mutex::new(None))).await.unwrap();
        assert!(state.config.reinforcement.enabled);
        assert!(state.reinforcement.is_none());
        assert!(crate::background::start(state.clone()).is_none());
        assert!(crate::effects::start(state.clone()).is_none());
        assert!(start_cluster_tasks(state.clone()).await.unwrap().is_empty());
        assert!(
            crate::federation::start(state.clone(), Some(PathBuf::from("never-read")))
                .await
                .is_err()
        );
        notify_project_changed(
            &state,
            "evaluation-no-timeline".into(),
            None,
            ActivityKind::Query,
            "not persisted".into(),
            None,
        );
        assert!(state.recent_activity.lock().unwrap().is_empty());
        let pool = state.pool().unwrap();
        // nextval is a write even though it appears in SELECT. A granted
        // sequence capability must not bypass the frozen benchmark contract.
        sqlx::raw_sql(&format!(
            "CREATE SEQUENCE r3_seal_probe;
             GRANT USAGE ON SEQUENCE r3_seal_probe TO {role};"
        ))
        .execute(&db.pool)
        .await
        .unwrap();
        assert!(
            verify_evaluation_seal(&pool)
                .await
                .unwrap_err()
                .to_string()
                .contains("sequence writes")
        );
        sqlx::query(&format!(
            "REVOKE USAGE ON SEQUENCE r3_seal_probe FROM {role}"
        ))
        .execute(&db.pool)
        .await
        .unwrap();
        verify_evaluation_seal(&pool).await.unwrap();
        let error = sqlx::query("SELECT nextval('r3_seal_probe')")
            .execute(&pool)
            .await
            .unwrap_err();
        assert_eq!(
            error.as_database_error().unwrap().code().as_deref(),
            Some("42501")
        );
        let error = sqlx::query("UPDATE memory_scores SET activation=0")
            .execute(&pool)
            .await
            .unwrap_err();
        assert_eq!(
            error.as_database_error().unwrap().code().as_deref(),
            Some("42501")
        );
        let error = sqlx::query("DELETE FROM memory_entries")
            .execute(&pool)
            .await
            .unwrap_err();
        assert_eq!(
            error.as_database_error().unwrap().code().as_deref(),
            Some("42501")
        );
        pool.close().await;
        drop(state);
        sqlx::raw_sql(&format!("DROP OWNED BY {role}; DROP ROLE {role};"))
            .execute(&db.pool)
            .await
            .unwrap();
        db.cleanup().await.unwrap();
    }
}

pub async fn run_service(config_path: Option<PathBuf>) -> Result<()> {
    tracing_subscriber::registry()
        .with(tracing_subscriber::EnvFilter::from_default_env())
        .with(tracing_subscriber::fmt::layer().json())
        .init();

    let mut config_fingerprint = config_path_fingerprint(config_path.as_deref())
        .await
        .context("inspect config file")?;

    loop {
        let config = AppConfig::load_from_path(config_path.clone()).context("load config")?;
        let addr: SocketAddr = config
            .service
            .bind_addr
            .parse()
            .context("parse bind_addr")?;
        if config.auth.mode == mem_record::AuthMode::MultiUser {
            let oidc = &config.auth.oidc;
            if oidc.issuer_url.trim().is_empty()
                || oidc.client_id.trim().is_empty()
                || config
                    .auth
                    .public_base_url
                    .as_deref()
                    .is_none_or(|url| url.trim().is_empty())
            {
                anyhow::bail!(
                    "auth.mode = \"multi_user\" requires auth.public_base_url, \
                     auth.oidc.issuer_url, and auth.oidc.client_id to be set - \
                     refusing to start with a half-configured identity provider"
                );
            }
        }
        let (shutdown_tx, shutdown_rx) = oneshot::channel();
        let shutdown_handle = Arc::new(Mutex::new(Some(shutdown_tx)));
        let state = build_state(config.clone(), shutdown_handle.clone()).await?;
        let _background_tasks = crate::background::start(state.clone());
        let _supervisor_checks = crate::supervisor::start(state.clone());
        let supervisor_model = crate::supervisor::model::start(state.clone());
        let _effect_recovery = crate::effects::start(state.clone());
        let _federation_tasks = federation::start(
            state.clone(),
            std::env::var_os("MEMORY_LAYER_FEDERATION_CONFIG").map(PathBuf::from),
        )
        .await?;
        let app = routes::build_http_app(state.clone());
        let listener = bind_http_listener_with_handoff(&config, addr)
            .await
            .context("bind http listener")?;
        let supervisor_shutdown = supervisor_model.as_ref().map(|worker| worker.control());
        let mut http_server = tokio::spawn(async move {
            axum::serve(
                listener,
                app.into_make_service_with_connect_info::<std::net::SocketAddr>(),
            )
            .with_graceful_shutdown(async {
                let _ = shutdown_rx.await;
                if let Some(control) = supervisor_shutdown {
                    control.stop();
                }
            })
            .await
        });
        let mut cluster_tasks = start_cluster_tasks(state.clone()).await?;

        let version = config.profile.display_version(env!("CARGO_PKG_VERSION"));
        eprintln!(
            "memory-layer v{version} listening on {addr} (profile={profile}, role={role}, service_id={service_id}, cluster={cluster})",
            profile = config.profile,
            role = state.role_name(),
            service_id = config.cluster.service_id,
            cluster = if config.cluster.enabled {
                "enabled"
            } else {
                "disabled"
            },
        );
        if matches!(config.profile, mem_config::Profile::Dev) {
            eprintln!("{}", mem_platform::dev_mode_status_line(None));
        }
        if let Some(path) = config.resolved_config_path.as_deref() {
            eprintln!("  config: {}", path.display());
        }
        if let Some(path) = config.resolved_dev_overlay_path.as_deref() {
            eprintln!("  dev overlay: {}", path.display());
        }
        eprintln!(
            "  database: {}",
            if state.pool_available() {
                "connected"
            } else if state.offline.is_some() {
                "offline degraded"
            } else {
                "unavailable"
            }
        );
        if let Some(offline) = &state.offline {
            eprintln!("  offline db: {}", offline.store.path().display());
        }
        tracing::info!(
            %addr,
            role = %state.role_name(),
            "memory-layer listening"
        );

        if let Some(path) = config_path.as_deref() {
            tokio::select! {
                result = &mut http_server => {
                    if let Some(worker) = &supervisor_model { worker.stop(); }
                    result.context("join mem-service task")??;
                    break;
                }
                result = tokio::signal::ctrl_c() => {
                    if let Some(worker) = &supervisor_model { worker.stop(); }
                    result.context("listen for ctrl-c")?;
                    shutdown_http_server(&shutdown_handle, &mut http_server, "ctrl-c").await?;
                    abort_tasks(&mut cluster_tasks);
                    break;
                }
                result = wait_for_config_change(path, config_fingerprint) => {
                    if let Some(worker) = &supervisor_model { worker.stop(); }
                    config_fingerprint = result.context("watch config file")?;
                    tracing::info!(path = %path.display(), "config changed; restarting backend");
                    shutdown_http_server(&shutdown_handle, &mut http_server, "config-reload").await?;
                    abort_tasks(&mut cluster_tasks);
                }
            }
        } else {
            tokio::select! {
                result = &mut http_server => {
                    if let Some(worker) = &supervisor_model { worker.stop(); }
                    result.context("join mem-service task")??;
                    break;
                }
                result = tokio::signal::ctrl_c() => {
                    if let Some(worker) = &supervisor_model { worker.stop(); }
                    result.context("listen for ctrl-c")?;
                    shutdown_http_server(&shutdown_handle, &mut http_server, "ctrl-c").await?;
                    abort_tasks(&mut cluster_tasks);
                    break;
                }
            }
        }
    }

    Ok(())
}

pub(crate) async fn build_state(
    config: AppConfig,
    shutdown: Arc<Mutex<Option<oneshot::Sender<()>>>>,
) -> Result<AppState> {
    if config.evaluation.active() {
        anyhow::ensure!(
            config.service.read_only,
            "evaluation requires service.read_only = true"
        );
        anyhow::ensure!(
            !config.cluster.enabled,
            "evaluation prohibits cluster operation"
        );
        anyhow::ensure!(
            !config.offline.enabled,
            "evaluation prohibits offline replay"
        );
    }
    config
        .background_work
        .validate()
        .map_err(anyhow::Error::msg)?;
    let http_client = reqwest::Client::builder()
        .timeout(config.service.request_timeout)
        .build()
        .context("build service http client")?;
    let pool_attempt = connect_primary_pool(&config).await;
    // This explicitly selected deployment must not disguise an unverified
    // history (or unavailable prepared DB) as a usable offline/relay service.
    let pool_attempt =
        if config.database.migrations == mem_config::DatabaseMigrationMode::VerifyOnly {
            Ok(pool_attempt.context("verify prepared database before service startup")?)
        } else {
            pool_attempt
        };
    let offline = build_offline_runtime(&config).await?;
    let (events, _) = broadcast::channel(128);
    let (role, pool) = match pool_attempt {
        Ok(pool) => (ServiceRole::Primary, Some(pool)),
        Err(error) if offline.is_some() => {
            tracing::warn!(
                error = %error,
                "postgres unavailable; starting in offline degraded mode"
            );
            (ServiceRole::Primary, None)
        }
        Err(error) if config.cluster.enabled => {
            tracing::warn!(
                error = %error,
                "postgres unavailable; starting in relay mode"
            );
            (ServiceRole::Relay, None)
        }
        Err(error) => return Err(error).context("connect postgres"),
    };

    let embedders = Arc::new(tokio::sync::RwLock::new(EmbeddingRegistry::from_config(
        &config.embeddings,
    )));
    let automated_embedding_creation_enabled =
        Arc::new(AtomicBool::new(config.embeddings.create_enabled));
    let llm_audit = Arc::new(RwLock::new(config.llm_audit.clone()));
    let reinforcement = if config.evaluation.active() {
        None
    } else {
        crate::reinforcement::build_runtime(&config.reinforcement)
    };

    let state = AppState {
        configuration_write: Arc::new(tokio::sync::Mutex::new(())),
        role,
        instance_id: Uuid::new_v4().to_string(),
        startup_at: chrono::Utc::now(),
        pool: Arc::new(RwLock::new(pool)),
        offline,
        api_token: config.service.api_token.clone(),
        web_root: discover_web_root(&config),
        http_client,
        embedders,
        automated_embedding_creation_enabled,
        llm_audit,
        config,
        events,
        recent_activity: Arc::new(Mutex::new(VecDeque::with_capacity(20))),
        watchers: Arc::new(Mutex::new(HashMap::new())),
        provenance: Arc::new(Mutex::new(ProvenanceRuntimeState {
            status: "idle".to_string(),
            ..ProvenanceRuntimeState::default()
        })),
        background: crate::background::Runtime::default(),
        supervisor_model: Default::default(),
        federation: Default::default(),
        reinforcement: reinforcement.as_ref().map(|(runtime, _)| runtime.clone()),
        cluster: ClusterRuntime {
            peers: Arc::new(Mutex::new(HashMap::new())),
        },
        shutdown,
    };
    if let Some((_, rx)) = reinforcement {
        crate::reinforcement::spawn_access_worker(state.clone(), rx);
    }
    start_offline_sync_task(state.clone());
    Ok(state)
}

pub(crate) async fn connect_primary_pool(config: &AppConfig) -> Result<PgPool> {
    let pool = PgPoolOptions::new()
        .max_connections(10)
        .connect(&config.database.url)
        .await
        .context("connect postgres")?;
    if config.evaluation.active() {
        // No schema/loop/auth bootstrap writes against a prepared snapshot.
        verify_evaluation_seal(&pool).await?;
        return Ok(pool);
    }
    if config.database.migrations == mem_config::DatabaseMigrationMode::VerifyOnly {
        schema::verify_history(&pool).await?;
    } else {
        let mut migrator = compatible_migrator();
        if config.profile == mem_config::Profile::Dev {
            migrator.set_ignore_missing(true);
        }
        migrator.run(&pool).await.context(
            "run migrations (pgvector extension 'vector' must be installed in PostgreSQL)",
        )?;
    }
    register_builtin_loop_definitions(&pool)
        .await
        .context("register builtin loop definitions")?;
    crate::auth::ensure_local_owner_principal(&pool)
        .await
        .context("bootstrap local-owner principal")?;
    Ok(pool)
}

/// Defense in depth: measurement must use a non-owner role with no knowledge
/// write privileges. Operational effect journals are the explicit exception.
async fn verify_evaluation_seal(pool: &PgPool) -> Result<()> {
    let privileged: bool = sqlx::query_scalar(
        "SELECT rolsuper OR rolcreatedb OR rolcreaterole OR rolbypassrls OR rolreplication
         OR has_schema_privilege(current_user, 'public', 'CREATE')
         OR EXISTS (SELECT 1 FROM pg_auth_members WHERE member = r.oid)
         FROM pg_roles r WHERE rolname = current_user",
    )
    .fetch_one(pool)
    .await?;
    anyhow::ensure!(
        !privileged,
        "evaluation database role must be sealed and non-privileged"
    );
    let writable: bool = sqlx::query_scalar(
        "SELECT EXISTS (SELECT 1 FROM pg_tables WHERE schemaname='public'
         AND tablename NOT IN ('external_effects','external_effect_outcomes',
             'foreground_executions','auth_audit_events')
         AND has_table_privilege(current_user, quote_ident(schemaname)||'.'||quote_ident(tablename),
             'INSERT,UPDATE,DELETE,TRUNCATE,TRIGGER'))",
    )
    .fetch_one(pool)
    .await?;
    anyhow::ensure!(!writable, "evaluation database allows benchmark writes");
    let writable_sequences: bool = sqlx::query_scalar(
        "SELECT EXISTS (SELECT 1 FROM pg_class c JOIN pg_namespace n ON n.oid=c.relnamespace
         WHERE n.nspname='public' AND c.relkind='S'
         AND has_sequence_privilege(current_user, c.oid, 'USAGE,UPDATE'))",
    )
    .fetch_one(pool)
    .await?;
    anyhow::ensure!(
        !writable_sequences,
        "evaluation database allows sequence writes"
    );
    let unsafe_functions: bool = sqlx::query_scalar(
        "SELECT EXISTS (SELECT 1 FROM pg_proc p JOIN pg_namespace n ON n.oid=p.pronamespace
         WHERE n.nspname='public' AND p.prosecdef AND has_function_privilege(current_user,p.oid,'EXECUTE'))",
    ).fetch_one(pool).await?;
    anyhow::ensure!(
        !unsafe_functions,
        "evaluation database exposes security-definer functions"
    );
    // Catch unprovisioned snapshots early instead of accepting a blank database.
    sqlx::query("SELECT id FROM memory_entries LIMIT 0")
        .execute(pool)
        .await?;
    Ok(())
}

fn compatible_migrator() -> sqlx::migrate::Migrator {
    const PGVECTOR_MIGRATION_WITHOUT_LEGACY_HNSW: &str = r#"
CREATE EXTENSION IF NOT EXISTS vector;

DROP INDEX IF EXISTS idx_memory_chunks_embedding_hnsw;

ALTER TABLE memory_chunks
    DROP COLUMN IF EXISTS embedding;

ALTER TABLE memory_chunks
    ADD COLUMN IF NOT EXISTS embedding vector;
"#;

    let mut migrator = sqlx::migrate!("../../migrations");
    migrator.migrations = std::borrow::Cow::Owned(
        migrator
            .migrations
            .iter()
            .cloned()
            .map(|mut migration| {
                if migration.version == 4 {
                    // pgvector rejects HNSW indexes on dimensionless vector columns.
                    // Migration 16 removes this obsolete index, so fresh databases can
                    // safely omit its creation. Keep migration 4's original checksum so
                    // databases that already applied it remain migration-compatible.
                    migration.sql =
                        std::borrow::Cow::Borrowed(PGVECTOR_MIGRATION_WITHOUT_LEGACY_HNSW);
                }
                migration
            })
            .collect(),
    );
    migrator
}

#[cfg(test)]
mod migration_compat_tests {
    #[test]
    fn pending_pgvector_migration_omits_invalid_dimensionless_hnsw_index() {
        let original = sqlx::migrate!("../../migrations");
        let compatible = super::compatible_migrator();
        let original_migration = original
            .iter()
            .find(|migration| migration.version == 4)
            .unwrap();
        let compatible_migration = compatible
            .iter()
            .find(|migration| migration.version == 4)
            .unwrap();

        assert!(original_migration.sql.contains("CREATE INDEX"));
        assert!(!compatible_migration.sql.contains("CREATE INDEX"));
        assert_eq!(compatible_migration.checksum, original_migration.checksum);
    }
}

#[cfg(not(feature = "offline"))]
async fn build_offline_runtime(config: &AppConfig) -> Result<Option<OfflineRuntime>> {
    if config.offline.enabled {
        tracing::warn!(
            "config enables [offline] but this build was compiled without the offline feature; \
             offline capture is disabled"
        );
    }
    Ok(None)
}

#[cfg(feature = "offline")]
async fn build_offline_runtime(config: &AppConfig) -> Result<Option<OfflineRuntime>> {
    if !config.offline.enabled {
        return Ok(None);
    }
    let path = offline_database_path(config)
        .ok_or_else(|| anyhow::anyhow!("resolve offline DuckDB path"))?;
    let store = OfflineStore::open(path)
        .await
        .context("open offline DuckDB")?;
    Ok(Some(OfflineRuntime::new(store)))
}

#[cfg(feature = "offline")]
fn offline_database_path(config: &AppConfig) -> Option<PathBuf> {
    if let Some(path) = &config.offline.path {
        return Some(path.clone());
    }
    config
        .resolved_dev_overlay_path
        .as_deref()
        .or(config.resolved_config_path.as_deref())
        .and_then(|path| {
            path.parent()
                .map(|parent| parent.join("memory-layer.duckdb"))
        })
        .or_else(|| preferred_user_state_dir().map(|dir| dir.join("memory-layer.duckdb")))
}

fn start_offline_sync_task(state: AppState) {
    if state.offline.is_none() {
        return;
    }
    tokio::spawn(async move {
        loop {
            if let Err(error) = sync_offline_once(&state).await
                && let Some(offline) = &state.offline
            {
                offline
                    .state
                    .lock()
                    .expect("offline sync state lock poisoned")
                    .last_error = Some(error.to_string());
            }
            tokio::time::sleep(state.config.offline.reconnect_interval).await;
        }
    });
}

async fn sync_offline_once(state: &AppState) -> Result<()> {
    let Some(offline) = &state.offline else {
        return Ok(());
    };
    if offline.store.pending_count().await? == 0 {
        return Ok(());
    }
    let pool = match state.pool() {
        Ok(pool) => pool,
        Err(_) => {
            let pool = connect_primary_pool(&state.config).await?;
            state.set_pool(pool.clone());
            pool
        }
    };
    sync_offline_batch(&pool, offline, state.config.offline.sync_batch_size).await
}

pub(crate) fn discover_web_root(config: &AppConfig) -> Option<PathBuf> {
    if let Some(root) = &config.service.web_root {
        let path = PathBuf::from(root);
        if path.join("index.html").is_file() {
            return Some(path);
        }
    }

    let candidates = vec![
        Some(PathBuf::from("web").join("dist")),
        mem_platform::current_exe_share_subdir("web"),
        mem_platform::preferred_user_state_dir().map(|dir| dir.join("web")),
        std::env::var("HOME")
            .ok()
            .map(|home| PathBuf::from(home).join(".local/share/memory-layer/web")),
        Some(PathBuf::from("/usr/share/memory-layer/web")),
    ];

    candidates
        .into_iter()
        .flatten()
        .find(|candidate| candidate.join("index.html").is_file())
}

pub(crate) fn abort_tasks(tasks: &mut Vec<JoinHandle<Result<()>>>) {
    for task in tasks.drain(..) {
        task.abort();
    }
}

pub(crate) fn request_runtime_shutdown(shutdown: &Arc<Mutex<Option<oneshot::Sender<()>>>>) {
    if let Some(sender) = shutdown.lock().expect("shutdown mutex poisoned").take() {
        let _ = sender.send(());
    }
}

pub(crate) async fn shutdown_http_server(
    shutdown: &Arc<Mutex<Option<oneshot::Sender<()>>>>,
    http_server: &mut JoinHandle<std::io::Result<()>>,
    reason: &str,
) -> Result<()> {
    shutdown_http_server_with_timeout(shutdown, http_server, HTTP_SHUTDOWN_GRACE_PERIOD, reason)
        .await
}

pub(crate) async fn shutdown_http_server_with_timeout(
    shutdown: &Arc<Mutex<Option<oneshot::Sender<()>>>>,
    http_server: &mut JoinHandle<std::io::Result<()>>,
    grace_period: Duration,
    reason: &str,
) -> Result<()> {
    request_runtime_shutdown(shutdown);
    tokio::select! {
        result = &mut *http_server => {
            result.context("join mem-service task")??;
        }
        _ = tokio::time::sleep(grace_period) => {
            tracing::warn!(
                reason,
                grace_period_ms = grace_period.as_millis(),
                "HTTP server did not stop during graceful shutdown window; aborting"
            );
            http_server.abort();
            match http_server.await {
                Ok(result) => result.context("run mem-service task after forced shutdown")?,
                Err(error) if error.is_cancelled() => {}
                Err(error) => return Err(error).context("join mem-service task after forced shutdown"),
            }
        }
    }
    Ok(())
}

pub(crate) async fn bind_http_listener_with_handoff(
    config: &AppConfig,
    addr: SocketAddr,
) -> Result<TcpListener> {
    match tokio::net::TcpListener::bind(addr).await {
        Ok(listener) => Ok(listener),
        Err(error) if error.kind() == ErrorKind::AddrInUse => {
            if !request_existing_instance_shutdown(config).await? {
                return Err(error).context("address already in use and handoff was refused");
            }
            wait_for_listener_release(addr).await?;
            bind_tcp_listener_with_addr_in_use_wait(addr, "http listener after handoff").await
        }
        Err(error) => Err(error).context("bind http listener"),
    }
}

pub(crate) async fn request_existing_instance_shutdown(config: &AppConfig) -> Result<bool> {
    let client = reqwest::Client::builder()
        .timeout(Duration::from_secs(2))
        .build()
        .context("build handoff client")?;
    let response = match client
        .post(format!(
            "http://{}/v1/admin/shutdown",
            config.service.bind_addr
        ))
        .header("x-api-token", &config.service.api_token)
        .send()
        .await
    {
        Ok(response) => response,
        Err(error) => {
            tracing::warn!(error = %error, "failed to contact existing backend for handoff");
            return Ok(false);
        }
    };
    Ok(response.status().is_success())
}

pub(crate) async fn wait_for_listener_release(addr: SocketAddr) -> Result<()> {
    for _ in 0..20 {
        match tokio::net::TcpListener::bind(addr).await {
            Ok(listener) => {
                drop(listener);
                return Ok(());
            }
            Err(error) if error.kind() == ErrorKind::AddrInUse => {
                tokio::time::sleep(Duration::from_millis(250)).await;
            }
            Err(error) => return Err(error).context("wait for listener release"),
        }
    }
    anyhow::bail!("timed out waiting for existing backend to release {addr}");
}

pub(crate) async fn bind_tcp_listener_with_addr_in_use_wait(
    addr: SocketAddr,
    context_label: &'static str,
) -> Result<TcpListener> {
    match tokio::net::TcpListener::bind(addr).await {
        Ok(listener) => Ok(listener),
        Err(error) if error.kind() == ErrorKind::AddrInUse => {
            wait_for_listener_release(addr).await?;
            tokio::net::TcpListener::bind(addr)
                .await
                .with_context(|| format!("bind {context_label} after waiting for release"))
        }
        Err(error) => Err(error).with_context(|| format!("bind {context_label}")),
    }
}

pub(crate) async fn start_cluster_tasks(state: AppState) -> Result<Vec<JoinHandle<Result<()>>>> {
    let mut tasks = Vec::new();
    if state.config.evaluation.active() {
        return Ok(tasks);
    }
    if state.is_primary() {
        tasks.push(tokio::spawn(run_watcher_watchdog(state.clone())));
        if state.config.provenance.reverify_enabled {
            tasks.push(tokio::spawn(run_provenance_reverify_scheduler(
                state.clone(),
            )));
        }
        if state.reinforcement.is_some() {
            tasks.push(tokio::spawn(
                crate::repository::handlers::reinforcement::run_reinforcement_scheduler(
                    state.clone(),
                ),
            ));
        }
    }
    if !state.config.cluster.enabled {
        return Ok(tasks);
    }

    let socket = Arc::new(bind_cluster_socket(
        &state.config.cluster.discovery_multicast_addr,
    )?);
    tasks.push(tokio::spawn(run_cluster_listener(
        socket.clone(),
        state.clone(),
    )));
    if state.is_primary() {
        tasks.push(tokio::spawn(run_cluster_announcer(socket, state)));
    } else {
        tasks.push(tokio::spawn(run_cluster_discoverer(socket, state)));
    }
    Ok(tasks)
}

pub(crate) fn bind_cluster_socket(multicast_addr: &str) -> Result<UdpSocket> {
    let addr: SocketAddr = multicast_addr
        .parse()
        .context("parse cluster multicast addr")?;
    let ip = match addr.ip() {
        std::net::IpAddr::V4(ip) => ip,
        std::net::IpAddr::V6(_) => {
            anyhow::bail!("cluster.discovery_multicast_addr must be an IPv4 multicast address")
        }
    };

    let socket = Socket::new(Domain::IPV4, Type::DGRAM, Some(Protocol::UDP))
        .context("create discovery socket")?;
    socket
        .set_reuse_address(true)
        .context("set discovery SO_REUSEADDR")?;
    #[cfg(target_vendor = "apple")]
    enable_socket_reuse_port(&socket)?;
    socket
        .bind(&SocketAddr::from(([0, 0, 0, 0], addr.port())).into())
        .context("bind discovery socket")?;
    socket
        .join_multicast_v4(&ip, &std::net::Ipv4Addr::UNSPECIFIED)
        .context("join discovery multicast group")?;
    socket
        .set_multicast_loop_v4(true)
        .context("enable multicast loopback")?;
    socket
        .set_nonblocking(true)
        .context("set discovery socket nonblocking")?;
    UdpSocket::from_std(socket.into()).context("convert discovery socket")
}

#[cfg(target_vendor = "apple")]
pub(crate) fn enable_socket_reuse_port(socket: &Socket) -> Result<()> {
    let enabled: libc::c_int = 1;
    let result = unsafe {
        libc::setsockopt(
            socket.as_raw_fd(),
            libc::SOL_SOCKET,
            libc::SO_REUSEPORT,
            &enabled as *const _ as *const libc::c_void,
            std::mem::size_of_val(&enabled) as libc::socklen_t,
        )
    };
    if result == 0 {
        Ok(())
    } else {
        Err(std::io::Error::last_os_error()).context("set discovery SO_REUSEPORT")
    }
}

pub(crate) async fn run_cluster_listener(socket: Arc<UdpSocket>, state: AppState) -> Result<()> {
    let mut buffer = vec![0_u8; 4096];
    loop {
        let (len, from) = socket.recv_from(&mut buffer).await?;
        let Ok(packet) = serde_json::from_slice::<DiscoveryPacket>(&buffer[..len]) else {
            continue;
        };
        if packet.service_id == state.config.cluster.service_id {
            continue;
        }

        match packet.kind {
            DiscoveryKind::Discover if state.is_primary() => {
                let announce = build_discovery_packet(&state, DiscoveryKind::Announce);
                let payload = serde_json::to_vec(&announce)?;
                socket.send_to(&payload, from).await?;
            }
            DiscoveryKind::Announce => update_cluster_peer(&state.cluster, packet),
            DiscoveryKind::Discover => {}
        }
    }
}

pub(crate) async fn run_cluster_announcer(socket: Arc<UdpSocket>, state: AppState) -> Result<()> {
    let mut interval = tokio::time::interval(state.config.cluster.announce_interval);
    loop {
        interval.tick().await;
        let packet = build_discovery_packet(&state, DiscoveryKind::Announce);
        let payload = serde_json::to_vec(&packet)?;
        socket
            .send_to(&payload, &state.config.cluster.discovery_multicast_addr)
            .await?;
    }
}

pub(crate) async fn run_cluster_discoverer(socket: Arc<UdpSocket>, state: AppState) -> Result<()> {
    let mut interval = tokio::time::interval(state.config.cluster.announce_interval);
    loop {
        interval.tick().await;
        let packet = build_discovery_packet(&state, DiscoveryKind::Discover);
        let payload = serde_json::to_vec(&packet)?;
        socket
            .send_to(&payload, &state.config.cluster.discovery_multicast_addr)
            .await?;
        prune_cluster_peers(&state.cluster, state.config.cluster.peer_ttl);
    }
}

pub(crate) fn build_discovery_packet(state: &AppState, kind: DiscoveryKind) -> DiscoveryPacket {
    DiscoveryPacket {
        kind,
        service_id: state.config.cluster.service_id.clone(),
        advertise_addr: advertised_http_addr(&state.config),
        version: state
            .config
            .profile
            .display_version(env!("CARGO_PKG_VERSION")),
        priority: state.config.cluster.priority,
        sent_at: chrono::Utc::now(),
    }
}

pub(crate) fn advertised_http_addr(config: &AppConfig) -> String {
    config
        .cluster
        .advertise_addr
        .clone()
        .unwrap_or_else(|| config.service.bind_addr.clone())
}

pub(crate) fn update_cluster_peer(cluster: &ClusterRuntime, packet: DiscoveryPacket) {
    let mut peers = cluster.peers.lock().expect("cluster peer mutex poisoned");
    peers.insert(
        packet.service_id.clone(),
        ClusterPeer {
            service_id: packet.service_id,
            advertise_addr: packet.advertise_addr,
            version: packet.version,
            priority: packet.priority,
            last_seen: chrono::Utc::now(),
        },
    );
}

pub(crate) fn prune_cluster_peers(cluster: &ClusterRuntime, ttl: Duration) {
    let ttl = chrono::Duration::from_std(ttl).expect("valid cluster peer ttl");
    let now = chrono::Utc::now();
    let mut peers = cluster.peers.lock().expect("cluster peer mutex poisoned");
    peers.retain(|_, peer| now - peer.last_seen <= ttl);
}

pub(crate) fn selected_primary_peer(state: &AppState) -> Option<ClusterPeer> {
    prune_cluster_peers(&state.cluster, state.config.cluster.peer_ttl);
    let peers = state
        .cluster
        .peers
        .lock()
        .expect("cluster peer mutex poisoned");
    let mut peers = peers.values().cloned().collect::<Vec<_>>();
    peers.sort_by(|left, right| {
        right
            .priority
            .cmp(&left.priority)
            .then_with(|| right.last_seen.cmp(&left.last_seen))
            .then_with(|| left.service_id.cmp(&right.service_id))
    });
    peers.into_iter().next()
}

pub(crate) fn cluster_peer_by_service_id(
    state: &AppState,
    service_id: &str,
) -> Option<ClusterPeer> {
    prune_cluster_peers(&state.cluster, state.config.cluster.peer_ttl);
    let peers = state
        .cluster
        .peers
        .lock()
        .expect("cluster peer mutex poisoned");
    peers.get(service_id).cloned()
}

#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub(crate) struct ConfigFingerprint {
    exists: bool,
    modified: Option<SystemTime>,
    len: Option<u64>,
}

pub(crate) async fn wait_for_config_change(
    path: &FsPath,
    previous: ConfigFingerprint,
) -> Result<ConfigFingerprint> {
    loop {
        tokio::time::sleep(Duration::from_secs(1)).await;
        let current = config_path_fingerprint(Some(path)).await?;
        if current != previous {
            return Ok(current);
        }
    }
}

pub(crate) async fn config_path_fingerprint(path: Option<&FsPath>) -> Result<ConfigFingerprint> {
    let Some(path) = path else {
        return Ok(ConfigFingerprint {
            exists: false,
            modified: None,
            len: None,
        });
    };

    match tokio::fs::metadata(path).await {
        Ok(metadata) => Ok(ConfigFingerprint {
            exists: true,
            modified: metadata.modified().ok(),
            len: Some(metadata.len()),
        }),
        Err(error) if error.kind() == std::io::ErrorKind::NotFound => Ok(ConfigFingerprint {
            exists: false,
            modified: None,
            len: None,
        }),
        Err(error) => Err(error).with_context(|| format!("read metadata for {}", path.display())),
    }
}
