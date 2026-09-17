// SPDX-License-Identifier: AGPL-3.0-or-later

//! Offline, schema-aware area partition planning. No database, model, credentials,
//! or import execution. An inventory is evidence supplied by an external snapshot
//! collector; validating it cannot prove that collector observed every source row.

mod plan;
mod policy;
mod types;

pub use plan::{
    draft_manifest, inspect_inventory, inventory_digest, verify_inventory, verify_manifest,
};
pub use policy::{KNOWN_TABLES, SUPERVISOR_TABLES_0053, table_strategy};
pub use types::*;
