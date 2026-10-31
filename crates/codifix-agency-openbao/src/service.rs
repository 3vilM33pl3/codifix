use crate::*;
use axum::{
    Json, Router,
    extract::{DefaultBodyLimit, State},
    http::{HeaderMap, StatusCode},
    routing::{get, post},
};
use std::{net::SocketAddr, sync::Arc};

#[derive(Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct Acknowledgement {
    pub custodian_id: String,
    pub sequence: u64,
    pub challenge: String,
}
fn failure(_: Error) -> StatusCode {
    StatusCode::CONFLICT
}
async fn submit(
    State(config): State<Arc<CustodianConfig>>,
    headers: HeaderMap,
    Json(submission): Json<Submission>,
) -> std::result::Result<Json<Acknowledgement>, StatusCode> {
    transport::authenticate(&config.controller_token_file, &headers)
        .map_err(|_| StatusCode::UNAUTHORIZED)?;
    transport::custodian_submit(&config, &submission)
        .await
        .map_err(failure)?;
    Ok(Json(Acknowledgement {
        custodian_id: config.custodian_id.clone(),
        sequence: submission.ceremony.sequence,
        challenge: submission.ceremony.challenge,
    }))
}
pub fn custodian_router(config: CustodianConfig) -> Result<Router> {
    config.validate(now())?;
    Ok(Router::new()
        .route("/v1/ceremony", post(submit))
        .layer(DefaultBodyLimit::max(8192))
        .with_state(Arc::new(config)))
}

/// Linux native OpenBao PID/start identity, including machine boot ID. A restart
/// invalidates the enrolled binding even if the address and seal parameters match.
#[derive(Clone, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct ProcessBinding {
    pub pid: u32,
    pub marker: String,
}
impl ProcessBinding {
    pub fn inspect(pid: u32) -> Result<Self> {
        let stat = std::fs::read_to_string(format!("/proc/{pid}/stat"))
            .map_err(|_| Error::TargetChanged)?;
        let fields = stat.rsplit_once(')').ok_or(Error::TargetChanged)?.1;
        let ticks = fields
            .split_whitespace()
            .nth(19)
            .ok_or(Error::TargetChanged)?;
        if ticks.parse::<u64>().is_err() {
            return Err(Error::TargetChanged);
        }
        let boot = std::fs::read_to_string("/proc/sys/kernel/random/boot_id")
            .map_err(|_| Error::TargetChanged)?;
        let marker = format!(
            "{:x}",
            Sha256::digest(format!("{}:{pid}:{ticks}", boot.trim()))
        );
        Ok(Self { pid, marker })
    }
    pub fn verify(&self) -> Result<()> {
        if Self::inspect(self.pid)?.marker == self.marker {
            Ok(())
        } else {
            Err(Error::TargetChanged)
        }
    }
}
#[derive(Clone, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct AttestorConfig {
    pub identity_file: PathBuf,
    pub reader_token_file: PathBuf,
    pub process: ProcessBinding,
}
async fn identity(
    State(config): State<Arc<AttestorConfig>>,
    headers: HeaderMap,
) -> std::result::Result<Json<Identity>, StatusCode> {
    transport::authenticate(&config.reader_token_file, &headers)
        .map_err(|_| StatusCode::UNAUTHORIZED)?;
    config.process.verify().map_err(failure)?;
    // Read on each request: removing/revoking the lease stops ceremonies immediately.
    let identity: Identity = protected::load(&config.identity_file).map_err(failure)?;
    if identity.boot_id != config.process.marker || identity.valid_until <= now() {
        return Err(StatusCode::CONFLICT);
    }
    Ok(Json(identity))
}
pub fn attestor_router(config: AttestorConfig) -> Result<Router> {
    config.process.verify()?;
    Ok(Router::new()
        .route("/codifix-agency/v1/identity", get(identity))
        .with_state(Arc::new(config)))
}

#[derive(Clone, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct Listener {
    pub address: SocketAddr,
    pub tls_cert_file: Option<PathBuf>,
    pub tls_key_file: Option<PathBuf>,
    #[serde(default)]
    pub fixture_localhost_http: bool,
}
pub async fn serve(router: Router, listener: Listener) -> Result<()> {
    match (&listener.tls_cert_file, &listener.tls_key_file) {
        (Some(cert), Some(key)) if !listener.fixture_localhost_http => {
            // Read private files through the same symlink/owner/mode guard; keys
            // never pass through shell, logs, Debug or environment variables.
            let cert = protected::read(cert)?;
            let key = protected::read(key)?;
            let _ = rustls::crypto::ring::default_provider().install_default();
            let tls = axum_server::tls_rustls::RustlsConfig::from_pem(cert.to_vec(), key.to_vec())
                .await
                .map_err(|_| Error::Configuration)?;
            axum_server::bind_rustls(listener.address, tls)
                .serve(router.into_make_service())
                .await
                .map_err(|_| Error::Transport)
        }
        (None, None) if listener.fixture_localhost_http && listener.address.ip().is_loopback() => {
            let tcp = tokio::net::TcpListener::bind(listener.address)
                .await
                .map_err(|_| Error::Transport)?;
            axum::serve(tcp, router).await.map_err(|_| Error::Transport)
        }
        _ => Err(Error::Configuration),
    }
}
