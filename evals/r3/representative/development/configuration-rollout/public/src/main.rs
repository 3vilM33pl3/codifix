mod input;
mod merge;
mod schema;
mod service;

use std::io::{self, BufRead};

#[cfg(test)]
mod tests {
    #[test]
    fn defaults_satisfy_the_schema() {
        let service = crate::service::Service::new();
        assert_eq!(service.revision, 0);
        assert!(crate::schema::valid(&service.live));
        assert_eq!(service.live.get("port").map(String::as_str), Some("8080"));
    }

    #[test]
    fn ordinary_base_change_is_staged_then_applied() {
        let mut service = crate::service::Service::new();
        service.base.insert("port".into(), "9000".into());
        assert_eq!(service.live.get("port").map(String::as_str), Some("8080"));
        assert_eq!(service.commit(0, false), "applied");
        assert_eq!(service.revision, 1);
        assert_eq!(service.live.get("port").map(String::as_str), Some("9000"));
    }
}

fn main() {
    let mut service = service::Service::new();
    for line in io::stdin().lock().lines() {
        let Ok(line) = line else {
            break;
        };
        match input::parse(&line) {
            Some(input::Command::Base(key, value)) => {
                service.base.insert(key, value);
                println!("staged");
            }
            Some(input::Command::Set(key, value)) => {
                service.overlay.insert(key, Some(value));
                println!("staged");
            }
            Some(input::Command::Unset(key)) => {
                service.overlay.insert(key, None);
                println!("staged");
            }
            Some(input::Command::Clear(key)) => {
                service.overlay.remove(&key);
                println!("staged");
            }
            Some(input::Command::Commit(version, fail)) => {
                println!("{}", service.commit(version, fail))
            }
            Some(input::Command::Get(key)) => println!(
                "{}",
                service
                    .live
                    .get(&key)
                    .map(String::as_str)
                    .unwrap_or("missing")
            ),
            Some(input::Command::Show) => {
                println!("revision={}", service.revision);
                for (key, value) in &service.live {
                    println!("{key}={value}");
                }
            }
            None => println!("invalid"),
        }
    }
}
