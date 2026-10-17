// SPDX-License-Identifier: AGPL-3.0-or-later

use crate::TableStrategy;

/// Explicit policy catalog for migration 0052. New or removed tables require
/// a code review; prefixes are never sufficient to classify an unknown table.
pub const KNOWN_TABLES: &[(&str, TableStrategy)] = &[
    ("public._sqlx_migrations", TableStrategy::HistoricalArchive),
    ("public.projects", TableStrategy::ProjectProvenance),
    ("public.sessions", TableStrategy::Owned),
    ("public.tasks", TableStrategy::Owned),
    ("public.raw_captures", TableStrategy::Owned),
    ("public.memory_entries", TableStrategy::Owned),
    ("public.memory_canonical_state", TableStrategy::Owned),
    ("public.memory_sources", TableStrategy::Owned),
    ("public.memory_source_checks", TableStrategy::Owned),
    ("public.memory_tags", TableStrategy::Owned),
    ("public.memory_relations", TableStrategy::Owned),
    ("public.project_commits", TableStrategy::Owned),
    ("public.memory_chunks", TableStrategy::RebuildDerived),
    (
        "public.memory_chunk_embeddings",
        TableStrategy::RebuildDerived,
    ),
    (
        "public.graph_extraction_runs",
        TableStrategy::HistoricalArchive,
    ),
    ("public.graph_nodes", TableStrategy::RebuildDerived),
    ("public.graph_edges", TableStrategy::RebuildDerived),
    ("public.graph_evidence", TableStrategy::HistoricalArchive),
    ("public.code_symbols", TableStrategy::RebuildDerived),
    ("public.code_references", TableStrategy::RebuildDerived),
    ("public.curation_runs", TableStrategy::HistoricalArchive),
    (
        "public.memory_bundle_imports",
        TableStrategy::HistoricalArchive,
    ),
    (
        "public.imported_memory_entries",
        TableStrategy::HistoricalArchive,
    ),
    (
        "public.project_timeline_events",
        TableStrategy::HistoricalArchive,
    ),
    (
        "public.memory_replacement_proposals",
        TableStrategy::HistoricalArchive,
    ),
    ("public.loop_definitions", TableStrategy::RetireRuntime),
    ("public.loop_global_state", TableStrategy::RetireRuntime),
    ("public.loop_settings", TableStrategy::RetireRuntime),
    (
        "public.loop_setting_audit",
        TableStrategy::HistoricalArchive,
    ),
    ("public.trigger_events", TableStrategy::RetireRuntime),
    ("public.loop_runs", TableStrategy::HistoricalArchive),
    ("public.approval_requests", TableStrategy::RetireRuntime),
    ("public.memory_proposals", TableStrategy::HistoricalArchive),
    ("public.learned_skills", TableStrategy::HistoricalArchive),
    ("public.run_traces", TableStrategy::HistoricalArchive),
    ("public.agent_workspaces", TableStrategy::RetireRuntime),
    ("public.memory_scores", TableStrategy::HistoricalArchive),
    (
        "public.memory_access_events",
        TableStrategy::HistoricalArchive,
    ),
    (
        "public.memory_score_audit",
        TableStrategy::HistoricalArchive,
    ),
    (
        "public.memory_validation_runs",
        TableStrategy::HistoricalArchive,
    ),
    (
        "public.memory_validation_evidence",
        TableStrategy::HistoricalArchive,
    ),
    (
        "public.procedural_utility",
        TableStrategy::HistoricalArchive,
    ),
    (
        "public.procedural_utility_audit",
        TableStrategy::HistoricalArchive,
    ),
    (
        "public.loop_produced_memory",
        TableStrategy::HistoricalArchive,
    ),
    ("public.auth_principals", TableStrategy::HistoricalArchive),
    ("public.auth_service_tokens", TableStrategy::RetireRuntime),
    (
        "public.auth_project_memberships",
        TableStrategy::RetireRuntime,
    ),
    ("public.auth_web_sessions", TableStrategy::RetireRuntime),
    ("public.auth_oidc_flows", TableStrategy::RetireRuntime),
    ("public.auth_audit_events", TableStrategy::HistoricalArchive),
    ("public.timeline_position", TableStrategy::RetireRuntime),
    (
        "public.timeline_retention_watermarks",
        TableStrategy::HistoricalArchive,
    ),
    (
        "public.background_project_slots",
        TableStrategy::RetireRuntime,
    ),
    ("public.background_jobs", TableStrategy::RetireRuntime),
    (
        "public.background_job_dependencies",
        TableStrategy::RetireRuntime,
    ),
    (
        "public.background_job_attempts",
        TableStrategy::HistoricalArchive,
    ),
    (
        "public.background_job_retry_receipts",
        TableStrategy::HistoricalArchive,
    ),
    (
        "public.background_work_versions",
        TableStrategy::RetireRuntime,
    ),
    (
        "public.background_job_outputs",
        TableStrategy::HistoricalArchive,
    ),
    (
        "public.background_synthesis_inputs",
        TableStrategy::HistoricalArchive,
    ),
    (
        "public.background_daily_budget",
        TableStrategy::HistoricalArchive,
    ),
    (
        "public.proposal_creation_receipts",
        TableStrategy::HistoricalArchive,
    ),
    ("public.external_effects", TableStrategy::HistoricalArchive),
    (
        "public.external_effect_outcomes",
        TableStrategy::HistoricalArchive,
    ),
    (
        "public.foreground_executions",
        TableStrategy::HistoricalArchive,
    ),
    (
        "public.federation_gateway_spaces",
        TableStrategy::RetireRuntime,
    ),
    (
        "public.federation_source_outbox",
        TableStrategy::RetireRuntime,
    ),
    (
        "public.federation_accepted",
        TableStrategy::HistoricalArchive,
    ),
    (
        "public.federation_publish_outbox",
        TableStrategy::RetireRuntime,
    ),
    ("public.federation_replicas", TableStrategy::RetireRuntime),
    ("public.federation_inbox", TableStrategy::RetireRuntime),
    (
        "public.federation_local_grants",
        TableStrategy::RetireRuntime,
    ),
    (
        "public.federation_publication_reviews",
        TableStrategy::HistoricalArchive,
    ),
    (
        "public.federation_source_subscriptions",
        TableStrategy::RetireRuntime,
    ),
    (
        "public.federation_source_changes",
        TableStrategy::RetireRuntime,
    ),
    ("public.federation_security", TableStrategy::RetireRuntime),
    ("public.federation_audit", TableStrategy::HistoricalArchive),
    (
        "public.federation_replica_receipts",
        TableStrategy::HistoricalArchive,
    ),
    (
        "public.federation_replica_trust",
        TableStrategy::RetireRuntime,
    ),
    (
        "public.federation_retention_decisions",
        TableStrategy::HistoricalArchive,
    ),
    (
        "public.federation_review_requests",
        TableStrategy::HistoricalArchive,
    ),
    (
        "public.federation_action_requests",
        TableStrategy::RetireRuntime,
    ),
    (
        "public.federation_expertise_collections",
        TableStrategy::RetireRuntime,
    ),
    (
        "public.federation_expertise_reviews",
        TableStrategy::HistoricalArchive,
    ),
    (
        "public.federation_expertise_collection_grants",
        TableStrategy::RetireRuntime,
    ),
    (
        "public.federation_expertise_collection_audit",
        TableStrategy::HistoricalArchive,
    ),
    (
        "public.federation_expertise_mailbox",
        TableStrategy::RetireRuntime,
    ),
    (
        "public.federation_expertise_attempts",
        TableStrategy::HistoricalArchive,
    ),
    (
        "public.federation_expertise_daily_budget",
        TableStrategy::HistoricalArchive,
    ),
    (
        "public.federation_expertise_deliveries",
        TableStrategy::HistoricalArchive,
    ),
    (
        "public.federation_expertise_requests",
        TableStrategy::HistoricalArchive,
    ),
    (
        "public.federation_expertise_request_audit",
        TableStrategy::HistoricalArchive,
    ),
    (
        "public.federation_expertise_origin_results",
        TableStrategy::HistoricalArchive,
    ),
];

