pub fn normalize(label: &str) -> Option<String> {
    (!label.is_empty()).then(|| label.to_ascii_lowercase())
}
