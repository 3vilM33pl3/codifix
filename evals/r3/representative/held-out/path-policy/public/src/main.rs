mod command;
mod links;
mod path;
mod policy;
use std::io::{self, BufRead};

fn main() {
    let mut policy = policy::Policy::default();
    for line in io::stdin().lock().lines() {
        let Ok(line) = line else { break };
        let result = match command::parse(&line) {
            Some(command::Command::Grant(op, path)) => policy.grant(&op, &path),
            Some(command::Command::Revoke(op, path)) => policy.revoke(&op, &path),
            Some(command::Command::Check(op, path)) => policy.check(&op, &path),
            Some(command::Command::Link(source, target)) => policy.link(&source, &target),
            Some(command::Command::Unlink(source)) => policy.unlink(&source),
            None => "invalid".into(),
        };
        println!("{result}");
    }
}

#[cfg(test)]
mod tests {
    #[test]
    fn granted_child_is_visible_and_revocable() {
        let mut policy = crate::policy::Policy::default();
        assert_eq!(policy.check("read", "/work/team/file"), "deny");
        assert_eq!(policy.grant("read", "/work/team"), "ok");
        assert_eq!(
            policy.check("read", "/work/team/file"),
            "allow:/work/team/file"
        );
        assert_eq!(policy.revoke("read", "/work/team"), "ok");
        assert_eq!(policy.check("read", "/work/team/file"), "deny");
    }
    #[test]
    fn invalid_grant_does_not_create_authority() {
        let mut policy = crate::policy::Policy::default();
        assert_eq!(policy.grant("read", "/else"), "invalid");
        assert_eq!(policy.check("read", "/else"), "deny");
        assert!(crate::command::parse("grant execute /work").is_none());
    }
}
