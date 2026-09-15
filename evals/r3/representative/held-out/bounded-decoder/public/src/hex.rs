pub fn decode(value: &str) -> Option<Vec<u8>> {
    if value == "-" {
        return Some(Vec::new());
    }
    if value.is_empty()
        || value.len() > 64
        || value.len() % 2 != 0
        || !value.bytes().all(|b| b.is_ascii_hexdigit())
    {
        return None;
    }
    (0..value.len())
        .step_by(2)
        .map(|i| u8::from_str_radix(&value[i..i + 2], 16).ok())
        .collect()
}
pub fn encode(value: &[u8]) -> String {
    if value.is_empty() {
        "-".into()
    } else {
        value.iter().map(|b| format!("{b:02x}")).collect()
    }
}
