//! Opt-in persistent adapter for the existing one-question primitive. No new
//! work, provider retries, grants or policies are created by this loop.
use super::*;
use codifix_supervisor_runtime::question_queue::QueueReport;
use std::future::Future;

const POLL_INTERVAL: Duration = Duration::from_secs(2);

fn may_continue(report: &QueueReport) -> bool {
    match report.outcome {
        QueueOutcome::Idle | QueueOutcome::Busy => report.work.is_none(),
        QueueOutcome::Processed => report.work.as_ref().is_some_and(|work| {
            matches!(work.outcome, Outcome::Completed | Outcome::Stored)
                && work.action_state == Some(codifix_supervisor::action::ActionState::Verified)
        }),
        _ => false,
    }
}

/// Cancellation is passed to, not raced against/dropped around, one_work.
/// Existing execution owns its uncertainty recording and lease cleanup.
async fn poll<F, Fut, E>(
    control: RunControl,
    interval: Duration,
    mut one_work: F,
    mut emit: E,
) -> Result<bool, &'static str>
where
    F: FnMut() -> Fut,
    Fut: Future<Output = Result<QueueReport, &'static str>>,
    E: FnMut(&QueueReport) -> Result<(), &'static str>,
{
    loop {
        if control.stopped() {
            return Ok(true);
        }
        let report = one_work().await?;
        emit(&report)?;
        if control.stopped() || report.outcome == QueueOutcome::Stopped {
            return Ok(true);
        }
        if !may_continue(&report) {
            return Ok(false);
        }
        tokio::select! { _ = control.cancelled() => return Ok(true), _ = tokio::time::sleep(interval) => {} }
    }
}

fn load(
    args: &[String],
    instance: Uuid,
) -> Result<(PinnedWork, Arc<ServiceExecutionGuard>), &'static str> {
    let work = PinnedWork::load(Path::new(&args[1]), &args[2], instance, Area::Workshop)
        .map_err(|_| "Pinned work manifest unavailable or changed; serving stopped.")?;
    let guard = Arc::new(
        ServiceExecutionGuard::load(Path::new(&args[4]), &args[5], &work)
            .map_err(|_| "Pinned service configuration unavailable or changed; serving stopped.")?,
    );
    Ok((work, guard))
}

pub(super) async fn execute(args: &[String]) -> Result<bool, &'static str> {
    if args.len() != 6 {
        return Err("Invalid serve-next arguments; use --help. No work started.");
    }
    #[cfg(unix)]
    if rustix::process::geteuid().as_raw() == 0 {
        return Err("Run as the explicitly authorized non-root operator, never as root.");
    }
    let instance = Uuid::parse_str(&args[3]).map_err(|_| "Invalid instance identity.")?;
    let (initial, _) = load(args, instance)?;
    let project = initial.grant().project_id;
    let options = local_database(
        &std::env::var("DATABASE_URL").map_err(|_| "Explicit Workshop database is required.")?,
    )?;
    let pool = PgPoolOptions::new()
        .max_connections(4)
        .acquire_timeout(Duration::from_secs(10))
        .connect_with(options)
        .await
        .map_err(|_| "Workshop database unavailable; no work started.")?;
    let control = RunControl::default();
    let cancelled = control.clone();
    let signals = tokio::spawn(async move {
        #[cfg(unix)]
        {
            use tokio::signal::unix::{SignalKind, signal};
            if let (Ok(mut terminate), Ok(mut interrupt)) = (
                signal(SignalKind::terminate()),
                signal(SignalKind::interrupt()),
            ) {
                tokio::select! { _ = terminate.recv() => {}, _ = interrupt.recv() => {} }
            }
        }
        #[cfg(windows)]
        {
            use tokio::signal::windows::{ctrl_break, ctrl_c, ctrl_close, ctrl_shutdown};
            if let (Ok(mut c), Ok(mut b), Ok(mut close), Ok(mut shutdown)) =
                (ctrl_c(), ctrl_break(), ctrl_close(), ctrl_shutdown())
            {
                tokio::select! { _ = c.recv() => {}, _ = b.recv() => {}, _ = close.recv() => {}, _ = shutdown.recv() => {} }
            }
        }
        cancelled.stop();
    });
    let result =
        poll(
            control.clone(),
            POLL_INTERVAL,
            || {
                let pool = pool.clone();
                let control = control.clone();
                async move {
                    // Both exact private inputs are reopened before EVERY queue poll.
                    // run_next also rechecks current source/disclosure authority.
                    let (work, guard) = load(args, instance)?;
                    if work.grant().project_id != project {
                        return Err("Project binding changed; serving stopped.");
                    }
                    Ok(question_queue::run_next(
                        &work,
                        pool,
                        OpenAiTransport::from_environment().ok(),
                        guard,
                        control,
                    )
                    .await)
                }
            },
            |report| {
                println!("{}", serde_json::to_string(&serde_json::json!({
            "schema_version": 1, "event": "supervisor_queue_poll", "instance_id": instance,
            "project_id": project, "manifest_sha256": args[2], "config_sha256": args[5],
            "observed_at": chrono::Utc::now(), "report": report,
            "readiness": "pinned_poll_only_not_provider_or_sandbox_health"
        })).map_err(|_| "Metadata serialization unavailable.")?);
                Ok(())
            },
        )
        .await;
    signals.abort();
    pool.close().await;
    result
}

