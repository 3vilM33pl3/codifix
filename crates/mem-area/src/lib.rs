// SPDX-License-Identifier: AGPL-3.0-or-later

//! Offline, schema-aware area partition planning. No database, model, credentials,
//! or import execution. An inventory is evidence supplied by an external snapshot
//! collector; validating it cannot prove that collector observed every source row.

mod materialize;
mod plan;
mod policy;
mod types;

pub use materialize::{
    MAX_PAYLOAD_ENVELOPE_BYTES, MAX_RETAINED_TARGET_BYTES, MaterializeError, MaterializedMigration,
    MaterializedPartition, MaterializedProjection, MaterializedRow, PayloadEvent, SERIALIZATION_ID,
    manifest_digest, materialize,
};
pub use plan::{
    draft_manifest, inspect_inventory, inventory_digest, verify_inventory, verify_manifest,
};
pub use policy::{
    AREA_IMPORT_TABLES_0056, KNOWN_TABLES, SUPERVISOR_TABLES_0053, SUPERVISOR_TABLES_0054,
    SUPERVISOR_TABLES_0055, table_strategy,
};
pub use types::*;
