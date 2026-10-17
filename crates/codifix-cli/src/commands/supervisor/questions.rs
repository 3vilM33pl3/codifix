// SPDX-License-Identifier: AGPL-3.0-or-later
use crate::commands::api::ApiClient;
use anyhow::{Result, ensure};
use clap::Args;
use codifix_record::{SupervisorQuestionPreview, SupervisorQuestionRequest};
use uuid::Uuid;

#[derive(Debug, Args)]
pub(super) struct AskArgs {
    #[arg(long)]
    pub project: String,
    /// Exact question only. No history, files or ordinary retrieval is attached.
    #[arg(long)]
    pub question: String,
    /// Keep this ID and the exact payload after an ambiguous submission.
    #[arg(long)]
    pub request_id: Uuid,
    /// Copy from the preparation preview, not a newly generated confirmation.
    #[arg(long, requires = "confirm")]
    pub profile_digest: Option<String>,
    #[arg(long, requires = "confirm")]
    pub request_digest: Option<String>,
    #[arg(long, requires_all=["profile_digest", "request_digest"])]
    pub confirm: bool,
}

pub(super) async fn ask(args: AskArgs, api: &ApiClient) -> Result<()> {
    ensure!(!args.request_id.is_nil(), "Request ID must not be nil");
    ensure!(
        !args.question.trim().is_empty() && args.question.len() <= 4096,
        "Question must contain 1–4096 UTF-8 bytes"
    );
    if args.confirm {
        // Never silently reprepare: the server must match precisely the previously displayed consent.
        let request = SupervisorQuestionRequest {
            request_id: args.request_id,
            question: args.question,
            profile_digest: args.profile_digest.expect("clap requires profile digest"),
            request_digest: args.request_digest.expect("clap requires request digest"),
        };
        let status = api
            .supervisor_question_submit(&args.project, &request)
            .await?;
        println!("{}", serde_json::to_string_pretty(&status)?);
        println!(
            "Queued is not answered. Inspect this request ID; never create another to retry uncertain work."
        );
    } else {
        let preview = api
            .supervisor_question_prepare(&args.project, &args.question)
            .await?;
        for line in preview_lines(&preview, args.request_id) {
            println!("{line}");
        }
        println!(
            "Preview only; no model call. Repeat the same question and request ID with --confirm --profile-digest {} --request-digest {}.",
            preview.profile_digest, preview.request_digest
        );
    }
    Ok(())
}

pub(crate) fn preview_lines(preview: &SupervisorQuestionPreview, request: Uuid) -> Vec<String> {
    // Escape text rather than interpreting terminal/bidi controls.
    vec![
        format!("Question: {}", serde_json::to_string(&preview.question).unwrap_or_default()),
        format!("Destination: {} | model: {}", serde_json::to_string(&preview.destination).unwrap_or_default(), serde_json::to_string(&preview.model).unwrap_or_default()),
        format!("Approved sources: {}. Daily ceiling: {} calls / {} USD millionths.", preview.source_count, preview.daily_call_limit, preview.daily_usd_micros),
        format!("Request {request} | profile {} | request digest {}", preview.profile_digest, preview.request_digest),
        "Confirmation permits one bounded paid answer attempt with only this question and the approved collection. No history or files. Uncertain work is not automatically retried; another question may repeat billing.".into(),
    ].into_iter().map(|line| terminal_text(&line)).collect()
}

pub(crate) fn terminal_text(text: &str) -> String {
    text.chars()
        .map(|ch| {
            if (ch.is_control() && ch != '\n' && ch != '\t')
                || matches!(ch, '\u{202a}'..='\u{202e}' | '\u{2066}'..='\u{2069}')
            {
                format!("\\u{{{:04x}}}", ch as u32)
            } else {
                ch.to_string()
            }
        })
        .collect()
}

#[cfg(test)]
mod tests {
    use super::*;
    use clap::FromArgMatches;
    #[test]
    fn supervisor_question_cli_requires_prepared_hashes_for_confirmation() {
        let command = || AskArgs::augment_args(clap::Command::new("ask"));
        let base = [
            "ask",
            "--project",
            "memory",
            "--question",
            "Why?",
            "--request-id",
            "a27ba0b8-2b4a-4fbb-988c-182178325a36",
        ];
        let mut args = base.to_vec();
        args.push("--confirm");
        assert!(command().try_get_matches_from(args.clone()).is_err());
        args.extend(["--profile-digest", "profile", "--request-digest", "request"]);
        let parsed =
            AskArgs::from_arg_matches(&command().try_get_matches_from(args).unwrap()).unwrap();
        assert!(parsed.confirm);
        assert_eq!(parsed.request_digest.as_deref(), Some("request"));
    }
    #[test]
    fn supervisor_question_terminal_text_never_executes_control_sequences() {
        assert_eq!(
            terminal_text("x\u{1b}[31m\u{202e}y"),
            "x\\u{001b}[31m\\u{202e}y"
        );
    }
}
