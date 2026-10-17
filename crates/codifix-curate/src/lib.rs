// SPDX-License-Identifier: AGPL-3.0-or-later

// Tests may unwrap; production code must not (workspace lints deny it).
#![cfg_attr(test, allow(clippy::unwrap_used))]

mod repository;

pub mod ingest;

pub use repository::rebuild_memory_chunks as rebuild_memory_chunks_tx;

pub use repository::{
    CaptureMutation, CurateMutation, ReplacementMutation, SemanticDuplicate,
    ValidationRevisionMutation, apply_validation_revision, apply_validation_revision_tx,
    approve_replacement_proposal, approve_replacement_proposal_tx, curate, curate_as, curate_tx,
    curate_tx_with_actor, list_replacement_proposals, prepare_validation_revision_tx,
    preview_capture, preview_curate, refresh_memory_relations, refresh_memory_relations_tx,
    refresh_semantic_relations, refresh_semantic_relations_tx, reject_replacement_proposal,
    reject_replacement_proposal_tx, store_capture, store_capture_as, store_capture_tx,
};
