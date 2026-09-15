use std::collections::BTreeMap;
pub struct Record {
    pub kind: String,
    pub fields: BTreeMap<String, String>,
}
pub fn name(value: &str) -> bool {
    !value.is_empty() && value.len() <= 12 && value.bytes().all(|b| b.is_ascii_lowercase())
}
pub fn encode(value: &str) -> String {
    if value.is_empty() {
        "-".into()
    } else {
        value.bytes().map(|b| format!("{b:02x}")).collect()
    }
}
pub fn fields(body: &str) -> Option<BTreeMap<String, String>> {
    let mut output = BTreeMap::new();
    if body == "-" {
        return Some(output);
    }
    for field in body.split(';') {
        let (key, hex) = field.split_once('=')?;
        let parts = key.split('.').collect::<Vec<_>>();
        if parts.len() > 4 || !parts.iter().all(|p| name(p)) {
            return None;
        }
        let value = if hex == "-" {
            String::new()
        } else {
            if hex.is_empty()
                || hex.len() > 256
                || hex.len() % 2 != 0
                || !hex.bytes().all(|b| b.is_ascii_hexdigit())
            {
                return None;
            }
            let bytes = (0..hex.len())
                .step_by(2)
                .map(|i| u8::from_str_radix(&hex[i..i + 2], 16).ok())
                .collect::<Option<Vec<_>>>()?;
            String::from_utf8(bytes).ok()?
        };
        if output.insert(key.into(), value).is_some() || output.len() > 16 {
            return None;
        }
    }
    Some(output)
}
