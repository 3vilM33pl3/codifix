// SPDX-License-Identifier: AGPL-3.0-or-later
//! Administrator-only configuration. No field is populated by imported data.
use crate::{Endpoint, GatewaySecurity, GatewayServer, PdsPublisher, Result, RuntimeError};
use mem_federation::publication::{InstanceId, SpaceId};
use mem_federation_store::{GatewayStore, ReceiptKey, SpacePolicy, StoreError, WorkerOutcome};
use serde::Deserialize;
use std::{
    net::SocketAddr,
    path::{Path, PathBuf},
    time::Duration,
};

#[derive(Deserialize)]
#[serde(deny_unknown_fields)]
pub struct GatewayConfig {
    pub space: SpaceId,
    pub authority: InstanceId,
    pub listen: SocketAddr,
    pub database_url_file: PathBuf,
    pub pds: String,
    #[serde(default)]
    pub pds_ca_file: Option<PathBuf>,
    /// Windows-only local PDS channel; HTTPS remains its logical origin.
    #[serde(default)]
    pub pds_named_pipe: Option<crate::PdsNamedPipe>,
    pub repository: String,
    pub signing_key: String,
    #[serde(default)]
    pub token_file: Option<PathBuf>,
    /// Mutually exclusive with token_file. Mount the containing directory so an
    /// operator's atomic rename is visible without restarting this process.
    #[serde(default)]
    pub rotating_session_file: Option<PathBuf>,
    #[serde(default)]
    pub maintenance_status_file: Option<PathBuf>,
    pub audit_key_file: PathBuf,
    pub receipt_keys: Vec<ReceiptKey>,
    pub policy: SpacePolicy,
    pub credentials: Vec<InstanceCredential>,
    #[serde(default)]
    pub disposable_test_mode: bool,
    /// Explicit schema declarations for every configured private-space credential.
    /// None keeps all expertise routes disabled.
    #[serde(default)]
    pub expertise_fleet: Option<Vec<(InstanceId, u32)>>,
    /// Separate fleet gate for the addressed-record profile. Existing schema-v3
    /// Q&A remains compatible when this is absent.
    #[serde(default)]
    pub addressed_expertise_fleet: Option<Vec<(InstanceId, u32)>>,
    /// Origin-only project delivery, never a device-facing whole-space pull.
    #[serde(default)]
    pub scoped_delivery: bool,
}
#[derive(Deserialize)]
#[serde(deny_unknown_fields)]
pub struct InstanceCredential {
    pub instance: InstanceId,
    pub token_file: PathBuf,
}