/// Exact additive migration-0053 catalog. No authority, lease, policy or pending
/// action is imported as live state; accounting/effect history remains archived.
pub const SUPERVISOR_TABLES_0053: &[(&str, TableStrategy)] = &[
    (
        "public.supervisor_authorities",
        TableStrategy::RetireRuntime,
    ),
    ("public.supervisor_policies", TableStrategy::RetireRuntime),
    ("public.supervisor_actions", TableStrategy::RetireRuntime),
    (
        "public.supervisor_budget_days",
        TableStrategy::RetireRuntime,
    ),
    (
        "public.supervisor_attempt_states",
        TableStrategy::RetireRuntime,
    ),
    (
        "public.supervisor_journal_heads",
        TableStrategy::RetireRuntime,
    ),
    (
        "public.supervisor_attempts",
        TableStrategy::HistoricalArchive,
    ),
    (
        "public.supervisor_settlements",
        TableStrategy::HistoricalArchive,
    ),
    (
        "public.supervisor_action_results",
        TableStrategy::HistoricalArchive,
    ),
    (
        "public.supervisor_journal",
        TableStrategy::HistoricalArchive,
    ),
];

/// Private immutable model outputs are historical evidence, never retrievable
/// knowledge or an authorization to enact a proposal after migration.
pub const SUPERVISOR_TABLES_0054: &[(&str, TableStrategy)] = &[(
    "public.supervisor_proposals",
    TableStrategy::HistoricalArchive,
)];

