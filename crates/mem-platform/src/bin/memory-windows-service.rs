// SPDX-License-Identifier: AGPL-3.0-or-later
//! SCM adapter for a single foreground executable. This does not install a
//! service or confer filesystem/network authority. The operator owns the SCM
//! configuration, protected immutable executable tree and service account.

#[cfg(windows)]
#[path = "../windows_service.rs"]
mod windows_service;

fn main() -> anyhow::Result<()> {
    #[cfg(windows)]
    {
        windows_service::run()
    }
    #[cfg(not(windows))]
    anyhow::bail!("memory-windows-service requires native Windows")
}
