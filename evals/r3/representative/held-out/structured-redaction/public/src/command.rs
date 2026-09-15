use crate::record::{self, Record};
pub enum Command {
    Put(u32, Record),
    Grant(u32, String, bool),
    Drop(u32),
    Export(u32, String),
    Generation,
}
fn id(value: &str) -> Option<u32> {
    let id = value.parse().ok()?;
    (id > 0).then_some(id)
}
pub fn parse(line: &str) -> Option<Command> {
    let p = line.split_whitespace().collect::<Vec<_>>();
    match p.as_slice() {
        ["generation"] => Some(Command::Generation),
        ["drop", key] => Some(Command::Drop(id(key)?)),
        ["put", key, kind, fields] if record::name(kind) => Some(Command::Put(
            id(key)?,
            Record {
                kind: (*kind).into(),
                fields: record::fields(fields)?,
            },
        )),
        [
            action @ ("grant" | "revoke"),
            key,
            dest @ ("human" | "model"),
        ] => Some(Command::Grant(id(key)?, (*dest).into(), *action == "grant")),
        ["export", key, dest] if record::name(dest) => {
            Some(Command::Export(id(key)?, (*dest).into()))
        }
        _ => None,
    }
}
