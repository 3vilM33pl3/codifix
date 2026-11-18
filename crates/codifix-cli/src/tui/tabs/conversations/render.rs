// SPDX-License-Identifier: AGPL-3.0-or-later
use super::*;
use crate::tui::theme::{Theme, themed_block, themed_focus_block};
use ratatui::{
    layout::{Constraint, Layout, Rect},
    style::Style,
    text::Line,
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
    let mut lines = companion_lines(state);
    let peers = state.peers();
    if peers.is_empty() {
        lines.push(line(
            "No peers on the observed exchange page. Enter opens Agent exchanges.",
        ));
    } else {
        lines.push(line(
            "Enrolled / observed peers (j/k select, Enter inspect exchanges):",
        ));
        for (i, peer) in peers
            .iter()
            .enumerate()
            .skip(state.peer_selected.saturating_sub(2))
            .take(5)
        {
            lines.push(line(format!(
                "{} {}",
                if i == state.peer_selected { ">" } else { " " },
                peer
            )));
        }
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
        .map(|a| format!("{} / {}", a.name, a.instance_id))
        .unwrap_or_else(|| "custom / no area identity".into());
    let scope = vec![
        line(format!("Area {area_name} | project {}", app.project)),
        line(if state.my_inbox {
            "Agent exchanges  |  [My Inbox] (human identity, area-wide) | v switch | read only"
        } else {
            "[Agent exchanges] (project scoped)  |  My Inbox | v switch | read only"
        }),
    ];
    frame.render_widget(
        Paragraph::new(scope).block(themed_block("Conversations")),
        chunks[0],
    );
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
        (exchange_lines(state), state.scroll, "Agent exchanges")
    };
    // Clamp to the rendered wrapped height: very long content remains reachable,
    // but repeated page keys never strand the viewport beyond the final line.
    let block = themed_block(title);
    let inner = block.inner(chunks[1]);
    let max = crate::tui::render::wrapped_line_count(&lines, inner.width)
        .saturating_sub(inner.height as usize)
        .min(u16::MAX as usize) as u16;
    frame.render_widget(
        Paragraph::new(lines)
            .scroll((scroll.min(max), 0))
            .wrap(Wrap { trim: false })
            .style(Style::default().fg(Theme::TEXT))
            .block(block),
        chunks[1],
    );
}
pub(super) fn exchange_lines(state: &ConversationsState) -> Vec<Line<'static>> {
    let mut lines = vec![line(format!(
        "Peer: {} | / exact peer filter (empty = all) | Home first page",
        state.peer.as_deref().unwrap_or("all")
    ))];
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
        let exchange = &detail.exchange;
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
        lines.push(line("Esc list | j/k or PgUp/PgDn scroll | r refresh"));
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
            .skip(state.selected.saturating_sub(3))
            .take(10)
        {
            lines.push(line(format!(
                "{} {} | {} | {} | {}",
                if i == state.selected { ">" } else { " " },
                exchange.task_id,
                exchange.peer,
                exchange.direction,
                exchange.status
            )));
            lines.push(line(format!(
                "  {} | {} | {}",
                exchange.updated_at, exchange.specialist_id, exchange.objective
            )));
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
        for (i, conversation) in page
            .iter()
            .enumerate()
            .skip(inbox.selected.saturating_sub(4))
            .take(12)
        {
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
