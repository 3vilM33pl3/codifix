// SPDX-License-Identifier: AGPL-3.0-or-later

//! Transactional persistence primitives. Callers authorize operations and own
//! transactions; this crate neither executes providers nor broadcasts events.
#![cfg_attr(test, allow(clippy::unwrap_used))]

pub mod effects;
pub mod events;
pub mod executions;
pub mod jobs;
pub mod post_curation;

#[derive(Debug, thiserror::Error)]
pub enum Error {
    #[error(transparent)]
    Database(#[from] sqlx::Error),
    #[error(transparent)]
    Json(#[from] serde_json::Error),
    #[error("invalid persistence input: {0}")]
    Invalid(&'static str),
    #[error("stable operation identifier was reused with different input")]
    Conflict,
    #[error("expired or superseded worker lease")]
    StaleLease,
}

pub type Result<T> = std::result::Result<T, Error>;
