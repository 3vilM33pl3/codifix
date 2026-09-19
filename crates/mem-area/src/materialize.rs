// SPDX-License-Identifier: AGPL-3.0-or-later

//! Offline exact-byte materialization. The archive sink is provisional until
//! this function succeeds and the caller durably flushes/protects its output.
//! No database, filesystem path, credentials, external review or importer.

#[cfg(test)]
mod tests;

use std::{
    collections::{BTreeMap, BTreeSet},
    fmt,
    io::{BufRead, Write},
    sync::Arc,
};

use serde::{
    Deserialize, Serialize,
    de::{self, MapAccess, Visitor},
};
use serde_json::value::RawValue;
use sha2::{Digest, Sha256};
use uuid::Uuid;

use crate::*;

/// Matches inventory.mjs: complete PostgreSQL to_jsonb(row)::text, UTF-8, UTC,
/// ISO/YMD dates, postgres intervals, extra_float_digits=3 and bytea_output=hex.
pub const SERIALIZATION_ID: &str = "postgresql-to-jsonb-utf8-utc-v1";
pub const MAX_PAYLOAD_ENVELOPE_BYTES: usize = 64 * 1024 * 1024;
/// Default retained-target budget; larger snapshots require explicit opt-in.
pub const MAX_RETAINED_TARGET_BYTES: usize = 256 * 1024 * 1024;
pub const DEFAULT_RETAINED_TARGET_MIB: u32 = 256;
pub const MAX_RETAINED_TARGET_MIB: u32 = 1024;

/// Validated total retained-target budget. This does not change per-envelope,
/// row validation or archive limits, and is not a process-wide memory limit.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct RetainedTargetBudget(u32);

impl RetainedTargetBudget {
    pub fn from_mib(mib: u32) -> Result<Self, MaterializeError> {
        if !(DEFAULT_RETAINED_TARGET_MIB..=MAX_RETAINED_TARGET_MIB).contains(&mib) {
            return Err(MaterializeError::InvalidTargetBudget);
        }
        Ok(Self(mib))
    }

    pub fn mib(self) -> u32 {
        self.0
    }

    pub fn bytes(self) -> usize {
        self.0 as usize * 1024 * 1024
    }
}

impl Default for RetainedTargetBudget {
    fn default() -> Self {
        Self(DEFAULT_RETAINED_TARGET_MIB)
    }
}

/// Strict NDJSON protocol; table and row keys must be in Rust UTF-8 lexical
/// order. Include each inventoried table, even empty tables, and an EOF footer.
#[derive(Serialize, Deserialize)]
#[serde(tag = "event", rename_all = "snake_case", deny_unknown_fields)]
pub enum PayloadEvent {
    Header {
        format_version: u32,
        source_instance: Uuid,
        snapshot_id: Uuid,
        inventory_sha256: String,
        serialization_id: String,
    },
    TableStart {
        table: String,
        row_count: u64,
    },
    Row {
        table: String,
        key: String,
        row_sha256: String,
        row_json: String,
    },
    TableEnd {
        table: String,
        row_count: u64,
    },
    Complete {
        table_count: u64,
        row_count: u64,
    },
}

/// Deliberately static errors: never include payload, key, column name, JSON
/// parser details, I/O messages or private plan blocker text.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum MaterializeError {
    InvalidInventory,
    InvalidPlan,
    InputIo,
    ArchiveIo,
    EnvelopeTooLarge,
    InvalidEnvelope,
    ProtocolMismatch,
    HashMismatch,
    ColumnMismatch,
    InvalidRow,
    UnsupportedProjection,
    InvalidTargetBudget,
    TargetLimit,
    InternalAccounting,
}

impl fmt::Display for MaterializeError {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        let message = match self {
            Self::InvalidInventory => "payload verification requires a complete valid inventory",
            Self::InvalidPlan => "materialization requires a valid reviewed plan",
            Self::InputIo => "private payload input failed",
            Self::ArchiveIo => "provisional protected archive output failed",
            Self::EnvelopeTooLarge => "private payload envelope exceeds its limit",
            Self::InvalidEnvelope => "invalid private payload envelope",
            Self::ProtocolMismatch => "private payload identity/order/count protocol mismatch",
            Self::HashMismatch => "private payload complete-row digest mismatch",
            Self::ColumnMismatch => "private payload column inventory mismatch",
            Self::InvalidRow => "invalid complete-row JSON or bound row identity",
            Self::UnsupportedProjection => "unsupported or inconsistent reviewed source projection",
            Self::InvalidTargetBudget => "retained target budget must be between 256 and 1024 MiB",
            Self::TargetLimit => "retained target payload exceeds its materialization limit",
            Self::InternalAccounting => "materialization accounting mismatch",
        };
        f.write_str(message)
    }
}
impl std::error::Error for MaterializeError {}

/// Opaque: only a successful materializer can construct a target row. Original
/// owned rows retain exact row_json bytes; projected rows expose a new hash.
#[derive(Clone)]
pub struct MaterializedRow {
    record: RecordRef,
    source_sha256: String,
    target_sha256: String,
    row_json: String,
    columns: Arc<[Column]>,
    source_project_id: Option<Uuid>,
    canonical_id: Option<Uuid>,
}

