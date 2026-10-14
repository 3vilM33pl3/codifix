// SPDX-License-Identifier: AGPL-3.0-or-later
use crate::{
    commands::api::{ApiClient, cache_scope_matches},
    tui::{
        app::{App, BackgroundEvent},
        theme::themed_block,
    },
};
use crossterm::event::{KeyCode, KeyEvent, KeyModifiers};
use mem_record::{
    LaptopCacheHistoryResponse, LaptopCacheList, LaptopCacheProject, LaptopCacheSearchResponse,
};
use ratatui::{
    layout::Rect,
    text::Line,
    widgets::{Paragraph, Wrap},
};

#[derive(Default)]
pub(in crate::tui) struct OfflineState {
    pub list: Option<LaptopCacheList>,
    pub result: Option<LaptopCacheSearchResponse>,
    pub history: Option<LaptopCacheHistoryResponse>,
    pub error: Option<String>,
    pub busy: bool,
    pub editing: bool,
    question: String,
    selected: usize,
    generation: u64,
    scroll: u16,
    task: Option<tokio::task::JoinHandle<()>>,
}
pub(in crate::tui) enum OfflineResult {
    List(LaptopCacheList),
    Search(LaptopCacheSearchResponse),
    History(LaptopCacheHistoryResponse),
}
impl OfflineState {
    pub(in crate::tui) fn invalidate(&mut self) {
        self.generation = self.generation.wrapping_add(1);
        if let Some(task) = self.task.take() {
            task.abort();
        }
        self.result = None;
        self.history = None;
        self.error = None;
        self.busy = false;
        self.scroll = 0;
    }
    fn project(&self) -> Option<&LaptopCacheProject> {
        self.list.as_ref()?.projects.get(self.selected)
    }
    pub(in crate::tui) fn receive(
        &mut self,
        generation: u64,
        result: Result<OfflineResult, String>,
    ) {
        if generation != self.generation {
            return;
        }
        self.busy = false;
        self.task = None;
        match result {
            Err(e) => {
                self.result = None;
                self.history = None;
                self.error = Some(e);
            }
            Ok(OfflineResult::List(list)) => {
                self.list = Some(list);
                self.selected = 0;
            }
            Ok(OfflineResult::Search(value)) => {
                if self
                    .project()
                    .is_some_and(|p| cache_scope_matches(p, &value.project))
                {
                    self.result = Some(value);
                } else {
                    self.error = Some("Local cache scope changed; refresh before searching".into());
                }
            }
            Ok(OfflineResult::History(value)) => {
                if self
                    .project()
                    .is_some_and(|p| cache_scope_matches(p, &value.project))
                {
                    if let Some(prior) = &mut self.history {
                        if prior.canonical_id == value.canonical_id {
                            prior.versions.extend(value.versions);
                            prior.next_after_version = value.next_after_version;
                        } else {
                            self.history = Some(value);
                        }
                    } else {
                        self.history = Some(value);
                    }
                } else {
                    self.history = None;
                    self.result = None;
                    self.error =
                        Some("Owner history scope changed; refresh the subscription".into());
                }
            }
        }
    }
    fn select(&mut self, index: usize) {
        self.invalidate();
        self.selected = index;
        self.question.clear();
        self.editing = false;
    }
}
impl Drop for OfflineState {
    fn drop(&mut self) {
        if let Some(task) = self.task.take() {
            task.abort();
        }
    }
}
impl App {
    fn request_offline_history(&mut self, api: &ApiClient, next: bool) {
        let Some(project) = self.offline.project().cloned() else {
            return;
        };
        let canonical = if next {
            self.offline.history.as_ref().map(|h| h.canonical_id)
        } else {
            self.offline
                .result
                .as_ref()
                .and_then(|r| {
                    r.hits
                        .iter()
                        .find(|h| matches!(h.origin, mem_record::LaptopCacheOrigin::CuratedProject))
                })
                .map(|h| h.stream_id)
        };
        let Some(canonical) = canonical else {
            return;
        };
        let after = if next {
            let Some(cursor) = self
                .offline
                .history
                .as_ref()
                .and_then(|h| h.next_after_version)
            else {
                return;
            };
            cursor
        } else {
            self.offline.history = None;
            0
        };
        self.offline.generation = self.offline.generation.wrapping_add(1);
        if let Some(task) = self.offline.task.take() {
            task.abort();
        }
        self.offline.busy = true;
        self.offline.error = None;
        let generation = self.offline.generation;
        let api = api.clone();
        let tx = self.background_tx.clone();
        self.offline.task = Some(tokio::spawn(async move {
            let result = api
                .laptop_cache_history(&project, canonical, after, 16)
                .await
                .map(OfflineResult::History)
                .map_err(|e| e.to_string());
            let _ = tx.send(BackgroundEvent::Offline { generation, result });
        }));
    }
    fn request_offline(&mut self, api: &ApiClient, search: bool) {
        let project = self.offline.project().cloned();
        if search
            && (!project.as_ref().is_some_and(|p| p.initialized)
                || self.offline.question.trim().is_empty())
        {
            return;
        }
        self.offline.invalidate();
        self.offline.busy = true;
        if !search {
            self.offline.list = None;
            self.offline.question.clear();
        }
        let generation = self.offline.generation;
        let question = self.offline.question.clone();
        let api = api.clone();
        let tx = self.background_tx.clone();
        self.offline.task = Some(tokio::spawn(async move {
            let result = if search {
                api.laptop_cache_search(&project.expect("checked subscription"), question, 8)
                    .await
                    .map(OfflineResult::Search)
            } else {
                api.laptop_cache().await.map(OfflineResult::List)
            }
            .map_err(|e| e.to_string());
            let _ = tx.send(BackgroundEvent::Offline { generation, result });
        }));
    }
    pub(in crate::tui) fn offline_key(&mut self, key: KeyEvent, api: &ApiClient) -> bool {
        if self.offline.editing {
            match key.code {
                KeyCode::Enter => {
                    self.offline.editing = false;
                    self.request_offline(api, true);
                }
                KeyCode::Esc => {
                    self.offline.editing = false;
                    self.offline.invalidate();
                }
                KeyCode::Backspace => {
                    self.offline.invalidate();
                    self.offline.question.pop();
                }
                KeyCode::Char(c)
                    if !key
                        .modifiers
                        .intersects(KeyModifiers::CONTROL | KeyModifiers::ALT)
                        && !c.is_control()
                        && self.offline.question.len() + c.len_utf8() <= 4096 =>
                {
                    self.offline.invalidate();
                    self.offline.question.push(c);
                }
                _ => {}
            }
            return true;
        }
        match key.code {
            KeyCode::Char('h') => self.request_offline_history(api, false),
            KeyCode::Char('n') => self.request_offline_history(api, true),
            KeyCode::Char('r') => self.request_offline(api, false),
            KeyCode::Char('/') => {
                self.offline.invalidate();
                self.offline.editing = true;
            }
            KeyCode::Enter => self.request_offline(api, true),
            KeyCode::Down | KeyCode::Char('j') => {
                let len = self.offline.list.as_ref().map_or(0, |p| p.projects.len());
                self.offline
                    .select((self.offline.selected + 1).min(len.saturating_sub(1)));
            }
            KeyCode::Up | KeyCode::Char('k') => {
                self.offline.select(self.offline.selected.saturating_sub(1))
            }
            KeyCode::PageDown | KeyCode::Char('d') => {
                self.offline.scroll = self.offline.scroll.saturating_add(1)
            }
            KeyCode::PageUp | KeyCode::Char('u') => {
                self.offline.scroll = self.offline.scroll.saturating_sub(1)
            }
            _ => return false,
        }
        true
    }
}
fn plain(value: &str) -> String {
    value
        .chars()
        .map(|c| {
            if c.is_control() && c != '\n' && c != '\t' {
                '�'
            } else {
                c
            }
        })
        .collect()
}
fn content(state: &OfflineState) -> Vec<Line<'static>> {
    let mut lines = vec![
        Line::from("h: first curated result's owner history; n: next history page"),
        Line::from("Read-only local excerpts; no remote search or model forwarding."),
        Line::from("Offline copy may be stale; observed sync is NOT current remote authority."),
    ];
    if let Some(history) = &state.history {
        lines.push(Line::from(format!(
            "Owner history {} · read-only",
            history.canonical_id
        )));
        for version in &history.versions {
            lines.push(Line::from(format!(
                "v{}{} · {} · {} · SHA256 {}",
                version.version,
                if version.tombstone { " tombstone" } else { "" },
                plain(&version.author),
                version.id,
                version.digest
            )));
            lines.extend(
                plain(&version.text)
                    .lines()
                    .map(|s| Line::from(s.to_owned())),
            );
            if !version.unavailable_source_ids.is_empty() {
                lines.push(Line::from(format!(
                    "{} unavailable source references retained",
                    version.unavailable_source_ids.len()
                )));
            }
        }
        if history.next_after_version.is_some() {
            lines.push(Line::from("n: more downloaded owner versions"));
        }
    }
    if state.busy {
        lines.push(Line::from("Loading local cache…"));
    }
    if let Some(e) = &state.error {
        lines.push(Line::from(format!("Unavailable/denied: {}", plain(e))));
    }
    if let Some(list) = &state.list {
        lines.push(Line::from(format!(
            "Local {} instance {} | {} subscriptions",
            plain(&list.area),
            list.instance_id,
            list.projects.len()
        )));
        if list.projects.is_empty() {
            lines.push(Line::from(
                "No configured local subscriptions; no remote permission implied.",
            ));
        }
    } else if !state.busy && state.error.is_none() {
        lines.push(Line::from(
            "r: load configured subscriptions; unsupported/disabled builds report unavailable.",
        ));
    }
    if let Some(p) = state.project() {
        if let Some(error) = &p.sync_error {
            lines.push(Line::from(format!(
                "Synchronization needs attention: {}",
                plain(error)
            )));
        }
        lines.push(Line::from(format!(
            "Area {} | project {} | owner {}",
            plain(&p.area),
            p.project_id,
            p.owner_instance_id
        )));
        lines.push(Line::from(format!(
            "Subscription {} | company {} | space {} | device {}",
            p.subscription_id, p.company_id, p.space_id, p.device_id
        )));
        let observed = state.result.as_ref().map_or(p, |r| &r.project);
        if !p.uploads.is_empty() {
            lines.push(Line::from(format!(
                "Curated uploads: {} (refresh to update)",
                p.uploads
                    .iter()
                    .map(|(s, n)| format!("{} {n}", plain(s)))
                    .collect::<Vec<_>>()
                    .join(" | ")
            )));
        }
        lines.push(Line::from(format!(
            "Observed sync {} | cursor {} | initialized {} (not guaranteed fresh)",
            observed
                .observed_at
                .map_or_else(|| "unknown".into(), |t| t.to_rfc3339()),
            observed.cursor,
            observed.initialized
        )));
    }
    lines.push(Line::from(format!(
        "{}question ({} /4096 bytes): {}",
        if state.editing { "Editing " } else { "/: " },
        state.question.len(),
        plain(&state.question)
    )));
    if let Some(result) = &state.result {
        if result.hits.is_empty() {
            lines.push(Line::from("No local excerpts match this bounded search."));
        }
        for (i, hit) in result.hits.iter().enumerate() {
            lines.push(Line::from(format!("[{}] {}", i + 1, plain(&hit.summary))));
            lines.extend(plain(&hit.text).lines().map(|s| Line::from(s.to_owned())));
            lines.push(Line::from(format!(
                "Author {} | {} | canonical {} | {} SHA256 {}",
                plain(&hit.author),
                hit.authored_at,
                hit.stream_id,
                hit.shared_version
                    .map(|v| format!("shared v{v}"))
                    .unwrap_or_else(|| "curated project".into()),
                hit.digest
            )));
            lines.push(Line::from(format!(
                "Source {} v{} | locator (text only) {}",
                hit.source_record_id,
                hit.source_version,
                plain(
                    hit.locator
                        .as_deref()
                        .unwrap_or("private project replica; no publication locator")
                )
            )));
            for e in &hit.evidence {
                lines.push(Line::from(format!(
                    "Evidence {}: {} | SHA256 {} | reviewer {} at {}",
                    plain(&e.label),
                    plain(&e.text),
                    e.digest,
                    plain(&e.reviewer),
                    e.reviewed_at
                )));
            }
            if !hit.unavailable_source_ids.is_empty() {
                lines.push(Line::from(format!(
                    "{} provenance references retained; source content is not downloaded",
                    hit.unavailable_source_ids.len()
                )));
            }
        }
    }
    lines
}
pub(in crate::tui) fn draw(frame: &mut ratatui::Frame<'_>, app: &App, area: Rect) {
    frame.render_widget(
        Paragraph::new(content(&app.offline))
            .wrap(Wrap { trim: false })
            .scroll((app.offline.scroll, 0))
            .block(themed_block("Synchronized knowledge (Offline)")),
        area,
    );
}

