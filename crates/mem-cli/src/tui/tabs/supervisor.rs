// SPDX-License-Identifier: AGPL-3.0-or-later
use crate::{
    commands::{api::ApiClient, supervisor::lines},
    tui::{
        app::{App, BackgroundEvent},
        theme::themed_block,
    },
};
use crossterm::event::{KeyCode, KeyEvent};
use mem_record::SupervisorResponse;
use ratatui::{
    layout::Rect,
    text::Line,
    widgets::{Paragraph, Wrap},
};

#[derive(Default)]
pub(in crate::tui) struct SupervisorState {
    pub response: Option<SupervisorResponse>,
    pub busy: bool,
    pub error: Option<String>,
    after: String,
    scroll: u16,
}
impl SupervisorState {
    pub(in crate::tui) fn receive(&mut self, result: Result<SupervisorResponse, String>) {
        self.busy = false;
        self.response = None;
        self.error = None;
        match result {
            Ok(value) => self.response = Some(value),
            Err(error) => self.error = Some(error),
        }
    }
}
impl App {
    fn request_supervisor(&mut self, api: &ApiClient, after: String) {
        if self.supervisor.busy {
            return;
        }
        self.supervisor.busy = true;
        self.supervisor.response = None;
        self.supervisor.error = None;
        self.supervisor.scroll = 0;
        self.supervisor.after = after.clone();
        let api = api.clone();
        let tx = self.background_tx.clone();
        tokio::spawn(async move {
            let result = api.supervisor(&after, 50).await.map_err(|e| e.to_string());
            let _ = tx.send(BackgroundEvent::Supervisor(result));
        });
    }
    pub(in crate::tui) fn supervisor_key(&mut self, key: KeyEvent, api: &ApiClient) -> bool {
        match key.code {
            KeyCode::Char('r') => self.request_supervisor(
                api,
                if self.supervisor.after.is_empty() {
                    "0".into()
                } else {
                    self.supervisor.after.clone()
                },
            ),
            KeyCode::Home => self.request_supervisor(api, "0".into()),
            KeyCode::PageDown => {
                if let Some(after) = self
                    .supervisor
                    .response
                    .as_ref()
                    .and_then(|r| r.snapshot.as_ref())
                    .filter(|s| s.has_more)
                    .map(|s| s.next_after.clone())
                {
                    self.request_supervisor(api, after);
                }
            }
            KeyCode::Down | KeyCode::Char('j') => {
                self.supervisor.scroll = self.supervisor.scroll.saturating_add(1)
            }
            KeyCode::Up | KeyCode::Char('k') => {
                self.supervisor.scroll = self.supervisor.scroll.saturating_sub(1)
            }
            _ => return false,
        }
        true
    }
}
pub(in crate::tui) fn draw(frame: &mut ratatui::Frame<'_>, app: &App, area: Rect) {
    let state = &app.supervisor;
    let mut text = vec![];
    if state.busy {
        text.push(Line::from("Loading supervisor…"));
    }
    if let Some(error) = &state.error {
        text.push(Line::from(error.clone()));
    }
    if let Some(response) = &state.response {
        text.extend(lines(response).into_iter().map(Line::from));
    } else if !state.busy && state.error.is_none() {
        text.push(Line::from(
            "Press r to inspect this instance. Global memory_read is required.",
        ));
    }
    frame.render_widget(
        Paragraph::new(text)
            .block(themed_block("Area supervisor"))
            .wrap(Wrap { trim: false })
            .scroll((state.scroll, 0)),
        area,
    );
}

#[cfg(test)]
mod tests {
    use super::*;
    #[test]
    fn supervisor_failure_clears_previous_state_and_loading() {
        let mut state = SupervisorState::default();
        state.receive(Ok(SupervisorResponse {
            read_only: true,
            execution: mem_record::SupervisorExecution::NotIntegrated,
            deterministic_checks_configured: false,
            snapshot: None,
        }));
        assert!(state.response.is_some());
        state.busy = true;
        state.receive(Err(
            "Forbidden: instance-wide memory_read is required".into()
        ));
        assert!(state.response.is_none() && !state.busy);
        assert!(
            state
                .error
                .as_ref()
                .expect("error")
                .starts_with("Forbidden")
        );
    }
    #[test]
    fn supervisor_is_in_both_tab_navigation_directions() {
        use crate::tui::state::{TabKind, VISIBLE_TABS};
        assert_eq!(TabKind::Supervisor.next(), TabKind::Memories);
        assert_eq!(TabKind::Supervisor.prev(), TabKind::Jobs);
        for (index, tab) in VISIBLE_TABS.iter().enumerate() {
            assert_eq!(tab.index(), index);
            assert_eq!(tab.next().prev(), *tab);
        }
    }
}