impl MaterializedRow {
    pub fn record(&self) -> &RecordRef {
        &self.record
    }
    pub fn row_json(&self) -> &str {
        &self.row_json
    }
    pub fn source_sha256(&self) -> &str {
        &self.source_sha256
    }
    /// SHA-256 of exact materialized row_json bytes, not a claim about a future
    /// PostgreSQL rendering. Identical to source bytes for untouched rows.
    pub fn target_sha256(&self) -> &str {
        &self.target_sha256
    }
    pub fn columns(&self) -> &[Column] {
        &self.columns
    }
    pub fn source_project_id(&self) -> Option<Uuid> {
        self.source_project_id
    }
    pub fn canonical_id(&self) -> Option<Uuid> {
        self.canonical_id
    }
}

impl Serialize for MaterializedRow {
    fn serialize<S: serde::Serializer>(&self, serializer: S) -> Result<S::Ok, S::Error> {
        #[derive(Serialize)]
        struct Wire<'a> {
            record: &'a RecordRef,
            source_sha256: &'a str,
            target_sha256: &'a str,
            row_json: &'a str,
            columns: &'a [Column],
            source_project_id: Option<Uuid>,
            canonical_id: Option<Uuid>,
        }
        Wire {
            record: &self.record,
            source_sha256: &self.source_sha256,
            target_sha256: &self.target_sha256,
            row_json: &self.row_json,
            columns: &self.columns,
            source_project_id: self.source_project_id,
            canonical_id: self.canonical_id,
        }
        .serialize(serializer)
    }
}

/// No original context or operator evidence strings are included in the target
/// projection metadata: only exact identity bindings and reviewed content.
#[derive(Clone, Serialize)]
pub struct MaterializedProjection {
    row: MaterializedRow,
    binding: ProvenanceBinding,
    decision_id: Uuid,
    decision_sha256: String,
    content: ProjectedSourceContent,
}

impl MaterializedProjection {
    pub fn row(&self) -> &MaterializedRow {
        &self.row
    }
    pub fn record(&self) -> &RecordRef {
        self.row.record()
    }
    pub fn target_row_json(&self) -> &str {
        self.row.row_json()
    }
    pub fn source_sha256(&self) -> &str {
        self.row.source_sha256()
    }
    pub fn target_sha256(&self) -> &str {
        self.row.target_sha256()
    }
    pub fn columns(&self) -> &[Column] {
        self.row.columns()
    }
    pub fn binding(&self) -> &ProvenanceBinding {
        &self.binding
    }
    pub fn decision_id(&self) -> Uuid {
        self.decision_id
    }
    pub fn decision_sha256(&self) -> &str {
        &self.decision_sha256
    }
    pub fn content(&self) -> &ProjectedSourceContent {
        &self.content
    }
}

/// Serializable for protected instructions, intentionally not Deserialize and
/// without public fields/constructors. Deserializing instructions must not
/// manufacture verified input for a future importer.
#[derive(Clone, Serialize)]
pub struct MaterializedPartition {
    area: AreaId,
    source_instance: Uuid,
    snapshot_id: Uuid,
    source_migration_version: u32,
    inventory_sha256: String,
    manifest_sha256: String,
    partition_sha256: String,
    rows: Vec<MaterializedRow>,
    projected_sources: Vec<MaterializedProjection>,
}

impl MaterializedPartition {
    pub fn area(&self) -> AreaId {
        self.area
    }
    pub fn source_instance(&self) -> Uuid {
        self.source_instance
    }
    pub fn snapshot_id(&self) -> Uuid {
        self.snapshot_id
    }
    pub fn source_migration_version(&self) -> u32 {
        self.source_migration_version
    }
    pub fn inventory_sha256(&self) -> &str {
        &self.inventory_sha256
    }
    pub fn manifest_sha256(&self) -> &str {
        &self.manifest_sha256
    }
    pub fn partition_sha256(&self) -> &str {
        &self.partition_sha256
    }
    pub fn rows(&self) -> &[MaterializedRow] {
        &self.rows
    }
    pub fn projected_sources(&self) -> &[MaterializedProjection] {
        &self.projected_sources
    }
}

#[derive(Clone, Serialize)]
pub struct MaterializedMigration {
    partitions: [MaterializedPartition; 4],
    archive_sha256: String,
    archive_bytes: u64,
    source_table_count: u64,
    source_row_count: u64,
}

fn area_index(area: AreaId) -> usize {
    match area {
        AreaId::Trust => 0,
        AreaId::Homelab => 1,
        AreaId::Workshop => 2,
        AreaId::General => 3,
    }
}

impl MaterializedMigration {
    pub fn partition(&self, area: AreaId) -> &MaterializedPartition {
        &self.partitions[area_index(area)]
    }
    pub fn partitions(&self) -> &[MaterializedPartition; 4] {
        &self.partitions
    }
    pub fn archive_sha256(&self) -> &str {
        &self.archive_sha256
    }
    pub fn archive_bytes(&self) -> u64 {
        self.archive_bytes
    }
    pub fn source_table_count(&self) -> u64 {
        self.source_table_count
    }
    pub fn source_row_count(&self) -> u64 {
        self.source_row_count
    }
}

