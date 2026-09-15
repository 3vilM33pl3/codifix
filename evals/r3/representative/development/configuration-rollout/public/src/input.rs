pub enum Command {
    Base(String, String),
    Set(String, String),
    Unset(String),
    Clear(String),
    Commit(u64, bool),
    Get(String),
    Show,
}
pub fn parse(line: &str) -> Option<Command> {
    let words: Vec<_> = line.split_whitespace().collect();
    Some(match words.as_slice() {
        ["base", key, value] => Command::Base((*key).into(), (*value).into()),
        ["set", key, value] => Command::Set((*key).into(), (*value).into()),
        ["unset", key] => Command::Unset((*key).into()),
        ["clear", key] => Command::Clear((*key).into()),
        ["commit", revision, "ok"] => Command::Commit(revision.parse().ok()?, false),
        ["commit", revision, "fail"] => Command::Commit(revision.parse().ok()?, true),
        ["get", key] => Command::Get((*key).into()),
        ["show"] => Command::Show,
        _ => return None,
    })
}
