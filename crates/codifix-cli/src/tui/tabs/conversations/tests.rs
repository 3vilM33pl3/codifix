// SPDX-License-Identifier: AGPL-3.0-or-later
use super::*;
use ratatui::{Terminal, backend::TestBackend};
use serde_json::json;

fn scope(project: &str) -> ObservationScope {
    ObservationScope {
        endpoint: "https://local.test".into(),
        native_origin: Some("https://local.test".into()),
        area: Some(("workshop".into(), Uuid::from_u128(1))),
        project: project.into(),
    }
}
fn status() -> CompanionStatus {
    CompanionStatus {
        enabled: true,
        area: Some("workshop".into()),
        instance_id: Some(Uuid::from_u128(1)),
        state: "fresh".into(),
        last_heartbeat_at: None,
        version: Some("test".into()),
        exchange_access: true,
        enabled_specialists: vec!["inspector".into()],
        enrolled_peers: vec![],
        last_exchange_at: None,
    }
}
fn state() -> ConversationsState {
    ConversationsState {
        scope: Some(scope("p")),
        companion: Some(status()),
        ..Default::default()
    }
}
fn detail() -> ExchangeDetail {
    serde_json::from_value(json!({"exchange":{"task_id":Uuid::from_u128(7),"peer":"peer\u{1b}[31m","direction":"outgoing","specialist_id":"inspector","objective":"untrusted\u{202e} objective","status":"completed","created_at":"2026-11-18T12:00:00Z","updated_at":"2026-11-18T12:01:00Z","journal_available":false},"events":[{"sequence":1,"kind":"snapshot","body":"body\u{1b}]8;;https://evil.test\u{7}link","recorded_at":"2026-11-18T12:01:00Z"}],"events_truncated":false,"independently_verified":false,"remote_task_id":null})).unwrap()
}
#[test]
fn observation_replies_require_scope_and_channel_generation() {
    let mut s = state();
    s.companion_generation = 4;
    s.exchange_generation = 8;
    s.receive_observation(
        scope("other-project"),
        4,
        Ok(ObservationResult::Companion(status())),
    );
    assert_eq!(s.companion_generation, 4);
    s.companion = None;
    s.receive_observation(scope("p"), 3, Ok(ObservationResult::Companion(status())));
    assert!(s.companion.is_none());
    s.receive_observation(scope("p"), 4, Ok(ObservationResult::Companion(status())));
    assert!(s.companion.is_some());
    s.receive_observation(scope("p"), 7, Ok(ObservationResult::Detail(detail())));
    assert!(s.detail.is_none());
    s.receive_observation(scope("p"), 8, Ok(ObservationResult::Detail(detail())));
    assert!(s.detail.is_some());
    s.exchange_invalidate();
    s.receive_observation(scope("p"), 8, Ok(ObservationResult::Detail(detail())));
    assert!(s.detail.is_none());
}
#[test]
fn status_identity_mismatch_cannot_become_fresh() {
    let mut s = state();
    let mut wrong = status();
    wrong.instance_id = Some(Uuid::from_u128(2));
    s.receive_observation(scope("p"), 0, Ok(ObservationResult::Companion(wrong)));
    assert!(s.companion.is_none());
    assert!(
        s.companion_error
            .as_deref()
            .unwrap()
            .contains("does not match")
    );
}
#[test]
fn exchange_detail_renders_observed_limits_and_escapes_controls() {
    let mut s = state();
    s.exchange_metadata = true;
    s.detail = Some(detail());
    let text = render::exchange_lines(&s)
        .iter()
        .map(ToString::to_string)
        .collect::<Vec<_>>()
        .join("\n");
    assert!(text.contains("independently verified: false"));
    assert!(text.contains("legacy task snapshot"));
    assert!(!text.contains('\u{1b}'));
    assert!(!text.contains('\u{7}'));
    assert!(!text.contains('\u{202e}'));
    assert!(text.contains("body�]8"));
}
#[test]
fn transcript_keeps_message_order_and_uses_the_actual_initiating_side() {
    use ratatui::layout::Alignment;

    let mut s = state();
    let mut d = detail();
    d.exchange.peer = "homelab".into();
    d.exchange.journal_available = true;
    d.events = [
        ("request", "Renew the certificates"),
        ("status", "working"),
        ("clarification", "Which namespace?"),
        ("answer", "Staging.\nUse the regular rollout."),
        ("result", "Job done"),
        ("status", "completed"),
    ]
    .into_iter()
    .enumerate()
    .map(
        |(sequence, (kind, body))| codifix_record::agency_observation::ExchangeEvent {
            sequence: sequence as i64,
            kind: kind.into(),
            body: body.into(),
            recorded_at: d.exchange.updated_at,
        },
    )
    .collect();
    s.detail = Some(d.clone());
    let lines = render::exchange_lines(&s);
    assert_eq!(lines[1].to_string(), "workshop > Renew the certificates");
    assert_eq!(lines[2].to_string(), "Which namespace? < homelab");
    assert_eq!(lines[2].alignment, Some(Alignment::Right));
    assert_eq!(lines[3].to_string(), "workshop > Staging.");
    assert_eq!(lines[4].to_string(), "Use the regular rollout.");
    assert_eq!(lines[5].to_string(), "Job done < homelab");
    assert_eq!(lines.len(), 6);

    d.exchange.direction = "incoming".into();
    s.detail = Some(d);
    let lines = render::exchange_lines(&s);
    assert_eq!(lines[1].to_string(), "Renew the certificates < homelab");
    assert_eq!(lines[2].to_string(), "workshop > Which namespace?");
    assert_eq!(lines[5].to_string(), "workshop > Job done");
}

