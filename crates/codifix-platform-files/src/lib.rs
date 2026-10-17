// SPDX-License-Identifier: AGPL-3.0-or-later
//! Narrow protected filesystem boundary shared by operators and runtimes.
//! No service management, process discovery, configuration, SQL or networking.
#![cfg_attr(test, allow(clippy::unwrap_used))]

#[cfg(windows)]
pub mod windows;
