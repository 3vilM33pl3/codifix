// SPDX-License-Identifier: AGPL-3.0-or-later
use super::*;
use crate::tui::theme::{Theme, themed_block, themed_focus_block};
use codifix_record::agency_observation::{ExchangeEvent, ExchangeSummary};
use ratatui::{
    layout::{Alignment, Constraint, Layout, Rect},
    style::Style,
    text::{Line, Span},
    widgets::{Paragraph, Wrap},
};
use serde_json::Value;

// Network strings are untrusted. Keep newlines as content, never terminal controls,
// bidirectional overrides, hyperlinks, or interpreted markdown/ANSI styling.
pub(super) fn plain(text: &str) -> String {
    text.chars().map(|c| if c == '\n' || (!c.is_control() && !matches!(c, '\u{061c}' | '\u{200b}'..='\u{200f}' | '\u{202a}'..='\u{202e}' | '\u{2060}'..='\u{206f}')) { c } else { '�' }).collect()
}
fn value(value: &Value, key: &str) -> String {
    value
        .get(key)
        .map(|v| {
            v.as_str()
                .map(str::to_owned)
                .unwrap_or_else(|| v.to_string())
        })
        .map(|s| plain(&s))
        .unwrap_or_else(|| "unknown".into())
}
fn line(text: impl AsRef<str>) -> Line<'static> {
    Line::from(plain(text.as_ref()))
}
fn companion_lines(state: &ConversationsState) -> Vec<Line<'static>> {
    let mut lines = vec![];
    if state.companion_busy {
        lines.push(line("Loading companion status…"));
    }
    if let Some(error) = &state.companion_error {
        lines.push(line(format!("Unavailable / stale observation: {error}")));
    }
    if let Some(status) = &state.companion {
        lines.push(line(format!(
            "Companion: {} | enabled={} | exchange access={}",
            status.state,
            status.enabled,
            state.can_observe()
        )));
        lines.push(line(format!(
            "Observed heartbeat: {} | version {}",
            status
                .last_heartbeat_at
                .map(|t| t.to_rfc3339())
                .unwrap_or_else(|| "unknown".into()),
            status.version.as_deref().unwrap_or("unknown")
        )));
        lines.push(line(format!(
            "Enabled specialists: {} | enrolled peers: {} | last exchange {}",
            status.enabled_specialists.join(", "),
            status.enrolled_peers.len(),
            status
                .last_exchange_at
                .map(|t| t.to_rfc3339())
                .unwrap_or_else(|| "unknown".into())
        )));
        lines.push(line(
            "Reported heartbeat is observation, not proof of peer reachability or task success.",
        ));
        if !status.enabled {
            lines.push(line("Companion disabled for this service."));
        }
        if !status.exchange_access {
            lines.push(line("Exchange inspection is disabled or your project credential has no observation grant."));
        }
    } else if !state.companion_busy && state.companion_error.is_none() {
        lines.push(line("Companion status not loaded. Press r."));
    }
    lines
}
pub(in crate::tui) fn draw_companions(frame: &mut ratatui::Frame<'_>, app: &App, area: Rect) {
    let state = &app.conversations;
    let mut lines = if app.watchers.companion_focus {
        vec![]
    } else {
        companion_lines(state)
    };
    let peers = state.peers();
    if peers.is_empty() {
        lines.push(line(
            "No peers on the observed exchange page. Enter opens Agent exchanges.",
        ));
    } else {
        lines.push(line("Peers: j/k select | Enter exchanges"));
        for (i, peer) in peers
            .iter()
            .enumerate()
            .skip(state.peer_selected)
            .take(usize::from(area.height.saturating_sub(4)).min(5))
        {
            lines.push(line(format!(
                "{} {}",
                if i == state.peer_selected { ">" } else { " " },
                peer.chars()
                    .take(usize::from(area.width.saturating_sub(5)))
                    .collect::<String>()
            )));
        }
    }
    if app.watchers.companion_focus {
        lines.extend(companion_lines(state));
    }
    frame.render_widget(
        Paragraph::new(lines)
            .wrap(Wrap { trim: false })
            .block(themed_focus_block(
                "Companions [c switches focus]",
                app.watchers.companion_focus,
            )),
        area,
    );
}

