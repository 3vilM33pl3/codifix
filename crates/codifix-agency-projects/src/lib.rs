//! Versioned project plans and explicit, reconciled Linear effects.
mod linear;
mod plan;
mod roadmap;
pub use linear::*;
pub use plan::*;
pub use roadmap::*;

#[derive(Debug, thiserror::Error)]
pub enum Error {
    #[error("invalid plan: {0}")]
    Validation(String),
    #[error("authority denied: {0}")]
    Authority(String),
    #[error("prepared changes no longer match the plan, baseline or authority")]
    StalePreparation,
    #[error("remote object conflict: {0}")]
    Conflict(String),
    #[error("journal: {0}")]
    Journal(String),
    #[error("Linear request failed or has uncertain outcome: {0}")]
    Provider(String),
}
pub type Result<T> = std::result::Result<T, Error>;
