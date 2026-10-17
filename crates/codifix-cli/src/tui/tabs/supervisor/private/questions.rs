// SPDX-License-Identifier: AGPL-3.0-or-later
use super::Request;
use crate::{
    commands::{
        api::ApiClient,
        supervisor::questions::{preview_lines, terminal_text},
    },
    tui::theme::themed_block,
};
use codifix_record::{
    SupervisorQuestionList, SupervisorQuestionPreview, SupervisorQuestionRequest,
    SupervisorQuestionStatus,
};
use crossterm::event::{KeyCode, KeyEvent, KeyModifiers};
use ratatui::{
    layout::Rect,
    text::Line,
    widgets::{Paragraph, Wrap},
};
use uuid::Uuid;

#[derive(Default)]
pub(super) struct QuestionState {
    pub active: bool,
    input: Option<String>,
    preview: Option<SupervisorQuestionPreview>,
    pending: Option<SupervisorQuestionRequest>,
    attempted: bool,
    page: Option<SupervisorQuestionList>,
    status: Option<SupervisorQuestionStatus>,
    selected: usize,
    scroll: u16,
}
pub(in crate::tui) enum QuestionResult {
    Preview(SupervisorQuestionPreview),
    Status(SupervisorQuestionStatus),
    Page(SupervisorQuestionList),
}
pub(super) enum QuestionRequest {
    Prepare(String),
    Submit(SupervisorQuestionRequest),
    Status(Uuid),
    List(Option<Uuid>),
}
pub(super) async fn execute(
    api: &ApiClient,
    project: &str,
    request: QuestionRequest,
) -> anyhow::Result<QuestionResult> {
    match request {
        QuestionRequest::Prepare(question) => api
            .supervisor_question_prepare(project, &question)
            .await
            .map(QuestionResult::Preview),
        QuestionRequest::Submit(request) => api
            .supervisor_question_submit(project, &request)
            .await
            .map(QuestionResult::Status),
        QuestionRequest::Status(id) => api
            .supervisor_question(project, id)
            .await
            .map(QuestionResult::Status),
        QuestionRequest::List(after) => api
            .supervisor_questions(project, after)
            .await
            .map(QuestionResult::Page),
    }
}
impl QuestionState {
    pub(super) fn receive(&mut self, result: QuestionResult) {
        match result {
            QuestionResult::Preview(p) => {
                self.pending = Some(SupervisorQuestionRequest {
                    request_id: Uuid::new_v4(),
                    question: p.question.clone(),
                    profile_digest: p.profile_digest.clone(),
                    request_digest: p.request_digest.clone(),
                });
                self.preview = Some(p);
                self.input = None;
                self.attempted = false;
            }
            QuestionResult::Status(s) => {
                self.status = Some(s);
                self.pending = None;
                self.preview = None;
                self.input = None;
            }
            QuestionResult::Page(p) => {
                self.page = Some(p);
                self.status = None;
                self.selected = 0;
            }
        }
        self.scroll = 0;
    }
    pub(super) fn key(&mut self, key: KeyEvent) -> Option<Request> {
        if let Some(input) = &mut self.input {
            match key.code {
                KeyCode::Esc => self.input = None,
                KeyCode::Backspace => {
                    input.pop();
                }
                KeyCode::Enter if !input.trim().is_empty() => {
                    return Some(Request::Question(QuestionRequest::Prepare(input.clone())));
                }
                KeyCode::Char(c)
                    if !key.modifiers.contains(KeyModifiers::CONTROL)
                        && !c.is_control()
                        && input.len() + c.len_utf8() <= 4096 =>
                {
                    input.push(c)
                }
                _ => {}
            }
            return None;
        }
        if let Some(pending) = &self.pending {
            return match key.code {
                KeyCode::Enter => {
                    self.attempted = true;
                    Some(Request::Question(QuestionRequest::Submit(pending.clone())))
                }
                KeyCode::Char('r') if self.attempted => Some(Request::Question(
                    QuestionRequest::Status(pending.request_id),
                )),
                KeyCode::Esc if !self.attempted => {
                    self.pending = None;
                    self.preview = None;
                    None
                }
                KeyCode::Down => {
                    self.scroll = self.scroll.saturating_add(1);
                    None
                }
                KeyCode::Up => {
                    self.scroll = self.scroll.saturating_sub(1);
                    None
                }
                _ => None,
            };
        }
        match key.code {
            KeyCode::Esc => self.active = false,
            KeyCode::Char('n') => {
                self.input = Some(String::new());
                self.status = None;
                self.scroll = 0;
            }
            KeyCode::Char('r') => {
                return Some(Request::Question(match &self.status {
                    Some(s) => QuestionRequest::Status(s.request_id),
                    None => QuestionRequest::List(None),
                }));
            }
            KeyCode::Char('l') => return Some(Request::Question(QuestionRequest::List(None))),
            KeyCode::PageDown => {
                if let Some(after) = self.page.as_ref().and_then(|p| p.next_after) {
                    return Some(Request::Question(QuestionRequest::List(Some(after))));
                }
            }
            KeyCode::Down | KeyCode::Char('j') => {
                self.selected = self.selected.saturating_add(1).min(
                    self.page
                        .as_ref()
                        .map_or(0, |p| p.questions.len().saturating_sub(1)),
                );
                self.scroll = self.scroll.saturating_add(1);
            }
            KeyCode::Up | KeyCode::Char('k') => {
                self.selected = self.selected.saturating_sub(1);
                self.scroll = self.scroll.saturating_sub(1);
            }
            KeyCode::Enter => {
                if let Some(s) = &self.status
                    && s.proposal_available
                {
                    self.active = false;
                    return Some(Request::Detail(s.action_id));
                }
                if let Some(s) = self
                    .page
                    .as_ref()
                    .and_then(|p| p.questions.get(self.selected))
                {
                    return Some(Request::Question(QuestionRequest::Status(s.request_id)));
                }
            }
            _ => {}
        }
        None
    }
}
pub(super) fn draw(
    frame: &mut ratatui::Frame<'_>,
    state: &QuestionState,
    project: &str,
    busy: bool,
    error: Option<&str>,
    area: Rect,
) {
    let mut lines = vec![format!(
        "Private questions · {project} | n: new | r: refresh | l: list | Enter: inspect/answer | Esc: proposals"
    )];
    if let Some(input) = &state.input {
        lines.push("Exact question: Enter prepares for review (no call); Esc cancels. 4096 UTF-8 bytes maximum.".into());
        lines.push(format!(
            "{} / 4096 bytes: {}",
            input.len(),
            serde_json::to_string(input).unwrap_or_default()
        ));
    }
    if let (Some(preview), Some(request)) = (&state.preview, &state.pending) {
        lines.extend(preview_lines(preview, request.request_id));
        lines.push(if state.attempted { "Unconfirmed: Enter retries EXACT submission; r inspects this request. No new ID will be generated.".into() } else { "Enter CONFIRMS PAID QUESTION. Esc cancels. Up/down scroll consent.".into() });
    }
    if let Some(s) = &state.status {
        lines.push(format!(
            "Request {}: {} | deadline {}",
            s.request_id, s.state, s.expires_at
        ));
        lines.push("Queued is not answered. r refreshes; no automatic paid retry. A new question may repeat billing after uncertain work.".into());
        if s.proposal_available {
            lines.push("Enter reads the cited saved answer (no new model call).".into());
        }
    } else if let Some(page) = &state.page {
        for (index, s) in page.questions.iter().enumerate() {
            lines.push(format!(
                "{} {} · {}",
                if index == state.selected { ">" } else { " " },
                s.request_id,
                s.state
            ));
        }
    }
    if busy {
        lines.push("Checking private question…".into());
    }
    if let Some(error) = error {
        lines.push(error.into());
    }
    frame.render_widget(
        Paragraph::new(
            lines
                .into_iter()
                .map(|line| Line::from(terminal_text(&line)))
                .collect::<Vec<_>>(),
        )
        .block(themed_block("Ask supervisor · explicit paid consent"))
        .wrap(Wrap { trim: false })
        .scroll((state.scroll, 0)),
        area,
    );
}

