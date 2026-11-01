use crate::{uuid, Transport};
use chrono::{DateTime, Utc};
use reqwest::Method;
use serde_json::Value;
use tauri::State;
use url::Url;

// Keep this finite: neither arbitrary private routes nor operator controls are
// reachable from the renderer. The service remains the authorization boundary.
pub(crate) fn allowed(path: &str, method: &str) -> bool {
    let (raw, query) = path
        .split_once('?')
        .map_or((path, None), |(p, q)| (p, Some(q)));
    let parts: Vec<_> = raw.split('/').collect();
    if let ("GET", ["", "v1", "inbox", "conversations", conversation, "messages"]) =
        (method, parts.as_slice())
    {
        return uuid(conversation) && query.is_some_and(history_query);
    }
    if query.is_some() {
        return false;
    }
    match (method, parts.as_slice()) {
        ("GET", ["", "v1", "network", "status" | "specialists"])
        | ("POST", ["", "v1", "network", "discover"])
        | ("POST", ["", "v1", "network", "identity", "link" | "unlink"])
        | ("GET" | "POST", ["", "v1", "inbox", "conversations"]) => true,
        ("GET", ["", "v1", "network", "identity", "link", attempt]) => uuid(attempt),
        ("GET", ["", "v1", "inbox", "conversations", conversation]) => uuid(conversation),
        (
            "POST",
            ["", "v1", "inbox", "conversations", conversation, "messages" | "members" | "lead" | "read" | "pause"],
        ) => uuid(conversation),
        (
            "POST",
            ["", "v1", "inbox", "conversations", conversation, "members", member, "remove"],
        ) => uuid(conversation) && uuid(member),
        (
            "POST",
            ["", "v1", "inbox", "conversations", conversation, "tasks", task, "respond" | "cancel"],
        ) => uuid(conversation) && uuid(task),
        _ => false,
    }
}

fn history_query(query: &str) -> bool {
    let mut before = None;
    let mut limit = None;
    for pair in query.split('&') {
        let Some((key, value)) = pair.split_once('=') else {
            return false;
        };
        if value.is_empty() || value.len() > 19 || !value.bytes().all(|b| b.is_ascii_digit()) {
            return false;
        }
        let Ok(number) = value.parse::<i64>() else {
            return false;
        };
        if number < 1 || number.to_string() != value {
            return false;
        }
        match key {
            "before_sequence" if before.is_none() => before = Some(number),
            "limit" if limit.is_none() && number <= 100 => limit = Some(number),
            _ => return false,
        }
    }
    before.is_some() && limit.is_some()
}

fn public_url(value: &str) -> Result<Url, String> {
    let url = Url::parse(value).map_err(|_| "Invalid external link")?;
    if url.scheme() != "https"
        || url.host_str().is_none()
        || !url.username().is_empty()
        || url.password().is_some()
        || value.len() > 16_384
    {
        return Err("Only HTTPS links without embedded credentials can be opened".into());
    }
    Ok(url)
}

fn authorization_url(value: &Value, attempt: &str) -> Result<Url, String> {
    if value.get("attempt_id").and_then(Value::as_str) != Some(attempt)
        || value.get("status").and_then(Value::as_str) != Some("pending")
    {
        return Err("This account-link attempt is no longer pending".into());
    }
    let expires = value
        .get("expires_at")
        .and_then(Value::as_str)
        .and_then(|v| DateTime::parse_from_rfc3339(v).ok())
        .ok_or("Invalid account-link expiry")?;
    if expires <= Utc::now() {
        return Err("This account-link attempt has expired".into());
    }
    public_url(
        value
            .get("authorization_url")
            .and_then(Value::as_str)
            .ok_or("No authorization address is available")?,
    )
}

