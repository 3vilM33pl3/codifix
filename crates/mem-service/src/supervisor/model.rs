//! No bootstrap, autonomous scheduling, retry or credential fallback. A private
//! hash-bound operator manifest fixes one action and one physical generation.
use crate::AppState;
use mem_record::{
    SupervisorActionState as ActionState, SupervisorArea, SupervisorModelWorkState as Phase,
    SupervisorModelWorkStatus,
};
use mem_supervisor_runtime::{
    launcher::{self, RunControl},
    pi_broker::GuardedBrokerJob,
    transport::OpenAiTransport,
    work::{PinnedWork, WorkError, WorkStep},
};
use std::{
    sync::{Arc, Mutex},
    time::Duration,
};

#[derive(Clone, Default)]
pub(crate) struct Status(Arc<Mutex<Option<SupervisorModelWorkStatus>>>);
impl Status {
    pub(crate) fn snapshot(&self) -> Option<SupervisorModelWorkStatus> {
        self.0
            .lock()
            .expect("supervisor status lock poisoned")
            .clone()
    }
    fn set(&self, state: Phase, work: Option<&PinnedWork>, action_state: Option<ActionState>) {
        *self.0.lock().expect("supervisor status lock poisoned") =
            Some(SupervisorModelWorkStatus {
                state,
                observed_at: chrono::Utc::now(),
                action_id: work.map(PinnedWork::action_id),
                attempt_id: work.map(PinnedWork::attempt_id),
                action_state,
            });
    }
}
pub(crate) struct Worker {
    control: RunControl,
    _task: tokio::task::JoinHandle<()>,
}
impl Worker {
    pub(crate) fn control(&self) -> RunControl {
        self.control.clone()
    }
    pub(crate) fn stop(&self) {
        self.control.stop();
    }
}

