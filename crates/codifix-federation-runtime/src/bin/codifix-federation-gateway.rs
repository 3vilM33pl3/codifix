// SPDX-License-Identifier: AGPL-3.0-or-later
//! Intentionally has no general Codifix service routes or user database access.
#[tokio::main]
async fn main() -> Result<(), Box<dyn std::error::Error>> {
    let all: Vec<_> = std::env::args_os().skip(1).collect();
    if all.first().is_some_and(|arg| arg == "check-status") {
        if all.len() != 5 && !(all.len() == 7 && all[5] == "--service-name") {
            return Err(
                "expected check-status CONFIG SPACE AUTHORITY AUDIENCE [--service-name NAME]"
                    .into(),
            );
        }
        let identity = |index: usize| -> Result<uuid::Uuid, &'static str> {
            all[index]
                .to_str()
                .and_then(|value| uuid::Uuid::parse_str(value).ok())
                .filter(|id| !id.is_nil())
                .ok_or("invalid readiness identity")
        };
        let service_name = if all.len() == 7 {
            Some(all[6].to_str().ok_or("invalid service name")?)
        } else {
            None
        };
        let ready = codifix_federation_runtime::operator_status::check_config_path(std::path::Path::new(&all[1]),
            codifix_federation::publication::SpaceId::new(identity(2)?)?,
            codifix_federation::publication::InstanceId::new(identity(3)?)?,
            codifix_federation::publication::InstanceId::new(identity(4)?)?, service_name).await
            .map_err(|_| "gateway readiness not verified; inspect private configuration and current membership")?;
        println!("{}", serde_json::to_string(&ready)?);
        return Ok(());
    }
    let mut args = all.into_iter();
    let path = args
        .next()
        .ok_or("expected private gateway configuration file")?;
    if args.next().is_some() {
        return Err("expected exactly one configuration file".into());
    }
    let config = codifix_federation_runtime::load_gateway_config(std::path::Path::new(&path))
        .map_err(|_| "private gateway configuration unavailable")?;
    // Only sanitized errors leave the process: driver errors can contain URLs.
    config
        .serve()
        .await
        .map_err(|_| "gateway stopped; check configuration and durable status".into())
}
