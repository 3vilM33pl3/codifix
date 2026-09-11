// SPDX-License-Identifier: AGPL-3.0-or-later
//! Local service only. Destination authority comes from the operator, never flags.
use super::*;

#[cfg(test)]
mod tests {
    use super::*;
    #[test]
    fn confirmation_keeps_exact_review_and_does_not_select_destination() {
        let review = json!({"digest":{"algorithm":"sha256","value":"fixture"},
            "question":{"request_id":"stable","question":"exact bytes\n","output_destination":"operator_bound"}});
        assert!(confirmation(review.clone(), "review", "digest", false).is_err());
        assert_eq!(
            confirmation(review.clone(), "review", "digest", true).unwrap(),
            json!({"review":review,"confirmed_digest":review["digest"]})
        );
        assert!(confirmation(json!({}), "draft", "review_digest", true).is_err());
    }
    #[tokio::test]
    async fn unconfirmed_actions_never_read_files_or_contact_service() {
        let api = FederationClient {
            client: Client::new(),
            base: "must not be used".into(),
        };
        assert!(
            ask(
                &api,
                AskCommand::Submit {
                    file: "/missing/private/file".into(),
                    confirm: false
                }
            )
            .await
            .unwrap_err()
            .to_string()
            .contains("--confirm")
        );
        assert!(
            collection(
                &api,
                CollectionCommand::Activate {
                    file: "/missing/private/file".into(),
                    confirm: false
                }
            )
            .await
            .unwrap_err()
            .to_string()
            .contains("--confirm")
        );
    }
}

