// SPDX-License-Identifier: AGPL-3.0-or-later
use super::api::ApiClient;
use anyhow::Result;
use clap::{Args, Subcommand};
use codifix_record::SupervisorResponse;

pub(crate) mod questions;

pub(crate) const HELP: &str = "Agent notes:\nRead-only local instance inspection by default. Requires global memory_read, not a project role. Explicit pause/resume require current global system_admin, exact instance/policy/request UUIDs and --confirm. Project-scoped ask prepares exact consent; --confirm with both preview hashes queues a potentially paid question. Local question clients exchange authenticated loopback owner access for an in-memory revocable session. No credentials are printed or stored. Enabled policy does not prove worker health or authorize new disclosure. History contains metadata only. See docs/user/cli/supervisor.md.";

#[derive(Debug, Args)]
pub(crate) struct SupervisorArgs {
    #[command(subcommand)]
    command: Option<SupervisorCommand>,
    #[arg(long, default_value = "0")]
    after: String,
    #[arg(long, default_value_t = 50)]
    limit: u32,
    #[arg(long)]
    json: bool,
}
#[derive(Debug, Subcommand)]
enum SupervisorCommand {
    /// Prepare an exact private question. Submit only with preview hashes and --confirm.
    #[command(after_help = HELP)]
    Ask(questions::AskArgs),
    /// Inspect an originally submitted question; never re-executes it.
    #[command(after_help = HELP)]
    Question {
        #[arg(long)]
        project: String,
        #[arg(long)]
        request_id: uuid::Uuid,
    },
    /// List only this authenticated requester's questions.
    #[command(after_help = HELP)]
    Questions {
        #[arg(long)]
        project: String,
        #[arg(long)]
        after: Option<uuid::Uuid>,
    },
    /// List private proposal IDs in exactly one project; no model call.
    #[command(after_help = HELP)]
    Proposals {
        #[arg(long)]
        project: String,
        #[arg(long)]
        after: Option<uuid::Uuid>,
    },
    /// Read a private saved question, response and exact evidence references.
    #[command(after_help = HELP)]
    Result {
        #[arg(long)]
        project: String,
        #[arg(long)]
        action: uuid::Uuid,
    },
    /// Record human review, not execution permission. Preview without --confirm.
    #[command(after_help = HELP)]
    Review {
        #[arg(long)]
        project: String,
        #[arg(long)]
        action: uuid::Uuid,
        #[arg(long)]
        digest: String,
        #[arg(long)]
        request_id: uuid::Uuid,
        #[arg(long, value_parser=["approve","reject"])]
        decision: String,
        #[arg(long)]
        confirm: bool,
    },
    /// Pause the exact currently enabled policy; in-flight bytes cannot be recalled.
    #[command(after_help = HELP)]
    Pause(ControlArgs),
    /// Resume the exact currently paused policy, without granting new permissions.
    #[command(after_help = HELP)]
    Resume(ControlArgs),
}
#[derive(Debug, Args)]
struct ControlArgs {
    #[arg(long)]
    instance: uuid::Uuid,
    #[arg(long)]
    policy: uuid::Uuid,
    /// Reuse this UUID only when retrying the exact same operation.
    #[arg(long)]
    request_id: uuid::Uuid,
    /// Required action-time confirmation; without it only print the request.
    #[arg(long)]
    confirm: bool,
}
pub(crate) async fn handle(args: SupervisorArgs, api: &ApiClient) -> Result<()> {
    if let Some(command) = args.command {
        let (control, enabled) = match command {
            SupervisorCommand::Ask(ask) => return questions::ask(ask, api).await,
            SupervisorCommand::Question {
                project,
                request_id,
            } => {
                println!(
                    "{}",
                    serde_json::to_string_pretty(
                        &api.supervisor_question(&project, request_id).await?
                    )?
                );
                return Ok(());
            }
            SupervisorCommand::Questions { project, after } => {
                println!(
                    "{}",
                    serde_json::to_string_pretty(
                        &api.supervisor_questions(&project, after).await?
                    )?
                );
                return Ok(());
            }
            SupervisorCommand::Proposals { project, after } => {
                println!(
                    "{}",
                    serde_json::to_string_pretty(
                        &api.supervisor_proposals(&project, after).await?
                    )?
                );
                return Ok(());
            }
            SupervisorCommand::Result { project, action } => {
                let result = api.supervisor_proposal(&project, action).await?;
                if args.json {
                    println!("{}", serde_json::to_string_pretty(&result)?);
                } else {
                    for line in proposal_lines(&result) {
                        println!("{line}");
                    }
                }
                return Ok(());
            }
            SupervisorCommand::Review {
                project,
                action,
                digest,
                request_id,
                decision,
                confirm,
            } => {
                let request = codifix_record::SupervisorReviewRequest {
                    request_id,
                    proposal_digest: digest,
                    decision: if decision == "approve" {
                        codifix_record::SupervisorReviewDecision::Approve
                    } else {
                        codifix_record::SupervisorReviewDecision::Reject
                    },
                };
                if !confirm {
                    println!(
                        "Preview only: project {project}, action {action}. Human review only; no action executes. Decision is permanent.\n{}\nRepeat with --confirm after inspecting the saved result.",
                        serde_json::to_string_pretty(&request)?
                    );
                } else {
                    println!(
                        "{}",
                        serde_json::to_string_pretty(
                            &api.supervisor_review(&project, action, &request).await?
                        )?
                    );
                }
                return Ok(());
            }
            SupervisorCommand::Pause(c) => (c, false),
            SupervisorCommand::Resume(c) => (c, true),
        };
        let request = codifix_record::SupervisorPolicyRequest {
            request_id: control.request_id,
            instance_id: control.instance,
            policy_revision: control.policy,
            expected_enabled: !enabled,
            enabled,
        };
        if !control.confirm {
            println!(
                "Preview only; no mutation sent. Review this exact request, then repeat with --confirm:\n{}",
                serde_json::to_string_pretty(&request)?
            );
            println!(
                "Existing policy only; no new disclosure consent. Resume can run eligible approved work, including a paid proposal, in an already configured worker. A pause cannot recall already transmitted bytes."
            );
            return Ok(());
        }
        println!(
            "{}",
            serde_json::to_string_pretty(&api.supervisor_policy(&request).await?)?
        );
        println!(
            "Receipt confirms this request, not current worker health. Run memory supervisor to inspect fresh state."
        );
        return Ok(());
    }
    let response = api.supervisor(&args.after, args.limit).await?;
    if args.json {
        println!("{}", serde_json::to_string_pretty(&response)?);
    } else {
        for line in lines(&response) {
            println!("{line}");
        }
    }
    Ok(())
}

