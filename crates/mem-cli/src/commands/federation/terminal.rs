// SPDX-License-Identifier: AGPL-3.0-or-later
//! Focused, keyboard-only federation workspace. Nothing shells out or persists
//! imported content; confirmation previews the exact next HTTP body.
use super::*;
use crossterm::{
    event::{self, Event, KeyCode, KeyEventKind, KeyModifiers},
    execute,
    terminal::{EnterAlternateScreen, LeaveAlternateScreen, disable_raw_mode, enable_raw_mode},
};
use ratatui::{
    Terminal,
    backend::CrosstermBackend,
    layout::{Constraint, Layout},
    widgets::{Block, Paragraph, Wrap},
};

const OPERATIONS: &[(&str, &[&str])] = &[
    ("Status", &[]),
    ("Catalog", &["offset"]),
    ("Local grant", &["principal", "capabilities", "enabled"]),
    ("Member capability", &["member", "capability", "enabled"]),
    ("Revoke member", &["member"]),
    (
        "Review draft",
        &[
            "canonical",
            "public label",
            "selected evidence text",
            "observed at",
        ],
    ),
    ("Publish reviewed draft", &[]),
    ("Shared context", &["origin", "canonical"]),
    (
        "Propose correction",
        &["corrected text", "corrected summary"],
    ),
    ("Inspect proposal", &["publisher", "record"]),
    ("Resolve proposal", &["approve"]),
    ("Pause subscription", &["canonical"]),
    ("Synchronize", &["operation"]),
    (
        "Discover expertise",
        &["after publisher (optional)", "after record (optional)"],
    ),
    (
        "Prepare remote question",
        &["publisher", "collection", "exact question"],
    ),
    ("Send reviewed question", &[]),
    ("Expertise result", &["request ID"]),
    (
        "List expertise collections",
        &["after collection (optional)"],
    ),
    ("Inspect expertise collection", &["collection"]),
    ("Prepare collection review", &["blueprint JSON"]),
    ("Activate reviewed collection", &[]),
    ("Withdraw inspected collection", &[]),
    (
        "Inspect collection service grant",
        &["collection", "principal"],
    ),
    (
        "Change collection service grant",
        &["collection", "principal", "enabled"],
    ),
];
struct Workspace {
    operation: usize,
    field: usize,
    fields: Vec<String>,
    request_id: Uuid,
    output: String,
    scroll: u16,
    draft: Option<Value>,
    context: Option<Value>,
    proposal: Option<Value>,
    prepared: Option<(String, Value)>,
    question_review: Option<Value>,
    collection_review: Option<Value>,
    collection_detail: Option<Value>,
}
impl Workspace {
    fn new(discovery: &Value) -> Self {
        Self {
            operation: 0,
            field: 0,
            fields: vec![],
            request_id: Uuid::new_v4(),
            output: serde_json::to_string_pretty(discovery).unwrap_or_default(),
            scroll: 0,
            draft: None,
            context: None,
            proposal: None,
            prepared: None,
            question_review: None,
            collection_review: None,
            collection_detail: None,
        }
    }
    fn select(&mut self, operation: usize) {
        self.operation = operation;
        self.field = 0;
        self.prepared = None;
        self.scroll = 0;
        self.request_id = Uuid::new_v4();
        self.fields = OPERATIONS[operation]
            .1
            .iter()
            .map(|name| match *name {
                "offset" => "0".into(),
                "enabled" | "approve" => "true".into(),
                "capabilities" => "pull,publish".into(),
                "capability" => "pull".into(),
                "observed at" => chrono::Utc::now().to_rfc3339(),
                "operation" => "catch_up".into(),
                _ => String::new(),
            })
            .collect();
        if operation == 8
            && let Some(context) = &self.context
            && let Ok((_, content)) = selected(context)
        {
            self.fields = vec![
                content["text"].as_str().unwrap_or_default().into(),
                content["summary"].as_str().unwrap_or_default().into(),
            ];
        }
    }
    fn value(&self, i: usize) -> &str {
        self.fields.get(i).map(String::as_str).unwrap_or_default()
    }
    fn id(&self, i: usize) -> Result<Uuid> {
        Uuid::parse_str(self.value(i)).context("enter a valid UUID")
    }
    fn bool(&self, i: usize) -> Result<bool> {
        self.value(i).parse().context("enter true or false")
    }
    fn request(&self) -> Result<(Method, String, Option<Value>, bool)> {
        let action = |action| {
            Ok((
                Method::POST,
                "/actions".into(),
                Some(json!({"request_id":self.request_id,"action":action})),
                true,
            ))
        };
        match self.operation {
            0 => Ok((Method::GET, "".into(), None, false)),
            1 => Ok((
                Method::GET,
                format!("/catalog?offset={}", self.value(0).parse::<u32>()?),
                None,
                false,
            )),
            2 => Ok((
                Method::POST,
                "/grants".into(),
                Some(
                    json!({"principal":self.id(0)?,"capabilities":self.value(1).split(',').map(str::trim).filter(|s|!s.is_empty()).map(terminal_capability).collect::<Result<Vec<_>>>()?,"enabled":self.bool(2)?}),
                ),
                true,
            )),
            3 => action(
                json!({"kind":"membership","member":self.id(0)?,"capability":terminal_capability(self.value(1))?,"enabled":self.bool(2)?}),
            ),
            4 => action(json!({"kind":"revoke","member":self.id(0)?})),
            5 => Ok((
                Method::POST,
                "/review-preview".into(),
                Some(
                    json!({"canonical":self.id(0)?,"evidence":if self.value(2).is_empty(){json!([])}else{json!([{"public_source_label":self.value(1),"selected_text":self.value(2),"observed_at":self.value(3)}])}}),
                ),
                false,
            )),
            6 => Ok((
                Method::POST,
                "/reviews".into(),
                Some(confirmed_review(
                    self.draft
                        .clone()
                        .context("prepare and inspect a review draft first")?,
                    true,
                )?),
                true,
            )),
            7 => Ok((
                Method::POST,
                "/context".into(),
                Some(json!({"streams":[{"origin":self.id(0)?,"canonical":self.id(1)?}]})),
                false,
            )),
            8 => {
                let (citation, mut content) = selected(
                    self.context
                        .as_ref()
                        .context("retrieve the target shared context first")?,
                )?;
                content["text"] = json!(self.value(0));
                content["summary"] = json!(self.value(1));
                action(
                    json!({"kind":"propose","proposal":{"target":citation["stream"],"base_version":citation["shared_version"],"content":content,"evidence":citation["evidence"]}}),
                )
            }
            9 => Ok((
                Method::GET,
                format!("/proposals/{}/{}", self.id(0)?, self.id(1)?),
                None,
                false,
            )),
            10 => action(
                json!({"kind":"resolve","proposal":self.proposal.as_ref().context("inspect the proposal first")?["key"],"approve":self.bool(0)?}),
            ),
            11 => action(json!({"kind":"pause","canonical":self.id(0)?})),
            12 => Ok((
                Method::POST,
                "/sync".into(),
                Some(json!({"operation":self.value(0)})),
                true,
            )),
            13 => {
                let (p, r) = if self.value(0).is_empty() && self.value(1).is_empty() {
                    (None, None)
                } else {
                    (Some(self.id(0)?), Some(self.id(1)?))
                };
                Ok((
                    Method::GET,
                    expertise::capability_path(p, r, 32),
                    None,
                    false,
                ))
            }
            14 => {
                ensure!(
                    !self.value(2).is_empty() && self.value(2).len() <= 4096,
                    "question must be 1 through 4096 UTF-8 bytes"
                );
                Ok((
                    Method::POST,
                    "/questions/prepare".into(),
                    Some(
                        json!({"request_id":self.request_id,"capability":{"publisher":self.id(0)?,"id":self.id(1)?},"question":self.value(2)}),
                    ),
                    false,
                ))
            }
            15 => Ok((
                Method::POST,
                "/questions/submit".into(),
                Some(expertise::confirmation(
                    self.question_review
                        .clone()
                        .context("prepare and inspect a question first")?,
                    "review",
                    "digest",
                    true,
                )?),
                true,
            )),
            16 => Ok((
                Method::GET,
                format!("/questions/{}", self.id(0)?),
                None,
                false,
            )),
            17 => Ok((
                Method::GET,
                format!(
                    "/collections?limit=32{}",
                    if self.value(0).is_empty() {
                        String::new()
                    } else {
                        format!("&after={}", self.id(0)?)
                    }
                ),
                None,
                false,
            )),
            18 => Ok((
                Method::GET,
                format!("/collections/{}", self.id(0)?),
                None,
                false,
            )),
            19 => Ok((
                Method::POST,
                "/collections/draft".into(),
                Some(
                    serde_json::from_str(self.value(0))
                        .context("enter a CollectionBlueprint JSON object")?,
                ),
                false,
            )),
            20 => Ok((
                Method::POST,
                "/collections/activate".into(),
                Some(expertise::confirmation(
                    self.collection_review
                        .clone()
                        .context("prepare and inspect a collection first")?,
                    "draft",
                    "review_digest",
                    true,
                )?),
                true,
            )),
            21 => {
                let ad = &self
                    .collection_detail
                    .as_ref()
                    .context("inspect the collection first")?["advertisement"];
                Ok((
                    Method::POST,
                    "/collections/withdraw".into(),
                    Some(
                        json!({"request_id":self.request_id,"collection":ad["collection"],"expected_revision":ad["revision"],"confirmed":true}),
                    ),
                    true,
                ))
            }
            22 => Ok((
                Method::GET,
                format!(
                    "/collections/{}/grants?principal={}",
                    self.id(0)?,
                    self.id(1)?
                ),
                None,
                false,
            )),
            23 => Ok((
                Method::POST,
                format!("/collections/{}/grants", self.id(0)?),
                Some(json!({"principal":self.id(1)?,"enabled":self.bool(2)?,"confirmed":true})),
                true,
            )),
            _ => unreachable!(),
        }
    }
    fn accept(&mut self, operation: usize, response: Value) -> Result<()> {
        if operation == 5 {
            self.draft = Some(response.clone());
        }
        if operation == 7 {
            self.context = Some(response.clone());
        }
        if operation == 9 {
            self.proposal = Some(response.clone());
        }
        if operation == 14 {
            self.question_review = Some(response.clone());
        }
        if operation == 19 {
            self.collection_review = Some(response.clone());
        }
        if operation == 18 {
            self.collection_detail = Some(response.clone());
        }
        // Policy mutations are not immutable outbox intents; never suggest
        // replaying an old grant over a newer grant after an uncertain reply.
        if operation == 23 {
            self.prepared = None;
        }
        self.output = serde_json::to_string_pretty(&response)?;
        self.scroll = 0;
        // Keep the exact prepared body and ID for uncertain/repeated submission.
        Ok(())
    }
}
fn selected(context: &Value) -> Result<(Value, Value)> {
    let citation = context["citations"]
        .as_array()
        .and_then(|v| v.first())
        .context("no selected shared citation")?;
    let find = |key: &Value| {
        context["records"]
            .as_array()
            .and_then(|rs| {
                rs.iter().find(|r| {
                    r["envelope"]["publisher"] == key["publisher"]
                        && r["envelope"]["record_id"] == key["id"]
                })
            })
            .map(|r| r["envelope"]["record"].clone())
            .context("missing cited record")
    };
    let mut record = find(&citation["record"])?;
    if record["kind"] == "resolve_proposal" {
        record = find(&record["payload"]["proposal"])?;
    }
    let content = if record["kind"] == "memory_version" {
        record["payload"]["publication"]["content"].clone()
    } else {
        record["payload"]["content"].clone()
    };
    ensure!(content.is_object(), "invalid shared content");
    Ok((citation.clone(), content))
}

