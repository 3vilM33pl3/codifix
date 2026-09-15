use crate::{pattern::Pattern, request};
pub enum Command {
    Add(u32, String, String, String),
    Remove(u32),
    Route(String, Vec<String>),
    Revision,
}
pub fn parse(line: &str) -> Option<Command> {
    let p = line.split_whitespace().collect::<Vec<_>>();
    match p.as_slice() {
        ["revision"] => Some(Command::Revision),
        ["remove", id] => {
            let id = id.parse().ok()?;
            (id > 0).then_some(Command::Remove(id))
        }
        ["route", method, path] if request::method(method) => {
            Some(Command::Route((*method).into(), request::path(path)?))
        }
        ["add", id, method, pattern, target]
            if request::method(method) && request::name(target) =>
        {
            let id = id.parse().ok()?;
            if id == 0 {
                return None;
            }
            Pattern::parse(pattern)?;
            Some(Command::Add(
                id,
                (*method).into(),
                (*pattern).into(),
                (*target).into(),
            ))
        }
        _ => None,
    }
}