pub(crate) fn proposal_lines(p: &codifix_record::SupervisorProposalDetail) -> Vec<String> {
    // Model text is untrusted: no terminal controls, bidi controls or ANSI links.
    fn plain(s: &str) -> String {
        s.chars()
            .filter(|c| {
                (!c.is_control() || *c == '\n' || *c == '\t')
                    && !matches!(*c,'\u{202a}'..='\u{202e}'|'\u{2066}'..='\u{2069}')
            })
            .collect()
    }
    let mut lines = vec![
        format!(
            "Private proposal {} | project {}",
            p.action_id, p.project_id
        ),
        format!("Question: {}", plain(&p.question)),
        plain(&p.summary),
        format!(
            "Suggested next step: {:?}. Model suggestion, not verified truth.",
            p.proposal
        ),
        format!("Digest: {}", p.proposal_digest),
    ];
    for c in &p.citations {
        lines.push(format!(
            "Evidence {}: version {} record {} digest {}",
            c.index + 1,
            c.version,
            c.version_id,
            c.evidence_digest
        ));
    }
    if let Some(r) = &p.review {
        lines.push(format!(
            "Review: {:?} by {} at {}",
            r.decision, r.reviewer_id, r.recorded_at
        ));
    }
    lines.push("Human review only. No memory edit, installation or action executes.".into());
    lines
}
pub(crate) fn lines(response: &SupervisorResponse) -> Vec<String> {
    let mut lines = vec![
        "Local area supervisor — instance-wide metadata only".into(),
        "Execution not integrated for Pi/model agency; no active agent or healthy heartbeat is claimed."
            .into(),
    ];
    if response.read_only {
        lines.push("Read-only service.".into());
    }
    if let Some(grant) = &response.grant_health {
        lines.extend(grant_lines(grant, chrono::Utc::now()));
    } else {
        lines.push("Disclosure grant expiry unavailable; no healthy grant is claimed.".into());
    }
    if let Some(work) = &response.model_work {
        lines.push(format!(
            "Bounded single-proposal worker: {:?} | observed {} | action {} | durable state {:?}",
            work.state,
            work.observed_at,
            work.action_id
                .map(|id| id.to_string())
                .unwrap_or_else(|| "not loaded".into()),
            work.action_state
        ));
        lines.push("One fixed proposal, not autonomous agency. Stored means durable output exists; delivery is unconfirmed. Restart never reissues finished/uncertain work.".into());
    }
    lines.push(format!(
        "Deterministic operational checks configured: {} (not model/agent activation)",
        response.deterministic_checks_configured
    ));
    let Some(s) = &response.snapshot else {
        lines
            .push("No durable supervisor identity configured; nothing enrolled or enabled.".into());
        return lines;
    };
    lines.push(format!(
        "Instance {} | area {:?} | observed {}",
        s.instance_id, s.area, s.observed_at
    ));
    if s.area.is_none() {
        lines.push("Database supervisor identity is not bound.".into());
    }
    if let Some(p) = &s.policy {
        lines.push(format!(
            "Policy {}: {} | model {}",
            p.revision,
            if p.revoked {
                "revoked"
            } else if p.enabled {
                "enabled (activation separate)"
            } else {
                "paused"
            },
            p.model
        ));
        lines.push(format!(
            "Daily ceiling: {} USD millionths / {} calls",
            p.daily_usd_micros, p.daily_calls
        ));
    } else {
        lines.push("No policy installed.".into());
    }
    if let Some(b) = &s.budget {
        lines.push(format!(
            "UTC {}: charged {} / reserved-or-uncertain {} USD millionths; {} calls",
            b.day, b.charged_usd_micros, b.reserved_usd_micros, b.calls
        ));
    }
    if let Some(health) = &s.last_health_observation {
        lines.push(format!(
            "Last operational observation: {} | policy {} | action {}",
            health.observed_at, health.policy_revision, health.action_id
        ));
        lines.push(format!("{} projects / {} canonical histories; {} blocked jobs / {} failed jobs / {} uncertain actions", health.project_count, health.canonical_count, health.blocked_jobs, health.failed_jobs, health.uncertain_actions));
        lines.push(format!("Next due: {}. Historical observation, not proof of current availability or external health.", health.next_due_at));
    }
    for count in &s.action_counts {
        lines.push(format!("{:?}: {}", count.state, count.count));
    }
    lines.push("History (no questions, source text, private proposals or prompts):".into());
    for e in &s.journal {
        lines.push(format!(
            "#{} {} {:?} | action {} | attempt {}",
            e.sequence,
            e.recorded_at,
            e.event,
            e.action_id.map_or_else(|| "—".into(), |v| v.to_string()),
            e.attempt_id.map_or_else(|| "—".into(), |v| v.to_string())
        ));
    }
    if s.journal.is_empty() {
        lines.push("No newer history entries on this page.".into());
    }
    if s.has_more {
        lines.push(format!("Next: memory supervisor --after {}", s.next_after));
    }
    lines
}

