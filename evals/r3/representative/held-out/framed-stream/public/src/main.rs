mod command;
mod decoder;
mod frame;
mod hex;

use std::io::{self, BufRead};

fn main() {
    let mut receiver = decoder::Decoder::default();
    for line in io::stdin().lock().lines() {
        let Ok(line) = line else { break };
        let output = match command::parse(&line) {
            Some(command::Command::Feed(bytes)) => receiver.feed(&bytes),
            Some(command::Command::End) => receiver.end(),
            Some(command::Command::Status) => receiver.status(),
            Some(command::Command::Reset) => {
                receiver = decoder::Decoder::default();
                "reset".into()
            }
            None => "invalid".into(),
        };
        println!("{output}");
    }
}

#[cfg(test)]
mod tests {
    #[test]
    fn complete_ascii_frame_is_delivered() {
        let mut receiver = crate::decoder::Decoder::default();
        assert_eq!(receiver.feed(&[0, 2, b'o', b'k']), "frames:6f6b");
        assert_eq!(receiver.status(), "pending:0 delivered:1 state:open");
    }

    #[test]
    fn malformed_shell_input_is_not_a_frame() {
        assert!(crate::command::parse("feed 0").is_none());
        assert!(crate::command::parse("feed gg").is_none());
        assert!(crate::command::parse("end extra").is_none());
        assert!(crate::command::parse("feed -").is_some());
    }
}
