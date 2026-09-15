pub enum Command {
    Set(i64),
    Add(i64),
    Scale(i64, i64),
    Show,
}

pub fn parse(line: &str) -> Option<Command> {
    let parts: Vec<_> = line.split_whitespace().collect();
    match parts.as_slice() {
        ["set", value] => Some(Command::Set(crate::amount::parse(value)?)),
        ["add", value] => Some(Command::Add(crate::amount::parse(value)?)),
        ["scale", numerator, denominator] => {
            let n: i64 = numerator.parse().ok()?;
            let d: i64 = denominator.parse().ok()?;
            if !(-1_000_000..=1_000_000).contains(&n) || !(1..=1_000_000).contains(&d) {
                return None;
            }
            Some(Command::Scale(n, d))
        }
        ["show"] => Some(Command::Show),
        _ => None,
    }
}