pub(in crate::tui) fn draw(frame: &mut ratatui::Frame<'_>, app: &App, area: Rect) {
    let state = &app.conversations;
    let chunks =
        Layout::vertical([Constraint::Length(4.min(area.height)), Constraint::Min(0)]).split(area);
    let area_name = app
        .meta
        .area
        .as_ref()
        .map_or("custom service", |a| a.name.as_str());
    let navigation = if state.my_inbox {
        "Agent exchanges  |  [My Inbox] (human identity, area-wide) | v switch | read only"
    } else if state.live_selected() {
        if state.live.paused() {
            "Paused · End to follow · m tasks · v Inbox"
        } else {
            "Live · / choose peer · m tasks · v Inbox"
        }
    } else {
        "[Agent exchanges] · m live transcript · v Inbox"
    };
    let status = if state.live_selected() && state.editing_peer {
        format!("Peer: {}█ · Enter apply · Esc cancel", state.peer_draft)
    } else if state.live_selected() && state.exchange_error.is_some() {
        state
            .exchange_error
            .as_deref()
            .unwrap_or_default()
            .to_owned()
    } else {
        navigation.to_owned()
    };
    let scope = vec![
        line(format!("Area {area_name} | project {}", app.project)),
        line(status),
    ];
    frame.render_widget(
        Paragraph::new(scope).block(themed_block("Conversations")),
        chunks[0],
    );
    if state.live_selected() {
        draw_live(frame, state, chunks[1]);
        return;
    }
    let (lines, scroll, title) = if state.my_inbox {
        (
            if app.service.inbox_enabled == Some(true) {
                inbox_lines(state)
            } else {
                vec![line(if app.service.inbox_enabled == Some(false) {
                    "My Inbox is disabled for the selected area. Press r to refresh service health."
                } else {
                    "My Inbox unavailable: service availability metadata is unknown. Press r to retry health."
                })]
            },
            state.inbox.scroll,
            "My Inbox",
        )
    } else {
        let title = match (state.detail.is_some(), state.exchange_metadata) {
            (true, true) => "Exchange details",
            (true, false) => "Transcript",
            (false, _) => "Agent exchanges",
        };
        (exchange_lines(state), state.scroll, title)
    };
    // Clamp to the rendered wrapped height: very long content remains reachable,
    // but repeated page keys never strand the viewport beyond the final line.
    let block = themed_block(title);
    let inner = block.inner(chunks[1]);
    let paragraph = Paragraph::new(lines).wrap(Wrap { trim: false });
    let max = scroll_limit(&paragraph, inner);
    frame.render_widget(
        paragraph
            .scroll((scroll.min(max), 0))
            .style(Style::default().fg(Theme::TEXT))
            .block(block),
        chunks[1],
    );
}
fn draw_live(frame: &mut ratatui::Frame<'_>, state: &ConversationsState, area: Rect) {
    let block = themed_block("Agent conversation");
    let inner = block.inner(area);
    let lines = live_lines(state);
    let limit = live_scroll_limit(&lines, inner);
    let offset = if state.live.paused() {
        state.live.scroll.min(limit)
    } else {
        limit
    };
    let (lines, offset) = live_window(lines, inner.width, offset);
    frame.render_widget(
        Paragraph::new(lines)
            .scroll((offset, 0))
            .wrap(Wrap { trim: false })
            .style(Style::default().fg(Theme::TEXT))
            .block(block),
        area,
    );
}