#[cfg(test)]
#[allow(clippy::unwrap_used)]
mod tests {
    use super::*;
    use std::sync::atomic::{AtomicUsize, Ordering};
    fn report(outcome: QueueOutcome) -> QueueReport {
        QueueReport {
            outcome,
            work: None,
        }
    }
    #[tokio::test]
    async fn polls_only_after_success_and_stops_on_uncertainty_without_retry() {
        let calls = AtomicUsize::new(0);
        let emitted = AtomicUsize::new(0);
        let success = poll(
            RunControl::default(),
            Duration::ZERO,
            || async {
                let call = calls.fetch_add(1, Ordering::SeqCst);
                Ok(if call == 0 {
                    report(QueueOutcome::Idle)
                } else {
                    QueueReport {
                        outcome: QueueOutcome::Processed,
                        work: Some(codifix_supervisor_runtime::operator::Report {
                            action_id: Uuid::nil(),
                            attempt_id: Uuid::nil(),
                            outcome: Outcome::DeliveryUnconfirmed,
                            action_state: None,
                        }),
                    }
                })
            },
            |_| {
                emitted.fetch_add(1, Ordering::SeqCst);
                Ok(())
            },
        )
        .await
        .unwrap();
        assert!(!success);
        assert_eq!(calls.load(Ordering::SeqCst), 2);
        assert_eq!(emitted.load(Ordering::SeqCst), 2);
    }
    #[tokio::test]
    async fn cancellation_stops_sleep_without_dropping_inflight_work() {
        let control = RunControl::default();
        let calls = AtomicUsize::new(0);
        let result = poll(
            control.clone(),
            Duration::from_secs(100),
            || async {
                calls.fetch_add(1, Ordering::SeqCst);
                control.stop();
                Ok(report(QueueOutcome::Stopped))
            },
            |_| Ok(()),
        )
        .await
        .unwrap();
        assert!(result);
        assert_eq!(calls.load(Ordering::SeqCst), 1);
        assert!(
            poll(
                control,
                Duration::ZERO,
                || async { panic!("cancelled loop must not poll") },
                |_| Ok(())
            )
            .await
            .unwrap()
        );
    }
    #[tokio::test]
    async fn changed_pins_stop_before_another_queue_execution() {
        let validations = AtomicUsize::new(0);
        let result = poll(
            RunControl::default(),
            Duration::ZERO,
            || async {
                if validations.fetch_add(1, Ordering::SeqCst) == 0 {
                    Ok(report(QueueOutcome::Busy))
                } else {
                    Err("synthetic pinned config changed")
                }
            },
            |_| Ok(()),
        )
        .await;
        assert!(result.is_err());
        assert_eq!(validations.load(Ordering::SeqCst), 2);
    }
    #[tokio::test]
    async fn cancellation_allows_inflight_work_to_record_its_outcome() {
        let control = RunControl::default();
        let entered = tokio::sync::Notify::new();
        let finished = tokio::sync::Notify::new();
        let completed = AtomicUsize::new(0);
        let serving = poll(
            control.clone(),
            Duration::ZERO,
            || async {
                entered.notify_one();
                finished.notified().await;
                completed.fetch_add(1, Ordering::SeqCst);
                Ok(report(QueueOutcome::Stopped))
            },
            |_| Ok(()),
        );
        let cancelling = async {
            entered.notified().await;
            control.stop();
            tokio::task::yield_now().await;
            finished.notify_one();
        };
        let (result, ()) = tokio::join!(serving, cancelling);
        assert!(result.unwrap());
        assert_eq!(completed.load(Ordering::SeqCst), 1);
    }
    #[test]
    fn blocked_and_unconfirmed_outcomes_never_continue() {
        for outcome in [
            QueueOutcome::Blocked,
            QueueOutcome::CredentialUnavailable,
            QueueOutcome::StorageUnavailable,
            QueueOutcome::Stopped,
            QueueOutcome::Processed,
        ] {
            assert!(!may_continue(&report(outcome)));
        }
        for outcome in [
            Outcome::ExistingWork,
            Outcome::AuthorityUnavailable,
            Outcome::InterventionRequired,
            Outcome::CleanupUnconfirmed,
            Outcome::DeliveryUnconfirmed,
            Outcome::Stopped,
        ] {
            let report = QueueReport {
                outcome: QueueOutcome::Processed,
                work: Some(codifix_supervisor_runtime::operator::Report {
                    action_id: Uuid::nil(),
                    attempt_id: Uuid::nil(),
                    outcome,
                    action_state: None,
                }),
            };
            assert!(!may_continue(&report));
        }
        for outcome in [Outcome::Completed, Outcome::Stored] {
            let mut report = QueueReport {
                outcome: QueueOutcome::Processed,
                work: Some(codifix_supervisor_runtime::operator::Report {
                    action_id: Uuid::nil(),
                    attempt_id: Uuid::nil(),
                    outcome,
                    action_state: None,
                }),
            };
            assert!(!may_continue(&report));
            report.work.as_mut().unwrap().action_state =
                Some(codifix_supervisor::action::ActionState::Verified);
            assert!(may_continue(&report));
        }
    }
}