#[derive(Debug, Subcommand)]
pub(super) enum CollectionCommand {
    /// Page locally stored collection reviews (not live eligibility).
    #[command(after_help = HELP)]
    List {
        #[arg(long)]
        after: Option<Uuid>,
        #[arg(long, default_value_t=32, value_parser=clap::value_parser!(u32).range(1..=32))]
        limit: u32,
    },
    /// Inspect every field in a stored collection review.
    #[command(after_help = HELP)]
    Inspect {
        #[arg(long)]
        collection: Uuid,
    },
    /// Read a CollectionBlueprint JSON file; print exact review without activation.
    #[command(after_help = HELP)]
    Draft {
        #[arg(long)]
        file: PathBuf,
    },
    /// Submit the unchanged, fully inspected draft and its review digest.
    #[command(after_help = HELP)]
    Activate {
        #[arg(long)]
        file: PathBuf,
        #[arg(long)]
        confirm: bool,
    },
    /// Withdraw reviewed standing consent at the exact inspected revision.
    #[command(after_help = HELP)]
    Withdraw {
        #[arg(long)]
        collection: Uuid,
        #[arg(long)]
        revision: u32,
        #[arg(long)]
        request_id: Uuid,
        #[arg(long)]
        confirm: bool,
    },
    /// Inspect a selected service identity's stored collection grant.
    #[command(after_help = HELP)]
    GrantStatus {
        #[arg(long)]
        collection: Uuid,
        #[arg(long)]
        principal: Uuid,
    },
    /// After uncertain results inspect current policy; do not blindly replay grants.
    #[command(after_help = HELP)]
    Grant {
        #[arg(long)]
        collection: Uuid,
        #[arg(long)]
        principal: Uuid,
        #[arg(long)]
        disable: bool,
        #[arg(long)]
        confirm: bool,
    },
}
#[derive(Debug, Subcommand)]
pub(super) enum AskCommand {
    /// Read only the exact question from a UTF-8 file, not history or local search.
    #[command(after_help = HELP)]
    Prepare {
        #[arg(long)]
        publisher: Uuid,
        #[arg(long)]
        collection: Uuid,
        #[arg(long)]
        request_id: Uuid,
        #[arg(long)]
        question_file: PathBuf,
    },
    /// Send the exact inspected review. Retain the file/ID for uncertain retries.
    #[command(after_help = HELP)]
    Submit {
        #[arg(long)]
        file: PathBuf,
        #[arg(long)]
        confirm: bool,
    },
}
pub(super) fn capability_path(publisher: Option<Uuid>, record: Option<Uuid>, limit: u32) -> String {
    let mut path = format!("/capabilities?limit={limit}");
    if let (Some(p), Some(r)) = (publisher, record) {
        path.push_str(&format!("&after_publisher={p}&after_record={r}"));
    }
    path
}
pub(super) fn confirmation(
    input: Value,
    field: &str,
    digest: &str,
    confirm: bool,
) -> Result<Value> {
    ensure!(
        confirm,
        "Review every disclosed byte and destination before --confirm; preserve the exact file on retry"
    );
    ensure!(
        input.is_object() && input.get(digest).is_some_and(Value::is_object),
        "expected an unchanged review with its digest"
    );
    Ok(json!({field:input,"confirmed_digest":input[digest]}))
}
pub(super) async fn collection(
    api: &FederationClient,
    command: CollectionCommand,
) -> Result<Value> {
    let (method, path, body) = match command {
        CollectionCommand::List { after, limit } => (
            Method::GET,
            format!(
                "/collections?limit={limit}{}",
                after.map(|id| format!("&after={id}")).unwrap_or_default()
            ),
            None,
        ),
        CollectionCommand::Inspect { collection } => {
            (Method::GET, format!("/collections/{collection}"), None)
        }
        CollectionCommand::Draft { file } => (
            Method::POST,
            "/collections/draft".into(),
            Some(file_json(&file)?),
        ),
        CollectionCommand::Activate { file, confirm } => {
            ensure!(confirm, "Inspect the collection review before --confirm");
            (
                Method::POST,
                "/collections/activate".into(),
                Some(confirmation(
                    file_json(&file)?,
                    "draft",
                    "review_digest",
                    confirm,
                )?),
            )
        }
        CollectionCommand::Withdraw {
            collection,
            revision,
            request_id,
            confirm,
        } => {
            ensure!(
                confirm && revision > 0 && !request_id.is_nil(),
                "Review revision and non-nil request ID before --confirm"
            );
            (
                Method::POST,
                "/collections/withdraw".into(),
                Some(
                    json!({"collection":collection,"expected_revision":revision,"request_id":request_id,"confirmed":true}),
                ),
            )
        }
        CollectionCommand::GrantStatus {
            collection,
            principal,
        } => (
            Method::GET,
            format!("/collections/{collection}/grants?principal={principal}"),
            None,
        ),
        CollectionCommand::Grant {
            collection,
            principal,
            disable,
            confirm,
        } => {
            ensure!(
                confirm,
                "Inspect current service policy before --confirm; an uncertain update requires fresh inspection"
            );
            (
                Method::POST,
                format!("/collections/{collection}/grants"),
                Some(json!({"principal":principal,"enabled":!disable,"confirmed":true})),
            )
        }
    };
    api.call(method, &path, body).await
}
pub(super) async fn ask(api: &FederationClient, command: AskCommand) -> Result<Value> {
    let (path, body) = match command {
        AskCommand::Prepare {
            publisher,
            collection,
            request_id,
            question_file,
        } => {
            let mut bytes = Vec::new();
            std::fs::File::open(question_file)?
                .take(4097)
                .read_to_end(&mut bytes)?;
            ensure!(
                !bytes.is_empty() && bytes.len() <= 4096,
                "question must be 1 through 4096 UTF-8 bytes"
            );
            let question = String::from_utf8(bytes).context("question must be UTF-8")?;
            (
                "/questions/prepare",
                json!({"request_id":request_id,"capability":{"publisher":publisher,"id":collection},"question":question}),
            )
        }
        AskCommand::Submit { file, confirm } => {
            ensure!(
                confirm,
                "Inspect the exact question, destination and disclosure policy before --confirm"
            );
            (
                "/questions/submit",
                confirmation(file_json(&file)?, "review", "digest", confirm)?,
            )
        }
    };
    api.call(Method::POST, path, Some(body)).await
}
