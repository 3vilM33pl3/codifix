//! Narrow supervisor adapters. Model suggestions never confer action authority.
//! The wire contract is usable offline; deployment, credentials and disclosure
//! authorization remain the responsibility of the trusted runtime.

pub mod launcher;
pub mod local_disclosure;
pub mod model;
pub mod operator;
pub mod pi_admission;
pub mod pi_broker;
mod private_files;
pub mod question_queue;
mod strict_json;
pub mod transport;
pub mod work;
