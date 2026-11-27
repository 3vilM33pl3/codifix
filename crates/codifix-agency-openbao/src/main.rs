#[cfg(unix)]
mod app {
    use clap::{Parser, Subcommand};
    use codifix_agency_openbao::{
        service::{self, AttestorConfig, Listener},
        *,
    };
    use std::path::PathBuf;

    #[derive(Parser)]
    #[command(about = "Independent deterministic OpenBao unseal ceremony pilot")]
    struct Args {
        #[command(subcommand)]
        command: Command,
    }
    #[derive(Subcommand)]
    enum Command {
        /// Read enrolled status; never sends a share.
        Status {
            config: PathBuf,
        },
        /// One standing-quorum attempt; emits only safe JSON observations.
        ControllerOnce {
            config: PathBuf,
        },
        /// Poll sealed status; stops immediately on any fail-closed escalation.
        ControllerWatch {
            config: PathBuf,
            #[arg(long, default_value_t = 15)]
            interval_seconds: u64,
        },
        /// Enroll new empty controller state; never overwrites existing state.
        InitController {
            config: PathBuf,
        },
        InitCustodian {
            config: PathBuf,
        },
        CustodianServe {
            config: PathBuf,
            listener: PathBuf,
        },
        AttestorServe {
            config: PathBuf,
            listener: PathBuf,
        },
        /// Secret-free current process evidence for local diagnostics.
        InspectProcess {
            pid: u32,
        },
        /// Inspect stable native enrollment; outputs only paths and fingerprints.
        InspectNative {
            pid: u32,
            config_file: PathBuf,
            storage_directory: PathBuf,
            installation_file: PathBuf,
        },
    }
    fn output<T: serde::Serialize>(value: &T) {
        if let Ok(s) = serde_json::to_string(value) {
            println!("{s}");
        }
    }
    #[tokio::main]
    pub async fn entry() -> std::process::ExitCode {
        match run(Args::parse()).await {
            Ok(()) => std::process::ExitCode::SUCCESS,
            Err(e) => {
                eprintln!("{e}");
                std::process::ExitCode::FAILURE
            }
        }
    }
    async fn run(args: Args) -> Result<()> {
        match args.command {
            Command::Status { config } => {
                let c: ControllerConfig = protected::load(&config)?;
                output(&transport::verified_status(&c.target).await?);
            }
            Command::ControllerOnce { config } => {
                let c = protected::load(&config)?;
                transport::controller_tick(&c, |o| output(&o)).await?;
            }
            Command::ControllerWatch {
                config,
                interval_seconds,
            } => {
                if !(1..=3600).contains(&interval_seconds) {
                    return Err(Error::Configuration);
                }
                let c = protected::load(&config)?;
                loop {
                    transport::controller_tick(&c, |o| output(&o)).await?;
                    tokio::time::sleep(std::time::Duration::from_secs(interval_seconds)).await;
                }
            }
            Command::InitController { config } => {
                let c: ControllerConfig = protected::load(&config)?;
                c.target.validate(now())?;
                validate_roster(&c.roster, &c.target)?;
                protected::initialize(
                    &c.state_file,
                    &ControllerState {
                        binding: c.target.binding()?,
                        ..Default::default()
                    },
                )?;
            }
            Command::InitCustodian { config } => {
                let c: CustodianConfig = protected::load(&config)?;
                c.validate(now())?;
                protected::initialize(
                    &c.state_file,
                    &CustodianState {
                        binding: c.target.binding()?,
                        ..Default::default()
                    },
                )?;
            }
            Command::CustodianServe { config, listener } => {
                let c: CustodianConfig = protected::load(&config)?;
                let l: Listener = protected::load(&listener)?;
                if c.target.endpoint.fixture_localhost_http != l.fixture_localhost_http {
                    return Err(Error::Configuration);
                }
                service::serve(service::custodian_router(c)?, l).await?;
            }
            Command::AttestorServe { config, listener } => {
                let c: AttestorConfig = protected::load(&config)?;
                let l: Listener = protected::load(&listener)?;
                service::serve(service::attestor_router(c)?, l).await?;
            }
            Command::InspectProcess { pid } => output(&service::ProcessBinding::inspect(pid)?),
            Command::InspectNative {
                pid,
                config_file,
                storage_directory,
                installation_file,
            } => {
                let native = service::NativeBinding::inspect(
                    pid,
                    config_file,
                    storage_directory,
                    installation_file,
                )?;
                output(&serde_json::json!({"native_binding":native.binding()?,"native":native}));
            }
        }
        Ok(())
    }
}

#[cfg(unix)]
fn main() -> std::process::ExitCode {
    app::entry()
}

#[cfg(not(unix))]
fn main() -> std::process::ExitCode {
    eprintln!("OpenBao ceremony pilot requires Unix protected-file support and a Linux attestor");
    std::process::ExitCode::FAILURE
}
