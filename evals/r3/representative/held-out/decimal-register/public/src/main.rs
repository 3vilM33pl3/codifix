mod amount;
mod command;
mod register;
mod scale;
use std::io::{self, BufRead};

fn main() {
    let mut register = register::Register::default();
    for line in io::stdin().lock().lines() {
        let Ok(line) = line else { break };
        let accepted = match command::parse(&line) {
            Some(command::Command::Set(value)) => {
                register.value = value;
                true
            }
            Some(command::Command::Add(value)) => register.add(value),
            Some(command::Command::Scale(numerator, denominator)) => {
                register.scale(numerator, denominator)
            }
            Some(command::Command::Show) => true,
            None => false,
        };
        if accepted {
            println!("value:{}", amount::display(register.value));
        } else {
            println!("invalid");
        }
    }
}

#[cfg(test)]
mod tests {
    #[test]
    fn ordinary_measurements_are_formatted() {
        assert_eq!(crate::amount::parse("12.5"), Some(12500));
        assert_eq!(crate::amount::display(-12500), "-12.500");
        let mut register = crate::register::Register::default();
        assert!(register.add(12500));
        assert!(register.scale(2, 1));
        assert_eq!(register.value, 25000);
    }
    #[test]
    fn invalid_scale_and_command_are_rejected() {
        assert!(crate::command::parse("scale 1 0").is_none());
        assert!(crate::command::parse("scale 1 -1").is_none());
        assert!(crate::command::parse("scale 1000001 1").is_none());
        assert!(crate::command::parse("show extra").is_none());
    }
}
