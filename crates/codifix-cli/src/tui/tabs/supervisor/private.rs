use crate::{
    commands::{api::ApiClient, supervisor::proposal_lines},
    tui::{
        app::{App, BackgroundEvent},
        theme::themed_block,
    },
};
use codifix_record::{
    SupervisorProposalDetail, SupervisorProposalList, SupervisorReviewDecision,
    SupervisorReviewRequest,
};
use crossterm::event::{KeyCode, KeyEvent};
use ratatui::{
    layout::Rect,
    text::Line,
    widgets::{Paragraph, Wrap},
};
use uuid::Uuid;
mod questions;

#[derive(Default)]
pub(in crate::tui) struct PrivateState {
    questions: questions::QuestionState,
    project: String,
    active: bool,
    busy: bool,
    error: Option<String>,
    page: Option<SupervisorProposalList>,
    detail: Option<SupervisorProposalDetail>,
    pending: Option<SupervisorReviewRequest>,
    selected: usize,
    scroll: u16,
}
pub(in crate::tui) enum PrivateResult {
    Page(SupervisorProposalList),
    Detail(SupervisorProposalDetail),
    Question(questions::QuestionResult),
}
enum Request {
    List(Option<Uuid>),
    Detail(Uuid),
    Review(Uuid, SupervisorReviewRequest),
    Question(questions::QuestionRequest),
}
impl PrivateState {
    pub(in crate::tui) fn consumes_quit(&self, project: &str) -> bool {
        self.active && self.project == project
    }
    pub(in crate::tui) fn receive(&mut self, result: Result<PrivateResult, String>) {
        self.busy = false;
        match result {
            Ok(PrivateResult::Question(result)) => self.questions.receive(result),
            Ok(PrivateResult::Page(p)) => {
                self.page = Some(p);
                self.detail = None;
                self.selected = 0;
                self.scroll = 0;
            }
            Ok(PrivateResult::Detail(d)) => {
                self.pending = None;
                self.detail = Some(d);
                self.scroll = 0;
            }
            Err(e) => {
                self.error = Some(e);
                if self.pending.is_none() {
                    self.detail = None;
                }
            }
        }
    }
}
impl App {
    fn private_request(&mut self, api: &ApiClient, request: Request) {
        let state = &mut self.supervisor.private;
        if state.busy {
            return;
        }
        state.busy = true;
        state.error = None;
        if state.pending.is_none() {
            state.detail = None;
        }
        let api = api.clone();
        let project = self.project.clone();
        let tx = self.background_tx.clone();
        tokio::spawn(async move {
            let result = async {
                match request {
                    Request::List(after) => api
                        .supervisor_proposals(&project, after)
                        .await
                        .map(PrivateResult::Page),
                    Request::Detail(id) => api
                        .supervisor_proposal(&project, id)
                        .await
                        .map(PrivateResult::Detail),
                    Request::Review(id, r) => {
                        api.supervisor_review(&project, id, &r).await?;
                        api.supervisor_proposal(&project, id)
                            .await
                            .map(PrivateResult::Detail)
                    }
                    Request::Question(request) => questions::execute(&api, &project, request)
                        .await
                        .map(PrivateResult::Question),
                }
            }
            .await
            .map_err(|e| e.to_string());
            let _ = tx.send(BackgroundEvent::SupervisorPrivate { project, result });
        });
    }
    pub(in crate::tui) fn supervisor_private_key(
        &mut self,
        key: KeyEvent,
        api: &ApiClient,
    ) -> bool {
        let state = &mut self.supervisor.private;
        if state.project != self.project {
            *state = PrivateState {
                project: self.project.clone(),
                ..Default::default()
            };
        }
        if !state.active {
            if key.code != KeyCode::Char('v') {
                return false;
            }
            state.active = true;
            self.private_request(api, Request::List(None));
            return true;
        }
        if state.busy {
            return true;
        }
        if state.questions.active {
            let request = state.questions.key(key);
            if let Some(request) = request {
                self.private_request(api, request);
            }
            return true;
        }
        if let Some(pending) = state.pending.clone() {
            match key.code {
                KeyCode::Enter => {
                    if let Some(d) = &state.detail {
                        let id = d.action_id;
                        self.private_request(api, Request::Review(id, pending));
                    }
                }
                KeyCode::Char('c') => {
                    state.pending = None;
                    state.detail = None;
                    self.private_request(api, Request::List(None));
                }
                _ => {}
            }
            return true;
        }
        match key.code {
            KeyCode::Char('q') => {
                state.questions.active = true;
                self.private_request(
                    api,
                    Request::Question(questions::QuestionRequest::List(None)),
                );
            }
            KeyCode::Esc => {
                *state = PrivateState::default();
            }
            KeyCode::Char('r') => self.private_request(api, Request::List(None)),
            KeyCode::PageDown => {
                if let Some(after) = state.page.as_ref().and_then(|p| p.next_after) {
                    self.private_request(api, Request::List(Some(after)));
                }
            }
            KeyCode::Down | KeyCode::Char('j') => {
                if state.detail.is_some() {
                    state.scroll = state.scroll.saturating_add(1);
                } else {
                    state.selected = state.selected.saturating_add(1).min(
                        state
                            .page
                            .as_ref()
                            .map_or(0, |p| p.actions.len().saturating_sub(1)),
                    );
                }
            }
            KeyCode::Up | KeyCode::Char('k') => {
                if state.detail.is_some() {
                    state.scroll = state.scroll.saturating_sub(1);
                } else {
                    state.selected = state.selected.saturating_sub(1);
                }
            }
            KeyCode::Enter => {
                if let Some(id) = state
                    .page
                    .as_ref()
                    .and_then(|p| p.actions.get(state.selected))
                    .copied()
                {
                    self.private_request(api, Request::Detail(id));
                }
            }
            KeyCode::Char('a' | 'x') => {
                if let Some(d) = &state.detail
                    && d.can_review
                    && d.review.is_none()
                {
                    state.pending = Some(SupervisorReviewRequest {
                        request_id: Uuid::new_v4(),
                        proposal_digest: d.proposal_digest.clone(),
                        decision: if key.code == KeyCode::Char('a') {
                            SupervisorReviewDecision::Approve
                        } else {
                            SupervisorReviewDecision::Reject
                        },
                    });
                    state.scroll = 0;
                }
            }
            _ => {}
        }
        true
    }
}
pub(in crate::tui) fn draw(frame: &mut ratatui::Frame<'_>, app: &App, area: Rect) -> bool {
    let state = &app.supervisor.private;
    if !state.active || state.project != app.project {
        return false;
    }
    if state.questions.active {
        questions::draw(
            frame,
            &state.questions,
            &app.project,
            state.busy,
            state.error.as_deref(),
            area,
        );
        return true;
    }
    let mut lines = vec![format!(
        "Private project: {} | q: questions | r: list | Enter: open | Esc: status | j/k: scroll/select | PgDn: next page",
        app.project
    )];
    if let Some(r) = &state.pending {
        lines.push(format!(
            "CONFIRM {:?} for digest {} | request {}",
            r.decision, r.proposal_digest, r.request_id
        ));
        lines.push("Enter confirms/retries exact terms; c closes and inspects. Permanent human review only; no action executes.".into());
    }
    if state.busy {
        lines.push("Loading private result…".into());
    }
    if let Some(e) = &state.error {
        lines.push(e.clone());
    }
    if let Some(d) = &state.detail {
        if d.can_review {
            lines.push("a: prepare approve | x: prepare reject | confirmation required".into());
        }
        lines.extend(proposal_lines(d));
    } else if let Some(p) = &state.page {
        if p.actions.is_empty() {
            lines.push("No saved proposals on this page.".into());
        }
        for (i, id) in p.actions.iter().enumerate() {
            lines.push(format!(
                "{} {id}",
                if i == state.selected { ">" } else { " " }
            ));
        }
    }
    frame.render_widget(
        Paragraph::new(lines.into_iter().map(Line::from).collect::<Vec<_>>())
            .block(themed_block("Private supervisor proposals"))
            .wrap(Wrap { trim: false })
            .scroll((state.scroll, 0)),
        area,
    );
    true
}
