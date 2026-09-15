use crate::interval::Interval;

pub enum Command {
    Add(Interval),
    Remove(Interval),
    Free(Interval),
    Show,
    Clear,
}

pub fn parse(line: &str) -> Option<Command> {
    let parts: Vec<_> = line.split_whitespace().collect();
    match parts.as_slice() {
        ["show"] => Some(Command::Show),
        ["clear"] => Some(Command::Clear),
        [operation, start, end] => {
            let interval = Interval::new(start.parse().ok()?, end.parse().ok()?)?;
            match *operation {
                "add" => Some(Command::Add(interval)),
                "remove" => Some(Command::Remove(interval)),
                "free" => Some(Command::Free(interval)),
                _ => None,
            }
        }
        _ => None,
    }
}