fn grant_lines(
    grant: &codifix_record::SupervisorGrantHealth,
    now: chrono::DateTime<chrono::Utc>,
) -> Vec<String> {
    let age = now.signed_duration_since(grant.observed_at).num_seconds();
    let label = if !(-30..=120).contains(&age) {
        "Observation stale or future-dated; refresh before relying on the grant"
    } else if grant.state == codifix_record::SupervisorGrantWindow::InvalidProfile {
        "Profile unavailable or invalid; operator review required"
    } else {
        match grant
            .expires_at
            .map(|expiry| expiry.signed_duration_since(now).num_seconds())
        {
            None => "Expiry unknown; no valid disclosure window is claimed",
            Some(seconds) if seconds <= 0 => "Expired; paid questions under this grant are blocked",
            Some(seconds) if seconds <= 3600 => "Expires within one hour",
            Some(seconds) if seconds <= 86400 => "Expires within one day",
            Some(_) => "Within the reviewed time window",
        }
    };
    vec![
        format!("Disclosure grant: {label}."),
        format!(
            "Expires {}; observed {}. No automatic renewal; current execution permissions and explicit question confirmation are checked separately.",
            grant
                .expires_at
                .map(|at| at.to_rfc3339())
                .unwrap_or_else(|| "unknown".into()),
            grant.observed_at
        ),
    ]
}

#[cfg(test)]
mod tests {
    use super::*;
    use codifix_record::SupervisorExecution;
    #[test]
    fn cached_grant_labels_age_and_expire_in_cli_and_tui() {
        let now = chrono::DateTime::from_timestamp(1_800_000_000, 0).unwrap();
        let mut grant =
            codifix_record::SupervisorGrantHealth::at(Some(now + chrono::Duration::days(2)), now);
        assert!(grant_lines(&grant, now)[0].contains("Within the reviewed"));
        grant.expires_at = Some(now - chrono::Duration::seconds(1)); // server's old state says valid
        assert!(grant_lines(&grant, now)[0].contains("Expired"));
        grant.expires_at = Some(now + chrono::Duration::hours(2));
        assert!(grant_lines(&grant, now)[0].contains("within one day"));
        assert!(grant_lines(&grant, now + chrono::Duration::seconds(121))[0].contains("stale"));
        grant.observed_at = now + chrono::Duration::seconds(31);
        assert!(grant_lines(&grant, now)[0].contains("future-dated"));
        assert!(grant_lines(&grant, now)[1].contains("No automatic renewal"));
    }