struct HashWriter<W> {
    writer: W,
    hash: Sha256,
    bytes: u64,
}
impl<W: Write> Write for HashWriter<W> {
    fn write(&mut self, bytes: &[u8]) -> std::io::Result<usize> {
        let written = self.writer.write(bytes)?;
        self.hash.update(&bytes[..written]);
        self.bytes += written as u64;
        Ok(written)
    }
    fn flush(&mut self) -> std::io::Result<()> {
        self.writer.flush()
    }
}

fn digest_json(value: &impl Serialize, domain: &[u8]) -> Result<String, MaterializeError> {
    let mut writer = HashWriter {
        writer: std::io::sink(),
        hash: Sha256::new(),
        bytes: 0,
    };
    writer.hash.update(domain);
    serde_json::to_writer(&mut writer, value).map_err(|_| MaterializeError::InternalAccounting)?;
    Ok(format!("{:x}", writer.hash.finalize()))
}

/// SHA-256 of compact serde Manifest JSON, not the whitespace of an input file.
pub fn manifest_digest(manifest: &Manifest) -> Result<String, MaterializeError> {
    digest_json(manifest, b"")
}

fn read_event(
    reader: &mut impl BufRead,
    limit: usize,
) -> Result<Option<PayloadEvent>, MaterializeError> {
    let mut bytes = Vec::new();
    loop {
        let available = reader.fill_buf().map_err(|_| MaterializeError::InputIo)?;
        if available.is_empty() {
            break;
        }
        let newline = available.iter().position(|byte| *byte == b'\n');
        let count = newline.map_or(available.len(), |position| position + 1);
        if count > limit.saturating_sub(bytes.len()) {
            return Err(MaterializeError::EnvelopeTooLarge);
        }
        bytes.extend_from_slice(&available[..count]);
        reader.consume(count);
        if newline.is_some() {
            break;
        }
    }
    if bytes.is_empty() {
        return Ok(None);
    }
    serde_json::from_slice(&bytes)
        .map(Some)
        .map_err(|_| MaterializeError::InvalidEnvelope)
}

/// Data-integrity evidence only: no ownership decisions, target rows, archive
/// permissions or execution authority are returned by this read-only check.
#[derive(Debug, Serialize)]
pub struct PayloadVerification {
    pub inventory_sha256: String,
    pub source_table_count: u64,
    pub source_row_count: u64,
    pub retained_inventory_notices: usize,
}

/// Check an independently exported payload before reviewing any partition.
/// Reuses the materializer's complete-byte/schema/identity validation and never
/// manufactures a reviewed manifest just to gain access to those checks.
pub fn verify_payload(
    inventory: &Inventory,
    mut payloads: impl BufRead,
) -> Result<PayloadVerification, MaterializeError> {
    let inspection = inspect_inventory(inventory);
    if !inspection.blockers.is_empty() {
        return Err(MaterializeError::InvalidInventory);
    }
    let inventory_sha256 =
        inventory_digest(inventory).map_err(|_| MaterializeError::InternalAccounting)?;
    match read_event(&mut payloads, MAX_PAYLOAD_ENVELOPE_BYTES)? {
        Some(PayloadEvent::Header {
            format_version: 1,
            source_instance,
            snapshot_id,
            inventory_sha256: supplied,
            serialization_id,
        }) if source_instance == inventory.source_instance
            && snapshot_id == inventory.snapshot_id
            && supplied == inventory_sha256
            && serialization_id == SERIALIZATION_ID => {}
        _ => return Err(MaterializeError::ProtocolMismatch),
    }
    let mut tables: Vec<_> = inventory.tables.iter().collect();
    tables.sort_by(|a, b| a.table.cmp(&b.table));
    let index: BTreeMap<_, _> = tables
        .iter()
        .filter(|table| primary_columns(&table.table).is_some())
        .flat_map(|table| {
            table
                .rows
                .iter()
                .map(move |row| ((table.table.as_str(), row.key.as_str()), row))
        })
        .collect();
    let mut total = 0u64;
    for table in &tables {
        match read_event(&mut payloads, MAX_PAYLOAD_ENVELOPE_BYTES)? {
            Some(PayloadEvent::TableStart {
                table: name,
                row_count,
            }) if name == table.table && row_count == table.row_count => {}
            _ => return Err(MaterializeError::ProtocolMismatch),
        }
        let mut rows: Vec<_> = table.rows.iter().collect();
        rows.sort_by(|a, b| a.key.cmp(&b.key));
        for expected in rows {
            read_verified_row(
                &mut payloads,
                MAX_PAYLOAD_ENVELOPE_BYTES,
                inventory,
                table,
                expected,
                &index,
            )?;
            total = total
                .checked_add(1)
                .ok_or(MaterializeError::InternalAccounting)?;
        }
        match read_event(&mut payloads, MAX_PAYLOAD_ENVELOPE_BYTES)? {
            Some(PayloadEvent::TableEnd {
                table: name,
                row_count,
            }) if name == table.table && row_count == table.row_count => {}
            _ => return Err(MaterializeError::ProtocolMismatch),
        }
    }
    match read_event(&mut payloads, MAX_PAYLOAD_ENVELOPE_BYTES)? {
        Some(PayloadEvent::Complete {
            table_count,
            row_count,
        }) if table_count == tables.len() as u64 && row_count == total => {}
        _ => return Err(MaterializeError::ProtocolMismatch),
    }
    if read_event(&mut payloads, MAX_PAYLOAD_ENVELOPE_BYTES)?.is_some() {
        return Err(MaterializeError::ProtocolMismatch);
    }
    Ok(PayloadVerification {
        inventory_sha256,
        source_table_count: tables.len() as u64,
        source_row_count: total,
        retained_inventory_notices: inspection.notices.len(),
    })
}

