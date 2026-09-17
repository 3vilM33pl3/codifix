// SPDX-License-Identifier: AGPL-3.0-or-later
use super::api::ApiClient;
use anyhow::Result;
use clap::Args;
use mem_record::SupervisorResponse;

pub(crate) const HELP: &str = "Agent notes:\nRead-only local instance inspection. Requires global memory_read, not a project role. No provider call, bootstrap, activation or policy mutation. Enabled policy does not prove worker health. History contains metadata only. See docs/user/cli/supervisor.md.";

#[derive(Debug, Args)]
pub(crate) struct SupervisorArgs {
    #[arg(long, default_value = "0")]
    after: String,
    #[arg(long, default_value_t = 50)]
    limit: u32,
    #[arg(long)]
    json: bool,
}
pub(crate) async fn handle(args: SupervisorArgs, api: &ApiClient) -> Result<()> {
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
pub(crate) fn lines(response: &SupervisorResponse) -> Vec<String> {
    let mut lines = vec![
        "Local area supervisor — instance-wide metadata only".into(),
        "Execution not integrated in this build; no active agent or healthy heartbeat is claimed."
            .into(),
    ];
    if response.read_only {
        lines.push("Read-only service.".into());
    }
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

#[cfg(test)]
mod tests {
    use super::*;
    use mem_record::SupervisorExecution;

    #[test]
    fn supervisor_text_never_infers_execution_from_missing_configuration() {
        let response = SupervisorResponse {
            read_only: true,
            execution: SupervisorExecution::NotIntegrated,
            snapshot: None,
        };
        let text = lines(&response).join("\n");
        assert!(
            text.contains("Execution not integrated")
                && text.contains("nothing enrolled or enabled")
        );
        assert!(text.contains("Read-only service"));
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