    #[test]
    fn private_proposal_text_cannot_emit_terminal_or_bidi_controls() {
        let detail = codifix_record::SupervisorProposalDetail {
            project_id: uuid::Uuid::new_v4(),
            action_id: uuid::Uuid::new_v4(),
            proposal_digest: "a".repeat(64),
            question: "Question\u{1b}]8;;https://invalid.example\u{7}\u{202e}?".into(),
            summary: "Answer\u{1b}[2J\r\u{2066}\u{2069}\nCitation follows".into(),
            proposal: serde_json::from_value(serde_json::json!("ask_human")).unwrap(),
            citations: Vec::new(),
            created_at: chrono::Utc::now(),
            review: None,
            can_review: true,
        };
        let text = proposal_lines(&detail).join("\n");
        assert!(!text.chars().any(|c| {
            (c.is_control() && c != '\n' && c != '\t')
                || matches!(c, '\u{202a}'..='\u{202e}' | '\u{2066}'..='\u{2069}')
        }));
        assert!(text.contains("Citation follows"));
        assert!(text.contains("Human review only"));
    }

    #[test]
    fn supervisor_text_never_infers_execution_from_missing_configuration() {
        let response = SupervisorResponse {
            read_only: true,
            execution: SupervisorExecution::NotIntegrated,
            deterministic_checks_configured: false,
            model_work: None,
            grant_health: None,
            can_control: false,
            snapshot: None,
        };
        let text = lines(&response).join("\n");
        assert!(
            text.contains("Execution not integrated")
                && text.contains("nothing enrolled or enabled")
        );
        assert!(text.contains("Read-only service"));
    }

    #[test]
    fn bounded_worker_status_is_not_full_agency_or_an_automatic_retry() {
        let response = SupervisorResponse {
            read_only: false,
            execution: SupervisorExecution::NotIntegrated,
            deterministic_checks_configured: false,
            can_control: false,
            snapshot: None,
            model_work: Some(codifix_record::SupervisorModelWorkStatus {
                state: codifix_record::SupervisorModelWorkState::Stored,
                observed_at: chrono::Utc::now(),
                action_id: Some(uuid::Uuid::new_v4()),
                attempt_id: Some(uuid::Uuid::new_v4()),
                action_state: Some(codifix_record::SupervisorActionState::Verified),
            }),
            grant_health: None,
        };
        let text = lines(&response).join("\n");
        assert!(text.contains("Bounded single-proposal worker: Stored"));
        assert!(text.contains("not autonomous agency"));
        assert!(text.contains("delivery is unconfirmed"));
        assert!(text.contains("Restart never reissues"));
        let mut old = serde_json::to_value(&response).expect("response");
        old.as_object_mut().expect("object").remove("model_work");
        assert!(
            serde_json::from_value::<SupervisorResponse>(old)
                .expect("old server compatible")
                .model_work
                .is_none()
        );
    }

    #[tokio::test]
    async fn supervisor_client_errors_never_display_raw_response_bodies() {
        use tokio::io::{AsyncReadExt, AsyncWriteExt};
        for (status, message) in [
            (401, "Signed out"),
            (403, "Forbidden"),
            (409, "Identity mismatch"),
            (503, "Offline"),
            (500, "details suppressed"),
        ] {
            let listener = tokio::net::TcpListener::bind("127.0.0.1:0")
                .await
                .expect("listen");
            let address = listener.local_addr().expect("address");
            let server = tokio::spawn(async move {
                let (mut stream, _) = listener.accept().await.expect("accept");
                let mut input = [0_u8; 8192];
                let n = stream.read(&mut input).await.expect("request");
                assert!(
                    String::from_utf8_lossy(&input[..n])
                        .contains("/v1/supervisor?after=0&limit=50")
                );
                let body = "SYNTHETIC_PRIVATE_RESPONSE";
                stream.write_all(format!("HTTP/1.1 {status} Error\r\nContent-Length: {}\r\nConnection: close\r\n\r\n{body}",body.len()).as_bytes()).await.expect("respond");
            });
            let config = serde_json::from_value(serde_json::json!({
                "service":{"bind_addr":address.to_string(),"api_token":"synthetic-client"},
                "database":{"url":"postgresql://unused/unused"}
            }))
            .expect("config");
            let api = ApiClient::new(reqwest::Client::new(), config);
            let error = api
                .supervisor("0", 50)
                .await
                .expect_err("denied")
                .to_string();
            assert!(error.contains(message));
            assert!(!error.contains("SYNTHETIC_PRIVATE_RESPONSE"));
            server.await.expect("server");
        }
    }
}
