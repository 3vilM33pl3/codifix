// SPDX-License-Identifier: AGPL-3.0-or-later

#![cfg_attr(test, allow(clippy::unwrap_used))]

mod adapter;
pub mod addressed_expertise;
mod aturi;
mod error;
mod fixtures;
mod lexicon;
mod mst;
pub mod publication;
mod record;
mod syntax;
mod xrpc;

#[cfg(test)]
mod sdk_coverage;

pub use adapter::*;
pub use aturi::*;
pub use error::*;
pub use fixtures::*;
pub use lexicon::*;
pub use mem_federation as federation;
pub use mst::*;
pub use record::*;
pub use xrpc::*;
