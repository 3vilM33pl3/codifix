mod command;
mod record;
mod snapshot;
mod store;
use std::io::{self, BufRead};
fn main() {
    let mut store = store::Store::default();
    for line in io::stdin().lock().lines() {
        let Ok(line) = line else { break };
        let output = match command::parse(&line) {
            Some(command::Command::Put(id, project, severity)) => {
                store.live.insert(
                    id,
                    record::Record {
                        id,
                        project,
                        severity,
                    },
                );
                "ok".into()
            }
            Some(command::Command::Drop(id)) => {
                if store.live.remove(&id).is_some() {
                    "removed".into()
                } else {
                    "missing".into()
                }
            }
            Some(command::Command::Begin(project)) => store.begin(&project),
            Some(command::Command::Page(id, project, cursor)) => store.page(id, &project, &cursor),
            Some(command::Command::Expire(id)) => store.expire(id),
            None => "invalid".into(),
        };
        println!("{output}");
    }
}
#[cfg(test)]
mod tests {
    #[test]
    fn one_row_snapshot_can_be_read_and_expired() {
        let mut store = crate::store::Store::default();
        store.live.insert(
            1,
            crate::record::Record {
                id: 1,
                project: "north".into(),
                severity: 5,
            },
        );
        assert_eq!(store.begin("north"), "snapshot:1 rows:1");
        assert_eq!(store.page(1, "north", "-"), "rows:1@5 next:end");
        assert_eq!(store.expire(1), "expired");
        assert_eq!(store.page(1, "north", "-"), "missing-snapshot");
    }
    #[test]
    fn invalid_records_do_not_parse() {
        for line in [
            "put 0 north 5",
            "put 1 north 0",
            "put 1 north 10",
            "begin North",
            "drop 4294967296",
        ] {
            assert!(crate::command::parse(line).is_none());
        }
    }
}
