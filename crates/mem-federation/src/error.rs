// SPDX-License-Identifier: AGPL-3.0-or-later

use thiserror::Error;

#[derive(Debug, Error)]
pub enum FederationError {
    #[error("mem-federation scaffold placeholder")]
    Placeholder,
}