#[cfg(test)]
mod tests;
impl Drop for Worker {
    fn drop(&mut self) {
        // Do not abort a dispatched provider's accounting or the launcher's
        // cleanup owner. Process death is instead handled by the durable lease.
        self.stop();
    }
}
pub(crate) fn start(state: AppState) -> Option<Worker> {
    let identity = state.config.supervisor.clone()?;
    let config = identity.model_work?;
    if state.config.evaluation.active()
        || state.config.service.read_only
        || !state.is_primary()
        || identity.area != SupervisorArea::Workshop
    {
        state.supervisor_model.set(Phase::Stopped, None, None);
        return None;
    }
    let control = RunControl::default();
    let owned = control.clone();
    let task = tokio::spawn(async move {
        state.supervisor_model.set(Phase::Starting, None, None);
        let loaded = tokio::task::spawn_blocking(move || {
            PinnedWork::load(
                &config.manifest,
                &config.sha256,
                identity.instance_id,
                identity.area,
            )
        })
        .await;
        let Ok(Ok(work)) = loaded else {
            state
                .supervisor_model
                .set(Phase::InvalidConfiguration, None, None);
            return;
        };
        if owned.stopped() {
            state
                .supervisor_model
                .set(Phase::Stopped, Some(&work), None);
            return;
        }
        // Validate the explicitly inherited normal credential before claiming;
        // absence must not turn otherwise eligible work into a running lease.
        let Ok(transport) = OpenAiTransport::from_environment() else {
            state
                .supervisor_model
                .set(Phase::CredentialUnavailable, Some(&work), None);
            return;
        };
        let mut transport = Some(transport);
        let mut launch_failure = None;
        let mut poll = tokio::time::interval(Duration::from_secs(2));
        poll.set_missed_tick_behavior(tokio::time::MissedTickBehavior::Skip);
        loop {
            tokio::select! { _=owned.cancelled()=>break, _=poll.tick()=>{} }
            let Ok(pool) = state.pool() else {
                state
                    .supervisor_model
                    .set(Phase::WaitingForDatabase, Some(&work), None);
                continue;
            };
            // Reads/transitions can wait on DB locks. Timeout cannot authorize a
            // retry of an ambiguously committed claim: next poll reads Running.
            let step = tokio::select! {
                _=owned.cancelled()=>break,
                result=tokio::time::timeout(Duration::from_secs(15),work.step(pool,owned.clone()))=>result,
            };
            let step = match step {
                Ok(Ok(step)) => step,
                Ok(Err(WorkError::Stopped)) => break,
                Ok(Err(WorkError::Authority)) => {
                    state
                        .supervisor_model
                        .set(Phase::AwaitingAuthority, Some(&work), None);
                    continue;
                }
                _ => {
                    state
                        .supervisor_model
                        .set(Phase::InterventionRequired, Some(&work), None);
                    continue;
                }
            };
            match step {
                WorkStep::State(action) => {
                    let phase = match action {
                        ActionState::Observed
                        | ActionState::Proposed
                        | ActionState::Authorized
                        | ActionState::Queued => Phase::Ready,
                        ActionState::Running => launch_failure.unwrap_or(Phase::Running),
                        ActionState::Verified
                            if launch_failure == Some(Phase::CleanupUnconfirmed) =>
                        {
                            Phase::CleanupUnconfirmed
                        }
                        ActionState::Verified => Phase::Stored,
                        _ => launch_failure.unwrap_or(Phase::InterventionRequired),
                    };
                    state.supervisor_model.set(phase, Some(&work), Some(action));
                    if !matches!(
                        action,
                        ActionState::Observed
                            | ActionState::Proposed
                            | ActionState::Authorized
                            | ActionState::Queued
                            | ActionState::Running
                    ) {
                        return;
                    }
                }
                WorkStep::Claimed(claimed) => {
                    state.supervisor_model.set(
                        Phase::Running,
                        Some(&work),
                        Some(ActionState::Running),
                    );
                    // Only the host transport sees the configured normal key.
                    // No dotenv discovery, R3 credential, model or URL fallback.
                    let Some(transport) = transport.take() else {
                        state.supervisor_model.set(
                            Phase::InterventionRequired,
                            Some(&work),
                            Some(ActionState::Running),
                        );
                        // Defense in depth: never issue a second physical job.
                        return;
                    };
                    let job = Arc::new(GuardedBrokerJob {
                        store: claimed.store.clone(),
                        lease: claimed.lease.clone(),
                        execution: claimed.execution.clone(),
                        request: Arc::clone(&claimed.request),
                        transport,
                        disclosure: claimed.disclosure.clone(),
                    });
                    let result = launcher::launch(
                        claimed.sandbox.clone(),
                        Arc::clone(&claimed.request),
                        job,
                        owned.clone(),
                    )
                    .await;
                    let confirmed = match result {
                        Ok(hash) => claimed.confirm(hash).await,
                        Err(error) => Err(error),
                    };
                    if confirmed.is_ok() {
                        state.supervisor_model.set(
                            Phase::Completed,
                            Some(&work),
                            Some(ActionState::Verified),
                        );
                        return;
                    }
                    // No second launch, even if count failed before generation.
                    // Continue observing/reaping; a dispatched broker task may
                    // still settle or have committed a proposal before disconnect.
                    let phase = match confirmed {
                        Err(launcher::LaunchError::Files | launcher::LaunchError::Artifact) => {
                            Phase::ArtifactRejected
                        }
                        Err(launcher::LaunchError::Unavailable) => Phase::SandboxUnavailable,
                        Err(launcher::LaunchError::Cleanup) => Phase::CleanupUnconfirmed,
                        Err(launcher::LaunchError::Deadline) => Phase::ExecutionDeadline,
                        Err(launcher::LaunchError::Stopped) => Phase::Stopped,
                        _ => Phase::DeliveryUnconfirmed,
                    };
                    launch_failure = Some(phase);
                    state.supervisor_model.set(phase, Some(&work), None);
                }
            }
        }
        state
            .supervisor_model
            .set(Phase::Stopped, Some(&work), None);
    });
    Some(Worker {
        control,
        _task: task,
    })
}
