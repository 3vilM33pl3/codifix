// SPDX-License-Identifier: AGPL-3.0-or-later
//! Plain-language metadata only. Keep stdout JSON unchanged; TUI summaries age
//! on redraw, and never turn an old successful observation into current health.
use chrono::{DateTime, Utc};
use serde_json::Value;

fn date(seconds: &Value) -> String {
    seconds
        .as_i64()
        .and_then(|s| DateTime::from_timestamp(s, 0))
        .map(|d| d.to_rfc3339())
        .unwrap_or_else(|| "unknown".into())
}
fn fresh(seconds: &Value, now: i64, maximum_age: i64) -> bool {
    seconds
        .as_i64()
        .is_some_and(|s| s <= now.saturating_add(30) && now.saturating_sub(s) <= maximum_age)
}
pub(super) fn lines(status: &Value, now: DateTime<Utc>) -> Vec<String> {
    let Some(gateway) = status.get("gateway").filter(|v| v.is_object()) else {
        return vec!["Gateway credential status unavailable; publication health is unknown. Local retained data remains usable.".into()];
    };
    let c = &gateway["credentials"];
    let now = now.timestamp();
    let label = if !fresh(&c["observed_at"], now, 120) {
        "Gateway observation stale or unknown; refresh status."
    } else if c["expires_at"].as_i64().is_some_and(|s| s <= now) {
        "Access credential expired; publication is blocked."
    } else if gateway["pending"]["blocked"] == true {
        "Publication is blocked; inspect its recorded failure."
    } else if c["state"] == "static_unverified" {
        "Static credential expiry is unknown; no healthy credential is claimed."
    } else if c["state"] == "usable"
        && c["publication_usable"] == true
        && c["expires_at"].as_i64().is_some()
    {
        "Credential eligible for publication attempts; this is not proof of PDS availability or successful publication."
    } else {
        "Publication credential requires operator attention; publication health is not confirmed."
    };
    let maintenance = if !fresh(&c["maintenance_observed_at"], now, 180) {
        "unknown or stale"
    } else {
        match c["maintenance_state"].as_str() {
            Some("healthy") => "observed running",
            Some("refreshing") => "refresh in progress",
            Some("retry_wait") => "waiting for a bounded retry",
            Some("intervention") => "operator intervention required",
            _ => "unknown",
        }
    };
    let failure = match gateway["pending"]["failure"].as_str() {
        Some("authentication") => "credential rejected; awaiting a newer verified generation",
        Some("authorization_denied") => "PDS policy denied access; operator intervention required",
        Some("immutable_conflict") => "immutable-record conflict; operator intervention required",
        Some("invalid_receipt") => "repository proof invalid; operator intervention required",
        Some("attempts_exhausted") => "bounded retries exhausted; operator intervention required",
        Some("unavailable") => "transport unavailable",
        None => "no recorded failure",
        Some(_) => "unknown recorded failure",
    };
    let mut out = vec![
        label.into(),
        format!(
            "Access expires {}; observed {}. Renewal: {maintenance}; last success {}; next retry {}.",
            date(&c["expires_at"]),
            date(&c["observed_at"]),
            date(&c["last_success"]),
            date(&c["retry_at"])
        ),
        format!(
            "Observed PDS operator TLS leaf expires {}. No automatic certificate or CA changes.",
            date(&c["tls_expires_at"])
        ),
    ];
    if let Some(expiry) = c["tls_expires_at"].as_i64() {
        if expiry <= now {
            out.push("WARNING: observed PDS operator TLS certificate has expired; operator intervention is required. No automatic issuer or CA changes.".into());
        } else if expiry.saturating_sub(now) <= 86_400 {
            out.push("WARNING: observed PDS operator TLS certificate expires within one day; arrange operator renewal. No automatic issuer or CA changes.".into());
        }
    }
    if gateway["pending"].is_object() {
        out.push(format!("Pending publication: {failure}."));
    }
    out
}

#[cfg(test)]
mod tests {
    use super::*;
    use serde_json::json;
    #[test]
    fn tls_warnings_follow_observed_expiry_and_age_on_redraw() {
        let now = DateTime::from_timestamp(1_800_000_000, 0).unwrap();
        let mut value =
            json!({"gateway":{"credentials":{"tls_expires_at":now.timestamp()+86_401}}});
        assert!(!lines(&value, now).join("\n").contains("WARNING:"));
        assert!(
            lines(&value, now + chrono::Duration::seconds(1))
                .join("\n")
                .contains("expires within one day")
        );
        assert!(
            lines(&value, now + chrono::Duration::seconds(86_401))
                .join("\n")
                .contains("has expired")
        );
        value["gateway"]["credentials"]["tls_expires_at"] = Value::Null;
        let unknown = lines(&value, now).join("\n");
        assert!(unknown.contains("leaf expires unknown"));
        assert!(!unknown.contains("WARNING:"));
    }
    #[test]
    fn missing_stale_static_expired_and_blocked_never_claim_eligible_credentials() {
        let now = DateTime::from_timestamp(1_800_000_000, 0).unwrap();
        assert!(lines(&json!({}), now).join("\n").contains("unknown"));
        let good = json!({"gateway":{"credentials":{"observed_at":now.timestamp(),"state":"usable","publication_usable":true,"expires_at":now.timestamp()+300,
            "maintenance_state":"healthy","maintenance_observed_at":now.timestamp()},"pending":null}});
        assert!(lines(&good, now)[0].starts_with("Credential eligible"));
        for change in ["stale", "static", "expired", "blocked", "missing_expiry"] {
            let mut value = good.clone();
            match change {
                "stale" => {
                    value["gateway"]["credentials"]["observed_at"] = json!(now.timestamp() - 121)
                }
                "static" => value["gateway"]["credentials"]["state"] = json!("static_unverified"),
                "expired" => value["gateway"]["credentials"]["expires_at"] = json!(now.timestamp()),
                "blocked" => {
                    value["gateway"]["pending"] =
                        json!({"blocked":true,"failure":"secret\u{1b}[2J"})
                }
                _ => value["gateway"]["credentials"]["expires_at"] = Value::Null,
            }
            let output = lines(&value, now).join("\n");
            assert!(!output.starts_with("Credential eligible"), "{change}");
            assert!(!output.contains("secret"));
            assert!(!output.contains('\u{1b}'));
        }
        assert!(
            lines(&good, now + chrono::Duration::seconds(181))
                .join("\n")
                .contains("Renewal: unknown or stale")
        );
    }
}
