#[derive(Clone)]
pub struct Record {
    pub id: u32,
    pub project: String,
    pub severity: u8,
}
pub fn project(value: &str) -> bool {
    !value.is_empty()
        && value.len() <= 16
        && value.bytes().all(|c| c.is_ascii_lowercase() || c == b'-')
}