fn live_scroll_limit(lines: &[Line<'static>], inner: Rect) -> usize {
    Paragraph::new(lines.to_vec())
        .wrap(Wrap { trim: false })
        .line_count(inner.width)
        .saturating_sub(usize::from(inner.height))
}

// Ratatui's scroll offset is u16. Skip complete logical lines first when a
// long transcript exceeds that range, then scroll the remaining wrapped text.
pub(super) fn live_window(
    mut lines: Vec<Line<'static>>,
    width: u16,
    mut offset: usize,
) -> (Vec<Line<'static>>, u16) {
    if offset > usize::from(u16::MAX) {
        let mut skip = 0;
        while offset > usize::from(u16::MAX) && skip < lines.len() {
            let height = Paragraph::new(lines[skip].clone())
                .wrap(Wrap { trim: false })
                .line_count(width);
            offset = offset.saturating_sub(height);
            skip += 1;
        }
        lines.drain(..skip);
    }
    (lines, offset.min(usize::from(u16::MAX)) as u16)
}

pub(super) fn live_content_scroll_limit(state: &ConversationsState, content: Rect) -> usize {
    let body = Layout::vertical([
        Constraint::Length(4.min(content.height)),
        Constraint::Min(0),
    ])
    .split(content)[1];
    live_scroll_limit(
        &live_lines(state),
        themed_block("Agent conversation").inner(body),
    )
}

pub(super) fn live_lines(state: &ConversationsState) -> Vec<Line<'static>> {
    let mut lines = Vec::new();
    if let Some(peer) = &state.peer {
        lines.push(line(format!("Peer: {peer}")));
    }
    if !state.can_observe() {
        lines.extend(companion_lines(state));
        return lines;
    }
    if !state.live.loaded {
        lines.push(line("Loading the conversation…"));
        return lines;
    }

    let mut events = Vec::new();
    let mut retained = false;
    let mut truncated = false;
    for detail in state.live.visible() {
        retained |= !detail.exchange.journal_available;
        truncated |= detail.events_truncated;
        for event in &detail.events {
            let (kind, body) = transcript_event(detail, event);
            if !visible_event(kind, body) {
                continue;
            }
            let at = if kind == "request" && !detail.exchange.journal_available {
                detail.exchange.created_at
            } else {
                event.recorded_at
            };
            events.push((
                (at, event.sequence, detail.exchange.task_id),
                detail,
                kind,
                body,
            ));
        }
    }
    events.sort_unstable_by_key(|(order, _, _, _)| *order);
    if retained {
        lines.push(
            line("Earlier exchanges contain retained messages.")
                .style(Style::default().fg(Theme::MUTED)),
        );
    }
    if truncated {
        lines.push(
            line("Some exchanges exceed the service's retained detail limit.")
                .style(Style::default().fg(Theme::MUTED)),
        );
    }
    if state.live.archive_before().is_some() && !state.live.paused() {
        lines.push(line("Loading earlier exchanges…").style(Style::default().fg(Theme::MUTED)));
    }
    if events.is_empty() {
        lines.push(line("No messages recorded for this project and peer."));
    }
    for (_, detail, kind, body) in events {
        append_transcript_event(&mut lines, state, detail, kind, body);
    }
    lines
}

pub(super) fn exchange_lines(state: &ConversationsState) -> Vec<Line<'static>> {
    let mut lines = vec![if state.detail.is_some() {
        line(if state.exchange_metadata {
            "m transcript · Esc back"
        } else {
            "m details · Esc back"
        })
        .style(Style::default().fg(Theme::MUTED))
    } else {
        line(format!(
            "Peer: {} · / filter · Enter open",
            state.peer.as_deref().unwrap_or("all")
        ))
    }];
    if state.editing_peer {
        lines.push(line(format!(
            "Peer filter: {}█ (Enter apply / Esc cancel)",
            state.peer_draft
        )));
    }
    if state.exchange_busy {
        lines.push(line("Loading… previous observations may be stale."));
    }
    if let Some(error) = &state.exchange_error {
        lines.push(line(format!(
            "Unavailable / denied; previous observations are stale: {error}"
        )));
    }
    if !state.can_observe() {
        lines.extend(companion_lines(state));
        return lines;
    }
    if state.companion_error.is_some() {
        lines.push(line(
            "Companion status unavailable; retained exchange observations may be stale.",
        ));
    }
    if let Some(detail) = &state.detail {
        if state.exchange_metadata {
            exchange_metadata(&mut lines, state, detail);
        } else {
            exchange_transcript(&mut lines, state, detail);
        }
    } else if let Some(page) = &state.page {
        lines.push(line(format!(
            "{} exchange(s) on page | {}",
            page.exchanges.len(),
            if page.next_before.is_some() {
                "PgDn older page"
            } else {
                "end of results"
            }
        )));
        if page.exchanges.is_empty() {
            lines.push(line("No exchanges match this project, peer and page."));
        }
        for (i, exchange) in page
            .exchanges
            .iter()
            .enumerate()
            .skip(state.selected)
            .take(5)
        {
            let (from, to) = participants(state, exchange);
            lines.push(line(format!(
                "{} {}",
                if i == state.selected { ">" } else { " " },
                exchange.objective
            )));
            lines.push(
                line(format!(
                    "  {from} → {to} · {}",
                    exchange.status.replace('_', " ")
                ))
                .style(Style::default().fg(Theme::MUTED)),
            );
        }
    } else {
        lines.extend(companion_lines(state));
        if !state.exchange_busy
            && state.exchange_error.is_none()
            && state.companion.as_ref().is_some_and(|c| c.exchange_access)
        {
            lines.push(line("Press r to load project exchanges."));
        }
    }
    lines
}
fn participants<'a>(
    state: &'a ConversationsState,
    exchange: &'a ExchangeSummary,
) -> (&'a str, &'a str) {
    let local = state
        .scope
        .as_ref()
        .and_then(|scope| scope.area.as_ref())
        .map(|(name, _)| name.as_str())
        .or_else(|| {
            state
                .companion
                .as_ref()
                .and_then(|status| status.area.as_deref())
        })
        .unwrap_or("local");
    if exchange.direction == "incoming" {
        (&exchange.peer, local)
    } else {
        (local, &exchange.peer)
    }
}

