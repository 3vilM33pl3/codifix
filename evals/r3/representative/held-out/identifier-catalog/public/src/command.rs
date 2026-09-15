pub enum Command {
    Add(u32, String),
    Get(String),
    Rename(u32, String),
    Remove(u32),
    Show,
}
pub fn parse(line: &str) -> Option<Command> {
    let parts: Vec<_> = line.split_whitespace().collect();
    let id = |text: &str| text.parse::<u32>().ok().filter(|id| *id > 0);
    match parts.as_slice() {
        ["add", value, label] => Some(Command::Add(id(value)?, label.to_string())),
        ["rename", value, label] => Some(Command::Rename(id(value)?, label.to_string())),
        ["remove", value] => Some(Command::Remove(id(value)?)),
        ["get", label] => Some(Command::Get(label.to_string())),
        ["show"] => Some(Command::Show),
        _ => None,
    }
}
