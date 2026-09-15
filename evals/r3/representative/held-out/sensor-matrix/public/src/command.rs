use crate::sparse::Matrix;
pub enum Command {
    Put(String, Matrix),
    Show(String),
    Transpose(String, String),
    Mul(String, String, String),
    Revision,
}
fn name(value: &str) -> Option<String> {
    (value.len() <= 12 && !value.is_empty() && value.bytes().all(|b| b.is_ascii_lowercase()))
        .then(|| value.into())
}
pub fn parse(line: &str) -> Option<Command> {
    let parts = line.split_whitespace().collect::<Vec<_>>();
    match parts.as_slice() {
        ["revision"] => Some(Command::Revision),
        ["show", id] => Some(Command::Show(name(id)?)),
        ["transpose", dst, src] => Some(Command::Transpose(name(dst)?, name(src)?)),
        ["mul", dst, left, right] => Some(Command::Mul(name(dst)?, name(left)?, name(right)?)),
        ["put", id, rows, cols, body] => {
            let mut entries = Vec::new();
            if *body != "-" {
                for entry in body.split(';') {
                    let p = entry.split(',').collect::<Vec<_>>();
                    let [r, c, v] = p.as_slice() else { return None };
                    entries.push((r.parse().ok()?, c.parse().ok()?, v.parse().ok()?));
                    if entries.len() > 128 {
                        return None;
                    }
                }
            }
            Some(Command::Put(
                name(id)?,
                Matrix {
                    rows: rows.parse().ok()?,
                    cols: cols.parse().ok()?,
                    entries,
                },
            ))
        }
        _ => None,
    }
}
