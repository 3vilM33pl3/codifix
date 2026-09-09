// SPDX-License-Identifier: AGPL-3.0-or-later

// Tests may unwrap; production code must not (workspace lints deny it).
#![cfg_attr(test, allow(clippy::unwrap_used))]

mod repository;

pub mod ingest;

pub use repository::{
    CaptureMutation, CurateMutation, SemanticDuplicate, apply_validation_revision,
    approve_replacement_proposal, curate, curate_as, curate_tx, curate_tx_with_actor,
    list_replacement_proposals, preview_capture, preview_curate, refresh_memory_relations,
    refresh_semantic_relations, refresh_semantic_relations_tx, reject_replacement_proposal,
    store_capture, store_capture_as, store_capture_tx,
};