struct Restore;
impl Drop for Restore {
    fn drop(&mut self) {
        let _ = disable_raw_mode();
        let _ = execute!(std::io::stdout(), LeaveAlternateScreen);
    }
}
pub(super) async fn run(api: FederationClient, discovery: Value) -> Result<()> {
    enable_raw_mode()?;
    let _restore = Restore;
    execute!(std::io::stdout(), EnterAlternateScreen)?;
    let mut terminal = Terminal::new(CrosstermBackend::new(std::io::stdout()))?;
    let mut workspace = Workspace::new(&discovery);
    let mut pending: Option<tokio::task::JoinHandle<(usize, Result<Value>)>> = None;
    loop {
        if pending.as_ref().is_some_and(|p| p.is_finished()) {
            let (op, response) = pending.take().expect("pending result").await?;
            match response {
                Ok(value) => workspace.accept(op, value)?,
                Err(e) => {
                    if op == 23 {
                        workspace.prepared = None;
                    }
                    workspace.output = format!(
                        "Request failed: {e:#}\n{}",
                        if op == 23 {
                            "Inspect current service grant and deliberately prepare a new policy change."
                        } else {
                            "If delivery is uncertain, retry the SAME prepared body and request ID."
                        }
                    )
                }
            }
        }
        terminal.draw(|frame| {
            let areas=Layout::vertical([Constraint::Length(5),Constraint::Length((workspace.fields.len()+3) as u16),Constraint::Min(4),Constraint::Length(3)]).split(frame.area());
            frame.render_widget(Paragraph::new(format!("Federation — {}\n{}\nUntrusted retained data. No model forwarding. Queued ≠ accepted ≠ published/replicated.\nPlaintext is readable by recipients/admins. Revocation cannot recall copies.",discovery["space"].as_str().unwrap_or_default(),OPERATIONS[workspace.operation].0)).block(Block::bordered()),areas[0]);
            let form=OPERATIONS[workspace.operation].1.iter().zip(&workspace.fields).enumerate().map(|(i,(name,value))|format!("{} {name}: {}",if i==workspace.field{"▶"}else{" "},value.escape_debug())).collect::<Vec<_>>().join("\n");
            frame.render_widget(Paragraph::new(form).wrap(Wrap{trim:false}).block(Block::bordered().title("Tab: next field • type to edit")),areas[1]);
            frame.render_widget(Paragraph::new(workspace.output.as_str()).wrap(Wrap{trim:false}).scroll((workspace.scroll,0)).block(Block::bordered().title(if pending.is_some(){"Waiting for local service"}else{"Response / exact confirmation preview • PgUp/PgDn"})),areas[2]);
            frame.render_widget(Paragraph::new(if matches!(workspace.operation, 2 | 3) {
                "Capabilities: pull, publish, manage-membership, ask-expertise, answer-expertise. Expertise needs v3; no collection consent is granted.\nF5 preview • F6 confirm • F2/F3 operation • Esc quit"
            } else {
                "F2/F3 operation • F5 read/prepare • F6 CONFIRM prepared mutation • Esc quit\nApproval pauses local updates. Publish consent enables future local updates."
            }).wrap(Wrap{trim:false}),areas[3]);
        })?;
        if event::poll(Duration::from_millis(30))? {
            match event::read()? {
                Event::Key(key) if key.kind == KeyEventKind::Press => match key.code {
                    KeyCode::Esc => {
                        if let Some(p) = pending.take() {
                            p.abort();
                        }
                        break;
                    }
                    KeyCode::PageDown => workspace.scroll = workspace.scroll.saturating_add(10),
                    KeyCode::PageUp => workspace.scroll = workspace.scroll.saturating_sub(10),
                    _ if pending.is_some() => {}
                    KeyCode::F(2) => workspace
                        .select((workspace.operation + OPERATIONS.len() - 1) % OPERATIONS.len()),
                    KeyCode::F(3) => workspace.select((workspace.operation + 1) % OPERATIONS.len()),
                    KeyCode::Tab if !workspace.fields.is_empty() => {
                        workspace.field = (workspace.field + 1) % workspace.fields.len()
                    }
                    KeyCode::Backspace if !workspace.fields.is_empty() => {
                        workspace.fields[workspace.field].pop();
                        workspace.prepared = None;
                    }
                    KeyCode::Char('u')
                        if key.modifiers.contains(KeyModifiers::CONTROL)
                            && !workspace.fields.is_empty() =>
                    {
                        workspace.fields[workspace.field].clear();
                        workspace.prepared = None;
                    }
                    KeyCode::Char(c)
                        if !key.modifiers.contains(KeyModifiers::CONTROL)
                            && !workspace.fields.is_empty() =>
                    {
                        if workspace.fields[workspace.field].len() < 48 * 1024 {
                            workspace.fields[workspace.field].push(c);
                            workspace.prepared = None;
                        }
                    }
                    KeyCode::F(5) => match workspace.request() {
                        Ok((method, path, body, confirm)) => {
                            if confirm {
                                let body = body.expect("mutation body");
                                workspace.output = format!(
                                    "Review every disclosed byte and destination before F6. Remote questions may incur repeat work/billing; queued is not answered. Collection activation grants standing disclosure consent.\nPOST {path}\n{}",
                                    serde_json::to_string_pretty(&body)?
                                );
                                workspace.prepared = Some((path, body));
                                workspace.scroll = 0;
                            } else {
                                if workspace.operation == 16 {
                                    workspace.output = "Fetching current expertise result; previous answer cleared.".into();
                                }
                                if workspace.operation == 14 {
                                    workspace.question_review = None;
                                }
                                if workspace.operation == 19 {
                                    workspace.collection_review = None;
                                }
                                if workspace.operation == 18 {
                                    workspace.collection_detail = None;
                                }
                                let api = api.clone();
                                let op = workspace.operation;
                                pending = Some(tokio::spawn(async move {
                                    (op, api.call(method, &path, body).await)
                                }));
                            }
                        }
                        Err(e) => workspace.output = format!("Cannot prepare: {e:#}"),
                    },
                    KeyCode::F(6) => {
                        if let Some((path, body)) = workspace.prepared.clone() {
                            let api = api.clone();
                            let op = workspace.operation;
                            pending = Some(tokio::spawn(async move {
                                (op, api.call(Method::POST, &path, Some(body)).await)
                            }));
                        } else {
                            workspace.output =
                                "Use F5 to prepare and inspect the exact mutation first.".into();
                        }
                    }
                    _ => {}
                },
                _ => {}
            }
        }
        tokio::task::yield_now().await;
    }
    Ok(())
}

