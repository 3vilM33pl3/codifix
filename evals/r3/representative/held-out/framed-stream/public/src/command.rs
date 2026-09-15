pub enum Command {
    Feed(Vec<u8>),
    End,
    Reset,
    Status,
}

pub fn parse(line: &str) -> Option<Command> {
    let parts: Vec<_> = line.split_whitespace().collect();
    match parts.as_slice() {
        ["feed", bytes] => Some(Command::Feed(crate::hex::decode(bytes)?)),
        ["end"] => Some(Command::End),
        ["reset"] => Some(Command::Reset),
        ["status"] => Some(Command::Status),
        _ => None,
    }
}
