// SPDX-License-Identifier: AGPL-3.0-or-later
//! Explicit publication/review repository APIs and opt-in worker lifecycle.
//! HTTP/UI review and membership administration remain the E-stage workflow.
use crate::AppState;
use mem_federation::publication::{InstanceId, SpaceId};
use mem_federation_runtime::{Endpoint, GatewayClient, InstanceSync, ReceiptTrust};
pub use mem_federation_store::{
    SourceSelection, preview_source, retry_source_work, review_and_share, set_local_grant,
};
use serde::Deserialize;
use std::{
    path::{Path, PathBuf},
    time::Duration,
};
use uuid::Uuid;

#[derive(Deserialize)]
#[serde(tag = "role", rename_all = "snake_case", deny_unknown_fields)]
enum WorkerConfig {
    Instance {
        space: SpaceId,
        authority: InstanceId,
        instance: InstanceId,
        project: Uuid,
        principal: Uuid,
        gateway: String,
        #[serde(default)]
        gateway_ca_file: Option<PathBuf>,
        token_file: PathBuf,
        receipt_keys: Vec<mem_federation_store::ReceiptKey>,
        #[serde(default)]
        disposable_test_mode: bool,
    },
    Gateway {},
}

pub(crate) struct FederationTasks(Vec<tokio::task::JoinHandle<()>>);
impl Drop for FederationTasks {
    fn drop(&mut self) {
        for task in &self.0 {
            task.abort();
        }
    }
}

fn secret(path: &Path) -> anyhow::Result<String> {
    #[cfg(unix)]
    {
        use std::os::unix::fs::PermissionsExt;
        anyhow::ensure!(
            std::fs::metadata(path)?.permissions().mode() & 0o077 == 0,
            "federation token file must be private (0600)"
        );
    }
    let value = std::fs::read_to_string(path)?;
    anyhow::ensure!(
        value.trim().len() >= 32 && value.len() <= 4096,
        "invalid federation token file"
    );
    Ok(value.trim().to_owned())
}

/// Disabled unless an administrator explicitly supplies this configuration file.
/// Credentials/URLs are never read from imported content. No existing grant or
/// project is created by starting the worker.
pub(crate) async fn start(
    state: AppState,
    path: Option<PathBuf>,
) -> anyhow::Result<FederationTasks> {
    let Some(path) = path else {
        return Ok(FederationTasks(Vec::new()));
    };
    let config: WorkerConfig = serde_json::from_slice(&std::fs::read(path)?)?;
    let mut tasks = Vec::new();
    match config {
        WorkerConfig::Instance {
            space,
            authority,
            instance,
            project,
            principal,
            gateway,
            gateway_ca_file,
            token_file,
            receipt_keys,
            disposable_test_mode,
        } => {
            anyhow::ensure!(
                !disposable_test_mode || cfg!(debug_assertions),
                "cleartext fixture mode is prohibited in release builds"
            );
            let ca = gateway_ca_file
                .as_deref()
                .map(mem_federation_runtime::root_certificate)
                .transpose()?;
            let client = GatewayClient::new(
                Endpoint::with_ca(&gateway, disposable_test_mode, ca.as_deref())?,
                secret(&token_file)?,
                space,
                authority,
                ReceiptTrust::new(receipt_keys)?,
            )?;
            let sync = InstanceSync::new(
                state
                    .pool()
                    .map_err(|_| anyhow::anyhow!("federation requires local PostgreSQL"))?,
                client,
                instance,
            )
            .await?;
            tasks.push(tokio::spawn(async move {
                loop {
                    match crate::auth::federation_local_access(&state,principal,project).await {
                        Ok(access)=>{if sync.cycle(&access).await.is_err() {tracing::warn!("federation instance work paused/retrying; inspect durable synchronization status");}}
                        Err(_)=>{
                            if sync.note_local_authorization_failure().await.is_err() {
                                tracing::warn!("cannot persist federation authorization pause");
                            }
                            tracing::warn!("federation local principal is not authorized");
                        }
                    }
                    tokio::time::sleep(Duration::from_secs(2)).await;
                }
            }));
        }
        WorkerConfig::Gateway { .. } => anyhow::bail!(
            "gateway role requires the standalone memory-federation-gateway executable"
        ),
    }
    Ok(FederationTasks(tasks))
}

#[cfg(test)]
mod tests {
    use super::*;
    use mem_record::{Permission, PermissionSet};
    use serde_json::json;
    use std::sync::{Arc, Mutex};

    #[test]
    fn config_is_explicit_and_secret_files_are_private() {
        assert!(serde_json::from_value::<WorkerConfig>(json!({"role":"automatic"})).is_err());
        let id = Uuid::new_v4();
        assert!(
            serde_json::from_value::<WorkerConfig>(
                json!({"role":"gateway","pds":"https://pds.example.com"})
            )
            .is_err()
        );
        assert!(
            serde_json::from_value::<WorkerConfig>(json!({"role":"instance","space":id})).is_err()
        );
        let dir = std::env::temp_dir().join(format!("federation-secret-{}", Uuid::new_v4()));
        std::fs::create_dir(&dir).unwrap();
        let path = dir.join("token");
        std::fs::write(&path, "a".repeat(64)).unwrap();
        #[cfg(unix)]
        {
            use std::os::unix::fs::PermissionsExt;
            std::fs::set_permissions(&path, std::fs::Permissions::from_mode(0o644)).unwrap();
            assert!(secret(&path).is_err());
            std::fs::set_permissions(&path, std::fs::Permissions::from_mode(0o600)).unwrap();
        }
        assert_eq!(secret(&path).unwrap().len(), 64);
        std::fs::remove_file(path).unwrap();
        std::fs::remove_dir(dir).unwrap();
    }

