// SPDX-License-Identifier: AGPL-3.0-or-later
//! All clients use the local service; no remote credentials or policy decisions.
use super::{api::get_json, output::service_url};
use anyhow::{Context, Result, ensure};
use clap::{Args, Subcommand, ValueEnum};
use mem_config::AppConfig;
use reqwest::{Client, Method};
use serde_json::{Value, json};
use std::{
    io::Read,
    path::{Path, PathBuf},
    time::Duration,
};
use uuid::Uuid;
mod expertise;
mod health;
#[cfg(test)]
mod session_tests;
#[cfg(feature = "tui")]
mod terminal;
const HELP: &str = "Agent notes:\nUse the configured local service and explicit project. Mutations require review and confirmation. Preserve request IDs and exact bytes on retry. Queued is not accepted; retained copies cannot be recalled. See docs/user/cli/federation.md.";

#[derive(Debug, Args)]
pub(super) struct FederationArgs {
    #[arg(long)]
    project: String,
    /// Omit to discover the administrator-configured space.
    #[arg(long)]
    space: Option<Uuid>,
    /// Use a private, in-memory loopback owner session. For a local human only,
    /// never an assistant/model destination. Default token behavior is unchanged.
    #[arg(long)]
    local_human_session: bool,
    #[command(subcommand)]
    command: FederationCommand,
}
#[derive(Debug, Clone, Copy, ValueEnum)]
enum Capability {
    Pull,
    Publish,
    ManageMembership,
    AskExpertise,
    AnswerExpertise,
}
impl Capability {
    fn wire(self) -> &'static str {
        match self {
            Self::Pull => "pull",
            Self::Publish => "publish",
            Self::ManageMembership => "manage_membership",
            Self::AskExpertise => "ask_expertise",
            Self::AnswerExpertise => "answer_expertise",
        }
    }
}
#[cfg(feature = "tui")]
fn terminal_capability(input: &str) -> Result<&'static str> {
    Capability::from_str(&input.trim().replace('_', "-"), false)
        .map(Capability::wire)
        .map_err(|_| {
            anyhow::anyhow!(
                "expected pull, publish, manage-membership, ask-expertise or answer-expertise"
            )
        })
}
#[derive(Debug, Clone, Copy, ValueEnum)]
enum SyncOperation {
    CatchUp,
    Resynchronize,
    Retry,
}
impl SyncOperation {
    fn wire(self) -> &'static str {
        match self {
            Self::CatchUp => "catch_up",
            Self::Resynchronize => "resynchronize",
            Self::Retry => "retry",
        }
    }
}
#[derive(Debug, Subcommand)]
enum FederationCommand {
    /// Inspect, review and manage approved expertise collections.
    #[command(after_help = HELP)]
    Collections {
        #[command(subcommand)]
        command: expertise::CollectionCommand,
    },
    /// Discover expertise with an enabled local-human session or operator-bound token.
    #[command(after_help = HELP)]
    Capabilities {
        #[arg(long, requires = "after_record")]
        after_publisher: Option<Uuid>,
        #[arg(long, requires = "after_publisher")]
        after_record: Option<Uuid>,
        #[arg(long, default_value_t = 32, value_parser = clap::value_parser!(u32).range(1..=32))]
        limit: u32,
    },
    /// Prepare or submit an explicitly reviewed remote question. No implicit context.
    #[command(after_help = HELP)]
    Ask {
        #[command(subcommand)]
        command: expertise::AskCommand,
    },
    /// Explicitly read a question's status/result under current disclosure policy.
    #[command(after_help = HELP)]
    Answer {
        #[arg(long)]
        request_id: Uuid,
    },
    /// Show metadata-only watched-agent exchanges for this project.
    #[command(after_help = HELP)]
    AgentExchanges,
    /// Discover configured identity and current local actor (does not grant Pull).
    #[command(after_help = HELP)]
    Discover,
    /// Observe this instance's queue, acceptance and retained replication status.
    #[command(after_help = HELP)]
    Status,
    /// Page shared streams, memberships, proposals and local subscriptions/reviews.
    #[command(after_help = HELP)]
    Catalog {
        #[arg(long, default_value_t = 0)]
        offset: u32,
    },
    /// Set separate local capabilities. Requires project AuthManage permission.
    #[command(after_help = HELP)]
    Grant {
        #[arg(long)]
        principal: Uuid,
        #[arg(long, value_delimiter = ',')]
        capabilities: Vec<Capability>,
        #[arg(long)]
        disable: bool,
        #[arg(long)]
        confirm: bool,
    },
    /// Enroll/revoke a remote capability (configured authority only). Expertise requires a v3-compatible fleet.
    #[command(after_help = HELP)]
    Member {
        #[arg(long)]
        member: Uuid,
        #[arg(long)]
        capability: Capability,
        #[arg(long)]
        revoke: bool,
        #[arg(long)]
        request_id: Uuid,
        #[arg(long)]
        confirm: bool,
    },
    /// Revoke all remote capabilities; retained copies may remain.
    #[command(after_help = HELP)]
    Revoke {
        #[arg(long)]
        member: Uuid,
        #[arg(long)]
        request_id: Uuid,
        #[arg(long)]
        confirm: bool,
    },
    /// Prepare an unconfirmed review. Evidence file is a JSON array of selected snapshots.
    #[command(after_help = HELP)]
    Draft {
        #[arg(long)]
        canonical: Uuid,
        #[arg(long)]
        evidence_file: Option<PathBuf>,
    },
    /// Submit the exact inspected draft; --confirm consents to future local updates.
    #[command(after_help = HELP)]
    Share {
        #[arg(long)]
        file: PathBuf,
        #[arg(long)]
        confirm: bool,
    },
    /// Retrieve explicitly selected shared context with complete provenance and restrictions.
    #[command(after_help = HELP)]
    Context {
        #[arg(long)]
        origin: Uuid,
        #[arg(long)]
        canonical: Uuid,
    },
    /// Propose a correction using a JSON CorrectionProposal, never overwrite its origin.
    #[command(after_help = HELP)]
    Propose {
        #[arg(long)]
        file: PathBuf,
        #[arg(long)]
        request_id: Uuid,
        #[arg(long)]
        confirm: bool,
    },
    /// Inspect a proposal's exact content, author, base version and resolution.
    #[command(after_help = HELP)]
    Proposal {
        #[arg(long)]
        publisher: Uuid,
        #[arg(long)]
        record: Uuid,
    },
    /// Approve or reject a reviewed proposal. Approval pauses local automatic updates.
    #[command(after_help = HELP)]
    Resolve {
        #[arg(long)]
        publisher: Uuid,
        #[arg(long)]
        record: Uuid,
        #[arg(long)]
        approve: bool,
        #[arg(long)]
        request_id: Uuid,
        #[arg(long)]
        confirm: bool,
    },
    /// Stop future capture, not already reviewed backlog or delivered copies.
    #[command(after_help = HELP)]
    Pause {
        #[arg(long)]
        canonical: Uuid,
        #[arg(long)]
        request_id: Uuid,
        #[arg(long)]
        confirm: bool,
    },
    #[command(after_help = HELP, about = "Catch up, fully resynchronize or retry authorized pending work.")]
    Sync {
        #[arg(value_enum)]
        operation: SyncOperation,
    },
    /// Open the focused federation terminal workspace (same local API).
    #[command(after_help = HELP)]
    Tui,
}