fn read_verified_row(
    payloads: &mut impl BufRead,
    envelope_limit: usize,
    inventory: &Inventory,
    table: &TableInventory,
    expected: &RowInventory,
    index: &BTreeMap<(&str, &str), &RowInventory>,
) -> Result<(RecordRef, String, RawRow), MaterializeError> {
    let Some(PayloadEvent::Row {
        table: name,
        key,
        row_sha256,
        row_json,
    }) = read_event(payloads, envelope_limit)?
    else {
        return Err(MaterializeError::ProtocolMismatch);
    };
    if name != table.table || key != expected.key {
        return Err(MaterializeError::ProtocolMismatch);
    }
    if row_sha256 != expected.row_sha256
        || format!("{:x}", Sha256::digest(row_json.as_bytes())) != expected.row_sha256
    {
        return Err(MaterializeError::HashMismatch);
    }
    let fields = validate_row(&row_json, table)?;
    validate_bound_metadata(&fields, &table.table, expected)?;
    validate_materializable_identity(&fields, table, expected, index)?;
    if table.table == "public.projects" {
        validate_project_provenance(&fields, inventory)?;
    }
    Ok((RecordRef { table: name, key }, row_json, fields))
}

/// RawValue preserves arbitrarily large exact numbers and nested JSON. A custom
/// visitor rejects duplicate top-level columns rather than overwriting them.
struct RawRow(BTreeMap<String, Box<RawValue>>);
impl<'de> Deserialize<'de> for RawRow {
    fn deserialize<D: serde::Deserializer<'de>>(deserializer: D) -> Result<Self, D::Error> {
        struct RowVisitor;
        impl<'de> Visitor<'de> for RowVisitor {
            type Value = RawRow;
            fn expecting(&self, formatter: &mut fmt::Formatter<'_>) -> fmt::Result {
                formatter.write_str("complete source row object")
            }
            fn visit_map<A: MapAccess<'de>>(self, mut map: A) -> Result<RawRow, A::Error> {
                let mut fields = BTreeMap::new();
                while let Some((key, value)) = map.next_entry::<String, Box<RawValue>>()? {
                    if fields.insert(key, value).is_some() {
                        return Err(de::Error::custom("duplicate source column"));
                    }
                }
                Ok(RawRow(fields))
            }
        }
        deserializer.deserialize_map(RowVisitor)
    }
}

fn validate_row(row_json: &str, table: &TableInventory) -> Result<RawRow, MaterializeError> {
    let row: RawRow = serde_json::from_str(row_json).map_err(|_| MaterializeError::InvalidRow)?;
    let expected: BTreeSet<_> = table
        .columns
        .iter()
        .map(|column| column.name.as_str())
        .collect();
    if row.0.keys().map(String::as_str).collect::<BTreeSet<_>>() != expected {
        return Err(MaterializeError::ColumnMismatch);
    }
    if table.columns.iter().any(|column| {
        !column.nullable
            && row
                .0
                .get(&column.name)
                .is_some_and(|value| value.get() == "null")
    }) {
        return Err(MaterializeError::InvalidRow);
    }
    Ok(row)
}

fn raw_uuid(row: &RawRow, column: &str) -> Option<Uuid> {
    row.0
        .get(column)
        .and_then(|value| serde_json::from_str(value.get()).ok())
}

fn validate_bound_metadata(
    fields: &RawRow,
    table: &str,
    row: &RowInventory,
) -> Result<(), MaterializeError> {
    if table == "public.memory_entries"
        && (row.canonical_id.is_none() || raw_uuid(fields, "canonical_id") != row.canonical_id)
    {
        return Err(MaterializeError::InvalidRow);
    }
    for (column, expected) in [
        ("project_id", row.source_project_id),
        ("canonical_id", row.canonical_id),
    ] {
        if let Some(expected) = expected
            && fields.0.contains_key(column)
            && raw_uuid(fields, column) != Some(expected)
        {
            return Err(MaterializeError::InvalidRow);
        }
    }
    if let Some(version) = &row.version
        && (!fields.0.get("version_no").is_some_and(|value| {
            serde_json::from_str::<u32>(value.get()).ok() == Some(version.version_no)
        }) || !fields.0.get("is_tombstone").is_some_and(|value| {
            serde_json::from_str::<bool>(value.get()).ok() == Some(version.is_tombstone)
        }))
    {
        return Err(MaterializeError::InvalidRow);
    }
    Ok(())
}