    #[tokio::test]
    async fn opt_in_worker_uses_current_service_policy_and_stops_on_drop() {
        let Some(pool) = mem_test_support::migrated_pool().await else {
            return;
        };
        // Deserialize explicit data, never load the developer's config/providers.
        let config = serde_json::from_value(json!({"service":{"bind_addr":"127.0.0.1:0","api_token":"synthetic"},
            "database":{"url":std::env::var("MEMORY_LAYER_TEST_DATABASE_URL").unwrap()},
            "offline":{"enabled":false},"cluster":{"enabled":false},"reinforcement":{"enabled":false}})).unwrap();
        let state = crate::runtime::build_state(config, Arc::new(Mutex::new(None)))
            .await
            .unwrap();
        assert!(start(state.clone(), None).await.unwrap().0.is_empty());
        let project = Uuid::new_v4();
        let principal = Uuid::new_v4();
        let space = SpaceId::new(Uuid::new_v4()).unwrap();
        let authority = InstanceId::new(Uuid::new_v4()).unwrap();
        let instance = InstanceId::new(Uuid::new_v4()).unwrap();
        sqlx::query(
            "INSERT INTO projects(id,slug,name,root_path) VALUES($1,$2,'fixture','/synthetic')",
        )
        .bind(project)
        .bind(project.to_string())
        .execute(&pool)
        .await
        .unwrap();
        sqlx::query("INSERT INTO auth_principals(id,kind,display_name,global_role) VALUES($1,'internal','fixture','admin')")
            .bind(principal).execute(&pool).await.unwrap();
        let access = crate::auth::federation_local_access(&state, principal, project)
            .await
            .unwrap();
        assert!(access.permissions.contains(Permission::MemoryCurate));
        let mut tx = pool.begin().await.unwrap();
        set_local_grant(
            &mut tx,
            &access,
            &mem_federation::publication::LocalSpaceGrant {
                principal,
                project,
                space,
                instance,
                capabilities: vec![mem_federation::SpaceCapability::Pull],
            },
            true,
        )
        .await
        .unwrap();
        tx.commit().await.unwrap();
        let dir = std::env::temp_dir().join(format!("federation-worker-{}", Uuid::new_v4()));
        std::fs::create_dir(&dir).unwrap();
        let token = dir.join("token");
        std::fs::write(&token, "t".repeat(64)).unwrap();
        #[cfg(unix)]
        {
            use std::os::unix::fs::PermissionsExt;
            std::fs::set_permissions(&token, std::fs::Permissions::from_mode(0o600)).unwrap();
        }
        let config = dir.join("worker.json");
        // A closed ephemeral listener gives an unavailable peer, not a live endpoint.
        let closed = tokio::net::TcpListener::bind("127.0.0.1:0").await.unwrap();
        let url = format!("http://{}", closed.local_addr().unwrap());
        drop(closed);
        std::fs::write(
            &config,
            serde_json::to_vec(&json!({"role":"instance", "space":space,
            "authority":authority,"instance":instance,"project":project,"principal":principal,
            "gateway":url,"token_file":token,"disposable_test_mode":true,
            "receipt_keys":mem_federation_runtime::GatewaySecurity::initial(&[3;32],Default::default()).unwrap().trust().keys()}))
            .unwrap(),
        )
        .unwrap();
        let tasks = start(state.clone(), Some(config.clone())).await.unwrap();
        assert_eq!(tasks.0.len(), 1);
        tokio::time::timeout(Duration::from_secs(5),async {
            loop {
                let failed:bool=sqlx::query_scalar("SELECT sync_failures>0 FROM federation_replicas WHERE space=$1 AND receiver=$2")
                    .bind(space.as_uuid()).bind(instance.as_uuid()).fetch_one(&pool).await.unwrap();
                if failed {break;}
                tokio::time::sleep(Duration::from_millis(20)).await;
            }
        }).await.unwrap();
        sqlx::query(
            "UPDATE auth_principals SET global_role=NULL,global_permissions_json='[]' WHERE id=$1",
        )
        .bind(principal)
        .execute(&pool)
        .await
        .unwrap();
        assert_eq!(
            crate::auth::federation_local_access(&state, principal, project)
                .await
                .unwrap()
                .permissions,
            PermissionSet::EMPTY
        );
        sqlx::query("UPDATE auth_principals SET disabled_at=clock_timestamp() WHERE id=$1")
            .bind(principal)
            .execute(&pool)
            .await
            .unwrap();
        assert!(
            crate::auth::federation_local_access(&state, principal, project)
                .await
                .is_err()
        );
        tokio::time::timeout(Duration::from_secs(5), async {
            loop {
                let error: Option<String> = sqlx::query_scalar(
                    "SELECT sync_error FROM federation_replicas WHERE space=$1 AND receiver=$2",
                )
                .bind(space.as_uuid())
                .bind(instance.as_uuid())
                .fetch_one(&pool)
                .await
                .unwrap();
                if error.as_deref() == Some("local_authorization") {
                    break;
                }
                tokio::time::sleep(Duration::from_millis(20)).await;
            }
        })
        .await
        .unwrap();
        let aborted = tasks.0[0].abort_handle();
        drop(tasks);
        tokio::time::timeout(Duration::from_secs(1), async {
            while !aborted.is_finished() {
                tokio::task::yield_now().await;
            }
        })
        .await
        .unwrap();
        std::fs::remove_file(token).unwrap();
        std::fs::remove_file(config).unwrap();
        std::fs::remove_dir(dir).unwrap();
        state.pool().unwrap().close().await;
    }
}