#[derive(Clone)]
struct FederationClient {
    client: Client,
    base: String,
    local_human_session: bool,
}
impl FederationClient {
    async fn call(&self, method: Method, suffix: &str, body: Option<Value>) -> Result<Value> {
        let mut req = self
            .client
            .request(method, format!("{}{suffix}", self.base))
            .timeout(Duration::from_secs(30));
        if let Some(body) = body {
            req = req.json(&body);
        }
        let response = req.send().await.context(
            "local federation request; retain the request ID if the response is uncertain",
        )?;
        if self.local_human_session {
            match response.status().as_u16() {
                200..=202 => response.json().await.map_err(|_| {
                    anyhow::anyhow!("Unreadable federation receipt; inspect the same request ID")
                }),
                401 | 403 => anyhow::bail!(
                    "Federation access denied; session or current policy rejected. No credentials were retried"
                ),
                409 => anyhow::bail!(
                    "Federation conflict; inspect current policy and preserve exact request IDs"
                ),
                _ => anyhow::bail!(
                    "Federation outcome unconfirmed; inspect the same request ID. Response details suppressed"
                ),
            }
        } else {
            get_json(response).await
        }
    }
    async fn action(&self, id: Uuid, action: Value, confirm: bool) -> Result<Value> {
        ensure!(
            confirm,
            "Inspect the action and pass --confirm; preserve --request-id when retrying."
        );
        ensure!(!id.is_nil(), "request ID must be non-nil");
        self.call(
            Method::POST,
            "/actions",
            Some(json!({"request_id":id,"action":action})),
        )
        .await
    }
}
fn file_json(path: &Path) -> Result<Value> {
    let mut bytes = Vec::new();
    std::fs::File::open(path)?
        .take(128 * 1024 + 1)
        .read_to_end(&mut bytes)?;
    ensure!(bytes.len() <= 128 * 1024, "review input exceeds 128 KiB");
    Ok(serde_json::from_slice(&bytes)?)
}
fn confirmed_review(mut input: Value, confirm: bool) -> Result<Value> {
    ensure!(
        confirm,
        "Inspect exact content, evidence and current shared content; --confirm enables future local updates."
    );
    if input.get("request").is_some() {
        input = input["request"].take();
    }
    ensure!(input.is_object(), "expected a review draft object");
    input["confirm_future_updates"] = json!(true);
    Ok(input)
}
pub(super) async fn handle(args: FederationArgs, client: Client, config: AppConfig) -> Result<()> {
    let client = if args.local_human_session {
        super::api::local_human_client(config.clone()).await?
    } else {
        client
    };
    // Encode the project as a single URL segment, never a user-selected endpoint.
    let mut url = reqwest::Url::parse(&service_url(&config, "/v1/projects/"))?;
    url.path_segments_mut()
        .map_err(|_| anyhow::anyhow!("invalid service URL"))?
        .pop_if_empty()
        .push(&args.project)
        .push("federation");
    let mut api = FederationClient {
        client,
        base: url.to_string(),
        local_human_session: args.local_human_session,
    };
    let discovery = api.call(Method::GET, "", None).await?;
    if matches!(args.command, FederationCommand::Discover) {
        println!("{}", serde_json::to_string_pretty(&discovery)?);
        return Ok(());
    }
    let configured = Uuid::parse_str(
        discovery["space"]
            .as_str()
            .context("service omitted configured space")?,
    )?;
    ensure!(
        args.space.is_none_or(|s| s == configured),
        "requested space is not configured for this project"
    );
    api.base.push_str(&format!("/{configured}"));
    let show_health = matches!(args.command, FederationCommand::Status);
    let response=match args.command {
        FederationCommand::Collections{command}=>expertise::collection(&api,command).await?,
        FederationCommand::Capabilities{after_publisher,after_record,limit}=>api.call(Method::GET,&expertise::capability_path(after_publisher,after_record,limit),None).await?,
        FederationCommand::Ask{command}=>expertise::ask(&api,command).await?,
        FederationCommand::Answer{request_id}=>api.call(Method::GET,&format!("/questions/{request_id}"),None).await?,
        FederationCommand::AgentExchanges=>api.call(Method::GET,"/agent-questions",None).await?,
        FederationCommand::Discover=>unreachable!(),
        FederationCommand::Status=>api.call(Method::GET,"",None).await?,
        FederationCommand::Catalog{offset}=>api.call(Method::GET,&format!("/catalog?offset={offset}"),None).await?,
        FederationCommand::Grant{principal,capabilities,disable,confirm}=>{
            ensure!(confirm,"Inspect local capabilities and pass --confirm");
            api.call(Method::POST,"/grants",Some(json!({"principal":principal,"capabilities":capabilities.into_iter().map(Capability::wire).collect::<Vec<_>>(),"enabled":!disable}))).await?
        },
        FederationCommand::Member{member,capability,revoke,request_id,confirm}=>api.action(request_id,json!({"kind":"membership","member":member,"capability":capability.wire(),"enabled":!revoke}),confirm).await?,
        FederationCommand::Revoke{member,request_id,confirm}=>api.action(request_id,json!({"kind":"revoke","member":member}),confirm).await?,
        FederationCommand::Draft{canonical,evidence_file}=>api.call(Method::POST,"/review-preview",Some(json!({"canonical":canonical,"evidence":evidence_file.as_deref().map(file_json).transpose()?.unwrap_or(json!([]))}))).await?,
        FederationCommand::Share{file,confirm}=>api.call(Method::POST,"/reviews",Some(confirmed_review(file_json(&file)?,confirm)?)).await?,
        FederationCommand::Context{origin,canonical}=>api.call(Method::POST,"/context",Some(json!({"streams":[{"origin":origin,"canonical":canonical}]}))).await?,
        FederationCommand::Propose{file,request_id,confirm}=>api.action(request_id,json!({"kind":"propose","proposal":file_json(&file)?}),confirm).await?,
        FederationCommand::Proposal{publisher,record}=>api.call(Method::GET,&format!("/proposals/{publisher}/{record}"),None).await?,
        FederationCommand::Resolve{publisher,record,approve,request_id,confirm}=>api.action(request_id,json!({"kind":"resolve","proposal":{"publisher":publisher,"id":record},"approve":approve}),confirm).await?,
        FederationCommand::Pause{canonical,request_id,confirm}=>api.action(request_id,json!({"kind":"pause","canonical":canonical}),confirm).await?,
        FederationCommand::Sync{operation}=>api.call(Method::POST,"/sync",Some(json!({"operation":operation.wire()}))).await?,
        FederationCommand::Tui=>{
            #[cfg(feature="tui")]
            {terminal::run(api,discovery).await?;return Ok(());}
            #[cfg(not(feature="tui"))]
            anyhow::bail!("this build excludes the tui feature");
        }
    };
    if show_health {
        eprintln!(
            "{}",
            health::lines(&response, chrono::Utc::now()).join("\n")
        );
    }
    println!("{}", serde_json::to_string_pretty(&response)?);
    Ok(())
}