// These are the only SQL record identities this materializer can send to an
// area. Inventory v1 does not carry a primary-key catalog, so never infer an
// unfamiliar shape from an `id`-looking column.
fn primary_columns(table: &str) -> Option<&'static [&'static str]> {
    match table {
        "public.projects"
        | "public.sessions"
        | "public.tasks"
        | "public.raw_captures"
        | "public.memory_entries"
        | "public.memory_sources"
        | "public.memory_source_checks"
        | "public.memory_relations"
        | "public.project_commits" => Some(&["id"]),
        "public.memory_canonical_state" => Some(&["canonical_id"]),
        "public.memory_tags" => Some(&["memory_entry_id", "tag"]),
        _ => None,
    }
}

fn key_from_columns(
    row: &RawRow,
    columns: &[&str],
    integer_key: bool,
) -> Result<Option<String>, MaterializeError> {
    if integer_key {
        let [column] = columns else {
            return Err(MaterializeError::InvalidRow);
        };
        let value = row.0.get(*column).ok_or(MaterializeError::InvalidRow)?;
        if value.get() == "null" {
            return Ok(None);
        }
        let value =
            serde_json::from_str::<i64>(value.get()).map_err(|_| MaterializeError::InvalidRow)?;
        return Ok(Some(value.to_string()));
    }
    let mut values = Vec::with_capacity(columns.len());
    for column in columns {
        let value = row.0.get(*column).ok_or(MaterializeError::InvalidRow)?;
        if value.get() == "null" {
            return Ok(None);
        }
        // Other supported PKs are UUID or text. Decode strings but never numeric
        // content. PostgreSQL JSONB array keys place one space after commas.
        let value: String =
            serde_json::from_str(value.get()).map_err(|_| MaterializeError::InvalidRow)?;
        if *column != "tag" && Uuid::parse_str(&value).is_err() {
            return Err(MaterializeError::InvalidRow);
        }
        values.push(value);
    }
    match values.as_slice() {
        [one] => Ok(Some(one.clone())),
        [first, second] => Ok(Some(format!(
            "[{}, {}]",
            serde_json::to_string(first).map_err(|_| MaterializeError::InvalidRow)?,
            serde_json::to_string(second).map_err(|_| MaterializeError::InvalidRow)?
        ))),
        _ => Err(MaterializeError::InvalidRow),
    }
}

fn validate_materializable_identity(
    fields: &RawRow,
    table: &TableInventory,
    row: &RowInventory,
    index: &BTreeMap<(&str, &str), &RowInventory>,
) -> Result<(), MaterializeError> {
    let Some(primary) = primary_columns(&table.table) else {
        return Ok(());
    };
    let required_foreign_keys: &[(&str, &str)] = match table.table.as_str() {
        "public.sessions"
        | "public.memory_entries"
        | "public.memory_canonical_state"
        | "public.project_commits" => &[("project_id", "public.projects")],
        "public.tasks" => &[("session_id", "public.sessions")],
        "public.raw_captures" => &[("task_id", "public.tasks")],
        "public.memory_sources" => &[
            ("memory_entry_id", "public.memory_entries"),
            ("task_id", "public.tasks"),
        ],
        "public.memory_tags" => &[("memory_entry_id", "public.memory_entries")],
        "public.memory_source_checks" => &[("source_id", "public.memory_sources")],
        _ => &[],
    };
    for (column, target) in required_foreign_keys {
        if !table.foreign_keys.iter().any(|fk| {
            fk.columns == [*column] && fk.target_table == *target && fk.target_columns == ["id"]
        }) {
            return Err(MaterializeError::InvalidRow);
        }
    }
    if required_foreign_keys
        .iter()
        .any(|(column, _)| *column == "project_id")
        && (row.source_project_id.is_none()
            || raw_uuid(fields, "project_id") != row.source_project_id)
    {
        return Err(MaterializeError::InvalidRow);
    }
    if key_from_columns(
        fields,
        primary,
        table.table == "public.memory_source_checks",
    )?
    .as_ref()
        != Some(&row.key)
    {
        return Err(MaterializeError::InvalidRow);
    }
    if let Some((column, parent_table)) = match table.table.as_str() {
        "public.tasks" => Some(("session_id", "public.sessions")),
        "public.raw_captures" => Some(("task_id", "public.tasks")),
        _ => None,
    } {
        let key =
            key_from_columns(fields, &[column], false)?.ok_or(MaterializeError::InvalidRow)?;
        let parent = index
            .get(&(parent_table, key.as_str()))
            .ok_or(MaterializeError::InvalidRow)?;
        if row.source_project_id.is_none() || row.source_project_id != parent.source_project_id {
            return Err(MaterializeError::InvalidRow);
        }
    }
    for reference in &row.references {
        let (target_table, actual_key) = if let Some(fk) = table
            .foreign_keys
            .iter()
            .find(|fk| fk.name == reference.constraint)
        {
            let target_primary =
                primary_columns(&fk.target_table).ok_or(MaterializeError::InvalidRow)?;
            if !fk
                .target_columns
                .iter()
                .map(String::as_str)
                .eq(target_primary.iter().copied())
            {
                return Err(MaterializeError::InvalidRow);
            }
            let source_columns: Vec<_> = fk.columns.iter().map(String::as_str).collect();
            (
                fk.target_table.as_str(),
                key_from_columns(
                    fields,
                    &source_columns,
                    fk.target_table == "public.memory_source_checks",
                )?,
            )
        } else {
            let (column, target) = match (table.table.as_str(), reference.constraint.as_str()) {
                ("public.memory_sources", "logical:target_memory_id") => {
                    ("target_memory_id", "public.memory_entries")
                }
                ("public.memory_relations", "logical:src_canonical_id") => {
                    ("src_canonical_id", "public.memory_canonical_state")
                }
                ("public.memory_relations", "logical:dst_canonical_id") => {
                    ("dst_canonical_id", "public.memory_canonical_state")
                }
                _ => return Err(MaterializeError::InvalidRow),
            };
            (target, key_from_columns(fields, &[column], false)?)
        };
        match (&reference.target, actual_key) {
            (None, None) => {}
            (Some(expected), Some(key))
                if expected.table == target_table && expected.key == key => {}
            _ => return Err(MaterializeError::InvalidRow),
        }
    }
    Ok(())
}

