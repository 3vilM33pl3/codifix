pub enum Command {
    License(String, bool, String),
    Component(String, String, String),
    Offer(String, String),
    Ship(String),
    Unship(String),
    Build,
    Show,
}
fn token(value: &str) -> bool {
    !value.is_empty()
        && value.len() <= 64
        && value
            .bytes()
            .all(|c| c.is_ascii_alphanumeric() || b"_.-".contains(&c))
}
pub fn parse(line: &str) -> Option<Command> {
    let parts: Vec<_> = line.split_whitespace().collect();
    if parts.iter().any(|part| !token(part)) {
        return None;
    }
    match parts.as_slice() {
        ["license", id, class @ ("notice" | "source"), text] => Some(Command::License(
            id.to_string(),
            *class == "source",
            text.to_string(),
        )),
        ["component", id, license, notice] => Some(Command::Component(
            id.to_string(),
            license.to_string(),
            notice.to_string(),
        )),
        ["offer", id, value] => Some(Command::Offer(id.to_string(), value.to_string())),
        ["ship", id] => Some(Command::Ship(id.to_string())),
        ["unship", id] => Some(Command::Unship(id.to_string())),
        ["build"] => Some(Command::Build),
        ["show"] => Some(Command::Show),
        _ => None,
    }
}
