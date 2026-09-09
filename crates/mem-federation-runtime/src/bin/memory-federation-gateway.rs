// SPDX-License-Identifier: AGPL-3.0-or-later
//! Intentionally has no general Memory service routes or user database access.
#[tokio::main]
async fn main() -> Result<(), Box<dyn std::error::Error>> {
    let mut args = std::env::args_os().skip(1);
    let path = args
        .next()
        .ok_or("expected private gateway configuration file")?;
    if args.next().is_some() {
        return Err("expected exactly one configuration file".into());
    }
    if std::fs::metadata(&path)?.len() > 1024 * 1024 {
        return Err("configuration too large".into());
    }
    let config: mem_federation_runtime::GatewayConfig =
        serde_json::from_slice(&std::fs::read(path)?)?;
    // Only sanitized errors leave the process: driver errors can contain URLs.
    config
        .serve()
        .await
        .map_err(|_| "gateway stopped; check configuration and durable status".into())
}
