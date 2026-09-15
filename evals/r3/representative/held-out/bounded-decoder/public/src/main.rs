mod command;
mod decoder;
mod hex;
mod store;
use std::io::{self, BufRead};
fn main() {
    let mut decoder = decoder::Decoder::default();
    for line in io::stdin().lock().lines() {
        let Ok(line) = line else { break };
        println!(
            "{}",
            command::parse(&line)
                .map(|c| decoder.run(c))
                .unwrap_or_else(|| "invalid".into())
        );
    }
}
#[cfg(test)]
mod tests {
    #[test]
    fn ordinary_complete_runs() {
        let mut decoder = crate::decoder::Decoder::default();
        for (line, expected) in [
            ("begin 8", "ready"),
            ("feed 02410142", "queued"),
            ("finish", "committed:1"),
            ("show", "1:414142"),
            ("status", "idle"),
        ] {
            assert_eq!(decoder.run(crate::command::parse(line).unwrap()), expected);
        }
    }
    #[test]
    fn malformed_commands_and_hex_do_not_parse() {
        for line in [
            "begin 65",
            "begin -1",
            "feed abc",
            "feed gg",
            "finish extra",
        ] {
            assert!(crate::command::parse(line).is_none());
        }
    }
}