fn validate_project_provenance(
    fields: &RawRow,
    inventory: &Inventory,
) -> Result<(), MaterializeError> {
    let project = inventory
        .projects
        .iter()
        .find(|project| Some(project.id) == raw_uuid(fields, "id"))
        .ok_or(MaterializeError::InvalidRow)?;
    for (column, expected) in [
        ("slug", &project.slug),
        ("name", &project.name),
        ("root_path", &project.root_path),
        ("created_at", &project.created_at),
    ] {
        if !fields.0.get(column).is_some_and(|value| {
            serde_json::from_str::<String>(value.get()).is_ok_and(|value| value == *expected)
        }) {
            return Err(MaterializeError::InvalidRow);
        }
    }
    Ok(())
}

fn projected_row(
    row: &RawRow,
    table: &TableInventory,
    projection: &ProvenanceProjection,
) -> Result<String, MaterializeError> {
    const FIELDS: [(&str, &str, bool); 14] = [
        ("id", "uuid", false),
        ("memory_entry_id", "uuid", false),
        ("task_id", "uuid", true),
        ("file_path", "text", true),
        ("git_commit", "text", true),
        ("source_kind", "text", false),
        ("excerpt", "text", true),
        ("created_at", "timestamp with time zone", false),
        ("symbol_name", "text", true),
        ("symbol_kind", "text", true),
        ("line_start", "integer", true),
        ("line_end", "integer", true),
        ("content_hash", "text", true),
        ("target_memory_id", "uuid", true),
    ];
    let binding = &projection.binding;
    if table.table != "public.memory_sources"
        || table.columns.len() != FIELDS.len()
        || FIELDS.iter().any(|(name, data_type, nullable)| {
            !table.columns.iter().any(|column| {
                column.name == *name
                    && column.data_type == *data_type
                    && column.nullable == *nullable
            })
        })
        || raw_uuid(row, "id").map(|id| id.to_string()).as_ref() != Some(&binding.source.record.key)
        || raw_uuid(row, "memory_entry_id")
            .map(|id| id.to_string())
            .as_ref()
            != Some(&binding.memory_version.record.key)
        || raw_uuid(row, "task_id").map(|id| id.to_string()).as_ref()
            != Some(&binding.archived_task.record.key)
        || !row
            .0
            .get("target_memory_id")
            .is_some_and(|value| value.get() == "null")
        || !row.0.get("created_at").is_some_and(|value| {
            serde_json::from_str::<String>(value.get()).is_ok_and(|value| !value.trim().is_empty())
        })
    {
        return Err(MaterializeError::UnsupportedProjection);
    }
    let excerpt = match &projection.content {
        ProjectedSourceContent::ArchiveReferenceOnly {} => "null".to_owned(),
        ProjectedSourceContent::ReviewedExcerpt { text, .. } => {
            serde_json::to_string(text).map_err(|_| MaterializeError::UnsupportedProjection)?
        }
    };
    // This is deterministic target wire JSON, not PostgreSQL's canonical text
    // rendering. The importer must compare typed/jsonb values and separately
    // record the PostgreSQL-rendered target hash. Original values stay raw.
    let null = RawValue::from_string("null".into())
        .map_err(|_| MaterializeError::UnsupportedProjection)?;
    let note = RawValue::from_string("\"note\"".into())
        .map_err(|_| MaterializeError::UnsupportedProjection)?;
    let excerpt =
        RawValue::from_string(excerpt).map_err(|_| MaterializeError::UnsupportedProjection)?;
    let mut target = BTreeMap::new();
    for (name, _, _) in FIELDS {
        let value = match name {
            "id" | "memory_entry_id" | "created_at" => row
                .0
                .get(name)
                .ok_or(MaterializeError::UnsupportedProjection)?,
            "source_kind" => &note,
            "excerpt" => &excerpt,
            _ => &null,
        };
        target.insert(name, value);
    }
    serde_json::to_string(&target).map_err(|_| MaterializeError::UnsupportedProjection)
}

