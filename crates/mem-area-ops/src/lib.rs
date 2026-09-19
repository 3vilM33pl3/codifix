// SPDX-License-Identifier: AGPL-3.0-or-later
//! Explicit offline preparation and local-only target import. No configuration
//! discovery, credential creation, service startup or source writes. Schema-only
//! initialization is a separate explicitly confirmed fresh-database operation.
pub mod preview;
mod private;
pub mod schema;
use mem_area::{AREAS, AreaId, Inventory, Manifest, MaterializedMigration, RetainedTargetBudget};
use private::{Directory, Input};
use serde::{Deserialize, Serialize, de::DeserializeOwned};
use sha2::{Digest, Sha256};
use sqlx::postgres::{PgConnectOptions, PgPoolOptions, PgSslMode};
use std::{
    io::{BufReader, BufWriter, Read, Write},
    path::{Path, PathBuf},
    time::Duration,
};
use uuid::Uuid;

const INVENTORY_LIMIT: u64 = 2 * 1024 * 1024 * 1024;
const MANIFEST_LIMIT: u64 = 256 * 1024 * 1024;
const PAYLOAD_LIMIT: u64 = 64 * 1024 * 1024 * 1024;
const RECEIPT_LIMIT: u64 = 128 * 1024;
const TARGET_LIMIT: u64 = 8192;
const ARCHIVE: &str = "archive.ndjson";
const PREPARED: &str = "prepared.json";
pub type Result<T> = std::result::Result<T, Failure>;
#[derive(Debug, thiserror::Error)]
pub enum Failure {
    #[error(
        "reviewed preview selection is incomplete, inconsistent or not explicitly approved; source details suppressed"
    )]
    Preview,
    #[error(
        "private file boundary failed; use owned 0700 directories and exclusive 0600 regular files"
    )]
    PrivateFiles,
    #[error("private input exceeds its bounded size")]
    Size,
    #[error("private JSON contract is invalid; source details suppressed")]
    Json,
    #[error("reviewed input or prepared artifact digest does not match")]
    Binding,
    #[error("source materialization failed: {0}; incomplete artifacts must not be used")]
    Materialize(#[from] mem_area::MaterializeError),
    #[error("private payload verification failed; no ownership approval or import performed")]
    Payload,
    #[error("explicit local target or execution assertions are invalid")]
    Target,
    #[error(
        "PostgreSQL environment overrides are forbidden; use only the explicit target descriptor"
    )]
    Environment,
    #[error("local target connection failed; credentials and database details suppressed")]
    Connection,
    #[error(
        "connected database, login or owner differs from the reviewed gateway target; no schema changes started"
    )]
    GatewayTarget,
    #[error(
        "schema preparation requires an empty dedicated database with no other client connections"
    )]
    NotEmpty,
    #[error(
        "schema preparation was not confirmed; inspect the target before retrying; SQL details suppressed"
    )]
    Schema,
    #[error(transparent)]
    Import(#[from] mem_area_store::ImportError),
    #[error("metadata output failed; inspect existing receipts before retry")]
    Output,
}

pub struct Inputs {
    pub inventory: PathBuf,
    pub manifest: PathBuf,
    pub payload: PathBuf,
    pub inventory_sha256: String,
    pub manifest_sha256: String,
}
#[derive(Debug, Serialize, Deserialize, PartialEq, Eq)]
#[serde(deny_unknown_fields)]
pub struct PartitionSummary {
    pub area: AreaId,
    pub partition_sha256: String,
    pub rows: usize,
    pub projections: usize,
}
#[derive(Debug, Serialize, Deserialize, PartialEq, Eq)]
#[serde(deny_unknown_fields)]
pub struct Prepared {
    pub format_version: u32,
    #[serde(default = "default_max_retained_target_mib")]
    pub max_retained_target_mib: u32,
    pub source_instance: Uuid,
    pub snapshot_id: Uuid,
    pub inventory_sha256: String,
    pub manifest_sha256: String,
    pub inventory_file_sha256: String,
    pub manifest_file_sha256: String,
    pub payload_file_sha256: String,
    pub archive_sha256: String,
    pub archive_bytes: u64,
    pub source_tables: u64,
    pub source_rows: u64,
    pub partitions: Vec<PartitionSummary>,
}

fn default_max_retained_target_mib() -> u32 {
    mem_area::DEFAULT_RETAINED_TARGET_MIB
}
#[derive(Serialize)]
pub struct PreparedReport {
    pub status: &'static str,
    pub prepared_sha256: String,
    pub prepared: Prepared,
}

