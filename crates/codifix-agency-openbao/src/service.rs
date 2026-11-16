use crate::*;
use axum::{
    Json, Router,
    extract::{DefaultBodyLimit, Query, State},
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

/// Fresh Linux process evidence; boot/PID/start changes are normal on a trusted
/// restart. Stable native enrollment is checked independently before issuance.
#[derive(Clone, Debug, Serialize, Deserialize)]
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
        if fields.split_whitespace().next() == Some("Z") {
            return Err(Error::TargetChanged);
        }
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

/// Operator-reviewed stable local installation. Fingerprints never reveal argv,
/// config contents, private keys or generation metadata contents.
#[derive(Clone, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct NativeBinding {
    pub executable: PathBuf,
    pub executable_sha256: String,
    pub argv_sha256: String,
    pub config_file: PathBuf,
    pub config_sha256: String,
    pub storage_directory: PathBuf,
    pub storage_device: u64,
    pub storage_inode: u64,
    pub installation_file: PathBuf,
}
fn file_digest(path: &std::path::Path) -> Result<String> {
    use std::io::Read;
    // /proc/PID/exe is intentionally followed to hash the actually loaded image.
    let mut f = std::fs::File::open(path).map_err(|_| Error::TargetChanged)?;
    if !f.metadata().map_err(|_| Error::TargetChanged)?.is_file() {
        return Err(Error::TargetChanged);
    }
    let mut hash = Sha256::new();
    let mut buffer = [0u8; 65536];
    loop {
        let n = f.read(&mut buffer).map_err(|_| Error::TargetChanged)?;
        if n == 0 {
            break;
        }
        hash.update(&buffer[..n]);
    }
    Ok(format!("{:x}", hash.finalize()))
}
fn argv_digest(pid: u32) -> Result<String> {
    let bytes = zeroize::Zeroizing::new(
        std::fs::read(format!("/proc/{pid}/cmdline")).map_err(|_| Error::TargetChanged)?,
    );
    if bytes.len() > 65536 || bytes.is_empty() {
        return Err(Error::TargetChanged);
    }
    Ok(format!("{:x}", Sha256::digest(&*bytes)))
}
impl NativeBinding {
    /// Inspect only a specifically selected local process and paths. The operator
    /// must review that this process uses this config/storage before enrollment.
    pub fn inspect(
        pid: u32,
        config_file: PathBuf,
        storage_directory: PathBuf,
        installation_file: PathBuf,
    ) -> Result<Self> {
        ProcessBinding::inspect(pid)?;
        let executable =
            std::fs::read_link(format!("/proc/{pid}/exe")).map_err(|_| Error::TargetChanged)?;
        if std::fs::canonicalize(&config_file).map_err(|_| Error::TargetChanged)? != config_file
            || std::fs::canonicalize(&storage_directory).map_err(|_| Error::TargetChanged)?
                != storage_directory
        {
            return Err(Error::Configuration);
        }
        if installation_file.parent() != Some(storage_directory.as_path()) {
            return Err(Error::Configuration);
        }
        let (storage_device, storage_inode) = protected::directory_identity(&storage_directory)?;
        let config = protected::read(&config_file)?;
        let result = Self {
            executable_sha256: file_digest(&executable)?,
            executable,
            argv_sha256: argv_digest(pid)?,
            config_sha256: format!("{:x}", Sha256::digest(&*config)),
            config_file,
            storage_directory,
            storage_device,
            storage_inode,
            installation_file,
        };
        result.verify(pid)?;
        Ok(result)
    }
    pub fn binding(&self) -> Result<String> {
        Ok(format!(
            "{:x}",
            Sha256::digest(serde_json::to_vec(self).map_err(|_| Error::Configuration)?)
        ))
    }
    pub fn verify(&self, pid: u32) -> Result<()> {
        self.verify_cached(pid, None)
    }
    fn verify_cached(&self, pid: u32, cache: Option<&ImageCache>) -> Result<()> {
        let actual =
            std::fs::read_link(format!("/proc/{pid}/exe")).map_err(|_| Error::TargetChanged)?;
        if actual != self.executable
            || std::fs::canonicalize(&self.executable).map_err(|_| Error::TargetChanged)?
                != self.executable
            || self.installation_file.parent() != Some(self.storage_directory.as_path())
            || protected::directory_identity(&self.storage_directory)?
                != (self.storage_device, self.storage_inode)
            || format!(
                "{:x}",
                Sha256::digest(&*protected::read(&self.config_file)?)
            ) != self.config_sha256
            || argv_digest(pid)? != self.argv_sha256
        {
            return Err(Error::TargetChanged);
        }
        use std::os::unix::fs::MetadataExt;
        let image = PathBuf::from(format!("/proc/{pid}/exe"));
        let loaded = std::fs::metadata(&image).map_err(|_| Error::TargetChanged)?;
        let disk = std::fs::metadata(&self.executable).map_err(|_| Error::TargetChanged)?;
        if loaded.dev() != disk.dev() || loaded.ino() != disk.ino() {
            return Err(Error::TargetChanged);
        }
        let process = ProcessBinding::inspect(pid)?;
        let key = format!(
            "{}:{}:{}:{}:{}:{}:{}:{}",
            process.marker,
            loaded.dev(),
            loaded.ino(),
            loaded.len(),
            loaded.mtime(),
            loaded.mtime_nsec(),
            loaded.ctime(),
            loaded.ctime_nsec()
        );
        // Linux denies writes to a running executable. Cache only this loaded
        // inode/metadata and process marker; exec, replacement, restart or boot
        // changes force a fresh content digest before any new attestation.
        let mut cached = match cache {
            Some(cache) => Some(cache.lock().map_err(|_| Error::TargetChanged)?),
            None => None,
        };
        if cached.as_ref().and_then(|v| v.as_ref()) != Some(&key) {
            if file_digest(&image)? != self.executable_sha256 {
                return Err(Error::TargetChanged);
            }
            if let Some(cached) = cached.as_mut() {
                **cached = Some(key);
            }
        }
        Ok(())
    }
}
type ImageCache = std::sync::Mutex<Option<String>>;
struct AttestorRuntime {
    config: AttestorConfig,
    image_cache: ImageCache,
}

