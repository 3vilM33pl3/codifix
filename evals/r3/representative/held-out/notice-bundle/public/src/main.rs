mod bundle;
mod command;
mod policy;
mod registry;
use std::io::{self, BufRead};
fn main() {
    let mut registry = registry::Registry::default();
    let mut bundle = bundle::Bundle::default();
    for line in io::stdin().lock().lines() {
        let Ok(line) = line else { break };
        let output = match command::parse(&line) {
            Some(command::Command::License(id, source, text)) => {
                registry
                    .licenses
                    .insert(id, registry::License { source, text });
                "ok".into()
            }
            Some(command::Command::Component(id, license, notice)) => {
                let offer = registry.components.get(&id).and_then(|c| c.offer.clone());
                registry.components.insert(
                    id,
                    registry::Component {
                        license,
                        notice,
                        offer,
                    },
                );
                "ok".into()
            }
            Some(command::Command::Offer(id, value)) => match registry.components.get_mut(&id) {
                Some(component) => {
                    component.offer = Some(value);
                    "ok".into()
                }
                None => "invalid".into(),
            },
            Some(command::Command::Ship(id)) => {
                if registry.components.contains_key(&id) {
                    registry.selected.insert(id);
                    "ok".into()
                } else {
                    "invalid".into()
                }
            }
            Some(command::Command::Unship(id)) => {
                registry.selected.remove(&id);
                "ok".into()
            }
            Some(command::Command::Build) => bundle.build(&registry),
            Some(command::Command::Show) => bundle.show(),
            None => "invalid".into(),
        };
        println!("{output}");
    }
}
#[cfg(test)]
mod tests {
    #[test]
    fn empty_bundle_build_is_stable() {
        let mut bundle = crate::bundle::Bundle::default();
        let registry = crate::registry::Registry::default();
        assert_eq!(bundle.show(), "bundle:none");
        assert_eq!(bundle.build(&registry), "built:1");
        assert_eq!(bundle.build(&registry), "unchanged:1");
    }
    #[test]
    fn invalid_metadata_commands_are_rejected() {
        for line in [
            "license l unknown text",
            "component a l bad=text",
            "show extra",
        ] {
            assert!(crate::command::parse(line).is_none());
        }
    }
}
