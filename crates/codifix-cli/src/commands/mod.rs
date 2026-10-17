// SPDX-License-Identifier: AGPL-3.0-or-later

mod activities;
mod agent;
pub(crate) mod api;
mod archive;
mod area;
mod auth;
mod auth_enrollment;
mod bundle;
mod capture;
mod capture_quality;
mod checkpoint;
mod commits;
mod completion;
mod consolidate;
mod curate;
mod demo;
mod dev;
mod doctor;
mod embeddings;
mod eval;
mod eval_campaign;
pub(crate) mod eval_support;
mod federation;
mod graph;
mod health;
mod history;
mod ingest;
mod init;
pub(crate) mod init_support;
pub(crate) mod jobs;
mod laptop_corrections;
mod loops;
mod mcp;
pub(crate) mod memory_ops;
pub(crate) mod output;
mod proposals;
mod prune_history;
mod query;
mod reinforcement;
mod remember;
mod repo;
mod resume;
pub(crate) mod runtime;
#[cfg(windows)]
mod runtime_env_windows;
mod scan;
mod service;
pub(crate) mod service_support;
pub(crate) mod skill_support;
mod status;
pub(crate) mod status_support;
mod structure;
pub(crate) mod supervisor;
mod tour;
#[cfg(feature = "tui")]
mod tui;
mod up_to_speed;
mod upgrade;
mod urls;
mod verify_provenance;
pub(crate) mod watch_support;
mod watcher;
#[cfg(feature = "tui")]
mod wizard;

pub(crate) async fn run() -> anyhow::Result<()> {
    runtime::run().await
}