/// Explicit external generation/maintenance fence, not a native rekey oracle.
#[derive(Clone, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct InstallationRecord {
    pub installation_id: String,
    pub key_generation: u64,
    pub maintenance: bool,
}
#[derive(Clone, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct AttestorConfig {
    pub identity_file: PathBuf,
    pub reader_token_file: PathBuf,
    /// Written automatically by native OpenBao's pid_file setting under UMask=0077.
    pub pid_file: PathBuf,
    pub native: NativeBinding,
}
#[derive(Clone, Debug, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct Attestation {
    pub identity: Identity,
    pub process: ProcessBinding,
    pub issued_at: u64,
    pub expires_at: u64,
    pub challenge: String,
}
#[derive(Deserialize)]
#[serde(deny_unknown_fields)]
struct Challenge {
    challenge: String,
}
impl AttestorConfig {
    pub fn attest(&self, challenge: String) -> Result<Attestation> {
        self.attest_cached(challenge, None)
    }
    fn attest_cached(&self, challenge: String, cache: Option<&ImageCache>) -> Result<Attestation> {
        if uuid::Uuid::parse_str(&challenge).is_err() {
            return Err(Error::Ceremony);
        }
        let identity: Identity = protected::load(&self.identity_file)?;
        let installation: InstallationRecord = protected::load(&self.native.installation_file)?;
        if installation.maintenance
            || installation.installation_id != identity.installation_id
            || installation.key_generation != identity.key_generation
            || identity.valid_until <= now()
            || identity.native_binding != self.native.binding()?
        {
            return Err(Error::TargetChanged);
        }
        let pid = protected::read(&self.pid_file)?;
        let pid = std::str::from_utf8(&pid)
            .map_err(|_| Error::ProtectedFile)?
            .trim()
            .parse::<u32>()
            .map_err(|_| Error::ProtectedFile)?;
        let process = ProcessBinding::inspect(pid)?;
        self.native.verify_cached(pid, cache)?;
        process.verify()?;
        // Recheck mutable fence/lease after the potentially large image digest.
        let record: InstallationRecord = protected::load(&self.native.installation_file)?;
        let checked: Identity = protected::load(&self.identity_file)?;
        if record.maintenance
            || record.installation_id != identity.installation_id
            || record.key_generation != identity.key_generation
            || checked != identity
            || identity.valid_until <= now()
        {
            return Err(Error::TargetChanged);
        }
        let issued_at = now();
        let expires_at = (issued_at + 30).min(identity.valid_until);
        Ok(Attestation {
            identity,
            process,
            issued_at,
            expires_at,
            challenge,
        })
    }
}
async fn identity(
    State(runtime): State<Arc<AttestorRuntime>>,
    headers: HeaderMap,
    Query(challenge): Query<Challenge>,
) -> std::result::Result<Json<Attestation>, StatusCode> {
    transport::authenticate(&runtime.config.reader_token_file, &headers)
        .map_err(|_| StatusCode::UNAUTHORIZED)?;
    // Hashing native images is blocking filesystem work; do not stall HTTP tasks.
    tokio::task::spawn_blocking(move || {
        runtime
            .config
            .attest_cached(challenge.challenge, Some(&runtime.image_cache))
    })
    .await
    .map_err(|_| StatusCode::CONFLICT)?
    .map(Json)
    .map_err(failure)
}
pub fn attestor_router(config: AttestorConfig) -> Result<Router> {
    if config.native.binding()?.is_empty() {
        return Err(Error::Configuration);
    }
    Ok(Router::new()
        .route("/codifix-agency/v1/identity", get(identity))
        .with_state(Arc::new(AttestorRuntime {
            config,
            image_cache: std::sync::Mutex::new(None),
        })))
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
