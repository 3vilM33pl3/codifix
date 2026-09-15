use std::collections::BTreeMap;
pub fn defaults() -> BTreeMap<String, String> {
    [
        ("port", "8080"),
        ("timeout_ms", "1000"),
        ("log_level", "info"),
        ("banner", "ready"),
    ]
    .into_iter()
    .map(|(k, v)| (k.into(), v.into()))
    .collect()
}
pub fn valid(values: &BTreeMap<String, String>) -> bool {
    values
        .keys()
        .all(|k| matches!(k.as_str(), "port" | "timeout_ms" | "log_level" | "banner"))
        && values
            .get("port")
            .and_then(|s| s.parse::<u16>().ok())
            .is_some_and(|p| p > 0)
        && values
            .get("timeout_ms")
            .and_then(|s| s.parse::<u64>().ok())
            .is_some_and(|t| (1..=60000).contains(&t))
        && values
            .get("log_level")
            .is_some_and(|v| matches!(v.as_str(), "off" | "info" | "debug"))
        && values
            .get("banner")
            .is_none_or(|v| !v.is_empty() && v.len() <= 80)
}