#[cfg(test)]
mod tests {
    use super::*;
    #[test]
    fn expertise_forms_normalize_names_and_require_exact_confirmation() {
        let mut state = Workspace::new(&json!({}));
        let member = Uuid::new_v4().to_string();
        for (input, wire) in [
            ("ask-expertise", "ask_expertise"),
            ("answer_expertise", "answer_expertise"),
        ] {
            for enabled in [true, false] {
                state.select(3);
                state.fields = vec![member.clone(), input.into(), enabled.to_string()];
                let (method, path, body, confirm) = state.request().unwrap();
                assert_eq!(method, Method::POST);
                assert_eq!(path, "/actions");
                assert!(confirm);
                assert!(state.prepared.is_none()); // Building a request is not consent.
                assert_eq!(
                    body,
                    Some(json!({"request_id":state.request_id,"action":{
                    "kind":"membership","member":member,"capability":wire,"enabled":enabled}}))
                );
                assert_eq!(state.request().unwrap().2, body);
            }
        }
        state.fields[1] = "invented".into();
        assert!(state.request().is_err());
        state.select(2);
        state.fields = vec![
            member,
            "pull,ask-expertise,answer_expertise".into(),
            "true".into(),
        ];
        let request = state.request().unwrap();
        assert!(request.3);
        assert_eq!(
            request.2.unwrap()["capabilities"],
            json!(["pull", "ask_expertise", "answer_expertise"])
        );
    }
    #[test]
    fn forms_require_selected_review_and_preserve_idempotency() {
        let mut state = Workspace::new(&json!({}));
        state.select(6);
        assert!(state.request().is_err());
        state.draft = Some(
            json!({"request":{"request_id":"stable","evidence":[],"confirm_future_updates":false}}),
        );
        let request = state.request().unwrap();
        assert!(request.3);
        assert_eq!(request.2.unwrap()["request_id"], "stable");
        state.select(10);
        assert!(state.request().is_err());
        state.proposal = Some(json!({"key":{"publisher":"origin","id":"proposal"}}));
        let first = state.request().unwrap();
        let second = state.request().unwrap();
        assert_eq!(first.2, second.2);
        assert_eq!(
            first.2.unwrap()["action"]["proposal"],
            state.proposal.as_ref().unwrap()["key"]
        );
        for (i, _) in OPERATIONS.iter().enumerate() {
            state.select(i);
            assert_eq!(state.fields.len(), OPERATIONS[i].1.len());
        }
    }
}
