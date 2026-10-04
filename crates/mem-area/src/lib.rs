// SPDX-License-Identifier: AGPL-3.0-or-later

//! Offline, schema-aware area partition planning. No database, model, credentials,
//! or import execution. An inventory is evidence supplied by an external snapshot
//! collector; validating it cannot prove that collector observed every source row.

mod materialize;
mod plan;
mod policy;
mod types;

pub use materialize::portable;

pub use materialize::{
    DEFAULT_RETAINED_TARGET_MIB, MAX_PAYLOAD_ENVELOPE_BYTES, MAX_RETAINED_TARGET_BYTES,
    MAX_RETAINED_TARGET_MIB, MaterializeError, MaterializedMigration, MaterializedPartition,
    MaterializedProjection, MaterializedRow, PayloadEvent, PayloadVerification,
    RetainedTargetBudget, SERIALIZATION_ID, manifest_digest, materialize, materialize_with_budget,
    verify_payload,
};
pub use plan::{
    draft_manifest, inspect_inventory, inventory_digest, verify_inventory, verify_manifest,
};
pub use policy::{
    ADDRESSED_EXPERTISE_TABLES_0062, AGENT_EXPERTISE_TABLES_0063, AREA_IMPORT_TABLES_0056,
    AREA_RECONCILIATION_TABLES_0061, KNOWN_TABLES, SUPERVISOR_TABLES_0053, SUPERVISOR_TABLES_0054,
    SUPERVISOR_TABLES_0055, SUPERVISOR_TABLES_0057, SUPERVISOR_TABLES_0059, SUPERVISOR_TABLES_0060,
    table_strategy,
};
pub use types::*;
