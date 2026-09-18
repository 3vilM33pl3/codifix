// SPDX-License-Identifier: AGPL-3.0-or-later
//! Private child entrypoint. Requires an independently enforced container; this
//! binary does not create a sandbox and must never be used as a host launcher.
#[tokio::main(flavor = "current_thread")]
async fn main() {
    let report = mem_supervisor_pi::sandbox::run_child().await;
    // Only typed outcome/digest metadata; no input, child stderr or model body.
    match serde_json::to_string(&report) {
        Ok(json) => println!("{json}"),
        Err(_) => std::process::exit(2),
    }
    if report.failure.is_some() {
        std::process::exit(1);
    }
}
