//! Opt-in specialist service. Policy admission and effects are durable; discovery grants no authority.
pub mod config;
pub mod discovery;
pub mod exchange;
pub mod inspection;
pub mod learning;
pub mod model;
pub mod openbao;
pub mod operator;
pub mod server;
pub mod store;
pub mod worker;
