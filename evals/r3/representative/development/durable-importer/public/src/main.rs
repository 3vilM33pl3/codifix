mod cursor;
mod input;
mod replay;
mod store;
use std::io::{self, BufRead};

fn main() {
    let mut store = store::Store::default();
    for line in io::stdin().lock().lines() {
        let Ok(line) = line else {
            break;
        };
        let parts: Vec<_> = line.split_whitespace().collect();
        match parts.as_slice() {
            ["show"] => println!("{}", store.snapshot()),
            ["restart"] => {
                replay::restart(&mut store);
                println!("restarted");
            }
            _ => match input::parse(&parts) {
                Some((delivery, fault)) => {
                    println!("{}", replay::apply(&mut store, delivery, &fault))
                }
                None => println!("invalid"),
            },
        }
    }
}
