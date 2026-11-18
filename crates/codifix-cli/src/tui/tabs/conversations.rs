// SPDX-License-Identifier: AGPL-3.0-or-later
//! Read-only companion observation. Requests carry exact target and generation bindings.
use crate::{
    commands::{api::ApiClient, output::service_url},
    tui::app::{App, BackgroundEvent, TabKind},
};
use codifix_record::agency_observation::{CompanionStatus, ExchangeDetail, ExchangePage};
use crossterm::event::{KeyCode, KeyEvent};
use std::time::{Duration, Instant};
use uuid::Uuid;

mod inbox;
mod render;
#[cfg(test)]
mod tests;
pub(in crate::tui) use render::{draw, draw_companions};

#[derive(Clone, Debug, PartialEq, Eq)]
pub(in crate::tui) struct ObservationScope {
    endpoint: String,
    native_origin: Option<String>,
    area: Option<(String, Uuid)>,
    project: String,
}
#[derive(Default)]
pub(in crate::tui) struct ConversationsState {
    scope: Option<ObservationScope>,
    companion: Option<CompanionStatus>,
    companion_generation: u64,
    companion_busy: bool,
    companion_error: Option<String>,
    companion_loaded: Option<Instant>,
    companion_task: Option<tokio::task::JoinHandle<()>>,
    page: Option<ExchangePage>,
    detail: Option<ExchangeDetail>,
    exchange_generation: u64,
    exchange_busy: bool,
    exchange_error: Option<String>,
    observation_denied: bool,
    exchange_loaded: Option<Instant>,
    exchange_task: Option<tokio::task::JoinHandle<()>>,
    selected: usize,
    peer_selected: usize,
    scroll: u16,
    peer: Option<String>,
    before: Option<String>,
    pub(in crate::tui) editing_peer: bool,
    peer_draft: String,
    my_inbox: bool,
    inbox: inbox::InboxState,
}
pub(in crate::tui) enum ObservationResult {
    Companion(CompanionStatus),
    Page(ExchangePage),
    Detail(ExchangeDetail),
    // Errors retain the channel so simultaneous reads cannot clear each other's status.
    CompanionError(String),
}
pub(in crate::tui) use inbox::InboxResult;
enum Read {
    Companion,
    Page(Option<String>),
    Detail(Uuid),
}

impl ConversationsState {
    pub(in crate::tui) fn my_inbox_selected(&self) -> bool {
        self.my_inbox
    }
    fn can_observe(&self) -> bool {
        !self.observation_denied && self.companion.as_ref().is_some_and(|s| s.exchange_access)
    }
    fn deny_exchanges(&mut self) {
        self.exchange_invalidate();
        self.observation_denied = true;
        self.peer = None;
        self.peer_draft.clear();
        self.editing_peer = false;
        self.peer_selected = 0;
    }