#[derive(Serialize)]
pub struct Inspection {
    pub status: &'static str,
    pub source_instance: Uuid,
    pub snapshot_id: Uuid,
    pub inventory_sha256: String,
    pub manifest_sha256: String,
    pub plan_valid: bool,
    pub blocker_count: usize,
}

#[derive(Serialize)]
pub struct VerifiedPayload {
    pub status: &'static str,
    pub source_instance: Uuid,
    pub snapshot_id: Uuid,
    pub inventory_file_sha256: String,
    pub payload_file_sha256: String,
    pub verification: mem_area::PayloadVerification,
}

/// Check exact source bytes without a manifest or any DB connection. This is
/// deliberately insufficient input for prepare/import or ownership approval.
pub fn verify_payload(inventory: &Path, payload: &Path, expected: &str) -> Result<VerifiedPayload> {
    if !valid_digest(expected) {
        return Err(Failure::Binding);
    }
    let mut inventory_file = Input::open(inventory, INVENTORY_LIMIT)?;
    let mut payload_file = Input::open(payload, PAYLOAD_LIMIT)?;
    let (inventory, inventory_file_sha256): (Inventory, _) =
        json(&mut inventory_file, INVENTORY_LIMIT)?;
    if mem_area::inventory_digest(&inventory).map_err(|_| Failure::Json)? != expected {
        return Err(Failure::Binding);
    }
    let mut reader = BufReader::new(Hashed::new(
        (&mut payload_file.file).take(PAYLOAD_LIMIT + 1),
    ));
    let verification =
        mem_area::verify_payload(&inventory, &mut reader).map_err(|_| Failure::Payload)?;
    let payload_file_sha256 = reader.into_inner().digest();
    inventory_file.unchanged()?;
    payload_file.unchanged()?;
    Ok(VerifiedPayload {
        status: "payload_verified_only_not_approved_or_imported",
        source_instance: inventory.source_instance,
        snapshot_id: inventory.snapshot_id,
        inventory_file_sha256,
        payload_file_sha256,
        verification,
    })
}

/// Read-only metadata inspection. Computing a digest does not grant review
/// approval and does not validate the separately collected row payload.
pub fn inspect(inventory: &Path, manifest: &Path) -> Result<Inspection> {
    let mut inventory_file = Input::open(inventory, INVENTORY_LIMIT)?;
    let mut manifest_file = Input::open(manifest, MANIFEST_LIMIT)?;
    let (inventory, _): (Inventory, _) = json(&mut inventory_file, INVENTORY_LIMIT)?;
    let (manifest, _): (Manifest, _) = json(&mut manifest_file, MANIFEST_LIMIT)?;
    let report = mem_area::verify_manifest(&inventory, &manifest);
    Ok(Inspection {
        status: "inspected_only_not_approved",
        source_instance: inventory.source_instance,
        snapshot_id: inventory.snapshot_id,
        inventory_sha256: mem_area::inventory_digest(&inventory).map_err(|_| Failure::Json)?,
        manifest_sha256: mem_area::manifest_digest(&manifest).map_err(|_| Failure::Json)?,
        plan_valid: report.plan_valid,
        blocker_count: report.blockers.len(),
    })
}

