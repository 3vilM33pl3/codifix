// SPDX-License-Identifier: AGPL-3.0-or-later
//! Opt-in private federation runtime. No discovery, arbitrary forwarding or
//! automatic enrollment. Configure independently operated nodes explicitly.
mod config;
mod context;
pub mod enrollment;
mod expertise_answer;
mod expertise_delivery;
mod expertise_transport;
pub use expertise_answer::*;
mod gateway;
pub use config::*;
pub use context::*;
pub use expertise_transport::*;
mod http;
mod pds;
mod private_files;
mod security;
mod sync;
pub use gateway::*;
pub use http::Endpoint;
pub use pds::*;
pub use security::*;
pub use sync::*;

#[derive(Debug, thiserror::Error)]
pub enum RuntimeError {
    #[error("invalid private federation configuration")]
    Configuration,
    #[error("transport unavailable")]
    Unavailable,
    #[error("transport authorization denied")]
    Unauthorized,
    #[error("invalid transport response or immutable binding")]
    InvalidResponse,
    #[error(transparent)]
    Store(#[from] mem_federation_store::StoreError),
}
pub type Result<T> = std::result::Result<T, RuntimeError>;
