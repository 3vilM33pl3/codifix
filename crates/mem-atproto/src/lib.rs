// SPDX-License-Identifier: AGPL-3.0-or-later

#![cfg_attr(test, allow(clippy::unwrap_used))]

mod aturi;
mod error;
mod lexicon;
mod record;

pub use aturi::*;
pub use error::*;
pub use lexicon::*;
pub use record::*;

pub use mem_federation as federation;
