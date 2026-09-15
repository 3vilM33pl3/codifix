mod capture;
mod policy;
mod publish;
mod review;
use std::io::{self, BufRead};
fn main() {
    let mut state = publish::State::default();
    for line in io::stdin().lock().lines() {
        let Ok(line) = line else { break };
        let parts: Vec<_> = line.split_whitespace().collect();
        let result = match parts.as_slice() {
            ["capture", author, body] => {
                state.capture = Some(capture::Capture {
                    author: (*author).into(),
                    body: (*body).into(),
                });
                "captured"
            }
            ["grant", publisher] => {
                state.policy.grants.insert((*publisher).into());
                "granted"
            }
            ["revoke", publisher] => {
                state.policy.grants.remove(*publisher);
                "revoked"
            }
            ["review", id, publisher] => state.review(id, publisher),
            ["propose", id, base, editor, body] => match base.parse::<usize>() {
                Ok(base) => state.propose(id, base, editor, body),
                Err(_) => "invalid",
            },
            ["approve", proposal, review, publisher] => state.approve(proposal, review, publisher),
            ["publish", request, review, publisher, fault]
                if matches!(*fault, "none" | "before" | "after") =>
            {
                state.send(request, review, publisher, fault)
            }
            ["show"] => {
                for entry in &state.log {
                    let s = &entry.snapshot;
                    println!(
                        "version={} author={} publisher={} editor={} body={}",
                        s.version, s.author, entry.publisher, s.editor, s.body
                    );
                }
                continue;
            }
            _ => "invalid",
        };
        println!("{result}");
    }
}