fn exchange_transcript(
    lines: &mut Vec<Line<'static>>,
    state: &ConversationsState,
    detail: &ExchangeDetail,
) {
    if !detail.exchange.journal_available {
        lines.push(
            line("Earlier exchange · retained messages only")
                .style(Style::default().fg(Theme::MUTED)),
        );
    }
    let start = lines.len();
    for event in &detail.events {
        let (kind, body) = transcript_event(detail, event);
        if visible_event(kind, body) {
            append_transcript_event(lines, state, detail, kind, body);
        }
    }
    if lines.len() == start {
        lines.push(line("No messages were retained."));
    }
    if detail.events_truncated {
        lines.push(line("More events were recorded than this view can show."));
    }
}

fn transcript_event<'a>(detail: &ExchangeDetail, event: &'a ExchangeEvent) -> (&'a str, &'a str) {
    if event.kind == "snapshot" && !detail.exchange.journal_available {
        // Legacy timestamps apply to the snapshot, not to individual turns.
        event
            .body
            .split_once(": ")
            .unwrap_or(("snapshot", event.body.as_str()))
    } else {
        (event.kind.as_str(), event.body.as_str())
    }
}

fn visible_event(kind: &str, body: &str) -> bool {
    !body.trim().is_empty()
        && matches!(
            kind,
            "request" | "clarification" | "answer" | "result" | "snapshot"
        )
}

fn append_transcript_event(
    lines: &mut Vec<Line<'static>>,
    state: &ConversationsState,
    detail: &ExchangeDetail,
    kind: &str,
    body: &str,
) {
    let (initiator, responder) = participants(state, &detail.exchange);
    let incoming = detail.exchange.direction == "incoming";
    if let Some(note) = recorded_notice(kind, body) {
        lines.push(line(note).style(Style::default().fg(Theme::MUTED)));
        return;
    }
    match kind {
        "request" | "answer" => transcript_message(lines, initiator, incoming, body),
        "clarification" | "result" => transcript_message(lines, responder, !incoming, body),
        "snapshot" => lines.push(line(body)),
        _ => {}
    }
}

fn recorded_notice(kind: &str, body: &str) -> Option<&'static str> {
    match (kind, body) {
        ("result", "Verified delegated_result_reported") => Some("Peer reported completion."),
        ("result", "Verified status_observed") => Some("Companion recorded the status check."),
        ("result", "Verified plan_valid") => {
            Some("Companion recorded that the plan passed validation.")
        }
        ("result", "Verified linear_readback_verified") => {
            Some("Companion recorded the Linear readback.")
        }
        ("result", "Verified linear_state_observed") => {
            Some("Companion recorded the Linear state.")
        }
        ("result", "Verified model_output_valid_json") => {
            Some("Companion recorded that the output passed JSON validation.")
        }
        (
            "clarification",
            "Execution stopped at an authorization, validation, or uncertain-effect boundary. Inspect the private effect ledger before resuming.",
        ) => Some("Execution stopped. See details for the recorded reason."),
        ("clarification", "The peer requires input through its authorized channel.") => {
            Some("The peer requires input through its authorized channel.")
        }
        ("clarification", reason)
            if reason.starts_with("This operation can verify the criterion '")
                && reason.ends_with("arbitrary outcome claims cannot be certified.") =>
        {
            Some("The companion could not verify the requested acceptance criterion. See details.")
        }
        _ => None,
    }
}