pub fn private_secret(path: &Path) -> Result<String> {
    let metadata = std::fs::metadata(path).map_err(|_| RuntimeError::Configuration)?;
    #[cfg(unix)]
    {
        use std::os::unix::fs::PermissionsExt;
        if metadata.permissions().mode() & 0o077 != 0 {
            return Err(RuntimeError::Configuration);
        }
    }
    if !metadata.is_file() || metadata.len() > 4096 {
        return Err(RuntimeError::Configuration);
    }
    let value = std::fs::read_to_string(path).map_err(|_| RuntimeError::Configuration)?;
    if value.trim().len() < 32 || value.len() > 4096 {
        return Err(RuntimeError::Configuration);
    }
    Ok(value.trim().to_owned())
}
/// A 32-byte scalar encoded as exactly 64 hex characters in a private file.
pub fn private_key(path: &Path) -> Result<[u8; 32]> {
    let text = private_secret(path)?;
    if text.len() != 64 {
        return Err(RuntimeError::Configuration);
    }
    let mut key = [0; 32];
    for (index, pair) in text.as_bytes().as_chunks::<2>().0.iter().enumerate() {
        let digit = |byte: u8| {
            (byte as char)
                .to_digit(16)
                .ok_or(RuntimeError::Configuration)
        };
        key[index] = ((digit(pair[0])? << 4) | digit(pair[1])?) as u8;
    }
    Ok(key)
}
pub fn root_certificate(path: &Path) -> Result<Vec<u8>> {
    if std::fs::metadata(path)
        .map_err(|_| RuntimeError::Configuration)?
        .len()
        > 16 * 1024
    {
        return Err(RuntimeError::Configuration);
    }
    std::fs::read(path).map_err(|_| RuntimeError::Configuration)
}
impl GatewayConfig {
    fn validate_credential_mode(&self) -> Result<()> {
        match (
            &self.token_file,
            &self.rotating_session_file,
            &self.maintenance_status_file,
        ) {
            (Some(_), None, None) | (None, Some(_), _) => Ok(()),
            _ => Err(RuntimeError::Configuration),
        }
    }
    /// Dedicated database + gateway-only routes. Never starts the Memory app,
    /// HTTP/MCP/export surfaces, model providers, or source watchers.
    pub async fn serve(self) -> Result<()> {
        self.validate_credential_mode()?;
        if self.disposable_test_mode && !cfg!(debug_assertions) {
            return Err(RuntimeError::Configuration);
        }
        let security = GatewaySecurity::new(
            &private_key(&self.audit_key_file)?,
            self.receipt_keys,
            self.policy,
        )?;
        if security
            .trust()
            .keys()
            .iter()
            .any(|key| key.public_key == self.signing_key)
        {
            return Err(RuntimeError::Configuration);
        }
        let mut endpoint = Endpoint::with_ca(
            &self.pds,
            self.disposable_test_mode,
            self.pds_ca_file
                .as_deref()
                .map(root_certificate)
                .transpose()?
                .as_deref(),
        )?;
        if let Some(pipe) = self.pds_named_pipe {
            endpoint = endpoint.with_pds_named_pipe(pipe)?;
        }
        let pds = match (
            self.token_file,
            self.rotating_session_file,
            self.maintenance_status_file,
        ) {
            (Some(token), None, None) => PdsPublisher::new(
                endpoint,
                self.repository,
                private_secret(&token)?,
                self.signing_key,
                self.space,
            )?,
            (None, Some(session), maintenance) => PdsPublisher::with_rotating_credentials(
                endpoint,
                self.repository,
                session,
                maintenance,
                self.signing_key,
                self.space,
            )?,
            _ => return Err(RuntimeError::Configuration),
        };
        let credentials = self
            .credentials
            .into_iter()
            .map(|c| Ok((private_secret(&c.token_file)?, c.instance)))
            .collect::<Result<Vec<_>>>()?;
        let pool = sqlx::postgres::PgPoolOptions::new()
            .max_connections(8)
            .connect(&private_secret(&self.database_url_file)?)
            .await
            .map_err(StoreError::from)?;
        // Schema provisioning is a separate administrator operation. Do not
        // grant this long-running network process database migration privileges.
        let safe:bool=sqlx::query_scalar("SELECT NOT (r.rolsuper OR r.rolcreatedb OR r.rolcreaterole OR r.rolreplication OR r.rolbypassrls) AND NOT pg_has_role(current_user,d.datdba,'MEMBER') AND NOT has_schema_privilege(current_user,'public','CREATE') AND NOT EXISTS (SELECT 1 FROM pg_class c JOIN pg_namespace n ON n.oid=c.relnamespace WHERE n.nspname='public' AND c.relkind IN ('r','p','v','m','f') AND c.relname NOT IN ('federation_gateway_spaces','federation_accepted','federation_publish_outbox','federation_security','federation_audit','federation_retention_decisions','federation_expertise_mailbox','federation_expertise_attempts','federation_expertise_daily_budget','federation_expertise_deliveries','federation_expertise_addressed','federation_project_feeds','federation_project_streams','federation_project_events','federation_project_operations') AND has_table_privilege(current_user,c.oid,'SELECT,INSERT,UPDATE,DELETE,TRUNCATE,REFERENCES,TRIGGER')) FROM pg_roles r JOIN pg_database d ON d.datname=current_database() WHERE r.rolname=current_user")
            .fetch_one(&pool).await.map_err(StoreError::from)?;
        if !safe {
            return Err(RuntimeError::Configuration);
        }
        let listener = tokio::net::TcpListener::bind(self.listen)
            .await
            .map_err(|_| RuntimeError::Configuration)?;
        let server = GatewayServer::new(
            GatewayStore::new(pool),
            self.space,
            self.authority,
            credentials,
            security,
        )
        .await?;
        let server = if let Some(fleet) = self.expertise_fleet {
            server.enable_expertise(&fleet).await?
        } else {
            server
        };
        let server = if let Some(fleet) = self.addressed_expertise_fleet {
            server.enable_addressed_expertise(&fleet).await?
        } else {
            server
        };
        let server = if self.scoped_delivery {
            server.enable_scoped_delivery()
        } else {
            server
        };
        let server = server.with_publication_status(pds.clone());
        let router = server.router();
        let worker = async {
            loop {
                match server.delete_addressed_one(&pds).await {
                    Ok(true) => continue,
                    Err(RuntimeError::Store(StoreError::Fenced)) => {
                        return Err(RuntimeError::Store(StoreError::Fenced));
                    }
                    Err(_) => tracing::warn!("addressed expertise deletion blocked or retrying"),
                    Ok(false) => {}
                }
                match server.publish_one(&pds).await {
                    Ok(WorkerOutcome::Published(_)) => continue,
                    Err(RuntimeError::Store(StoreError::Fenced)) => {
                        return Err(RuntimeError::Store(StoreError::Fenced));
                    }
                    Err(_) | Ok(WorkerOutcome::Blocked) => {
                        tracing::warn!("gateway publication blocked or retrying")
                    }
                    _ => {}
                }
                match server.publish_addressed_one(&pds).await {
                    Ok(true) => continue,
                    Err(RuntimeError::Store(StoreError::Fenced)) => {
                        return Err(RuntimeError::Store(StoreError::Fenced));
                    }
                    Err(_) => tracing::warn!("addressed expertise publication blocked or retrying"),
                    Ok(false) => {}
                }
                tokio::time::sleep(Duration::from_secs(2)).await;
            }
        };
        tokio::select! {
            result=axum::serve(listener,router)=>result.map_err(|_|RuntimeError::Unavailable),
            result=worker=>result,
            result=async {
                loop {
                    server.maintain_expertise().await?;
                    tokio::time::sleep(Duration::from_secs(60)).await;
                }
                #[allow(unreachable_code)]
                Ok::<(),RuntimeError>(())
            }=>result,
        }
    }
}

