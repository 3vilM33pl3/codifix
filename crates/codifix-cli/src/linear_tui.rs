// SPDX-License-Identifier: AGPL-3.0-or-later
mod client;

use self::client::Agency;
use anyhow::{Context, Result};
use crossterm::event::{self, Event, KeyCode, KeyEventKind, KeyModifiers};
use ratatui::{
    Frame,
    layout::{Constraint, Layout},
    style::{Color, Style},
    widgets::{Block, Paragraph, Wrap},
};
use serde_json::Value;
use std::{
    path::Path,
    time::{Duration, Instant},
};
use uuid::Uuid;

#[derive(Default)]
struct App {
    title: String,
    description: String,
    description_active: bool,
    phase: Phase,
    correction: Option<(Uuid, i64)>,
    message: String,
    scroll: u16,
}

#[derive(Default)]
enum Phase {
    #[default]
    Draft,
    Submitting(Submission),
    Waiting(Uuid),
    Review {
        id: Uuid,
        revision: i64,
        question: String,
        expected: String,
        answer: String,
    },
    Finished(String),
    Stopped(String),
}

enum Submission {
    Draft(Value),
    Correction {
        id: Uuid,
        revision: i64,
        payload: Value,
    },
}

pub(super) async fn run(path: &Path) -> Result<()> {
    let agency = Agency::open(path)?;
    let mut terminal = ratatui::try_init()?;
    let result = run_ui(&mut terminal, &agency).await;
    ratatui::try_restore()?;
    result
}

async fn run_ui(terminal: &mut ratatui::DefaultTerminal, agency: &Agency) -> Result<()> {
    let mut app = App::default();
    let mut next_poll = Instant::now();
    loop {
        terminal.draw(|frame| draw(frame, &app))?;
        if let Phase::Waiting(id) = app.phase
            && Instant::now() >= next_poll
        {
            match refresh(agency, id, &app.title, &app.description).await {
                Ok(phase) => {
                    app.phase = phase;
                    app.message.clear();
                }
                Err(e) => app.message = format!("Status unavailable. Retrying. {e}"),
            }
            next_poll = Instant::now() + Duration::from_secs(1);
            continue;
        }
        if !event::poll(Duration::from_millis(125))? {
            continue;
        }
        let Event::Key(key) = event::read()? else {
            continue;
        };
        if key.kind == KeyEventKind::Release {
            continue;
        }
        let ctrl = key.modifiers.contains(KeyModifiers::CONTROL);
        if key.code == KeyCode::Esc || (ctrl && key.code == KeyCode::Char('c')) {
            return Ok(());
        }
        match key.code {
            KeyCode::PageDown => {
                app.scroll = app.scroll.saturating_add(5);
                continue;
            }
            KeyCode::PageUp => {
                app.scroll = app.scroll.saturating_sub(5);
                continue;
            }
            _ => {}
        }
        match &mut app.phase {
            Phase::Draft => {
                if key.code == KeyCode::Tab || key.code == KeyCode::BackTab {
                    app.description_active = !app.description_active;
                } else if ctrl && key.code == KeyCode::Char('s') {
                    app.title = app.title.trim().into();
                    app.description = app.description.trim().into();
                    if app.title.is_empty() || app.description.is_empty() {
                        app.message = "Enter a title and description.".into();
                    } else {
                        let submission = match app.correction.take() {
                            Some((id, revision)) => Submission::Correction {
                                id,
                                revision,
                                payload: serde_json::json!({
                                    "kind": "linear_track",
                                    "title": app.title,
                                    "description": app.description,
                                }),
                            },
                            None => Submission::Draft(agency.draft(&app.title, &app.description)),
                        };
                        app.phase = Phase::Submitting(submission);
                        submit(agency, &mut app).await;
                        next_poll = Instant::now();
                    }
                } else if key.code == KeyCode::Enter && !app.description_active {
                    app.description_active = true;
                } else if app.description_active {
                    edit(&mut app.description, key.code, ctrl, true, 16384);
                } else {
                    edit(&mut app.title, key.code, ctrl, false, 512);
                }
            }
            Phase::Submitting(_) if key.code == KeyCode::Enter => submit(agency, &mut app).await,
            Phase::Review {
                id,
                revision,
                answer,
                expected,
                ..
            } => {
                if ctrl && key.code == KeyCode::Char('e') {
                    app.correction = Some((*id, *revision));
                    app.phase = Phase::Draft;
                    app.message.clear();
                    app.scroll = 0;
                } else if key.code == KeyCode::Enter {
                    if answer != expected {
                        app.message = "Confirmation must match the proposal exactly.".into();
                    } else {
                        match agency.answer(*id, *revision, answer, None).await {
                            Ok(()) => {
                                app.phase = Phase::Waiting(*id);
                                app.scroll = 0;
                                app.message.clear();
                                next_poll = Instant::now();
                            }
                            Err(e) => {
                                app.message = format!(
                                    "Approval was not acknowledged. Enter retries the same response. {e}"
                                )
                            }
                        }
                    }
                } else {
                    edit(answer, key.code, ctrl, false, 128);
                }
            }
            Phase::Finished(_) | Phase::Stopped(_) if key.code == KeyCode::Enter => {
                app = App::default()
            }
            _ => {}
        }
    }
}