fn transcript_message(lines: &mut Vec<Line<'static>>, speaker: &str, from_peer: bool, body: &str) {
    let speaker_style = Style::default().fg(if from_peer {
        Theme::ACCENT
    } else {
        Theme::ACCENT_STRONG
    });
    let speaker = plain(speaker).replace('\n', " ");
    let mut rows = body.trim_end().split('\n').peekable();
    let mut first = true;
    while let Some(row) = rows.next() {
        let mut spans = Vec::new();
        if first && !from_peer {
            spans.push(Span::styled(format!("{speaker} > "), speaker_style));
        }
        spans.push(Span::raw(plain(row)));
        if from_peer && rows.peek().is_none() {
            spans.push(Span::styled(format!(" < {speaker}"), speaker_style));
        }
        lines.push(Line::from(spans).alignment(if from_peer {
            Alignment::Right
        } else {
            Alignment::Left
        }));
        first = false;
    }
}

fn exchange_metadata(
    lines: &mut Vec<Line<'static>>,
    state: &ConversationsState,
    detail: &ExchangeDetail,
) {
    let exchange = &detail.exchange;
    if let Some((_, id)) = state.scope.as_ref().and_then(|scope| scope.area.as_ref()) {
        lines.push(line(format!("Area instance: {id}")));
    }
    lines.push(line(format!(
        "{} | {} | {} | {}",
        exchange.task_id, exchange.peer, exchange.direction, exchange.status
    )));
    lines.push(line(format!(
        "Specialist {} | created {} | updated {}",
        exchange.specialist_id, exchange.created_at, exchange.updated_at
    )));
    if let Some(id) = &detail.remote_task_id {
        lines.push(line(format!("Remote task: {id}")));
    }
    lines.push(line(format!("Objective: {}", exchange.objective)));
    lines.push(line(if detail.independently_verified {
        "Service reports independently verified evidence."
    } else {
        "Observed journal / legacy snapshot; independently verified: false."
    }));
    if !exchange.journal_available {
        lines.push(line(
            "No event journal is available; this may be a legacy task snapshot.",
        ));
    }
    if detail.events_truncated {
        lines.push(line(
            "Journal truncated by the service's bounded detail limit.",
        ));
    }
    for event in &detail.events {
        lines.push(line(format!(
            "\n#{} {} {}",
            event.sequence, event.kind, event.recorded_at
        )));
        for body in event.body.lines() {
            lines.push(line(body));
        }
    }
    if detail.events.is_empty() {
        lines.push(line("No journal events were returned."));
    }
}

