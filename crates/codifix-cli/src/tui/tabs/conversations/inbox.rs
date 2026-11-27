// SPDX-License-Identifier: AGPL-3.0-or-later
use super::*;
use crate::human_inbox::{HumanInbox, InboxTarget, LoginStatus};
use serde_json::Value;

#[derive(Default)]
pub(super) struct InboxState {
    pub(super) client: Option<HumanInbox>,
    // Retain revoke-only handles after a failed revocation or a target change.
    cleanup: Vec<HumanInbox>,
    generation: u64,
    task: Option<tokio::task::JoinHandle<()>>,
    pub(super) busy: bool,
    pub(super) login_pending: bool,
    pub(super) error: Option<String>,
    pub(super) message: Option<String>,
    pub(super) page: Option<Value>,
    pub(super) network: Option<Value>,
    logout_pending: bool,
    pub(super) detail: Option<Value>,
    pub(super) selected: usize,
    pub(super) scroll: u16,
    pub(super) has_more: bool,
}
pub(in crate::tui) enum InboxResult {
    LoginPending(chrono::DateTime<chrono::Utc>),
    SignedIn(Value),
    SignedOut,
    Page(Value),
    Detail(Value),
    History { id: Uuid, page: Value },
}
enum InboxRead {
    Login,
    Logout,
    Page,
    Detail(Uuid),
    History(Uuid, i64),
}
impl InboxState {
    pub(super) fn invalidate(&mut self) {
        self.generation = self.generation.wrapping_add(1);
        if let Some(task) = self.task.take() {
            task.abort();
        }
        if let Some(client) = self.client.take() {
            client.cancel_login();
            self.cleanup.push(client.clone());
            // Revocation cannot restore a local session. Its handle stays retryable.
            tokio::spawn(async move {
                let _ = client.sign_out().await;
            });
        }
        self.clear_private();
        self.busy = false;
        self.login_pending = false;
        self.logout_pending = false;
        self.error = None;
        self.message = None;
    }
    pub(super) fn check_expiry(&mut self) {
        if !self.signed_in() && (self.page.is_some() || self.detail.is_some()) {
            self.clear_private();
            self.error = Some("Human session expired or revoked. Sign in again (i).".into());
        }
    }
    fn cancel_read(&mut self) {
        self.generation = self.generation.wrapping_add(1);
        if let Some(task) = self.task.take() {
            task.abort();
        }
        self.busy = false;
    }
    fn clear_private(&mut self) {
        self.page = None;
        self.network = None;
        self.detail = None;
        self.selected = 0;
        self.scroll = 0;
        self.has_more = false;
    }
    pub(super) fn signed_in(&self) -> bool {
        self.client.as_ref().is_some_and(HumanInbox::logged_in)
    }
}
impl ConversationsState {
    pub(in crate::tui) fn disable_inbox(&mut self) {
        self.inbox.invalidate();
    }

