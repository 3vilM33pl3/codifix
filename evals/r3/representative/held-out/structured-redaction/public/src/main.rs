mod command;
mod filter;
mod policy;
mod record;
use std::io::{self, BufRead};
fn main() {
    let mut filter = filter::Filter::default();
    for line in io::stdin().lock().lines() {
        let Ok(line) = line else { break };
        println!(
            "{}",
            command::parse(&line)
                .map(|c| filter.run(c))
                .unwrap_or_else(|| "invalid".into())
        );
    }
}
#[cfg(test)]
mod tests {
    #[test]
    fn ordinary_minimal_metric_export() {
        let mut filter = crate::filter::Filter::default();
        for (line, expected) in [
            ("put 1 metric name=637075;value=37", "ok"),
            ("export 1 human", "denied"),
            ("grant 1 human", "ok"),
            ("export 1 human", "name=637075;value=37"),
            ("generation", "2"),
        ] {
            assert_eq!(filter.run(crate::command::parse(line).unwrap()), expected);
        }
    }
    #[test]
    fn malformed_fields_are_not_accepted() {
        for line in [
            "put 1 metric name=ff",
            "put 1 metric name=61;name=62",
            "put 0 metric -",
            "grant 1 unknown",
            "put 1 metric A=61",
            "put 1 metric name=1",
        ] {
            assert!(crate::command::parse(line).is_none());
        }
    }
}
