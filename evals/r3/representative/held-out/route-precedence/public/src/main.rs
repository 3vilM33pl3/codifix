mod command;
mod pattern;
mod request;
mod router;
use std::io::{self, BufRead};
fn main() {
    let mut router = router::Router::default();
    for line in io::stdin().lock().lines() {
        let Ok(line) = line else { break };
        println!(
            "{}",
            command::parse(&line)
                .map(|c| router.run(c))
                .unwrap_or_else(|| "invalid".into())
        );
    }
}
#[cfg(test)]
mod tests {
    #[test]
    fn ordinary_literal_route_and_removal() {
        let mut router = crate::router::Router::default();
        for (line, expected) in [
            ("add 1 GET /ping health", "ok"),
            ("route GET /ping", "target:health via:GET params:-"),
            ("route POST /ping", "missing"),
            ("remove 1", "ok"),
            ("revision", "2"),
        ] {
            assert_eq!(router.run(crate::command::parse(line).unwrap()), expected);
        }
    }
    #[test]
    fn unsupported_syntax_is_explicit() {
        for line in [
            "route GET /a//b",
            "route GET /a?b",
            "add 0 GET / x",
            "add 1 GET /:x/:x a",
            "add 1 GET /*x/a b",
            "route DELETE /",
        ] {
            assert!(crate::command::parse(line).is_none());
        }
    }
}
