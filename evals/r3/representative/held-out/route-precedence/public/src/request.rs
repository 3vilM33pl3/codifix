pub fn name(value: &str) -> bool {
    !value.is_empty() && value.len() <= 12 && value.bytes().all(|b| b.is_ascii_lowercase())
}
pub fn method(value: &str) -> bool {
    matches!(value, "GET" | "POST" | "HEAD")
}
pub fn literal(value: &str) -> bool {
    !value.is_empty()
        && value.len() <= 32
        && value
            .bytes()
            .all(|b| b.is_ascii_lowercase() || b.is_ascii_digit() || b == b'_' || b == b'-')
}
pub fn path(value: &str) -> Option<Vec<String>> {
    if value == "/" {
        return Some(Vec::new());
    }
    let parts = value
        .strip_prefix('/')?
        .split('/')
        .map(String::from)
        .collect::<Vec<_>>();
    (parts.len() <= 8 && parts.iter().all(|p| literal(p))).then_some(parts)
}