fn retain_budget(used: &mut usize, bytes: usize, limit: usize) -> Result<(), MaterializeError> {
    *used = used
        .checked_add(bytes)
        .ok_or(MaterializeError::TargetLimit)?;
    if *used > limit {
        return Err(MaterializeError::TargetLimit);
    }
    Ok(())
}

fn write_archive(writer: &mut impl Write, value: &impl Serialize) -> Result<(), MaterializeError> {
    serde_json::to_writer(&mut *writer, value).map_err(|_| MaterializeError::ArchiveIo)?;
    writer
        .write_all(b"\n")
        .map_err(|_| MaterializeError::ArchiveIo)
}

/// Validate all source bytes and construct opaque, deterministic per-area
/// instructions. On ANY failure no trusted object is returned; the archive
/// writer may contain a private incomplete prefix and must be discarded. Caller
/// owns mode0700/0600, exclusive files, durable fsync and promotion. This result
/// is not operator authentication, database import, or cutover authority.
pub fn materialize<R: BufRead, W: Write>(
    inventory: &Inventory,
    manifest: &Manifest,
    payloads: R,
    protected_archive: &mut W,
) -> Result<MaterializedMigration, MaterializeError> {
    materialize_with_budget(
        inventory,
        manifest,
        payloads,
        protected_archive,
        RetainedTargetBudget::default(),
    )
}

/// Explicit bounded opt-in for a larger retained target. All validation and
/// provisional archive semantics are identical to [`materialize`].
pub fn materialize_with_budget<R: BufRead, W: Write>(
    inventory: &Inventory,
    manifest: &Manifest,
    payloads: R,
    protected_archive: &mut W,
    budget: RetainedTargetBudget,
) -> Result<MaterializedMigration, MaterializeError> {
    materialize_bounded(
        inventory,
        manifest,
        payloads,
        protected_archive,
        MAX_PAYLOAD_ENVELOPE_BYTES,
        budget.bytes(),
    )
}

