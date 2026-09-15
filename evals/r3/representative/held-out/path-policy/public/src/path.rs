pub fn normalize(input: &str) -> Option<String> {
    if !input.starts_with('/') || !input.is_ascii() || input.len() > 256 {
        return None;
    }
    let mut parts = Vec::new();
    for part in input.split('/') {
        if part.is_empty() || part == "." {
            continue;
        }
        if part == ".."
            || !part
                .bytes()
                .all(|c| c.is_ascii_alphanumeric() || b"._-".contains(&c))
        {
            return None;
        }
        parts.push(part);
    }
    Some(format!("/{}", parts.join("/")))
}

pub fn under(path: &str, prefix: &str) -> bool {
    path == prefix
        || path
            .strip_prefix(prefix)
            .is_some_and(|suffix| suffix.starts_with('/'))
}
