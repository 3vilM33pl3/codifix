//! Opt-in specialist service. Policy admission and effects are durable; discovery grants no authority.
pub mod config;
pub mod discovery;
pub mod exchange;
pub mod inbox;
pub mod inspection;
pub mod learning;
pub mod model;
pub mod notice;
pub mod openbao;
mod operation;
pub mod operator;
pub mod server;
pub mod social;
mod social_transport;
pub mod ssh_renewal;
pub mod store;
pub mod upgrade;
pub mod worker;

mod linear_tracking;

mod incidents;