#[cfg(test)]
#[allow(clippy::unwrap_used)]
mod tests {
    use super::*;
    use serde_json::json;

    #[test]
    fn legacy_static_configuration_and_explicit_rotating_mode_are_mutually_exclusive() {
        let legacy = json!({"space":uuid::Uuid::new_v4(),"authority":uuid::Uuid::new_v4(),
            "listen":"127.0.0.1:9999","database_url_file":"/private/database",
            "pds":"https://private.example.test","repository":"did:web:private.example.test",
            "signing_key":"fixture","token_file":"/private/access",
            "audit_key_file":"/private/audit","receipt_keys":[],"policy":{"external_models":[]},"credentials":[]});
        let config: GatewayConfig = serde_json::from_value(legacy.clone()).unwrap();
        assert!(config.validate_credential_mode().is_ok());
        let mut rotating = legacy.clone();
        rotating.as_object_mut().unwrap().remove("token_file");
        rotating["rotating_session_file"] = json!("/private/rotating/session.json");
        rotating["maintenance_status_file"] = json!("/private/rotating/status.json");
        assert!(
            serde_json::from_value::<GatewayConfig>(rotating.clone())
                .unwrap()
                .validate_credential_mode()
                .is_ok()
        );
        rotating["token_file"] = json!("/private/old-token");
        assert!(
            serde_json::from_value::<GatewayConfig>(rotating)
                .unwrap()
                .validate_credential_mode()
                .is_err()
        );
        let mut missing = legacy.clone();
        missing.as_object_mut().unwrap().remove("token_file");
        assert!(
            serde_json::from_value::<GatewayConfig>(missing)
                .unwrap()
                .validate_credential_mode()
                .is_err()
        );
        let mut sidecar_only = legacy;
        sidecar_only["maintenance_status_file"] = json!("/private/status");
        assert!(
            serde_json::from_value::<GatewayConfig>(sidecar_only)
                .unwrap()
                .validate_credential_mode()
                .is_err()
        );
    }
}
