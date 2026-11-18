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
    }
}
fn state() -> ConversationsState {
    ConversationsState {
        scope: Some(scope("p")),
        ..Default::default()
    }
}
fn detail() -> ExchangeDetail {
    serde_json::from_value(json!({"exchange":{"task_id":Uuid::from_u128(7),"peer":"peer\u{1b}[31m","direction":"outgoing","specialist_id":"inspector","objective":"untrusted\u{202e} objective","status":"completed","created_at":"2026-11-18T12:00:00Z","updated_at":"2026-11-18T12:01:00Z","journal_available":false},"events":[{"sequence":1,"kind":"snapshot","body":"body\u{1b}]8;;https://evil.test\u{7}link","recorded_at":"2026-11-18T12:01:00Z"}],"events_truncated":false,"independently_verified":false})).unwrap()
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
    assert!(s.companion.is_none());
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
