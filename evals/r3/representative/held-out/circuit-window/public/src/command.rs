pub enum Action {
    Call,
    Finish(u64, bool),
    State,
}
pub struct Command {
    pub time: u64,
    pub action: Action,
}
pub fn parse(line: &str) -> Option<Command> {
    let p = line.split_whitespace().collect::<Vec<_>>();
    let (time, action) = match p.as_slice() {
        ["call", t] => (t.parse().ok()?, Action::Call),
        ["state", t] => (t.parse().ok()?, Action::State),
        ["finish", t, id, outcome] => {
            let success = match *outcome {
                "ok" => true,
                "fail" => false,
                _ => return None,
            };
            let token = id.parse().ok()?;
            if token == 0 {
                return None;
            }
            (t.parse().ok()?, Action::Finish(token, success))
        }
        _ => return None,
    };
    (time <= 1_000_000).then_some(Command { time, action })
}
