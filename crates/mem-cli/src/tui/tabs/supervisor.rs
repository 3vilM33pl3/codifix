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
    pending: Option<mem_record::SupervisorPolicyRequest>,
    after: String,
    scroll: u16,
}
impl SupervisorState {
    pub(in crate::tui) fn controlled(&mut self, result: Result<SupervisorResponse, String>) {
        if result.is_ok() {
            self.pending = None;
        }
        self.receive(result);
    }
    fn prepare(&mut self, enabled: bool) {
        if self.busy || self.pending.is_some() {
            return;
        }
        let Some(response) = &self.response else {
            return;
        };
        if !response.can_control || response.read_only {
            return;
        }
        let Some(snapshot) = &response.snapshot else {
            return;
        };
        let Some(policy) = &snapshot.policy else {
            return;
        };
        if policy.revoked || policy.enabled == enabled {
            return;
        }
        self.pending = Some(mem_record::SupervisorPolicyRequest {
            request_id: uuid::Uuid::new_v4(),
            instance_id: snapshot.instance_id,
            policy_revision: policy.revision,
            expected_enabled: policy.enabled,
            enabled,
        });
        self.scroll = 0;
    }
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
        if self.supervisor.busy || self.supervisor.pending.is_some() {
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
        if self.supervisor.pending.is_some() {
            match key.code {
                KeyCode::Enter if !self.supervisor.busy => {
                    let request = self
                        .supervisor
                        .pending
                        .clone()
                        .expect("pending confirmation");
                    self.supervisor.busy = true;
                    self.supervisor.error = None;
                    let api = api.clone();
                    let tx = self.background_tx.clone();
                    tokio::spawn(async move {
                        let result = async {
                            api.supervisor_policy(&request).await?;
                            api.supervisor("0", 50).await
                        }
                        .await
                        .map_err(|e: anyhow::Error| e.to_string());
                        let _ = tx.send(BackgroundEvent::SupervisorControlled(result));
                    });
                }
                KeyCode::Char('c') if !self.supervisor.busy => {
                    self.supervisor.pending = None;
                    self.request_supervisor(api, "0".into());
                }
                KeyCode::Char('r')
                | KeyCode::Home
                | KeyCode::PageDown
                | KeyCode::Char('p' | 'e')
                | KeyCode::Enter
                | KeyCode::Char('c') => {}
                _ => return false,
            }
            return true;
        }
        match key.code {
            KeyCode::Char('p') => self.supervisor.prepare(false),
            KeyCode::Char('e') => self.supervisor.prepare(true),
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
    if let Some(request) = &state.pending {
        text.push(Line::from(format!(
            "CONFIRM {} instance {} policy {}",
            if request.enabled { "RESUME" } else { "PAUSE" },
            request.instance_id,
            request.policy_revision
        )));
        text.push(Line::from(format!(
            "Request {}. Enter confirms/retries these exact terms; c closes and inspects history.",
            request.request_id
        )));
        text.push(Line::from("No new disclosure consent or installation. Resume permits configured workers to run eligible approved paid work. Already transmitted bytes cannot be recalled."));
    }
    if state.busy {
        text.push(Line::from("Loading supervisor…"));
    }
    if let Some(error) = &state.error {
        text.push(Line::from(error.clone()));
    }
    if let Some(response) = &state.response {
        if response.can_control && !response.read_only {
            text.push(Line::from(
                "p: prepare pause | e: prepare resume | confirmation required",
            ));
        }
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
    fn supervisor_controls_bind_confirmation_and_retain_unknown_request() {
        let mut state = SupervisorState::default();
        let response: SupervisorResponse = serde_json::from_value(serde_json::json!({
            "read_only": false, "can_control":true, "execution":"not_integrated",
            "snapshot":{"instance_id":uuid::Uuid::new_v4(),"area":"workshop","observed_at":"2026-09-17T10:00:00Z",
                "policy":{"revision":uuid::Uuid::new_v4(),"enabled":false,"revoked":false,"model":"gpt-5.6-sol","daily_usd_micros":"2500000","daily_calls":20},
                "budget":null,"action_counts":[],"journal":[],"next_after":"0","has_more":false}
        })).expect("synthetic response");
        let mut denied = response.clone();
        denied.read_only = true;
        state.receive(Ok(denied));
        state.prepare(true);
        assert!(state.pending.is_none());
        let mut denied = response.clone();
        denied.can_control = false;
        state.receive(Ok(denied));
        state.prepare(true);
        assert!(state.pending.is_none());
        state.receive(Ok(response.clone()));
        state.prepare(false);
        assert!(state.pending.is_none());
        state.prepare(true);
        let request = state.pending.clone().expect("confirmation");
        assert_eq!(
            request.instance_id,
            response.snapshot.as_ref().expect("snapshot").instance_id
        );
        assert!(request.enabled && !request.expected_enabled);
        state.prepare(false);
        assert_eq!(state.pending, Some(request.clone()));
        state.controlled(Err("Outcome unconfirmed".into()));
        assert_eq!(state.pending, Some(request));
        assert!(state.response.is_none());
        state.controlled(Ok(response));
        assert!(state.pending.is_none());
    }
    #[test]
    fn supervisor_failure_clears_previous_state_and_loading() {
        let mut state = SupervisorState::default();
        state.receive(Ok(SupervisorResponse {
            read_only: true,
            execution: mem_record::SupervisorExecution::NotIntegrated,
            deterministic_checks_configured: false,
            model_work: None,
            can_control: false,
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