#[tauri::command]
pub(crate) async fn open_network_identity(
    attempt_id: String,
    state: State<'_, Transport>,
) -> Result<(), String> {
    if !uuid(&attempt_id) {
        return Err("Invalid account-link attempt".into());
    }
    let (generation, session) = {
        let connection = state.0.lock().map_err(|_| "Connection unavailable")?;
        (
            connection.generation,
            connection.session.clone().ok_or("Sign in first")?,
        )
    };
    let result = session
        .request(
            &format!("/v1/network/identity/link/{attempt_id}"),
            Method::GET,
            None,
        )
        .await?;
    let url = authorization_url(&result, &attempt_id)?;
    let connection = state.0.lock().map_err(|_| "Connection unavailable")?;
    if connection.generation != generation {
        return Err("Connection changed; account linking was not opened".into());
    }
    // No Codifix cookie or bearer credential is sent to the identity provider.
    open::that(url.as_str()).map_err(|_| "Cannot open the system browser".into())
}

#[tauri::command]
pub(crate) fn open_network_link(url: String) -> Result<(), String> {
    open::that(public_url(&url)?.as_str()).map_err(|_| "Cannot open the system browser".into())
}

#[cfg(test)]
mod tests {
    use super::*;
    const ID: &str = "11111111-1111-4111-8111-111111111111";

    #[test]
    fn native_allowlist_is_exact() {
        assert!(allowed("/v1/network/status", "GET"));
        assert!(allowed("/v1/network/identity/link", "POST"));
        assert!(allowed(
            &format!("/v1/inbox/conversations/{ID}/messages"),
            "POST"
        ));
        assert!(allowed(
            &format!("/v1/inbox/conversations/{ID}/tasks/{ID}/cancel"),
            "POST"
        ));
        for path in [
            "/v1/network/grants",
            "/v1/network/identity/link/../",
            "/v1/inbox/conversations/bad/messages",
            "/v1/inbox/conversations/",
            "/v1/inbox/mandates",
        ] {
            assert!(!allowed(path, "POST"), "{path}");
        }
        assert!(!crate::allowed(
            "/v1/network/status?endpoint=https://evil",
            "GET"
        ));
        assert!(!crate::allowed("/v1/network/%73tatus", "GET"));
        assert!(!allowed("/v1/network/specialists", "POST"));
    }

    #[test]
    fn browser_link_requires_current_exact_attempt_and_safe_url() {
        let valid = serde_json::json!({"attempt_id": ID, "status": "pending", "expires_at":"2099-01-01T00:00:00Z", "authorization_url":"https://pds.example/authorize?request_uri=fixture"});
        assert!(authorization_url(&valid, ID).is_ok());
        assert!(authorization_url(&valid, "wrong").is_err());
        for (field, value) in [
            ("status", "complete"),
            ("expires_at", "2000-01-01T00:00:00Z"),
            ("authorization_url", "javascript:alert(1)"),
            ("authorization_url", "https://user:pass@pds.example/"),
        ] {
            let mut invalid = valid.clone();
            invalid[field] = Value::String(value.into());
            assert!(authorization_url(&invalid, ID).is_err(), "{field}: {value}");
        }
    }

    #[test]
    fn history_queries_allow_only_bounded_numeric_cursors() {
        let path = format!("/v1/inbox/conversations/{ID}/messages");
        assert!(crate::allowed(
            &format!("{path}?before_sequence=501&limit=100"),
            "GET"
        ));
        assert!(crate::allowed(
            &format!("{path}?limit=1&before_sequence=2"),
            "GET"
        ));
        for query in [
            "",
            "before_sequence=1",
            "before_sequence=1&limit=101",
            "before_sequence=0&limit=10",
            "before_sequence=-1&limit=10",
            "before_sequence=1&limit=0",
            "before_sequence=1&limit=10&limit=20",
            "before_sequence=1&limit=10&endpoint=https://evil",
            "before_sequence=9223372036854775808&limit=10",
            "before_sequence=%31&limit=10",
            "before_sequence=01&limit=10",
        ] {
            assert!(
                !crate::allowed(&format!("{path}?{query}"), "GET"),
                "{query}"
            );
        }
        assert!(!crate::allowed(
            &format!("{path}?before_sequence=1&limit=10"),
            "POST"
        ));
        assert!(!crate::allowed(&path, "GET"));
    }
}
