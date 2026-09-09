// SPDX-License-Identifier: AGPL-3.0-or-later
use crate::{
    commands::api::ApiClient,
    tui::{
        app::{App, BackgroundEvent},
        theme::themed_block,
    },
};
use crossterm::event::{KeyCode, KeyEvent};
use mem_record::{JobDetail, JobStatus, ProjectJobsResponse};
use ratatui::{
    layout::Rect,
    text::Line,
    widgets::{Paragraph, Wrap},
};
use uuid::Uuid;

#[derive(Default)]
pub(in crate::tui) struct JobsState {
    pub page: Option<ProjectJobsResponse>,
    pub detail: Option<JobDetail>,
    pub pending: Option<(Uuid, Uuid)>,
    pub busy: bool,
    pub error: Option<String>,
    pub message: Option<String>,
    selected: usize,
    scroll: u16,
    status: Option<JobStatus>,
}
pub(in crate::tui) enum JobsResult {
    Page(ProjectJobsResponse),
    Detail(JobDetail),
    Retried(mem_record::JobRetryResponse),
}
enum Request {
    List(Option<Uuid>),
    Detail(Uuid),
    Retry(Uuid, Uuid),
}
impl JobsState {
    pub(in crate::tui) fn receive(&mut self, result: Result<JobsResult, String>) {
        self.busy = false;
        match result {
            Err(e) => self.error = Some(e),
            Ok(JobsResult::Page(page)) => {
                self.page = Some(page);
                self.selected = 0;
                self.detail = None;
                self.scroll = 0;
            }
            Ok(JobsResult::Detail(detail)) => {
                self.detail = Some(detail);
                self.scroll = 0;
            }
            Ok(JobsResult::Retried(r)) => {
                self.message = Some(format!(
                    "Retry accepted: {}, generation {}, request {}. Refresh to observe execution.",
                    r.job_id, r.generation, r.request_id
                ));
                self.pending = None;
                self.detail = None;
                self.page = None;
            }
        }
    }
}
impl App {
    fn request_jobs(&mut self, api: &ApiClient, request: Request) {
        if self.jobs.busy {
            return;
        }
        self.jobs.busy = true;
        self.jobs.error = None;
        self.jobs.message = None;
        let api = api.clone();
        let project = self.project.clone();
        let tx = self.background_tx.clone();
        let status = self.jobs.status;
        tokio::spawn(async move {
            let result = match request {
                Request::List(after) => api
                    .jobs(&project, status, after, 50)
                    .await
                    .map(JobsResult::Page),
                Request::Detail(id) => api.job(&project, id).await.map(JobsResult::Detail),
                Request::Retry(id, request) => api
                    .retry_job(&project, id, request)
                    .await
                    .map(JobsResult::Retried),
            }
            .map_err(|e| e.to_string());
            let _ = tx.send(BackgroundEvent::Jobs { project, result });
        });
    }
    pub(in crate::tui) fn jobs_key(&mut self, key: KeyEvent, api: &ApiClient) -> bool {
        if self.jobs.pending.is_some() {
            match key.code {
                KeyCode::Char('y') if !self.jobs.busy => {
                    if self.jobs.page.as_ref().is_some_and(|p| p.read_only) {
                        return true;
                    }
                    let (id, request) = self.jobs.pending.expect("checked pending");
                    self.request_jobs(api, Request::Retry(id, request));
                }
                KeyCode::Char('n') | KeyCode::Esc if !self.jobs.busy => self.jobs.pending = None,
                _ => {}
            }
            return true;
        }
        match key.code {
            KeyCode::Char('r') => self.request_jobs(api, Request::List(None)),
            KeyCode::PageDown => {
                if let Some(after) = self.jobs.page.as_ref().and_then(|p| p.page.next_after) {
                    self.request_jobs(api, Request::List(Some(after)));
                }
            }
            KeyCode::Char('s') if !self.jobs.busy => {
                self.jobs.status = match self.jobs.status {
                    None => Some(JobStatus::Queued),
                    Some(JobStatus::Queued) => Some(JobStatus::Running),
                    Some(JobStatus::Running) => Some(JobStatus::RetryWait),
                    Some(JobStatus::RetryWait) => Some(JobStatus::Blocked),
                    Some(JobStatus::Blocked) => Some(JobStatus::Failed),
                    Some(JobStatus::Failed) => Some(JobStatus::Succeeded),
                    Some(JobStatus::Succeeded) => Some(JobStatus::Skipped),
                    Some(JobStatus::Skipped) => None,
                };
                self.request_jobs(api, Request::List(None));
            }
            KeyCode::Down | KeyCode::Char('j') => {
                let len = self.jobs.page.as_ref().map_or(0, |p| p.page.jobs.len());
                self.jobs.selected = (self.jobs.selected + 1).min(len.saturating_sub(1));
                self.jobs.detail = None;
            }
            KeyCode::Up | KeyCode::Char('k') => {
                self.jobs.selected = self.jobs.selected.saturating_sub(1);
                self.jobs.detail = None;
            }
            KeyCode::Enter => {
                if let Some(id) = self
                    .jobs
                    .page
                    .as_ref()
                    .and_then(|p| p.page.jobs.get(self.jobs.selected))
                    .map(|j| j.id)
                {
                    self.request_jobs(api, Request::Detail(id));
                }
            }
            KeyCode::Char('R') if !self.jobs.busy => {
                if !self.jobs.page.as_ref().is_some_and(|p| p.read_only)
                    && let Some(detail) = &self.jobs.detail
                    && matches!(detail.job.status, JobStatus::Blocked | JobStatus::Failed)
                {
                    self.jobs.pending = Some((detail.job.id, Uuid::new_v4()));
                }
            }
            KeyCode::Char('d') => self.jobs.scroll = self.jobs.scroll.saturating_add(8),
            KeyCode::PageUp => self.jobs.scroll = self.jobs.scroll.saturating_sub(8),
            _ => return false,
        }
        true
    }
}
pub(in crate::tui) fn draw(frame: &mut ratatui::Frame<'_>, app: &App, area: Rect) {
    let state = &app.jobs;
    let mut lines = vec![Line::from(format!(
        "Project {} | filter {:?} | r refresh",
        app.project, state.status
    ))];
    if state.busy {
        lines.push(Line::from("Loading…"));
    }
    if let Some(e) = &state.error {
        lines.push(Line::from(format!("Unavailable/denied: {e}")));
    }
    if let Some(message) = &state.message {
        lines.push(Line::from(message.clone()));
    }
    if let Some((id, request)) = state.pending {
        lines.push(Line::from(format!(
            "CONFIRM RETRY {id} | request {request}"
        )));
        lines.push(Line::from(
            "y confirm / n cancel. Inputs, completed stages and history stay unchanged.",
        ));
        lines.push(Line::from("Current authority/policy required; repeat provider billing possible. Reuse this ID after uncertain replies."));
    }
    if let Some(page) = &state.page {
        lines.push(Line::from(format!("Queued {} running {} retry-wait {} blocked {} failed {} | worker {} last poll {:?} | oldest ready {:?}s", page.stats.queued,page.stats.running,page.stats.retry_wait,page.stats.blocked,page.stats.failed,page.worker.enabled,page.worker.last_poll_at,page.stats.oldest_ready_age_seconds)));
        if page.read_only {
            lines.push(Line::from("Read-only service: retry disabled."));
        }
        if page.page.jobs.is_empty() {
            lines.push(Line::from("No jobs match this page and filter."));
        }
        for (i, job) in page
            .page
            .jobs
            .iter()
            .enumerate()
            .skip(state.selected.saturating_sub(4))
            .take(9)
        {
            lines.push(Line::from(format!(
                "{} {} {:?} {:?} gen={} attempts={} reason={:?} failure={:?}",
                if i == state.selected { ">" } else { " " },
                job.id,
                job.kind,
                job.status,
                job.generation,
                job.attempts,
                job.reason,
                job.last_failure
            )));
        }
        if page.page.next_after.is_some() {
            lines.push(Line::from("PgDn: next page"));
        }
    }
    if let Some(detail) = &state.detail {
        let mut content = vec![Line::from(format!(
            "{}: next eligible {} | dependencies {:?}",
            detail.job.id, detail.job.available_at, detail.dependencies
        ))];
        for stage in &detail.stages {
            content.push(Line::from(format!(
                "Stage {} {:?}: {:?}",
                stage.id, stage.kind, stage.status
            )));
        }
        for a in &detail.history {
            content.push(Line::from(format!(
                "Generation {} attempt {}: {:?}, failure {:?}, uncertain={} | {} to {:?}",
                a.generation,
                a.attempt,
                a.outcome,
                a.failure,
                a.uncertain,
                a.started_at,
                a.finished_at
            )));
        }
        if detail.stages_truncated || detail.history_truncated {
            content.push(Line::from(
                "Additional stages/history omitted from this bounded view.",
            ));
        }
        lines.extend(content.into_iter().skip(usize::from(state.scroll)));
    }
    frame.render_widget(
        Paragraph::new(lines)
            .wrap(Wrap { trim: false })
            .block(themed_block("Jobs")),
        area,
    );
}