#[test]
fn transcript_labels_retained_snapshots_and_keeps_machine_receipts_neutral() {
    let mut s = state();
    let mut d = detail();
    d.exchange.peer = "homelab".into();
    let mut result = d.events[0].clone();
    d.events[0].body = "request: Check the \u{1b}service".into();
    result.body = "result: Verified delegated_result_reported".into();
    d.events.push(result);
    s.detail = Some(d);
    let lines = render::exchange_lines(&s);
    assert_eq!(
        lines[1].to_string(),
        "Earlier exchange · retained messages only"
    );
    assert_eq!(lines[2].to_string(), "workshop > Check the �service");
    assert_eq!(lines[3].to_string(), "Peer reported completion.");
}

#[test]
fn narrow_transcript_can_scroll_to_the_final_reply() {
    use ratatui::widgets::{Paragraph, Wrap};

    let mut s = state();
    let mut d = detail();
    d.exchange.peer = "lab".into();
    d.exchange.journal_available = true;
    d.events[0].kind = "request".into();
    d.events[0].body = "abcdef abcdef abcdef\n".repeat(12);
    let mut result = d.events[0].clone();
    result.kind = "result".into();
    result.body = "Done".into();
    d.events.push(result);
    s.detail = Some(d);
    let mut terminal = Terminal::new(TestBackend::new(11, 4)).unwrap();
    let paragraph = Paragraph::new(render::exchange_lines(&s)).wrap(Wrap { trim: false });
    terminal
        .draw(|f| {
            let end = render::scroll_limit(&paragraph, f.area());
            f.render_widget(paragraph.scroll((end, 0)), f.area());
        })
        .unwrap();
    let rendered: String = terminal
        .backend()
        .buffer()
        .content()
        .iter()
        .map(|cell| cell.symbol())
        .collect();
    assert!(rendered.contains("Done < lab"));
}

#[test]
fn human_replies_from_previous_scope_or_generation_are_discarded() {
    let mut s = state();
    s.receive_inbox(
        scope("old"),
        0,
        Ok(InboxResult::Page(json!([{"title":"private"}]))),
    );
    assert!(s.inbox.page.is_none());
    s.receive_inbox(
        scope("p"),
        99,
        Ok(InboxResult::Page(json!([{"title":"private"}]))),
    );
    assert!(s.inbox.page.is_none());
}
#[test]
fn signed_out_inbox_never_renders_retained_private_values() {
    let mut s = state();
    s.inbox.page = Some(json!([{"title":"private sentinel"}]));
    let text = render::inbox_lines(&s)
        .iter()
        .map(ToString::to_string)
        .collect::<Vec<_>>()
        .join("\n");
    assert!(text.contains("Signed out"));
    assert!(!text.contains("private sentinel"));
    s.inbox.check_expiry();
    assert!(s.inbox.page.is_none());
}
#[test]
fn plain_text_replaces_terminal_and_bidi_controls_but_keeps_lines() {
    assert_eq!(render::plain("a\n\t\u{1b}\u{7}\u{202e}b"), "a\n����b");
}
#[test]
fn exchange_renderer_is_safe_in_tiny_and_standard_viewports() {
    // Test the actual bounded Paragraph layout independently of live networking.
    let mut s = state();
    s.detail = Some(detail());
    let lines = render::exchange_lines(&s);
    for (width, height) in [(1, 1), (30, 8), (80, 24)] {
        let mut terminal = Terminal::new(TestBackend::new(width, height)).unwrap();
        terminal
            .draw(|f| {
                f.render_widget(
                    ratatui::widgets::Paragraph::new(lines.clone())
                        .wrap(ratatui::widgets::Wrap { trim: false }),
                    f.area(),
                )
            })
            .unwrap();
    }
}