struct Hashed<R> {
    reader: R,
    hash: Sha256,
}
impl<R> Hashed<R> {
    fn new(reader: R) -> Self {
        Self {
            reader,
            hash: Sha256::new(),
        }
    }
    fn digest(self) -> String {
        format!("{:x}", self.hash.finalize())
    }
}
impl<R: Read> Read for Hashed<R> {
    fn read(&mut self, buffer: &mut [u8]) -> std::io::Result<usize> {
        let size = self.reader.read(buffer)?;
        self.hash.update(&buffer[..size]);
        Ok(size)
    }
}
fn hash(bytes: &[u8]) -> String {
    format!("{:x}", Sha256::digest(bytes))
}
fn valid_digest(value: &str) -> bool {
    value.len() == 64
        && value
            .bytes()
            .all(|b| b.is_ascii_digit() || (b'a'..=b'f').contains(&b))
}
fn json<T: DeserializeOwned>(input: &mut Input, limit: u64) -> Result<(T, String)> {
    let mut hashed = Hashed::new((&mut input.file).take(limit + 1));
    let value = serde_json::from_reader(BufReader::new(&mut hashed)).map_err(|_| Failure::Json)?;
    let digest = hashed.digest();
    input.unchanged()?;
    Ok((value, digest))
}
fn materialize(
    inputs: &Inputs,
    archive: &mut impl Write,
    budget: RetainedTargetBudget,
) -> Result<(MaterializedMigration, Prepared)> {
    if !valid_digest(&inputs.inventory_sha256) || !valid_digest(&inputs.manifest_sha256) {
        return Err(Failure::Binding);
    }
    let mut inventory_file = Input::open(&inputs.inventory, INVENTORY_LIMIT)?;
    let mut manifest_file = Input::open(&inputs.manifest, MANIFEST_LIMIT)?;
    let mut payload = Input::open(&inputs.payload, PAYLOAD_LIMIT)?;
    let (inventory, inventory_file_sha256): (Inventory, _) =
        json(&mut inventory_file, INVENTORY_LIMIT)?;
    let (manifest, manifest_file_sha256): (Manifest, _) = json(&mut manifest_file, MANIFEST_LIMIT)?;
    if mem_area::inventory_digest(&inventory).map_err(|_| Failure::Json)? != inputs.inventory_sha256
        || mem_area::manifest_digest(&manifest).map_err(|_| Failure::Json)?
            != inputs.manifest_sha256
    {
        return Err(Failure::Binding);
    }
    let mut reader = BufReader::new(Hashed::new((&mut payload.file).take(PAYLOAD_LIMIT + 1)));
    let migration =
        mem_area::materialize_with_budget(&inventory, &manifest, &mut reader, archive, budget)?;
    let payload_file_sha256 = reader.into_inner().digest();
    inventory_file.unchanged()?;
    manifest_file.unchanged()?;
    payload.unchanged()?;
    let prepared = Prepared {
        format_version: 1,
        max_retained_target_mib: budget.mib(),
        source_instance: inventory.source_instance,
        snapshot_id: inventory.snapshot_id,
        inventory_sha256: inputs.inventory_sha256.clone(),
        manifest_sha256: inputs.manifest_sha256.clone(),
        inventory_file_sha256,
        manifest_file_sha256,
        payload_file_sha256,
        archive_sha256: migration.archive_sha256().into(),
        archive_bytes: migration.archive_bytes(),
        source_tables: migration.source_table_count(),
        source_rows: migration.source_row_count(),
        partitions: AREAS
            .iter()
            .map(|area| {
                let p = migration.partition(*area);
                PartitionSummary {
                    area: *area,
                    partition_sha256: p.partition_sha256().into(),
                    rows: p.rows().len(),
                    projections: p.projected_sources().len(),
                }
            })
            .collect(),
    };
    Ok((migration, prepared))
}

/// Existing private directory; no overwrites. Partial files are retained on
/// failure. A complete archive without prepared.json is not import authority.
pub fn prepare(inputs: &Inputs, output: &Path) -> Result<PreparedReport> {
    prepare_with_budget(inputs, output, RetainedTargetBudget::default())
}

/// Explicit larger bounded target budget, recorded in the prepared receipt.
pub fn prepare_with_budget(
    inputs: &Inputs,
    output: &Path,
    budget: RetainedTargetBudget,
) -> Result<PreparedReport> {
    let directory = Directory::open(output)?;
    directory.ensure_absent(&["archive.partial", ARCHIVE, "prepared.partial", PREPARED])?;
    let archive = directory.create("archive.partial")?;
    let mut writer = BufWriter::new(archive);
    let (_, prepared) = materialize(inputs, &mut writer, budget)?;
    writer.flush().map_err(|_| Failure::PrivateFiles)?;
    let archive = writer.into_inner().map_err(|_| Failure::PrivateFiles)?;
    directory.publish("archive.partial", ARCHIVE, &archive)?;
    let receipt = serde_json::to_vec_pretty(&prepared).map_err(|_| Failure::Json)?;
    let mut file = directory.create("prepared.partial")?;
    file.write_all(&receipt)
        .map_err(|_| Failure::PrivateFiles)?;
    directory.publish("prepared.partial", PREPARED, &file)?;
    Ok(PreparedReport {
        status: "prepared_only_not_imported",
        prepared_sha256: hash(&receipt),
        prepared,
    })
}

