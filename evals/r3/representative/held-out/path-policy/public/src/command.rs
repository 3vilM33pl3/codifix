pub enum Command {
    Grant(String, String),
    Revoke(String, String),
    Check(String, String),
    Link(String, String),
    Unlink(String),
}

pub fn parse(line: &str) -> Option<Command> {
    let parts: Vec<_> = line.split_whitespace().collect();
    match parts.as_slice() {
        ["grant", operation @ ("read" | "write"), path] => {
            Some(Command::Grant(operation.to_string(), path.to_string()))
        }
        ["revoke", operation @ ("read" | "write"), path] => {
            Some(Command::Revoke(operation.to_string(), path.to_string()))
        }
        ["check", operation @ ("read" | "write"), path] => {
            Some(Command::Check(operation.to_string(), path.to_string()))
        }
        ["link", source, target] => Some(Command::Link(source.to_string(), target.to_string())),
        ["unlink", source] => Some(Command::Unlink(source.to_string())),
        _ => None,
    }
}
