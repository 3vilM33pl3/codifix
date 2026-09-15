use crate::record::{Record, name};
fn valid(record: &Record) -> bool {
    let f = &record.fields;
    if f.keys().any(|parent| {
        f.keys()
            .any(|child| child.starts_with(&format!("{parent}.")))
    }) {
        return false;
    }
    match record.kind.as_str() {
        "incident" => {
            if !f
                .get("title")
                .is_some_and(|v| !v.is_empty() && v.len() <= 64)
            {
                return false;
            }
            if !f
                .get("severity")
                .is_some_and(|v| v.len() == 1 && matches!(v.as_bytes()[0], b'1'..=b'5'))
            {
                return false;
            }
            if f.get("author").is_some_and(|v| v.is_empty()) {
                return false;
            }
            match (f.get("evidence.label"), f.get("evidence.digest")) {
                (None, None) => true,
                (Some(label), Some(digest)) => {
                    !label.is_empty()
                        && digest.len() == 8
                        && digest
                            .bytes()
                            .all(|b| b.is_ascii_digit() || (b'a'..=b'f').contains(&b))
                }
                _ => false,
            }
        }
        "metric" => {
            f.get("name").is_some_and(|v| name(v))
                && f.get("value")
                    .is_some_and(|v| v.parse::<i64>().is_ok_and(|n| n.to_string() == *v))
                && f.get("unit").is_none_or(|v| name(v))
        }
        _ => false,
    }
}
pub fn disclose(record: &Record, destination: &str) -> Result<String, &'static str> {
    if !valid(record) {
        return Err("schema");
    }
    let allowed: &[&str] = match (record.kind.as_str(), destination) {
        ("incident", "human") => &[
            "title",
            "severity",
            "author",
            "evidence.label",
            "evidence.digest",
        ],
        ("incident", "model") => &["title", "severity", "evidence.digest"],
        ("metric", "human" | "model") => &["name", "value", "unit"],
        _ => return Err("schema"),
    };
    Ok(record
        .fields
        .iter()
        .filter(|(key, _)| allowed.contains(&key.as_str()))
        .map(|(key, value)| format!("{key}={}", crate::record::encode(value)))
        .collect::<Vec<_>>()
        .join(";"))
}
