// SPDX-License-Identifier: AGPL-3.0-or-later
#[cfg(windows)]
#[tokio::main]
async fn main() -> std::process::ExitCode {
    let mut args = std::env::args_os().skip(1);
    let path = args.next();
    if path.as_ref().is_some_and(|p| p == "--help" || p == "-h") && args.next().is_none() {
        println!(
            "memory-pds-pipe C:\\PRIVATE\\pds-pipe.json\nNative local-only PDS named-pipe broker; no TCP listener. Pinned Node/PDS are required."
        );
        return std::process::ExitCode::SUCCESS;
    }
    let Some(path) = path else {
        eprintln!("expected one private PDS broker configuration file");
        return std::process::ExitCode::FAILURE;
    };
    if args.next().is_some() {
        eprintln!("expected one private PDS broker configuration file");
        return std::process::ExitCode::FAILURE;
    }
    match mem_federation_runtime::serve_native_pds_pipe(std::path::Path::new(&path)).await {
        Ok(()) => std::process::ExitCode::SUCCESS,
        Err(_) => {
            eprintln!(
                "native PDS broker stopped; inspect protected service configuration and durable publication status"
            );
            std::process::ExitCode::FAILURE
        }
    }
}
#[cfg(not(windows))]
fn main() {
    eprintln!("memory-pds-pipe requires native Windows; HTTPS PDS operation is unchanged");
    std::process::exit(1);
}
