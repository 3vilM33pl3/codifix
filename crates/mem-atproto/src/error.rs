// SPDX-License-Identifier: AGPL-3.0-or-later

use thiserror::Error;

#[derive(Debug, Error)]
pub enum MemAtprotoError {
    #[error("AT URI syntax error: {0}")]
    AtUri(&'static str),
    #[error("confidence must be within 0.0..=1.0, got {0}")]
    ConfidenceRange(f32),
    #[error("confidence ppm must be <= 1_000_000, got {0}")]
    ConfidencePpmRange(u32),
    #[error("{0}")]
    Message(String),
    #[error(transparent)]
    AtriumApi(#[from] atrium_api::error::Error),
    #[error(transparent)]
    Chrono(#[from] chrono::ParseError),
    #[error(transparent)]
    DagCbor(#[from] serde_ipld_dagcbor::EncodeError<std::convert::Infallible>),
    #[error(transparent)]
    Federation(#[from] mem_federation::FederationError),
    #[error(transparent)]
    SerdeJson(#[from] serde_json::Error),
}

impl MemAtprotoError {
    pub fn message(message: impl Into<String>) -> Self {
        Self::Message(message.into())
    }
}