#[cfg(test)]
mod tests {
    use super::*;
    #[test]
    fn supervisor_question_confirmation_retains_identity_after_uncertain_submission() {
        let mut state = QuestionState::default();
        state.receive(QuestionResult::Preview(SupervisorQuestionPreview {
            question: "Why?".into(),
            profile_digest: "a".repeat(64),
            request_digest: "b".repeat(64),
            model: "gpt-5.6-sol".into(),
            destination: "OpenAI".into(),
            source_count: 3,
            daily_call_limit: 20,
            daily_usd_micros: "2500000".into(),
        }));
        let id = state.pending.as_ref().unwrap().request_id;
        assert!(
            matches!(state.key(KeyEvent::new(KeyCode::Enter, KeyModifiers::NONE)), Some(Request::Question(QuestionRequest::Submit(r))) if r.request_id == id)
        );
        state.key(KeyEvent::new(KeyCode::Esc, KeyModifiers::NONE));
        state.key(KeyEvent::new(KeyCode::Char('n'), KeyModifiers::NONE));
        assert_eq!(state.pending.as_ref().unwrap().request_id, id);
        assert!(
            matches!(state.key(KeyEvent::new(KeyCode::Char('r'), KeyModifiers::NONE)), Some(Request::Question(QuestionRequest::Status(actual))) if actual == id)
        );
    }
    #[test]
    fn supervisor_question_input_bounds_utf8_and_q_is_input() {
        let mut state = QuestionState {
            input: Some("é".repeat(2047)),
            ..Default::default()
        };
        state.key(KeyEvent::new(KeyCode::Char('q'), KeyModifiers::NONE));
        state.key(KeyEvent::new(KeyCode::Char('é'), KeyModifiers::NONE));
        assert_eq!(state.input.as_ref().unwrap().len(), 4095);
        state.key(KeyEvent::new(KeyCode::Char('q'), KeyModifiers::NONE));
        assert_eq!(state.input.as_ref().unwrap().len(), 4096);
    }
}