/// Instance binding is retired, never installed as a new area's identity.
/// Model contexts remain private immutable evidence, not live authority.
pub const SUPERVISOR_TABLES_0055: &[(&str, TableStrategy)] = &[
    (
        "public.supervisor_instance_binding",
        TableStrategy::RetireRuntime,
    ),
    (
        "public.supervisor_model_contexts",
        TableStrategy::HistoricalArchive,
    ),
];

/// Old import receipts are retained evidence, never proof of a fresh target
/// import. A future importer creates its own destination-bound receipts.
pub const AREA_IMPORT_TABLES_0056: &[(&str, TableStrategy)] = &[
    (
        "public.area_import_receipts",
        TableStrategy::HistoricalArchive,
    ),
    ("public.area_import_rows", TableStrategy::HistoricalArchive),
    (
        "public.area_import_provenance",
        TableStrategy::HistoricalArchive,
    ),
];

pub const SUPERVISOR_TABLES_0057: &[(&str, TableStrategy)] = &[
    (
        "public.supervisor_health_schedule",
        TableStrategy::RetireRuntime,
    ),
    (
        "public.supervisor_health_observations",
        TableStrategy::HistoricalArchive,
    ),
];

/// Human decisions are retained evidence, never fresh execution permissions.
pub const SUPERVISOR_TABLES_0059: &[(&str, TableStrategy)] = &[(
    "public.supervisor_proposal_reviews",
    TableStrategy::HistoricalArchive,
)];

/// Preserve private consent/outcomes as historical evidence, never runnable
/// queue authority in a new area or a restored target.
pub const SUPERVISOR_TABLES_0060: &[(&str, TableStrategy)] = &[
    (
        "public.supervisor_questions",
        TableStrategy::HistoricalArchive,
    ),
    (
        "public.supervisor_question_denials",
        TableStrategy::HistoricalArchive,
    ),
];

pub const AREA_RECONCILIATION_TABLES_0061: &[(&str, TableStrategy)] = &[(
    "public.area_reconciliation_receipts",
    TableStrategy::HistoricalArchive,
)];

/// Addressed expertise is private operational history, not searchable area
/// knowledge or transferable question/answer authority.
pub const ADDRESSED_EXPERTISE_TABLES_0062: &[(&str, TableStrategy)] = &[(
    "public.federation_expertise_addressed",
    TableStrategy::HistoricalArchive,
)];
pub const AGENT_EXPERTISE_TABLES_0063: &[(&str, TableStrategy)] = &[
    (
        "public.federation_agent_expertise_intents",
        TableStrategy::HistoricalArchive,
    ),
    (
        "public.federation_agent_expertise_daily",
        TableStrategy::HistoricalArchive,
    ),
];

/// Optional, observed historical evaluation snapshot. Presence of any member
/// requires the whole catalog, including empty tables. Never match a prefix or
/// apply the public schema's ownership/rebuild policy to these archived rows.
pub(crate) const HISTORICAL_TABLES: &[&str] = &[
    "mab_full_20260516_150014._sqlx_migrations",
    "mab_full_20260516_150014.curation_runs",
    "mab_full_20260516_150014.memory_chunks",
    "mab_full_20260516_150014.memory_entries",
    "mab_full_20260516_150014.memory_relations",
    "mab_full_20260516_150014.memory_sources",
    "mab_full_20260516_150014.memory_tags",
    "mab_full_20260516_150014.projects",
    "mab_full_20260516_150014.raw_captures",
    "mab_full_20260516_150014.sessions",
    "mab_full_20260516_150014.tasks",
];

pub(crate) const HISTORICAL_PROJECT_TABLE: &str = "mab_full_20260516_150014.projects";

pub(crate) fn historical_project_table(table: &str) -> Option<&'static str> {
    HISTORICAL_TABLES
        .contains(&table)
        .then_some(HISTORICAL_PROJECT_TABLE)
}

pub fn table_strategy(table: &str) -> Option<TableStrategy> {
    KNOWN_TABLES
        .iter()
        .chain(SUPERVISOR_TABLES_0053)
        .chain(SUPERVISOR_TABLES_0054)
        .chain(SUPERVISOR_TABLES_0055)
        .chain(AREA_IMPORT_TABLES_0056)
        .chain(SUPERVISOR_TABLES_0057)
        .chain(SUPERVISOR_TABLES_0059)
        .chain(SUPERVISOR_TABLES_0060)
        .chain(AREA_RECONCILIATION_TABLES_0061)
        .chain(ADDRESSED_EXPERTISE_TABLES_0062)
        .chain(AGENT_EXPERTISE_TABLES_0063)
        .find_map(|(name, strategy)| (*name == table).then_some(*strategy))
        .or_else(|| historical_project_table(table).map(|_| TableStrategy::HistoricalArchive))
}