fn edit(s: &mut String, key: KeyCode, ctrl: bool, multiline: bool, max: usize) {
    match key {
        KeyCode::Backspace => {
            s.pop();
        }
        KeyCode::Char('u') if ctrl => s.clear(),
        KeyCode::Enter if multiline && s.len() < max => s.push('\n'),
        KeyCode::Char(ch) if !ctrl && !ch.is_control() && s.len() + ch.len_utf8() <= max => {
            s.push(ch)
        }
        _ => {}
    }
}

async fn submit(agency: &Agency, app: &mut App) {
    let Phase::Submitting(submission) = &app.phase else {
        return;
    };
    let result = match submission {
        Submission::Draft(body) => agency.submit(body).await,
        Submission::Correction {
            id,
            revision,
            payload,
        } => agency
            .answer(*id, *revision, "Correct the wording", Some(payload))
            .await
            .map(|()| *id),
    };
    match result {
        Ok(id) => {
            app.phase = Phase::Waiting(id);
            app.message.clear();
            app.scroll = 0;
        }
        Err(e) => {
            app.message =
                format!("Submission was not acknowledged. Enter retries the same request. {e}")
        }
    }
}

async fn refresh(agency: &Agency, id: Uuid, title: &str, description: &str) -> Result<Phase> {
    let task = agency.task(id).await?;
    let contract = &task["contract"];
    Ok(
        match contract["status"].as_str().context("missing task status")? {
            "completed" => Phase::Finished(agency.issue_url(id).await?),
            "input_required" => {
                let question = contract["clarification"]["question"]
                    .as_str()
                    .context("missing clarification")?;
                match confirmation(question, title, description) {
                    Some(token) => Phase::Review {
                        id,
                        revision: task["revision"].as_i64().context("missing revision")?,
                        question: question.into(),
                        expected: token.into(),
                        answer: token.into(),
                    },
                    None => Phase::Stopped(format!("Companion needs attention\n\n{question}")),
                }
            }
            "submitted" | "working" => Phase::Waiting(id),
            status => Phase::Stopped(format!("Task {id}\nStatus: {status}")),
        },
    )
}

fn confirmation<'a>(question: &'a str, title: &str, description: &str) -> Option<&'a str> {
    let (issue, wording) = question
        .strip_prefix("Proposed Linear issue ")?
        .split_once("\n\n")?;
    Uuid::parse_str(issue).ok()?;
    let token = wording
        .strip_prefix(&format!("{title}\n\n{description}\n\nReply with `"))?
        .strip_suffix("` or supply corrected wording.")?;
    let digest = token.strip_prefix("confirm ")?;
    (digest.len() == 64
        && digest
            .bytes()
            .all(|b| b.is_ascii_digit() || (b'a'..=b'f').contains(&b)))
    .then_some(token)
}