pub(super) fn inbox_lines(state: &ConversationsState) -> Vec<Line<'static>> {
    let inbox = &state.inbox;
    let mut lines = vec![
        line(if inbox.signed_in() {
            "Signed in as a human. o sign out | r refresh | Enter inspect"
        } else {
            "Signed out. i sign in with your browser | o retry sign-out cleanup"
        }),
        line(
            "This personal inbox belongs to the selected area; project service tokens do not authenticate it.",
        ),
    ];
    if inbox.busy {
        lines.push(line(if inbox.login_pending {
            "Waiting for browser sign-in… Esc cancels."
        } else {
            "Loading… previous data may be stale."
        }));
    }
    if let Some(message) = &inbox.message {
        lines.push(line(message));
    }
    if let Some(error) = &inbox.error {
        lines.push(line(format!(
            "Unavailable / denied; refresh required: {error}"
        )));
    }
    // A revoked or expired session must never retain visible private data.
    if !inbox.signed_in() {
        return lines;
    }
    if let Some(network) = &inbox.network {
        lines.push(line(format!(
            "Inbox enabled={} | home instance {} | identity {}",
            value(network, "enabled"),
            value(network, "home_instance_id"),
            network
                .get("identity")
                .filter(|v| !v.is_null())
                .map(|i| value(i, "handle"))
                .unwrap_or_else(|| "not linked".into())
        )));
    }
    if let Some(detail) = &inbox.detail {
        lines.extend(inbox_detail_lines(detail));
    } else if let Some(page) = inbox.page.as_ref().and_then(Value::as_array) {
        if page.is_empty() {
            lines.push(line(
                "No conversations in this area for the signed-in human.",
            ));
        }
        for (i, conversation) in page.iter().enumerate().skip(inbox.selected).take(8) {
            lines.push(line(format!(
                "{} {} | {} | unread {} | {}",
                if i == inbox.selected { ">" } else { " " },
                value(conversation, "id"),
                value(conversation, "title"),
                value(conversation, "unread_count"),
                value(conversation, "updated_at")
            )));
        }
    } else {
        lines.push(line("Press r to load conversations."));
    }
    lines
}

pub(super) fn inbox_detail_lines(detail: &Value) -> Vec<Line<'static>> {
    let mut lines = vec![];
    if let Some(conversation) = detail.get("conversation") {
        lines.push(line(format!(
            "{} | {} | paused={}",
            value(conversation, "id"),
            value(conversation, "title"),
            value(conversation, "paused")
        )));
    }
    lines.push(line(
        "Esc list | j/k PgUp/PgDn scroll | n load older messages",
    ));
    lines.push(line("Participants:"));
    if let Some(members) = detail.get("members").and_then(Value::as_array) {
        for member in members {
            let specialist = member.get("specialist").unwrap_or(&Value::Null);
            lines.push(line(format!(
                "  {} | {} | active={} | member {}",
                value(specialist, "display_name"),
                value(specialist, "instance_id"),
                value(member, "active"),
                value(member, "id")
            )));
            lines.push(line(format!(
                "  Disclosure: {}",
                value(member, "disclosure_summary")
            )));
        }
    }
    lines.push(line("Messages (untrusted plain text):"));
    if let Some(messages) = detail.get("messages").and_then(Value::as_array) {
        if messages.is_empty() {
            lines.push(line("No messages returned."));
        }
        for message in messages {
            lines.push(line(format!(
                "#{} {} | {} | delivery {}",
                value(message, "sequence"),
                value(message, "role"),
                value(message, "created_at"),
                value(message, "delivery")
            )));
            for body in value(message, "body").lines() {
                lines.push(line(body));
            }
        }
    }
    lines.push(line("Task cards:"));
    if let Some(tasks) = detail.get("tasks").and_then(Value::as_array) {
        if tasks.is_empty() {
            lines.push(line("No task cards returned."));
        }
        for task in tasks {
            lines.push(line(format!(
                "{} | {} | {}",
                value(task, "id"),
                value(task, "status"),
                value(task, "objective")
            )));
            for field in ["question", "result", "artifacts"] {
                if let Some(v) = task.get(field).filter(|v| !v.is_null()) {
                    lines.push(line(format!(
                        "{field}: {}",
                        v.as_str()
                            .map(str::to_owned)
                            .unwrap_or_else(|| v.to_string())
                    )));
                }
            }
        }
    }
    lines
}

pub(super) fn content_scroll_limit(state: &ConversationsState, content: Rect) -> u16 {
    let chunks = Layout::vertical([
        Constraint::Length(4.min(content.height)),
        Constraint::Min(0),
    ])
    .split(content);
    let inner = themed_block("detail").inner(chunks[1]);
    let lines = if state.my_inbox {
        inbox_lines(state)
    } else {
        exchange_lines(state)
    };
    scroll_limit(&Paragraph::new(lines).wrap(Wrap { trim: false }), inner)
}

pub(super) fn scroll_limit(paragraph: &Paragraph<'_>, inner: Rect) -> u16 {
    paragraph
        .line_count(inner.width)
        .saturating_sub(inner.height as usize)
        .min(u16::MAX as usize) as u16
}
