use anyhow::{Result, ensure};
use clap::{Parser, Subcommand};
use codifix_agency_runtime::{
    config::{Auth, Config},
    server::{App, router},
    store::Store,
};
use serde_json::{Value, json};
use std::{path::PathBuf, sync::Arc};

#[derive(Parser)]
#[command(name = "codifix-agency", about = "Opt-in bounded Codifix specialists")]
struct Cli {
    #[arg(long)]
    config: Option<PathBuf>,
    #[command(subcommand)]
    command: Command,
}
#[derive(Subcommand)]
enum Command {
    Check,
    Migrate,
    Serve,
    WorkerOnce,
    Tasks {
        #[arg(long)]
        peer: String,
    },
    Submit {
        #[arg(long)]
        peer: String,
        #[arg(long)]
        file: PathBuf,
    },
    Grant {
        #[arg(long)]
        peer: String,
        #[arg(long)]
        file: PathBuf,
    },
    Control {
        #[arg(long)]
        peer: String,
        #[arg(long)]
        id: uuid::Uuid,
        #[arg(long)]
        action: String,
    },
    Schedule {
        #[arg(long)]
        peer: String,
        #[arg(long)]
        file: PathBuf,
    },
    Improve {
        #[arg(long)]
        peer: String,
        #[arg(long)]
        file: PathBuf,
    },
    Respond {
        #[arg(long)]
        peer: String,
        #[arg(long)]
        file: PathBuf,
    },
    InboxEnroll {
        #[arg(long)]
        peer: String,
        #[arg(long)]
        file: PathBuf,
    },
    InboxRevoke {
        #[arg(long)]
        peer: String,
        #[arg(long)]
        id: uuid::Uuid,
    },
    Publish {
        #[arg(long)]
        pds: String,
        #[arg(long)]
        did: String,
        #[arg(long)]
        token_env: String,
        #[arg(long)]
        confirm_public_disclosure: bool,
    },
    Discover {
        #[arg(long)]
        pds: String,
        #[arg(long)]
        did: String,
        #[arg(long)]
        record_key: String,
    },
    Compare {
        #[arg(long)]
        plan: PathBuf,
        #[arg(long)]
        outcomes: PathBuf,
    },
    Profile {
        #[arg(long)]
        manifest: PathBuf,
        #[arg(long)]
        approval: PathBuf,
    },
}
fn read<T: serde::de::DeserializeOwned>(file: &PathBuf) -> Result<T> {
    let bytes = std::fs::read(file)?;
    ensure!(bytes.len() <= 4 * 1024 * 1024, "input too large");
    Ok(serde_json::from_slice(&bytes)?)
}
#[tokio::main]
async fn main() {
    if run().await.is_err() {
        eprintln!(
            "Agency operation failed. Verify configuration, authorization, and the private task/effect ledger; no credentials or provider response bodies are printed."
        );
        std::process::exit(1);
    }
}
async fn run() -> Result<()> {
    let cli = Cli::parse();
    match &cli.command {
        Command::Compare { plan, outcomes } => {
            let result = codifix_agency::evaluation::compare_outcomes(
                &read(plan)?,
                &read::<Vec<_>>(outcomes)?,
            )?;
            println!("{}", serde_json::to_string_pretty(&result)?);
            return Ok(());
        }
        Command::Profile { manifest, approval } => {
            let result = codifix_agency::discovery::PublicCapabilityProfile::from_approved(
                &read(manifest)?,
                read(approval)?,
                chrono::Utc::now(),
            )?;
            println!("{}", serde_json::to_string_pretty(&result)?);
            return Ok(());
        }
        Command::Discover {
            pds,
            did,
            record_key,
        } => {
            println!(
                "{}",
                serde_json::to_string_pretty(
                    &codifix_agency_runtime::discovery::discover(pds, did, record_key).await?
                )?
            );
            return Ok(());
        }
        _ => {}
    }
    let cfg = Config::load(
        cli.config
            .as_deref()
            .ok_or_else(|| anyhow::anyhow!("--config required"))?,
    )?;
    match cli.command {
        Command::InboxEnroll { peer, file } => {
            ensure!(cfg.inbox_enabled, "inbox gateway disabled");
            ensure!(
                cfg.peers
                    .iter()
                    .any(|p| p.id == peer && p.operator && p.instance_id == cfg.instance_id),
                "local operator required"
            );
            let grant: codifix_inbox::InvocationGrant = read(&file)?;
            ensure!(
                grant.home_instance_id == cfg.instance_id,
                "grant home mismatch"
            );
            let pool = sqlx::PgPool::connect(&std::env::var(&cfg.database_env)?).await?;
            let store = codifix_inbox::store::Store {
                pool,
                home_instance_id: cfg.instance_id,
            };
            store.install_grant(&grant).await?;
            println!("{}", json!({"enrolled":true,"grant_id":grant.id}));
        }
        Command::InboxRevoke { peer, id } => {
            ensure!(cfg.inbox_enabled, "inbox gateway disabled");
            ensure!(
                cfg.peers
                    .iter()
                    .any(|p| p.id == peer && p.operator && p.instance_id == cfg.instance_id),
                "local operator required"
            );
            let pool = sqlx::PgPool::connect(&std::env::var(&cfg.database_env)?).await?;
            let store = codifix_inbox::store::Store {
                pool,
                home_instance_id: cfg.instance_id,
            };
            store.revoke_grant(id).await?;
            println!("{}", json!({"revoked":true,"grant_id":id}));
        }
        Command::Check => println!(
            "{}",
            json!({"valid":true,"instance_id":cfg.instance_id,"specialists":cfg.specialists.len(),"peers":cfg.peers.len()})
        ),
        Command::Publish {
            pds,
            did,
            token_env,
            confirm_public_disclosure,
        } => {
            ensure!(
                confirm_public_disclosure,
                "explicit public disclosure confirmation required"
            );
            let token = std::env::var(token_env)?;
            println!(
                "{}",
                codifix_agency_runtime::discovery::publish(&cfg, &pds, &did, &token).await?
            );
        }
        Command::Tasks { peer } => request(&cfg, &peer, "/v1/tasks", None).await?,
        Command::Submit { peer, file } => {
            request(&cfg, &peer, "/v1/tasks", Some(read(&file)?)).await?
        }
        Command::Grant { peer, file } => {
            request(&cfg, &peer, "/v1/mandates", Some(read(&file)?)).await?
        }
        Command::Control { peer, id, action } => {
            ensure!(
                ["pause", "resume", "revoke"].contains(&action.as_str()),
                "invalid control"
            );
            request(
                &cfg,
                &peer,
                &format!("/v1/mandates/{id}/{action}"),
                Some(json!({})),
            )
            .await?;
        }
        Command::Schedule { peer, file } => {
            request(&cfg, &peer, "/v1/schedules", Some(read(&file)?)).await?
        }
        Command::Improve { peer, file } => {
            request(&cfg, &peer, "/v1/improvements", Some(read(&file)?)).await?
        }
        Command::Respond { peer, file } => {
            request(&cfg, &peer, "/message:send", Some(read(&file)?)).await?
        }
        command => {
            let db = std::env::var(&cfg.database_env)
                .map_err(|_| anyhow::anyhow!("database credential unavailable"))?;
            let pool = sqlx::postgres::PgPoolOptions::new()
                .max_connections(24)
                .connect(&db)
                .await?;
            let store = Store { pool };
            match command {
                Command::Migrate => {
                    sqlx::migrate!("../../migrations").run(&store.pool).await?;
                    println!("{}", json!({"migrated":true}));
                }
                Command::WorkerOnce => {
                    println!(
                        "{}",
                        json!({"worked":codifix_agency_runtime::worker::run_once(&store,&cfg).await?})
                    );
                }
                Command::Serve => {
                    // No schema changes in serve: provisioning remains an explicit operation.
                    if cfg.observation_enabled {
                        sqlx::query("INSERT INTO agency_runtime_heartbeats(instance_id,version) VALUES($1,$2) ON CONFLICT(instance_id) DO UPDATE SET version=excluded.version,observed_at=clock_timestamp()")
                        .bind(cfg.instance_id).bind(env!("CARGO_PKG_VERSION")).execute(&store.pool).await?;
                    }
                    let auth = Auth::from_config(&cfg)?;
                    let listener = tokio::net::TcpListener::bind(cfg.listen).await?;
                    let cfg = Arc::new(cfg);
                    let app = App {
                        store: store.clone(),
                        config: cfg.clone(),
                        auth,
                    };
                    let mut workers = tokio::task::JoinSet::new();
                    for _ in 0..4 {
                        let store = store.clone();
                        let cfg = cfg.clone();
                        workers.spawn(async move {
                            loop {
                                match codifix_agency_runtime::worker::run_once(&store, &cfg).await {
                                    Ok(true) => {}
                                    _ => {
                                        tokio::time::sleep(std::time::Duration::from_secs(1)).await
                                    }
                                }
                            }
                        });
                    }
                    workers.spawn(async move {
                        loop {
                            if cfg.observation_enabled { let _ = sqlx::query("INSERT INTO agency_runtime_heartbeats(instance_id,version) VALUES($1,$2) ON CONFLICT(instance_id) DO UPDATE SET version=excluded.version,observed_at=clock_timestamp()")
                                .bind(cfg.instance_id).bind(env!("CARGO_PKG_VERSION")).execute(&store.pool).await; }
                            let _ = codifix_agency_runtime::worker::schedules(&store, &cfg).await;
                            let _ = codifix_agency_runtime::exchange::reconcile_cancellations(
                                &store, &cfg,
                            )
                            .await;
                            let _ = codifix_agency_runtime::inbox::reconcile(&store, &cfg).await;
                            tokio::time::sleep(std::time::Duration::from_secs(5)).await;
                        }
                    });
                    let result = axum::serve(listener, router(app))
                        .with_graceful_shutdown(async {
                            let _ = tokio::signal::ctrl_c().await;
                        })
                        .await;
                    workers.abort_all();
                    result?;
                }
                _ => unreachable!(),
            }
        }
    }
    Ok(())
}
async fn request(cfg: &Config, peer: &str, path: &str, body: Option<Value>) -> Result<()> {
    let peer = cfg
        .peers
        .iter()
        .find(|p| p.id == peer)
        .ok_or_else(|| anyhow::anyhow!("unknown local caller"))?;
    let token = std::env::var(&peer.token_env)?;
    let client = reqwest::Client::builder()
        .no_proxy()
        .redirect(reqwest::redirect::Policy::none())
        .timeout(std::time::Duration::from_secs(30))
        .build()?;
    let url = format!("http://{}{path}", cfg.listen);
    let call = if let Some(body) = body {
        client.post(url).json(&body)
    } else {
        client.get(url)
    };
    let response = call
        .bearer_auth(token)
        .header("A2A-Version", "1.0")
        .send()
        .await?;
    ensure!(
        response.status().is_success(),
        "request denied or conflicted"
    );
    println!(
        "{}",
        serde_json::to_string_pretty(&response.json::<Value>().await?)?
    );
    Ok(())
}