#[test]
fn revoked_exchange_access_purges_and_fences_private_observations() {
    for failure in [
        "Observation returned HTTP 401",
        "Observation returned HTTP 403",
        "Observation returned HTTP 404",
    ] {
        let mut s = state();
        s.detail = Some(detail());
        s.peer = Some("private-peer".into());
        s.peer_draft = "private-peer".into();
        s.page = Some(ExchangePage {
            exchanges: vec![detail().exchange],
            next_before: None,
        });
        s.receive_observation(scope("p"), 0, Err(failure.into()));
        assert!(s.page.is_none());
        assert!(s.detail.is_none());
        assert!(s.peers().is_empty());
        assert!(s.peer.is_none());
        assert!(s.peer_draft.is_empty());
        s.receive_observation(scope("p"), 0, Ok(ObservationResult::Detail(detail())));
        assert!(s.detail.is_none());
        let text = render::exchange_lines(&s)
            .iter()
            .map(ToString::to_string)
            .collect::<String>();
        assert!(!text.contains("untrusted"));
    }
    let mut s = state();
    s.detail = Some(detail());
    let mut revoked = status();
    revoked.exchange_access = false;
    s.receive_observation(scope("p"), 0, Ok(ObservationResult::Companion(revoked)));
    assert!(s.detail.is_none());
    assert!(!s.can_observe());
    s.receive_observation(scope("p"), 0, Ok(ObservationResult::Detail(detail())));
    assert!(s.detail.is_none());
}
#[test]
fn identity_mismatch_clears_previously_visible_exchange_bodies() {
    let mut s = state();
    s.detail = Some(detail());
    let mut wrong = status();
    wrong.instance_id = Some(Uuid::from_u128(42));
    s.receive_observation(scope("p"), 0, Ok(ObservationResult::Companion(wrong)));
    assert!(s.detail.is_none());
    assert!(s.page.is_none());
    assert!(!s.can_observe());
}
#[test]
fn inbox_access_withdrawal_discards_rendered_private_cache() {
    for status in [401, 403, 404] {
        let mut s = state();
        s.inbox.page = Some(json!([{"title":"private sentinel"}]));
        s.receive_inbox(scope("p"), 0, Err(format!("Inbox returned HTTP {status}")));
        assert!(s.inbox.page.is_none());
        assert!(s.inbox.detail.is_none());
        s.receive_inbox(
            scope("p"),
            0,
            Ok(InboxResult::Page(json!([{"title":"stale private"}]))),
        );
        assert!(s.inbox.page.is_none());
    }
}

#[test]
fn human_detail_shows_participants_disclosures_messages_and_task_cards_as_plain_text() {
    let detail = json!({
        "conversation":{"id":"conversation","title":"Human conversation","paused":false},
        "members":[{"id":"member", "active":true, "specialist":{"display_name":"Inspector", "instance_id":"instance"}, "disclosure_summary":"Reviewed history only"}],
        "messages":[{"sequence":1,"role":"human","created_at":"today","delivery":"delivered","body":"hello\u{1b}[31m\u{202e}"}],
        "tasks":[{"id":"task", "status":"awaiting_human", "objective":"Inspect safely", "question":"Which scope?", "result":{"answer":"observed"}, "artifacts":[]}]
    });
    let text = render::inbox_detail_lines(&detail)
        .iter()
        .map(ToString::to_string)
        .collect::<Vec<_>>()
        .join("\n");
    for expected in [
        "Inspector",
        "Reviewed history only",
        "#1 human",
        "hello�[31m�",
        "awaiting_human",
        "Which scope?",
        "observed",
    ] {
        assert!(text.contains(expected), "missing {expected}");
    }
    assert!(!text.contains('\u{1b}'));
    assert!(!text.contains('\u{202e}'));
}

#[tokio::test]
async fn repeated_detail_scrolling_stops_at_the_end_and_can_move_back() {
    let (tx, _rx) = tokio::sync::mpsc::unbounded_channel();
    let mut app = App::new(
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
    );
    let api = ApiClient::new(reqwest::Client::new(), serde_json::from_value(json!({"service":{"bind_addr":"127.0.0.1:1"},"database":{"url":"postgresql://unused/unused"}})).unwrap());
    app.observation_tick(&api);
    app.active_tab = TabKind::Conversations;
    app.conversations.companion = Some(status());
    app.conversations.companion_loaded = Some(Instant::now());
    app.conversations.page = Some(ExchangePage {
        exchanges: vec![],
        next_before: None,
    });
    let mut d = detail();
    d.events[0].body = (0..150).map(|i| format!("line {i}\n")).collect();
    app.conversations.detail = Some(d);
    for _ in 0..100 {
        app.conversations_key(KeyEvent::from(KeyCode::PageDown), &api);
    }
    let end = app.conversations.scroll;
    assert!(end > 0 && end < 200);
    app.conversations_key(KeyEvent::from(KeyCode::PageUp), &api);
    assert_eq!(app.conversations.scroll, end.saturating_sub(8));
    assert!(app.conversations_key(KeyEvent::from(KeyCode::Char('m')), &api));
    assert!(app.conversations.exchange_metadata);
    assert_eq!(app.conversations.scroll, 0);
    app.conversations_key(KeyEvent::from(KeyCode::Char('m')), &api);
    assert!(!app.conversations.exchange_metadata);
}

#[test]
fn missing_companion_route_clears_cached_heartbeat_and_exchange_detail() {
    let mut s = state();
    s.detail = Some(detail());
    s.receive_observation(
        scope("p"),
        0,
        Ok(ObservationResult::CompanionError(
            "Observation returned HTTP 404".into(),
        )),
    );
    assert!(s.companion.is_none());
    assert!(s.detail.is_none());
    assert!(!s.can_observe());
}