#[cfg(test)]
mod tests {
    use super::*;
    use clap::Parser;

    #[derive(Parser)]
    struct TestCli {
        #[command(flatten)]
        args: FederationArgs,
    }

    #[test]
    fn expertise_capabilities_parse_for_local_grants_and_remote_enrollment() {
        let id = Uuid::new_v4().to_string();
        for (name, wire) in [
            ("ask-expertise", "ask_expertise"),
            ("answer-expertise", "answer_expertise"),
        ] {
            for revoke in [false, true] {
                let mut args = vec![
                    "memory",
                    "--project",
                    "fixture",
                    "member",
                    "--member",
                    &id,
                    "--capability",
                    name,
                    "--request-id",
                    &id,
                    "--confirm",
                ];
                if revoke {
                    args.push("--revoke");
                }
                let parsed = TestCli::try_parse_from(args).unwrap();
                let FederationCommand::Member {
                    capability,
                    revoke: actual,
                    confirm,
                    ..
                } = parsed.args.command
                else {
                    panic!("member command")
                };
                assert_eq!(capability.wire(), wire);
                assert_eq!(actual, revoke);
                assert!(confirm);
            }
        }
        let parsed = TestCli::try_parse_from([
            "memory",
            "--project",
            "fixture",
            "grant",
            "--principal",
            &id,
            "--capabilities",
            "pull,publish,manage-membership,ask-expertise,answer-expertise",
            "--confirm",
        ])
        .unwrap();
        let FederationCommand::Grant { capabilities, .. } = parsed.args.command else {
            panic!("grant command")
        };
        assert_eq!(
            capabilities
                .into_iter()
                .map(Capability::wire)
                .collect::<Vec<_>>(),
            [
                "pull",
                "publish",
                "manage_membership",
                "ask_expertise",
                "answer_expertise"
            ]
        );
        assert!(
            TestCli::try_parse_from([
                "memory",
                "--project",
                "fixture",
                "member",
                "--member",
                &id,
                "--capability",
                "unknown",
                "--request-id",
                &id
            ])
            .is_err()
        );
    }

