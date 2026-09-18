//! Private operator command, deliberately not an HTTP or Memory tool endpoint.
use mem_supervisor::Area;
use mem_supervisor_runtime::{
    launcher::RunControl,
    operator::{self, Outcome},
    transport::OpenAiTransport,
    work::PinnedWork,
};
use sqlx::postgres::{PgConnectOptions, PgPoolOptions};
use std::{path::Path, str::FromStr, time::Duration};
use uuid::Uuid;

#[tokio::main]
async fn main() -> std::process::ExitCode {
    match execute().await {
        Ok(success) => {
            if success {
                std::process::ExitCode::SUCCESS
            } else {
                std::process::ExitCode::FAILURE
            }
        }
        Err(message) => {
            eprintln!("{message}");
            std::process::ExitCode::FAILURE
        }
    }
}

async fn execute() -> Result<bool, &'static str> {
    let args: Vec<String> = std::env::args().skip(1).collect();
    if args.len() == 1 && matches!(args[0].as_str(), "--help" | "-h") {
        println!(
            "memory-workshop-worker <observe|run> MANIFEST SHA256 INSTANCE_UUID\n\nOperator-only one-work command. DATABASE_URL must address an explicit local\nWorkshop database. run may make ONE approved paid generation with the normal\nOPENAI_API_KEY. observe only journals/reconciles existing metadata. Neither\ncommand provisions, migrates, enables policy, prints private content or retries\na previous generation. Never supply R3 credentials. No HTTP listener."
        );
        return Ok(true);
    }
    if args.len() != 4 || !matches!(args[0].as_str(), "observe" | "run") {
        return Err("Invalid arguments; use --help. No work started.");
    }
    if rustix::process::geteuid().as_raw() == 0 {
        return Err("Run as the explicitly authorized non-root operator, never as root.");
    }
    let instance = Uuid::parse_str(&args[3]).map_err(|_| "Invalid instance identity.")?;
    let work = PinnedWork::load(Path::new(&args[1]), &args[2], instance, Area::Workshop)
        .map_err(|_| "Private work manifest is invalid or mismatched.")?;
    let database =
        std::env::var("DATABASE_URL").map_err(|_| "Explicit Workshop database is required.")?;
    let options = local_database(&database)?;
    let pool = PgPoolOptions::new()
        .max_connections(4)
        .acquire_timeout(Duration::from_secs(10))
        .connect_with(options)
        .await
        .map_err(|_| "Workshop database unavailable; no work started.")?;
    let control = RunControl::default();
    let cancelled = control.clone();
    let signals = tokio::spawn(async move {
        use tokio::signal::unix::{SignalKind, signal};
        let Ok(mut terminate) = signal(SignalKind::terminate()) else {
            cancelled.stop();
            return;
        };
        let Ok(mut interrupt) = signal(SignalKind::interrupt()) else {
            cancelled.stop();
            return;
        };
        tokio::select! { _=terminate.recv()=>{}, _=interrupt.recv()=>{} }
        cancelled.stop();
    });
    let report = if args[0] == "observe" {
        operator::observe(&work, pool.clone()).await
    } else {
        operator::run(
            &work,
            pool.clone(),
            OpenAiTransport::from_environment().ok(),
            control,
        )
        .await
    };
    signals.abort();
    pool.close().await;
    println!(
        "{}",
        serde_json::to_string(&report).map_err(|_| "Metadata serialization unavailable.")?
    );
    Ok(matches!(
        report.outcome,
        Outcome::Completed | Outcome::Stored | Outcome::ExistingWork
    ))
}

fn local_database(value: &str) -> Result<PgConnectOptions, &'static str> {
    let options =
        PgConnectOptions::from_str(value).map_err(|_| "Invalid database configuration.")?;
    if !value.starts_with("postgresql://")
        || !matches!(options.get_host(), "127.0.0.1" | "::1")
        || options
            .get_database()
            .is_none_or(|name| !name.starts_with("memory_workshop_") || name.len() > 63)
    {
        return Err("Worker requires an explicit loopback memory_workshop_ database.");
    }
    Ok(options)
}

#[cfg(test)]
mod tests {
    use super::*;
    #[test]
    fn database_is_explicit_local_and_workshop_scoped() {
        assert!(local_database("postgresql://user@127.0.0.1:6543/memory_workshop_live").is_ok());
        for value in [
            "postgresql://user@example.com/memory_workshop_live",
            "postgresql://user@localhost/memory_workshop_live",
            "postgresql://user@127.0.0.1/memory",
            "postgresql://user@127.0.0.1/memory_workshop_live?host=elsewhere",
            "postgresql://user@127.0.0.1/",
        ] {
            assert!(local_database(value).is_err(), "{value}");
        }
    }
}
