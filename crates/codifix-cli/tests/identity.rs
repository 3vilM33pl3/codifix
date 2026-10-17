// SPDX-License-Identifier: AGPL-3.0-or-later
use std::process::Command;

#[test]
fn shipped_binary_reports_codifix_identity_without_service_configuration() {
    for argument in ["--version", "-V", "--help"] {
        let output = Command::new(env!("CARGO_BIN_EXE_codifix"))
            .arg(argument)
            .output()
            .expect("run shipped Codifix binary");
        assert!(output.status.success(), "{argument}");
        let text = String::from_utf8(output.stdout).expect("CLI text output");
        if argument == "--help" {
            assert!(text.contains("Usage: codifix "), "{text}");
        } else {
            assert!(text.starts_with("codifix "), "{text}");
        }
    }
}
