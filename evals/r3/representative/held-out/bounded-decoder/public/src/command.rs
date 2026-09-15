pub enum Command {
    Begin(usize),
    Feed(Vec<u8>),
    Finish,
    Show,
    Status,
}
pub fn parse(line: &str) -> Option<Command> {
    let p = line.split_whitespace().collect::<Vec<_>>();
    match p.as_slice() {
        ["begin", limit] => {
            let limit = limit.parse().ok()?;
            (limit <= 64).then_some(Command::Begin(limit))
        }
        ["feed", hex] => Some(Command::Feed(crate::hex::decode(hex)?)),
        ["finish"] => Some(Command::Finish),
        ["show"] => Some(Command::Show),
        ["status"] => Some(Command::Status),
        _ => None,
    }
}
