mod calendar;
mod command;
mod display;
mod interval;
use std::io::{self, BufRead};

fn main() {
    let mut calendar = calendar::Calendar::default();
    for line in io::stdin().lock().lines() {
        let Ok(line) = line else { break };
        let output = match command::parse(&line) {
            Some(command::Command::Add(interval)) => {
                calendar.add(interval);
                "ok".into()
            }
            Some(command::Command::Remove(interval)) => {
                calendar.remove(interval);
                "ok".into()
            }
            Some(command::Command::Free(window)) => display::spans("free", &calendar.free(window)),
            Some(command::Command::Show) => display::spans("busy", &calendar.busy),
            Some(command::Command::Clear) => {
                calendar = calendar::Calendar::default();
                "cleared".into()
            }
            None => "invalid".into(),
        };
        println!("{output}");
    }
}

#[cfg(test)]
mod tests {
    #[test]
    fn one_booking_is_busy_and_can_be_removed() {
        let mut calendar = crate::calendar::Calendar::default();
        let span = crate::interval::Interval::new(3, 7).unwrap();
        calendar.add(span);
        assert_eq!(calendar.busy, vec![span]);
        assert!(calendar.free(span).is_empty());
        calendar.remove(span);
        assert!(calendar.busy.is_empty());
    }

    #[test]
    fn invalid_commands_and_empty_intervals_are_rejected() {
        for line in [
            "add 3 3",
            "add 9 2",
            "free a b",
            "show extra",
            "remove 0 9223372036854775808",
        ] {
            assert!(crate::command::parse(line).is_none());
        }
    }
}