    fn invalidate(&mut self, scope: ObservationScope) {
        let companion_generation = self.companion_generation.wrapping_add(1);
        let exchange_generation = self.exchange_generation.wrapping_add(1);
        if let Some(task) = self.companion_task.take() {
            task.abort();
        }
        if let Some(task) = self.exchange_task.take() {
            task.abort();
        }
        self.inbox.invalidate();
        // Keep counters monotonic even if a request raced an abort or the target changes back.
        let inbox = std::mem::take(&mut self.inbox);
        *self = Self {
            scope: Some(scope),
            companion_generation,
            exchange_generation,
            inbox,
            ..Default::default()
        };
    }
    pub(in crate::tui) fn receive_observation(
        &mut self,
        scope: ObservationScope,
        generation: u64,
        result: Result<ObservationResult, String>,
    ) {
        if self.scope.as_ref() != Some(&scope) {
            return;
        }
        if matches!(
            result,
            Ok(ObservationResult::Companion(_) | ObservationResult::CompanionError(_))
        ) {
            if generation != self.companion_generation {
                return;
            }
            self.companion_busy = false;
            self.companion_task = None;
            match result {
                Ok(ObservationResult::Companion(status)) => {
                    if let Some((_, expected)) = &scope.area
                        && status.instance_id.is_some_and(|actual| actual != *expected)
                    {
                        self.deny_exchanges();
                        self.companion = None;
                        self.companion_error =
                            Some("Companion identity does not match the selected area.".into());
                        return;
                    }
                    if !status.exchange_access {
                        self.deny_exchanges();
                    }
                    self.observation_denied = !status.exchange_access;
                    self.companion = Some(status);
                    self.companion_error = None;
                    self.companion_loaded = Some(Instant::now());
                }
                Ok(ObservationResult::CompanionError(e)) => {
                    if denied(&e) {
                        self.deny_exchanges();
                        self.companion = None;
                        self.companion_loaded = None;
                    }
                    self.companion_error = Some(e);
                }
                _ => unreachable!(),
            }
            return;
        }
        if generation != self.exchange_generation {
            return;
        }
        self.exchange_busy = false;
        self.exchange_task = None;
        if !self.can_observe() {
            return;
        }
        match result {
            Ok(ObservationResult::Page(page)) => {
                self.page = Some(page);
                self.detail = None;
                self.selected = 0;
                self.scroll = 0;
                self.exchange_loaded = Some(Instant::now());
                self.exchange_error = None;
            }
            Ok(ObservationResult::Detail(detail)) => {
                self.detail = Some(detail);
                self.scroll = 0;
                self.exchange_loaded = Some(Instant::now());
                self.exchange_error = None;
            }
            Err(error) => {
                if denied(&error) {
                    self.deny_exchanges();
                    if let Some(status) = &mut self.companion {
                        status.last_exchange_at = None;
                    }
                }
                self.exchange_error = Some(error);
            }
            _ => unreachable!(),
        }
    }
    fn peers(&self) -> Vec<String> {
        if !self.can_observe() {
            return Vec::new();
        }
        let mut peers = self
            .page
            .as_ref()
            .map(|p| {
                p.exchanges
                    .iter()
                    .map(|e| e.peer.clone())
                    .collect::<Vec<_>>()
            })
            .unwrap_or_default();
        if let Some(status) = &self.companion {
            peers.extend(status.enrolled_peers.iter().cloned());
        }
        peers.sort();
        peers.dedup();
        peers
    }
    fn cancel_exchange_read(&mut self) {
        if let Some(task) = self.exchange_task.take() {
            task.abort();
        }
        self.exchange_generation = self.exchange_generation.wrapping_add(1);
        self.exchange_busy = false;
        self.exchange_error = None;
    }
    fn exchange_invalidate(&mut self) {
        if let Some(task) = self.exchange_task.take() {
            task.abort();
        }
        self.exchange_generation = self.exchange_generation.wrapping_add(1);
        self.exchange_busy = false;
        self.exchange_error = None;
        self.detail = None;
        self.page = None;
        self.exchange_loaded = None;
        self.before = None;
        self.selected = 0;
        self.scroll = 0;
    }
}
/// Viewer tabs own all keys except the shell's explicit navigation/help actions.
fn viewer_fallback(key: KeyEvent) -> bool {
    !matches!(
        key.code,
        KeyCode::Tab
            | KeyCode::BackTab
            | KeyCode::Left
            | KeyCode::Right
            | KeyCode::Char('l' | 'h' | '?')
    )
}
fn denied(error: &str) -> bool {
    ["HTTP 401", "HTTP 403", "HTTP 404"]
        .into_iter()
        .any(|status| error.contains(status))
}
impl App {
    /// Called by the event loop, never by refresh or rendering. Inactive tabs do no polling.
    pub(in crate::tui) fn observation_tick(&mut self, api: &ApiClient) {
        let scope = ObservationScope {
            endpoint: service_url(&api.config, ""),
            native_origin: api.config.auth.public_base_url.clone(),
            area: self
                .meta
                .area
                .as_ref()
                .map(|a| (a.name.clone(), a.instance_id)),
            project: self.project.clone(),
        };
        if self.conversations.scope.as_ref() != Some(&scope) {
            self.conversations.invalidate(scope);
        }
        self.conversations.inbox.check_expiry();
        if !matches!(self.active_tab, TabKind::Watchers | TabKind::Conversations) {
            return;
        }
        let state = &self.conversations;
        if !state.companion_busy
            && state.companion_loaded.is_none()
            && state.companion_error.is_none()
        {
            self.observation_request(api, Read::Companion);
        } else if !state.companion_busy
            && state
                .companion_loaded
                .is_some_and(|t| t.elapsed() >= Duration::from_secs(30))
            && state.companion_error.is_none()
        {
            self.observation_request(api, Read::Companion);
        }
        let state = &self.conversations;
        if !state.my_inbox
            && !state.exchange_busy
            && state.page.is_none()
            && state.exchange_error.is_none()
            && state.companion.as_ref().is_some_and(|s| s.exchange_access)
        {
            self.observation_request(api, Read::Page(None));
        }
    }
    fn observation_request(&mut self, api: &ApiClient, read: Read) {
        let state = &mut self.conversations;
        let Some(scope) = state.scope.clone() else {
            return;
        };
        if !matches!(read, Read::Companion) && !state.can_observe() {
            state.exchange_error = Some(
                "Exchange inspection is disabled, denied or not yet available for this project."
                    .into(),
            );
            return;
        }
        let generation = if matches!(read, Read::Companion) {
            if let Some(task) = state.companion_task.take() {
                task.abort();
            }
            state.companion_generation = state.companion_generation.wrapping_add(1);
            state.companion_busy = true;
            state.companion_error = None;
            state.companion_generation
        } else {
            if let Some(task) = state.exchange_task.take() {
                task.abort();
            }
            state.exchange_generation = state.exchange_generation.wrapping_add(1);
            state.exchange_busy = true;
            state.exchange_error = None;
            state.detail = None;
            state.scroll = 0;
            if let Read::Page(before) = &read {
                state.before = before.clone();
            }
            state.exchange_generation
        };
        let api = api.clone();
        let tx = self.background_tx.clone();
        let peer = state.peer.clone();
        let is_companion = matches!(read, Read::Companion);
        let task = tokio::spawn(async move {
            let result = match read {
                Read::Companion => Ok(match api.companion_status(&scope.project).await {
                    Ok(status) => ObservationResult::Companion(status),
                    Err(error) => ObservationResult::CompanionError(error.to_string()),
                }),
                Read::Page(before) => api
                    .agent_exchanges(&scope.project, peer.as_deref(), before.as_deref(), 25)
                    .await
                    .map(ObservationResult::Page)
                    .map_err(|e| e.to_string()),
                Read::Detail(id) => api
                    .agent_exchange(&scope.project, id)
                    .await
                    .map(ObservationResult::Detail)
                    .map_err(|e| e.to_string()),
            };
            let _ = tx.send(BackgroundEvent::Observation {
                scope,
                generation,
                result,
            });
        });
        if is_companion {
            state.companion_task = Some(task);
        } else {
            state.exchange_task = Some(task);
        }
    }
    pub(in crate::tui) fn conversations_key(&mut self, key: KeyEvent, api: &ApiClient) -> bool {
        // Ensure a freshly changed area cannot dispatch against the previous native session.
        self.observation_tick(api);
        if self.active_tab == TabKind::Watchers {
            match key.code {
                KeyCode::Char('c') => {
                    self.watchers.companion_focus = !self.watchers.companion_focus
                }
                KeyCode::Char('r') => {
                    self.observation_request(api, Read::Companion);
                    self.observation_request(api, Read::Page(None));
                    return false; // Preserve the existing project/watcher refresh.
                }
                KeyCode::Down | KeyCode::Char('j') if self.watchers.companion_focus => {
                    self.conversations.peer_selected = (self.conversations.peer_selected + 1)
                        .min(self.conversations.peers().len().saturating_sub(1));
                }
                KeyCode::Up | KeyCode::Char('k') if self.watchers.companion_focus => {
                    self.conversations.peer_selected =
                        self.conversations.peer_selected.saturating_sub(1)
                }
                KeyCode::Enter if self.watchers.companion_focus => {
                    let peer = self
                        .conversations
                        .peers()
                        .get(self.conversations.peer_selected)
                        .cloned();
                    self.conversations.exchange_invalidate();
                    self.conversations.peer = peer;
                    self.conversations.my_inbox = false;
                    self.active_tab = TabKind::Conversations;
                    self.observation_request(api, Read::Page(None));
                }
                _ => return self.watchers.companion_focus && viewer_fallback(key),
            }
            return true;
        }
        if self.conversations.editing_peer {
            match key.code {
                KeyCode::Enter => {
                    self.conversations.peer = (!self.conversations.peer_draft.trim().is_empty())
                        .then(|| self.conversations.peer_draft.trim().to_owned());
                    self.conversations.editing_peer = false;
                    self.conversations.exchange_invalidate();
                    self.observation_request(api, Read::Page(None));
                }
                KeyCode::Esc => self.conversations.editing_peer = false,
                KeyCode::Backspace => {
                    self.conversations.peer_draft.pop();
                }
                KeyCode::Char(c)
                    if !c.is_control()
                        && self.conversations.peer_draft.len() + c.len_utf8() <= 256 =>
                {
                    self.conversations.peer_draft.push(c)
                }
                _ => {}
            }
            return true;
        }
        if key.code == KeyCode::Char('v') {
            self.conversations.my_inbox = !self.conversations.my_inbox;
            return true;
        }
        if self.conversations.my_inbox {
            let handled = self.inbox_key(key, api);
            self.clamp_conversation_scroll();
            return handled;
        }
        match key.code {
            KeyCode::Char('/') => {
                self.conversations.editing_peer = true;
                self.conversations.peer_draft = self.conversations.peer.clone().unwrap_or_default();
            }
            KeyCode::Char('r') => {
                let read = self
                    .conversations
                    .detail
                    .as_ref()
                    .map(|d| Read::Detail(d.exchange.task_id))
                    .unwrap_or(Read::Page(self.conversations.before.clone()));
                self.observation_request(api, Read::Companion);
                self.observation_request(api, read);
            }
            KeyCode::Esc
                if self.conversations.detail.is_some() || self.conversations.exchange_busy =>
            {
                self.conversations.cancel_exchange_read();
                self.conversations.detail = None;
                self.conversations.scroll = 0;
            }
            KeyCode::Down | KeyCode::Char('j') if self.conversations.detail.is_none() => {
                self.conversations.cancel_exchange_read();
                self.conversations.scroll = 0;
                self.conversations.selected = (self.conversations.selected + 1).min(
                    self.conversations
                        .page
                        .as_ref()
                        .map_or(0, |p| p.exchanges.len())
                        .saturating_sub(1),
                )
            }
            KeyCode::Up | KeyCode::Char('k') if self.conversations.detail.is_none() => {
                self.conversations.cancel_exchange_read();
                self.conversations.scroll = 0;
                self.conversations.selected = self.conversations.selected.saturating_sub(1)
            }
            KeyCode::Enter => {
                if let Some(id) = self
                    .conversations
                    .page
                    .as_ref()
                    .and_then(|p| p.exchanges.get(self.conversations.selected))
                    .map(|e| e.task_id)
                {
                    self.observation_request(api, Read::Detail(id));
                }
            }
            KeyCode::PageDown if self.conversations.detail.is_none() => {
                if let Some(before) = self
                    .conversations
                    .page
                    .as_ref()
                    .and_then(|p| p.next_before.clone())
                {
                    self.observation_request(api, Read::Page(Some(before)));
                }
            }
            KeyCode::Home if self.conversations.detail.is_none() => {
                self.observation_request(api, Read::Page(None))
            }
            KeyCode::Down | KeyCode::Char('j') | KeyCode::Char('d') => {
                self.conversations.scroll = self.conversations.scroll.saturating_add(1)
            }
            KeyCode::Up | KeyCode::Char('k') | KeyCode::Char('u') => {
                self.conversations.scroll = self.conversations.scroll.saturating_sub(1)
            }
            KeyCode::PageDown => {
                self.conversations.scroll = self.conversations.scroll.saturating_add(8)
            }
            KeyCode::PageUp => {
                self.conversations.scroll = self.conversations.scroll.saturating_sub(8)
            }
            KeyCode::Home => self.conversations.scroll = 0,
            _ => return viewer_fallback(key),
        }
        self.clamp_conversation_scroll();
        true
    }
    fn clamp_conversation_scroll(&mut self) {
        use ratatui::layout::{Constraint, Layout};
        let area = crate::tui::render::current_frame_area()
            .unwrap_or_else(crate::tui::render::default_frame_area);
        let root = Layout::vertical([
            Constraint::Length(3),
            Constraint::Length(3),
            Constraint::Min(10),
            Constraint::Length(crate::tui::render::footer_height(self.meta.profile)),
        ])
        .split(area);
        let max = render::content_scroll_limit(&self.conversations, root[2]);
        if self.conversations.my_inbox {
            self.conversations.inbox.scroll = self.conversations.inbox.scroll.min(max);
        } else {
            self.conversations.scroll = self.conversations.scroll.min(max);
        }
    }
}
