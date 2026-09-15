pub enum Command {
    Put(u32, String, u8),
    Drop(u32),
    Begin(String),
    Page(u32, String, String),
    Expire(u32),
}
pub fn parse(line: &str) -> Option<Command> {
    let parts: Vec<_> = line.split_whitespace().collect();
    let id = |text: &str| text.parse::<u32>().ok().filter(|id| *id > 0);
    match parts.as_slice() {
        ["put", value, project, severity] if crate::record::project(project) => {
            let severity = severity
                .parse::<u8>()
                .ok()
                .filter(|s| (1..=9).contains(s))?;
            Some(Command::Put(id(value)?, project.to_string(), severity))
        }
        ["drop", value] => Some(Command::Drop(id(value)?)),
        ["begin", project] if crate::record::project(project) => {
            Some(Command::Begin(project.to_string()))
        }
        ["page", value, project, cursor] if crate::record::project(project) => Some(Command::Page(
            id(value)?,
            project.to_string(),
            cursor.to_string(),
        )),
        ["expire", value] => Some(Command::Expire(id(value)?)),
        _ => None,
    }
}
