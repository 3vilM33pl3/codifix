pub enum Command {
    Node(String, u64),
    Edge(String, String, bool),
    Build(String, bool),
    Show,
}
fn name(value: &str) -> bool {
    !value.is_empty()
        && value.len() <= 64
        && value
            .bytes()
            .all(|c| c.is_ascii_alphanumeric() || c == b'_')
}
pub fn parse(line: &str) -> Option<Command> {
    let words: Vec<_> = line.split_whitespace().collect();
    Some(match words.as_slice() {
        ["node", id, version] if name(id) => {
            let version: u64 = version.parse().ok()?;
            if version == 0 {
                return None;
            }
            Command::Node((*id).into(), version)
        }
        ["edge", child, parent] if name(child) && name(parent) => {
            Command::Edge((*child).into(), (*parent).into(), true)
        }
        ["drop", child, parent] if name(child) && name(parent) => {
            Command::Edge((*child).into(), (*parent).into(), false)
        }
        ["build", id, "ok"] if name(id) => Command::Build((*id).into(), false),
        ["build", id, "fail"] if name(id) => Command::Build((*id).into(), true),
        ["show"] => Command::Show,
        _ => return None,
    })
}