    pub(in crate::tui) fn receive_inbox(
        &mut self,
        scope: ObservationScope,
        generation: u64,
        result: Result<InboxResult, String>,
    ) {
        if self.scope.as_ref() != Some(&scope) || self.inbox.generation != generation {
            return;
        }
        let state = &mut self.inbox;
        if let Ok(InboxResult::LoginPending(expires)) = result {
            state.message = Some(format!(
                "Complete sign-in in your browser (expires {expires}). Esc cancels."
            ));
            return;
        }
        state.busy = false;
        let was_login = state.login_pending;
        let was_logout = state.logout_pending;
        state.login_pending = false;
        state.logout_pending = false;
        state.task = None;
        if matches!(
            result,
            Ok(InboxResult::Page(_) | InboxResult::Detail(_) | InboxResult::History { .. })
        ) && !state.signed_in()
        {
            state.clear_private();
            state.error = Some("Human session expired or revoked. Sign in again (i).".into());
            return;
        }
        match result {
            Err(error) => {
                state.message = None;
                if denied(&error) || error.contains("HTTP 404") {
                    state.cancel_read();
                    state.clear_private();
                }
                if was_login && let Some(client) = &state.client {
                    client.cancel_login();
                }
                if was_logout {
                    state.message = Some(
                        "Local session cleared; server revocation not confirmed. Press o to retry."
                            .into(),
                    );
                }
                if !state.signed_in() {
                    state.clear_private();
                }
                state.error = Some(error);
            }
            Ok(InboxResult::SignedIn(network)) => {
                state.network = Some(network);
                state.message =
                    Some("Signed in for My Inbox. Press r to load your conversations.".into())
            }
            Ok(InboxResult::SignedOut) => {
                state.clear_private();
                state.message = Some("Signed out; native session revoked.".into());
            }
            Ok(InboxResult::Page(page)) => {
                state.page = Some(page);
                state.detail = None;
                state.selected = 0;
                state.scroll = 0;
            }
            Ok(InboxResult::Detail(detail)) => {
                state.has_more = detail
                    .get("messages")
                    .and_then(Value::as_array)
                    .is_some_and(|m| m.len() >= 500);
                // Server detail is bounded; an explicit older-history probe remains available.
                state.detail = Some(detail);
                state.scroll = 0;
            }
            Ok(InboxResult::History { id, page }) => {
                if state
                    .detail
                    .as_ref()
                    .and_then(|d| d.pointer("/conversation/id"))
                    .and_then(Value::as_str)
                    .and_then(|s| s.parse::<Uuid>().ok())
                    != Some(id)
                {
                    return;
                }
                state.has_more = page
                    .get("has_more")
                    .and_then(Value::as_bool)
                    .unwrap_or(false);
                if let Some(detail) = state.detail.as_mut()
                    && let Some(old) = page.get("messages").and_then(Value::as_array)
                    && let Some(messages) = detail.get_mut("messages").and_then(Value::as_array_mut)
                {
                    let mut merged = old.clone();
                    merged.extend(messages.iter().cloned());
                    merged.sort_by_key(|m| {
                        m.get("sequence")
                            .and_then(Value::as_i64)
                            .unwrap_or_default()
                    });
                    merged.dedup_by_key(|m| {
                        m.get("sequence")
                            .and_then(Value::as_i64)
                            .unwrap_or_default()
                    });
                    *messages = merged;
                    state.scroll = 0;
                }
            }
            Ok(InboxResult::LoginPending(_)) => unreachable!(),
        }
    }
}
impl App {
    pub(in crate::tui) async fn shutdown_inbox(&mut self) {
        let state = &mut self.conversations.inbox;
        if let Some(task) = state.task.take() {
            task.abort();
        }
        let mut clients = state.cleanup.clone();
        if let Some(client) = state.client.take() {
            clients.push(client);
        }
        for client in &clients {
            client.cancel_login();
        }
        state.clear_private();
        // Restore the terminal before this bounded best-effort remote cleanup.
        let _ = tokio::time::timeout(
            Duration::from_secs(3),
            futures_util::future::join_all(clients.iter().map(HumanInbox::sign_out)),
        )
        .await;
    }
    fn inbox_request(&mut self, api: &ApiClient, read: InboxRead) {
        let Some(scope) = self.conversations.scope.clone() else {
            return;
        };
        let state = &mut self.conversations.inbox;
        if !matches!(read, InboxRead::Logout) && self.service.inbox_enabled != Some(true) {
            state.error = Some(
                if self.service.inbox_enabled == Some(false) {
                    "My Inbox is disabled for the selected area."
                } else {
                    "My Inbox availability is unknown; press r to refresh service health."
                }
                .into(),
            );
            return;
        }
        if state.busy {
            return;
        }
        let cleanup_client = if matches!(read, InboxRead::Logout) && state.client.is_none() {
            if let Some(client) = state.cleanup.last() {
                Some(client.clone())
            } else {
                state.clear_private();
                state.error = None;
                state.message = Some("No native session remains in this TUI.".into());
                return;
            }
        } else {
            None
        };
        if state.client.is_none() && cleanup_client.is_none() {
            // A metadata/config mismatch is not authority to use another area's identity.
            if api
                .config
                .area
                .as_ref()
                .map(|a| (a.name.clone(), a.instance_id))
                != scope.area
            {
                state.error = Some("Selected area changed; reopen the TUI for that area.".into());
                return;
            }
            match InboxTarget::from_config(&api.config).and_then(HumanInbox::new) {
                Ok(client) => state.client = Some(client),
                Err(error) => {
                    state.error = Some(error);
                    return;
                }
            }
        }
        let client = cleanup_client
            .or_else(|| state.client.clone())
            .expect("current or revoke-only client");
        if !matches!(read, InboxRead::Login | InboxRead::Logout) && !client.logged_in() {
            state.clear_private();
            state.error = Some("Sign in as a human to read My Inbox (i).".into());
            return;
        }
        state.generation = state.generation.wrapping_add(1);
        let generation = state.generation;
        state.error = None;
        state.message = None;
        state.busy = true;
        state.login_pending = matches!(read, InboxRead::Login);
        state.logout_pending = matches!(read, InboxRead::Logout);
        if matches!(read, InboxRead::Logout) {
            client.cancel_login(); // Fence credential use synchronously, before any await.
            state.clear_private();
        }
        let cleanup = state.cleanup.clone();
        let tx = self.background_tx.clone();
        state.task = Some(tokio::spawn(async move {
            let result = async {
                match read {
                    InboxRead::Login => {
                        let login = client.begin_login().await?;
                        tx.send(BackgroundEvent::HumanInbox {
                            scope: scope.clone(),
                            generation,
                            result: Ok(InboxResult::LoginPending(login.expires_at)),
                        })
                        .map_err(|_| "TUI closed")?;
                        client.open_login(login.request_id).await?;
                        loop {
                            let progress = client.poll_login(login.request_id)?;
                            if progress.status == LoginStatus::Approved {
                                let _ = progress.auth; // Never include authentication payloads in terminal output.
                                return client.network_status().await.map(InboxResult::SignedIn);
                            }
                            tokio::time::sleep(Duration::from_millis(400)).await;
                        }
                    }
                    InboxRead::Logout => {
                        client.sign_out().await?;
                        for old in cleanup {
                            old.sign_out().await?;
                        }
                        Ok(InboxResult::SignedOut)
                    }
                    InboxRead::Page => client.list().await.map(InboxResult::Page),
                    InboxRead::Detail(id) => client.detail(id).await.map(InboxResult::Detail),
                    InboxRead::History(id, before) => client
                        .history(id, before, 100)
                        .await
                        .map(|page| InboxResult::History { id, page }),
                }
            }
            .await;
            let _ = tx.send(BackgroundEvent::HumanInbox {
                scope,
                generation,
                result,
            });
        }));
    }
    pub(super) fn inbox_key(&mut self, key: KeyEvent, api: &ApiClient) -> bool {
        let state = &mut self.conversations.inbox;
        if state.login_pending && key.code == KeyCode::Esc {
            state.generation = state.generation.wrapping_add(1);
            if let Some(task) = state.task.take() {
                task.abort();
            }
            if let Some(client) = &state.client {
                client.cancel_login();
            }
            state.login_pending = false;
            state.busy = false;
            state.message =
                Some("Sign-in cancelled. Press o to retry any pending session cleanup.".into());
            return true;
        }
        if state.logout_pending || (state.login_pending && !matches!(key.code, KeyCode::Char('o')))
        {
            // Selection/read controls cannot abort a callback or revocation.
            return viewer_fallback(key);
        }
        match key.code {
            KeyCode::Char('i') if !state.signed_in() => self.inbox_request(api, InboxRead::Login),
            KeyCode::Char('o') => {
                if let Some(task) = state.task.take() {
                    task.abort();
                }
                state.busy = false;
                self.inbox_request(api, InboxRead::Logout);
            }
            KeyCode::Char('r') => {
                let read = state
                    .detail
                    .as_ref()
                    .and_then(|d| d.pointer("/conversation/id"))
                    .and_then(Value::as_str)
                    .and_then(|s| s.parse().ok())
                    .map(InboxRead::Detail)
                    .unwrap_or(InboxRead::Page);
                self.inbox_request(api, read);
            }
            KeyCode::Esc if state.detail.is_some() || state.busy => {
                state.generation = state.generation.wrapping_add(1);
                if let Some(task) = state.task.take() {
                    task.abort();
                }
                state.busy = false;
                state.detail = None;
                state.scroll = 0;
            }
            KeyCode::Down | KeyCode::Char('j') if state.detail.is_none() => {
                state.cancel_read();
                state.scroll = 0;
                state.selected = (state.selected + 1).min(
                    state
                        .page
                        .as_ref()
                        .and_then(Value::as_array)
                        .map_or(0, Vec::len)
                        .saturating_sub(1),
                )
            }
            KeyCode::Up | KeyCode::Char('k') if state.detail.is_none() => {
                state.cancel_read();
                state.scroll = 0;
                state.selected = state.selected.saturating_sub(1)
            }
            KeyCode::Enter if state.detail.is_none() => {
                if let Some(id) = state
                    .page
                    .as_ref()
                    .and_then(Value::as_array)
                    .and_then(|p| p.get(state.selected))
                    .and_then(|c| c.get("id"))
                    .and_then(Value::as_str)
                    .and_then(|s| s.parse().ok())
                {
                    self.inbox_request(api, InboxRead::Detail(id));
                }
            }
            KeyCode::Char('n') if state.detail.is_some() => {
                if let Some(detail) = &state.detail
                    && let Some(id) = detail
                        .pointer("/conversation/id")
                        .and_then(Value::as_str)
                        .and_then(|s| s.parse().ok())
                    && let Some(before) =
                        detail
                            .get("messages")
                            .and_then(Value::as_array)
                            .and_then(|m| {
                                m.iter()
                                    .filter_map(|m| m.get("sequence").and_then(Value::as_i64))
                                    .min()
                            })
                {
                    self.inbox_request(api, InboxRead::History(id, before));
                }
            }
            KeyCode::Down | KeyCode::Char('j') | KeyCode::Char('d') => {
                state.scroll = state.scroll.saturating_add(1)
            }
            KeyCode::Up | KeyCode::Char('k') | KeyCode::Char('u') => {
                state.scroll = state.scroll.saturating_sub(1)
            }
            KeyCode::PageDown => state.scroll = state.scroll.saturating_add(8),
            KeyCode::PageUp => state.scroll = state.scroll.saturating_sub(8),
            KeyCode::Home => state.scroll = 0,
            _ => return viewer_fallback(key),
        }
        true
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    fn app() -> App {
        let (tx, _rx) = tokio::sync::mpsc::unbounded_channel();
        App::new(
            "p".into(),
            "/tmp/test".into(),
            crate::tui::state::ToolVersions {
                mem_cli: "test".into(),
                mem_service: "test".into(),
                watch_manager: "test".into(),
                memory_watch: "test".into(),
            },
            false,
            codifix_config::Profile::Prod,
            tx,
        )
    }
    fn api() -> ApiClient {
        ApiClient::new(reqwest::Client::new(), serde_json::from_value(serde_json::json!({"service":{"bind_addr":"127.0.0.1:1"},"database":{"url":"postgresql://unused/unused"}})).unwrap())
    }
    #[tokio::test]
    async fn read_controls_do_not_interrupt_login_or_signout_workers() {
        for logout in [false, true] {
            let mut app = app();
            app.conversations.inbox.generation = 7;
            app.conversations.inbox.busy = true;
            app.conversations.inbox.login_pending = !logout;
            app.conversations.inbox.logout_pending = logout;
            app.conversations.inbox.task = Some(tokio::spawn(std::future::pending()));
            for code in [
                KeyCode::Char('j'),
                KeyCode::Char('k'),
                KeyCode::Char('r'),
                KeyCode::Char('n'),
                KeyCode::PageDown,
            ] {
                assert!(app.inbox_key(KeyEvent::from(code), &api()));
                assert_eq!(app.conversations.inbox.generation, 7);
                assert!(
                    app.conversations
                        .inbox
                        .task
                        .as_ref()
                        .is_some_and(|t| !t.is_finished())
                );
            }
            if logout {
                assert!(app.inbox_key(KeyEvent::from(KeyCode::Char('o')), &api()));
                assert_eq!(app.conversations.inbox.generation, 7);
            }
            app.conversations.inbox.task.take().unwrap().abort();
        }
    }
    #[tokio::test]
    async fn disabled_and_unknown_inbox_never_start_browser_login() {
        for availability in [None, Some(false)] {
            let mut app = app();
            app.observation_tick(&api());
            app.service.inbox_enabled = availability;
            app.inbox_request(&api(), InboxRead::Login);
            assert!(app.conversations.inbox.client.is_none());
            assert!(!app.conversations.inbox.login_pending);
            assert!(app.conversations.inbox.task.is_none());
            assert!(app.conversations.inbox.error.is_some());
        }
    }

    #[tokio::test]
    async fn old_target_cleanup_does_not_require_a_new_target_login_client() {
        let mut app = app();
        let api = api();
        app.observation_tick(&api);
        let old = HumanInbox::new(
            InboxTarget::new("https://old.example.test", Uuid::from_u128(1)).unwrap(),
        )
        .unwrap();
        app.conversations.inbox.cleanup.push(old);
        app.inbox_request(&api, InboxRead::Logout);
        assert!(app.conversations.inbox.client.is_none());
        assert!(app.conversations.inbox.error.is_none());
        assert!(app.conversations.inbox.task.is_some());
        app.conversations.inbox.task.take().unwrap().await.unwrap();
    }
}