fn draw(frame: &mut Frame<'_>, app: &App) {
    let areas = Layout::vertical([
        Constraint::Length(3),
        Constraint::Min(3),
        Constraint::Length(4),
        Constraint::Length(if app.message.is_empty() { 0 } else { 2 }),
        Constraint::Length(2),
    ])
    .split(frame.area());
    let (heading, body, field, input, keys) = match &app.phase {
        Phase::Draft => (
            if app.correction.is_some() {
                "Correct Linear proposal"
            } else {
                "Draft Linear issue"
            },
            app.description.as_str(),
            "Title",
            app.title.as_str(),
            "Tab switch field   Ctrl+S submit   Ctrl+U clear field   Esc quit",
        ),
        Phase::Review {
            question, answer, ..
        } => (
            "Review Linear proposal",
            question.as_str(),
            "Confirmation",
            answer.as_str(),
            "Enter approve   Ctrl+E correct wording   PgUp/PgDn scroll   Esc quit",
        ),
        Phase::Waiting(_) => (
            "Waiting for the Linear specialist",
            app.title.as_str(),
            "",
            "",
            "Status refreshes automatically   Esc quit",
        ),
        Phase::Submitting(_) => (
            "Submit Linear issue",
            app.title.as_str(),
            "",
            "",
            "Enter retry   Esc quit",
        ),
        Phase::Finished(url) => (
            "Verified Linear issue",
            url.as_str(),
            "",
            "",
            "Enter new issue   Esc quit",
        ),
        Phase::Stopped(msg) => (
            "Linear task stopped",
            msg.as_str(),
            "",
            "",
            "Enter new issue   Esc quit",
        ),
    };
    frame.render_widget(Paragraph::new(heading).block(Block::bordered()), areas[0]);
    let draft = matches!(app.phase, Phase::Draft);
    let (body_area, input_area) = if draft {
        let fields = Layout::vertical([Constraint::Length(3), Constraint::Min(3)])
            .split(areas[1].union(areas[2]));
        (fields[1], fields[0])
    } else {
        (areas[1], areas[2])
    };
    let focus = Style::default().fg(Color::Cyan);
    let mut block = Block::bordered();
    if draft {
        block = block.title("Description");
        if app.description_active {
            block = block.border_style(focus);
        }
    }
    frame.render_widget(
        Paragraph::new(body)
            .wrap(Wrap { trim: false })
            .scroll((app.scroll, 0))
            .block(block),
        body_area,
    );
    if !field.is_empty() {
        let mut block = Block::bordered().title(field);
        if !draft || !app.description_active {
            block = block.border_style(focus);
        }
        frame.render_widget(
            Paragraph::new(input)
                .wrap(Wrap { trim: false })
                .block(block),
            input_area,
        );
    }
    frame.render_widget(
        Paragraph::new(app.message.as_str()).wrap(Wrap { trim: false }),
        areas[3],
    );
    frame.render_widget(Paragraph::new(keys).wrap(Wrap { trim: false }), areas[4]);
}

#[cfg(test)]
mod tests {
    use super::*;
    #[test]
    fn consent_requires_the_exact_submitted_wording_and_final_token() {
        let token = format!("confirm {}", "a".repeat(64));
        let title = "Fix `retry`";
        let description = "First paragraph\n\nSecond paragraph";
        let question = format!(
            "Proposed Linear issue {}\n\n{title}\n\n{description}\n\nReply with `{token}` or supply corrected wording.",
            Uuid::new_v4()
        );
        assert_eq!(
            confirmation(&question, title, description),
            Some(token.as_str())
        );
        assert_eq!(confirmation(&question, title, "different"), None);
    }
}
