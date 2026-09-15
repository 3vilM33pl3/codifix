mod catalog;
mod command;
mod keys;
mod record;
use std::io::{self, BufRead};
fn main() {
    let mut catalog = catalog::Catalog::default();
    for line in io::stdin().lock().lines() {
        let Ok(line) = line else { break };
        let output = match command::parse(&line) {
            Some(command::Command::Add(id, label)) => catalog.add(id, &label),
            Some(command::Command::Get(label)) => catalog.get(&label),
            Some(command::Command::Rename(id, label)) => catalog.rename(id, &label),
            Some(command::Command::Remove(id)) => catalog.remove(id),
            Some(command::Command::Show) => catalog.show(),
            None => "invalid".into(),
        };
        println!("{output}");
    }
}
#[cfg(test)]
mod tests {
    #[test]
    fn ascii_lookup_preserves_display_label() {
        let mut catalog = crate::catalog::Catalog::default();
        assert_eq!(catalog.add(1, "Alpha"), "inserted");
        assert_eq!(catalog.get("ALPHA"), "found:1:Alpha");
        assert_eq!(catalog.remove(1), "removed");
        assert_eq!(catalog.get("alpha"), "missing");
    }
    #[test]
    fn invalid_ids_and_commands_are_rejected() {
        for line in ["add 0 a", "remove 4294967296", "get a extra", "show extra"] {
            assert!(crate::command::parse(line).is_none());
        }
    }
}
