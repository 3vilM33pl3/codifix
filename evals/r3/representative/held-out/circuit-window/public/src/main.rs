mod breaker;
mod command;
mod permit;
mod policy;
use std::io::{self, BufRead};
fn main() {
    let mut circuit = breaker::Circuit::default();
    for line in io::stdin().lock().lines() {
        let Ok(line) = line else { break };
        println!(
            "{}",
            command::parse(&line)
                .map(|c| circuit.run(c))
                .unwrap_or_else(|| "invalid".into())
        );
    }
}
#[cfg(test)]
mod tests {
    #[test]
    fn ordinary_success_and_consumed_token() {
        let mut circuit = crate::breaker::Circuit::default();
        for (line, expected) in [
            ("state 0", "closed failures:0"),
            ("call 0", "grant:1"),
            ("finish 1 1 ok", "done"),
            ("finish 1 1 ok", "stale"),
            ("state 1", "closed failures:0"),
        ] {
            assert_eq!(circuit.run(crate::command::parse(line).unwrap()), expected);
        }
    }
    #[test]
    fn commands_have_bounded_time_and_outcomes() {
        for line in [
            "call -1",
            "state 1000001",
            "finish 0 0 ok",
            "finish 0 1 maybe",
            "state 0 extra",
        ] {
            assert!(crate::command::parse(line).is_none());
        }
    }
}
