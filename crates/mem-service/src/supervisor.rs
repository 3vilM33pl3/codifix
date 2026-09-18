// SPDX-License-Identifier: AGPL-3.0-or-later
//! Separately opt-in deterministic observations and bounded single-work models.
use crate::AppState;
use mem_supervisor_store::SupervisorStore;
use std::time::Duration;

pub(crate) mod model;

#[cfg(test)]
mod tests;

pub(crate) struct Worker(tokio::task::JoinHandle<()>);
impl Drop for Worker {
    fn drop(&mut self) {
        self.0.abort();
    }
}

pub(crate) fn start(state: AppState) -> Option<Worker> {
    if state.config.evaluation.active() || state.config.service.read_only || !state.is_primary() {
        return None;
    }
    let identity = state
        .config
        .supervisor
        .clone()
        .filter(|identity| identity.deterministic_checks)?;
    Some(Worker(tokio::spawn(async move {
        let mut poll = tokio::time::interval(Duration::from_secs(10));
        poll.set_missed_tick_behavior(tokio::time::MissedTickBehavior::Skip);
        loop {
            poll.tick().await;
            let Ok(pool) = state.pool() else {
                continue;
            };
            // Persisted due time is authoritative across processes/restarts.
            // A disabled/unbound/revoked identity cannot bootstrap itself here.
            if SupervisorStore::new(pool)
                .observe_health(identity.instance_id, identity.area)
                .await
                .is_err()
            {
                // Operational errors never expose DB diagnostics or contents.
                tracing::debug!(
                    "deterministic supervisor observation not confirmed; durable history remains authoritative"
                );
            }
        }
    })))
}