#[derive(Deserialize)]
#[serde(deny_unknown_fields)]
pub struct Target {
    pub format_version: u32,
    pub instance_id: Uuid,
    pub area: AreaId,
    pub connection: Connection,
    pub database: String,
    pub username: String,
    pub password: Option<String>,
}
#[derive(Clone, Serialize, Deserialize)]
#[serde(tag = "kind", rename_all = "snake_case", deny_unknown_fields)]
pub enum Connection {
    Loopback { port: u16 },
    Unix { directory: PathBuf, port: u16 },
}
impl Target {
    fn options(&self) -> Result<PgConnectOptions> {
        if self.format_version != 1 || self.instance_id.is_nil() {
            return Err(Failure::Target);
        }
        self.connection.options(
            &self.database,
            &self.username,
            self.password.as_deref(),
            "memory-area-import",
        )
    }
}
impl Connection {
    fn options(
        &self,
        database: &str,
        username: &str,
        password: Option<&str>,
        application: &str,
    ) -> Result<PgConnectOptions> {
        if std::env::vars_os().any(|(key, _)| key.to_string_lossy().starts_with("PG")) {
            return Err(Failure::Environment);
        }
        let bounded_name = |s: &str| {
            !s.is_empty()
                && s.len() <= 63
                && s.bytes()
                    .all(|b| b.is_ascii_alphanumeric() || b == b'_' || b == b'-')
        };
        if !bounded_name(database)
            || !bounded_name(username)
            || password.is_some_and(|s| s.len() > 4096)
        {
            return Err(Failure::Target);
        }
        let (host, port) = match self {
            Connection::Loopback { port } => ("127.0.0.1", *port),
            Connection::Unix { directory, port } => {
                if !directory.is_absolute()
                    || directory
                        .components()
                        .any(|c| matches!(c, std::path::Component::ParentDir))
                {
                    return Err(Failure::Target);
                }
                (directory.to_str().ok_or(Failure::Target)?, *port)
            }
        };
        if port == 0 {
            return Err(Failure::Target);
        }
        Ok(PgConnectOptions::new_without_pgpass()
            .host(host)
            .port(port)
            .username(username)
            .database(database)
            .password(password.unwrap_or(""))
            .ssl_mode(PgSslMode::Disable)
            .application_name(application)
            .statement_cache_capacity(0))
    }
}
pub struct Execution {
    pub prepared: PathBuf,
    pub prepared_sha256: String,
    pub target: PathBuf,
    pub target_sha256: String,
    pub operation_id: Uuid,
    pub execute: bool,
    pub writers_stopped: bool,
    pub archive_backed_up: bool,
}
/// The assertions are operator attestations, not proof of host identity, backups
/// or writer fencing. No database connection is attempted before full validation.
pub async fn import(
    inputs: &Inputs,
    execution: &Execution,
) -> Result<mem_area_store::ImportReceipt> {
    import_with_budget(inputs, execution, RetainedTargetBudget::default()).await
}

/// The explicit budget must match the hash-bound prepared receipt before any
/// archive rematerialization or target connection. Older receipts mean 256 MiB.
pub async fn import_with_budget(
    inputs: &Inputs,
    execution: &Execution,
    budget: RetainedTargetBudget,
) -> Result<mem_area_store::ImportReceipt> {
    if !execution.execute
        || !execution.writers_stopped
        || !execution.archive_backed_up
        || execution.operation_id.is_nil()
        || !valid_digest(&execution.prepared_sha256)
        || !valid_digest(&execution.target_sha256)
    {
        return Err(Failure::Target);
    }
    let mut receipt_file = Input::open(&execution.prepared, RECEIPT_LIMIT)?;
    let (expected, digest): (Prepared, _) = json(&mut receipt_file, RECEIPT_LIMIT)?;
    if digest != execution.prepared_sha256
        || expected.format_version != 1
        || expected.max_retained_target_mib != budget.mib()
    {
        return Err(Failure::Binding);
    }
    let parent = execution.prepared.parent().ok_or(Failure::PrivateFiles)?;
    let mut archive = Input::open(&parent.join(ARCHIVE), PAYLOAD_LIMIT * 2)?;
    let mut archive_reader = Hashed::new((&mut archive.file).take(PAYLOAD_LIMIT * 2 + 1));
    let length = std::io::copy(&mut archive_reader, &mut std::io::sink())
        .map_err(|_| Failure::PrivateFiles)?;
    if length != expected.archive_bytes || archive_reader.digest() != expected.archive_sha256 {
        return Err(Failure::Binding);
    }
    archive.unchanged()?;
    let (migration, actual) = materialize(inputs, &mut std::io::sink(), budget)?;
    if actual != expected {
        return Err(Failure::Binding);
    }
    let mut target_file = Input::open(&execution.target, TARGET_LIMIT)?;
    let (target, digest): (Target, _) = json(&mut target_file, TARGET_LIMIT)?;
    if digest != execution.target_sha256 {
        return Err(Failure::Binding);
    }
    let options = target.options()?;
    let pool = PgPoolOptions::new()
        .max_connections(1)
        .acquire_timeout(Duration::from_secs(10))
        .connect_with(options)
        .await
        .map_err(|_| Failure::Connection)?;
    receipt_file.unchanged()?;
    target_file.unchanged()?;
    archive.unchanged()?;
    let result = mem_area_store::import_partition(
        &pool,
        target.instance_id,
        execution.operation_id,
        migration.partition(target.area),
    )
    .await;
    pool.close().await;
    result.map_err(Failure::from)
}