fn materialize_bounded<R: BufRead, W: Write>(
    inventory: &Inventory,
    manifest: &Manifest,
    mut payloads: R,
    protected_archive: &mut W,
    envelope_limit: usize,
    target_limit: usize,
) -> Result<MaterializedMigration, MaterializeError> {
    let verification = verify_manifest(inventory, manifest);
    if !verification.plan_valid {
        return Err(MaterializeError::InvalidPlan);
    }
    let manifest_sha256 = manifest_digest(manifest)?;
    match read_event(&mut payloads, envelope_limit)? {
        Some(PayloadEvent::Header {
            format_version: 1,
            source_instance,
            snapshot_id,
            inventory_sha256,
            serialization_id,
        }) if source_instance == inventory.source_instance
            && snapshot_id == inventory.snapshot_id
            && inventory_sha256 == manifest.inventory_sha256
            && serialization_id == SERIALIZATION_ID => {}
        _ => return Err(MaterializeError::ProtocolMismatch),
    }
    let mut partitions = std::array::from_fn(|index| MaterializedPartition {
        area: AREAS[index],
        source_instance: inventory.source_instance,
        snapshot_id: inventory.snapshot_id,
        source_migration_version: inventory.migration_version,
        inventory_sha256: manifest.inventory_sha256.clone(),
        manifest_sha256: manifest_sha256.clone(),
        partition_sha256: String::new(),
        rows: Vec::new(),
        projected_sources: Vec::new(),
    });
    let mut archive = HashWriter {
        writer: protected_archive,
        hash: Sha256::new(),
        bytes: 0,
    };
    write_archive(
        &mut archive,
        &serde_json::json!({ "event":"header", "format_version":1, "source_instance":inventory.source_instance, "snapshot_id":inventory.snapshot_id, "source_migration_version":inventory.migration_version, "inventory_sha256":manifest.inventory_sha256, "manifest_sha256":manifest_sha256, "serialization_id":SERIALIZATION_ID }),
    )?;
    let mut tables: Vec<_> = inventory.tables.iter().collect();
    tables.sort_by(|a, b| a.table.cmp(&b.table));
    let identity_index: BTreeMap<_, _> = tables
        .iter()
        .filter(|table| primary_columns(&table.table).is_some())
        .flat_map(|table| {
            table
                .rows
                .iter()
                .map(move |row| ((table.table.as_str(), row.key.as_str()), row))
        })
        .collect();
    let mut actions = verification.records.into_iter();
    let projections: BTreeMap<_, _> = verification
        .provenance_projections
        .into_iter()
        .map(|projection| (projection.binding.source.record.clone(), projection))
        .collect();
    let mut retained = 0;
    let mut total_rows = 0u64;
    for table in &tables {
        match read_event(&mut payloads, envelope_limit)? {
            Some(PayloadEvent::TableStart {
                table: name,
                row_count,
            }) if name == table.table && row_count == table.row_count => {}
            _ => return Err(MaterializeError::ProtocolMismatch),
        }
        let mut rows: Vec<_> = table.rows.iter().collect();
        rows.sort_by(|a, b| a.key.cmp(&b.key));
        write_archive(
            &mut archive,
            &serde_json::json!({ "event":"table_start", "table":table.table, "row_count":table.row_count, "columns":table.columns, "foreign_keys":table.foreign_keys }),
        )?;
        let columns: Arc<[Column]> = table.columns.clone().into();
        for expected in rows {
            let (record, row_json, fields) = read_verified_row(
                &mut payloads,
                envelope_limit,
                inventory,
                table,
                expected,
                &identity_index,
            )?;
            let row_sha256 = expected.row_sha256.clone();
            let action = actions.next().ok_or(MaterializeError::InternalAccounting)?;
            if action.record != record || action.source_sha256 != row_sha256 {
                return Err(MaterializeError::InternalAccounting);
            }
            #[derive(Serialize)]
            struct Original<'a> {
                event: &'static str,
                record: &'a RecordRef,
                source_sha256: &'a str,
                source_project_id: Option<Uuid>,
                canonical_id: Option<Uuid>,
                destination: &'a Destination,
                row_json: &'a str,
            }
            write_archive(
                &mut archive,
                &Original {
                    event: "original_row",
                    record: &record,
                    source_sha256: &row_sha256,
                    source_project_id: action.source_project_id,
                    canonical_id: action.canonical_id,
                    destination: &action.destination,
                    row_json: &row_json,
                },
            )?;
            let mut exact_areas = Vec::new();
            if let Destination::Area(area) = action.destination {
                exact_areas.push(area);
            }
            if table.table == "public.projects" {
                exact_areas.extend(
                    manifest
                        .projects
                        .iter()
                        .filter(|binding| binding.source.id.to_string() == record.key)
                        .map(|binding| binding.area),
                );
            }
            for area in exact_areas {
                retain_budget(
                    &mut retained,
                    row_json.len() + record.table.len() + record.key.len() + 512,
                    target_limit,
                )?;
                partitions[area_index(area)].rows.push(MaterializedRow {
                    record: record.clone(),
                    source_sha256: row_sha256.clone(),
                    target_sha256: row_sha256.clone(),
                    row_json: row_json.clone(),
                    columns: Arc::clone(&columns),
                    source_project_id: action.source_project_id,
                    canonical_id: action.canonical_id,
                });
            }
            if let Destination::ArchiveAndProjectProvenance(area) = action.destination {
                let projection = projections
                    .get(&record)
                    .ok_or(MaterializeError::InternalAccounting)?;
                let target = projected_row(&fields, table, projection)?;
                // Bound both the output string and retained reviewed excerpt.
                let metadata_bytes =
                    serde_json::to_vec(&(&projection.binding, &projection.content))
                        .map_err(|_| MaterializeError::InternalAccounting)?
                        .len();
                retain_budget(
                    &mut retained,
                    target.len() + metadata_bytes + 1024,
                    target_limit,
                )?;
                let target_sha256 = format!("{:x}", Sha256::digest(target.as_bytes()));
                partitions[area_index(area)]
                    .projected_sources
                    .push(MaterializedProjection {
                        row: MaterializedRow {
                            record,
                            source_sha256: row_sha256,
                            target_sha256,
                            row_json: target,
                            columns: Arc::clone(&columns),
                            source_project_id: action.source_project_id,
                            canonical_id: action.canonical_id,
                        },
                        binding: projection.binding.clone(),
                        decision_id: projection.decision_id,
                        decision_sha256: projection.operator_attestation.decision_sha256.clone(),
                        content: projection.content.clone(),
                    });
            }
            total_rows = total_rows
                .checked_add(1)
                .ok_or(MaterializeError::InternalAccounting)?;
        }
        match read_event(&mut payloads, envelope_limit)? {
            Some(PayloadEvent::TableEnd {
                table: name,
                row_count,
            }) if name == table.table && row_count == table.row_count => {}
            _ => return Err(MaterializeError::ProtocolMismatch),
        }
        write_archive(
            &mut archive,
            &serde_json::json!({"event":"table_end", "table":table.table, "row_count":table.row_count}),
        )?;
    }
    if actions.next().is_some()
        || partitions
            .iter()
            .map(|part| part.projected_sources.len())
            .sum::<usize>()
            != projections.len()
    {
        return Err(MaterializeError::InternalAccounting);
    }
    match read_event(&mut payloads, envelope_limit)? {
        Some(PayloadEvent::Complete {
            table_count,
            row_count,
        }) if table_count == tables.len() as u64 && row_count == total_rows => {}
        _ => return Err(MaterializeError::ProtocolMismatch),
    }
    if read_event(&mut payloads, envelope_limit)?.is_some() {
        return Err(MaterializeError::ProtocolMismatch);
    }
    for partition in &mut partitions {
        // A blank partition_sha256 is part of the specified digest preimage;
        // the completed serialized instruction includes its final digest.
        partition.partition_sha256 =
            digest_json(partition, b"mem-area-materialized-partition-v1\n")?;
    }
    write_archive(
        &mut archive,
        &serde_json::json!({"event":"complete", "table_count":tables.len(), "row_count":total_rows}),
    )?;
    archive.flush().map_err(|_| MaterializeError::ArchiveIo)?;
    Ok(MaterializedMigration {
        partitions,
        archive_sha256: format!("{:x}", archive.hash.finalize()),
        archive_bytes: archive.bytes,
        source_table_count: tables.len() as u64,
        source_row_count: total_rows,
    })
}