    #[tokio::test]
    async fn expertise_action_http_preserves_exact_retry_and_wire_capability() {
        use tokio::io::{AsyncReadExt, AsyncWriteExt};
        let listener = tokio::net::TcpListener::bind("127.0.0.1:0").await.unwrap();
        let base = format!(
            "http://{}/v1/projects/fixture/federation/fixture",
            listener.local_addr().unwrap()
        );
        let server = tokio::spawn(async move {
            let mut bodies = Vec::new();
            for _ in 0..2 {
                let (mut stream, _) = listener.accept().await.unwrap();
                let mut bytes = Vec::new();
                loop {
                    let mut chunk = [0; 1024];
                    let n = stream.read(&mut chunk).await.unwrap();
                    assert!(n > 0 && bytes.len() < 8192);
                    bytes.extend_from_slice(&chunk[..n]);
                    if let Some(end) = bytes.windows(4).position(|w| w == b"\r\n\r\n") {
                        let headers = std::str::from_utf8(&bytes[..end]).unwrap();
                        assert!(headers.starts_with(
                            "POST /v1/projects/fixture/federation/fixture/actions HTTP/1.1"
                        ));
                        let length: usize = headers
                            .lines()
                            .find_map(|line| {
                                let (name, value) = line.split_once(':')?;
                                name.eq_ignore_ascii_case("content-length")
                                    .then(|| value.trim().parse().unwrap())
                            })
                            .unwrap();
                        if bytes.len() >= end + 4 + length {
                            bodies.push(
                                serde_json::from_slice::<Value>(&bytes[end + 4..end + 4 + length])
                                    .unwrap(),
                            );
                            break;
                        }
                    }
                }
                stream.write_all(b"HTTP/1.1 202 Accepted\r\nContent-Type: application/json\r\nContent-Length: 23\r\nConnection: close\r\n\r\n{\"submission\":\"queued\"}").await.unwrap();
            }
            bodies
        });
        let api = FederationClient {
            client: Client::new(),
            base,
            local_human_session: false,
        };
        let id = Uuid::new_v4();
        let action = json!({"kind":"membership", "member":Uuid::new_v4(),
            "capability":Capability::AnswerExpertise.wire(), "enabled":false});
        for _ in 0..2 {
            let response = api.action(id, action.clone(), true).await.unwrap();
            assert_eq!(response["submission"], "queued");
        }
        let bodies = tokio::time::timeout(Duration::from_secs(5), server)
            .await
            .unwrap()
            .unwrap();
        assert_eq!(bodies, vec![json!({"request_id":id,"action":action}); 2]);
    }

    #[tokio::test]
    async fn expertise_action_requires_confirmation_before_network_access() {
        let api = FederationClient {
            client: Client::new(),
            base: "invalid endpoint must never be used".into(),
            local_human_session: false,
        };
        let error = api
            .action(
                Uuid::new_v4(),
                json!({"kind":"membership", "capability":"ask_expertise"}),
                false,
            )
            .await
            .unwrap_err();
        assert!(error.to_string().contains("--confirm"));
    }

    #[test]
    fn confirmation_preserves_review_identity_and_exact_evidence() {
        let request = json!({"request_id":Uuid::new_v4(),"expected":{"content":"quoted untrusted input"},"evidence":[{"snapshot_digest":"unchanged"}],"confirm_future_updates":false});
        assert!(confirmed_review(request.clone(), false).is_err());
        let confirmed = confirmed_review(json!({"request":request}), true).unwrap();
        assert_eq!(confirmed["request_id"], request["request_id"]);
        assert_eq!(confirmed["evidence"], request["evidence"]);
        assert_eq!(confirmed["expected"], request["expected"]);
        assert_eq!(confirmed["confirm_future_updates"], true);
    }
}