#[cfg(test)]
mod tests {
    use super::*;
    #[test]
    fn owner_history_is_read_only_and_cleared_on_scope_change() {
        let mut state = state();
        let project = state.project().unwrap().clone();
        let canonical = Uuid::new_v4();
        let mut page = LaptopCacheHistoryResponse {
            schema_version: 1,
            project,
            canonical_id: canonical,
            versions: vec![mem_record::LaptopCacheHistoryVersion {
                memory_type: mem_record::MemoryType::DomainFact,
                id: Uuid::new_v4(),
                version: 2,
                author: "original author".into(),
                authored_at: chrono::Utc::now(),
                updated_at: chrono::Utc::now(),
                tombstone: true,
                text: "retained owner history".into(),
                summary: "historical".into(),
                digest: "a".repeat(64),
                unavailable_source_ids: vec![Uuid::new_v4()],
            }],
            next_after_version: None,
            read_only: true,
            current_state: None,
        };
        state.receive(state.generation, Ok(OfflineResult::History(page.clone())));
        let rendered = content(&state)
            .iter()
            .map(ToString::to_string)
            .collect::<Vec<_>>()
            .join("\n");
        assert!(rendered.contains("v2 tombstone"));
        assert!(rendered.contains("original author"));
        assert!(rendered.contains("unavailable source references"));
        page.project.owner_instance_id = Uuid::new_v4();
        state.receive(state.generation, Ok(OfflineResult::History(page)));
        assert!(state.history.is_none());
        assert!(state.error.as_ref().unwrap().contains("scope changed"));
    }
    use uuid::Uuid;
    fn project() -> LaptopCacheProject {
        LaptopCacheProject {
            subscription_id: Uuid::new_v4(),
            device_id: Uuid::new_v4(),
            company_id: Uuid::new_v4(),
            area: "workshop".into(),
            owner_instance_id: Uuid::new_v4(),
            project_id: Uuid::new_v4(),
            space_id: Uuid::new_v4(),
            cursor: 3,
            observed_at: None,
            initialized: true,
            read_only: true,
            uploads: Default::default(),
            sync_error: None,
        }
    }
    fn state() -> OfflineState {
        let mut state = OfflineState::default();
        state.list = Some(LaptopCacheList {
            schema_version: 1,
            area: "workshop".into(),
            instance_id: Uuid::new_v4(),
            projects: vec![project(), project()],
        });
        state
    }
    #[test]
    fn cache_selection_and_refresh_fence_old_searches_and_clear_input() {
        let mut state = state();
        let old = state.generation;
        let project = state.project().unwrap().clone();
        state.question = "private query".into();
        state.select(1);
        state.receive(
            old,
            Ok(OfflineResult::Search(LaptopCacheSearchResponse {
                schema_version: 1,
                project,
                hits: vec![],
            })),
        );
        assert!(state.result.is_none());
        assert!(state.question.is_empty());
    }
    #[test]
    fn cache_error_and_changed_owner_never_show_stale_results() {
        let mut state = state();
        let mut p = state.project().unwrap().clone();
        p.owner_instance_id = Uuid::new_v4();
        state.receive(
            0,
            Ok(OfflineResult::Search(LaptopCacheSearchResponse {
                schema_version: 1,
                project: p,
                hits: vec![],
            })),
        );
        assert!(state.result.is_none());
        assert!(state.error.as_ref().unwrap().contains("scope changed"));
        state.receive(0, Err("Forbidden".into()));
        assert!(state.result.is_none());
        assert!(!state.busy);
    }
    #[test]
    fn cache_rendering_is_plain_untrusted_with_unknown_freshness() {
        let state = state();
        let text = content(&state)
            .iter()
            .map(|l| l.to_string())
            .collect::<Vec<_>>()
            .join("\n");
        assert!(text.contains("NOT current remote authority"));
        assert!(text.contains("sync unknown"));
        assert!(text.contains(&state.project().unwrap().subscription_id.to_string()));
        assert_eq!(
            plain("text\u{1b}]8;;https://evil\u{7}"),
            "text�]8;;https://evil�"
        );
    }
    #[test]
    fn cache_excerpts_keep_shared_source_and_reviewed_evidence_citations() {
        let mut state = state();
        let source = Uuid::new_v4();
        let project = state.project().unwrap().clone();
        state.result = Some(LaptopCacheSearchResponse {
            schema_version: 1,
            project,
            hits: vec![mem_record::LaptopCacheHit {
                stream_id: Uuid::new_v4(),
                author: "shared author".into(),
                authored_at: chrono::Utc::now(),
                locator: Some("javascript:alert(1)".into()),
                digest: "a".repeat(64),
                shared_version: Some(2),
                origin: mem_record::LaptopCacheOrigin::SharedPublication,
                source_record_id: source,
                source_version: 7,
                text: "<script>untrusted</script>".into(),
                summary: "Shared summary".into(),
                read_only: true,
                unavailable_source_ids: vec![],
                evidence: vec![mem_record::LaptopCacheEvidence {
                    label: "reviewed source".into(),
                    text: "Evidence".into(),
                    digest: "b".repeat(64),
                    reviewer: "reviewer".into(),
                    reviewed_at: chrono::Utc::now(),
                }],
            }],
        });
        let text = content(&state)
            .iter()
            .map(|l| l.to_string())
            .collect::<Vec<_>>()
            .join("\n");
        for exact in [
            "shared v2",
            "v7",
            "shared author",
            "reviewed source",
            "reviewer",
            "locator (text only)",
            "<script>untrusted</script>",
        ] {
            assert!(text.contains(exact));
        }
        assert!(text.contains(&source.to_string()));
        assert!(text.contains(&"a".repeat(64)));
        assert!(text.contains(&"b".repeat(64)));
    }
}
