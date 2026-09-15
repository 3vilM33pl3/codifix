mod command;
mod display;
mod engine;
mod sparse;
use std::io::{self, BufRead};
fn main() {
    let mut engine = engine::Engine::default();
    for line in io::stdin().lock().lines() {
        let Ok(line) = line else { break };
        println!(
            "{}",
            command::parse(&line)
                .map(|c| engine.run(c))
                .unwrap_or_else(|| "invalid".into())
        );
    }
}
#[cfg(test)]
mod tests {
    #[test]
    fn scalar_transform_and_transpose() {
        let mut engine = crate::engine::Engine::default();
        for (line, expected) in [
            ("put a 1 1 0,0,3", "ok"),
            ("put b 1 1 0,0,4", "ok"),
            ("mul c a b", "ok"),
            ("show c", "1x1:0,0,12"),
            ("transpose c c", "ok"),
            ("revision", "4"),
        ] {
            assert_eq!(engine.run(crate::command::parse(line).unwrap()), expected);
        }
    }
    #[test]
    fn commands_are_bounded_and_explicit() {
        for line in [
            "show A",
            "put a 1 1 0,0",
            "mul a b",
            "revision extra",
            "put a 1 1 0,0,9223372036854775808",
        ] {
            assert!(crate::command::parse(line).is_none());
        }
    }
}
