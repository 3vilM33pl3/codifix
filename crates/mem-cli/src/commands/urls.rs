// SPDX-License-Identifier: AGPL-3.0-or-later

use anyhow::Result;
use clap::Args;
use mem_config::{AreaRouting, Profile};
use reqwest::Url;
use serde::Serialize;
use std::{collections::BTreeSet, fs, net::SocketAddr, path::PathBuf};

pub(super) const HELP: &str = "Agent notes:\nRead-only configuration inventory; no network requests or credentials are printed. Configured URLs do not prove reachability or authorization. UI links come first. Use --area to filter and --json for scripts.\n\nExamples:\n  memory urls\n  memory urls --area workshop --json\n\nSee also:\n  docs/user/cli/urls.md";

#[derive(Debug, Args)]
pub(super) struct UrlsArgs {
    /// Emit the configuration-derived inventory as JSON.
    #[arg(long)]
    pub json: bool,
    /// Optional operator participant configuration containing a gateway address.
    #[arg(long, env = "MEMORY_LAYER_FEDERATION_CONFIG")]
    pub federation_config: Option<PathBuf>,
}

#[derive(Debug, Serialize)]
struct Entry {
    category: &'static str,
    installations: BTreeSet<String>,
    url: String,
    description: &'static str,
    access: &'static str,
}

#[derive(Default, Debug, Serialize)]
struct Report {
    schema_version: u32,
    configured_only: bool,
    entries: Vec<Entry>,
    notices: Vec<String>,
}

impl Report {
    fn add(
        &mut self,
        label: &str,
        category: &'static str,
        url: String,
        description: &'static str,
        access: &'static str,
    ) {
        if let Some(entry) = self
            .entries
            .iter_mut()
            .find(|e| e.url == url && e.description == description)
        {
            entry.installations.insert(label.to_owned());
        } else {
            self.entries.push(Entry {
                category,
                installations: BTreeSet::from([label.to_owned()]),
                url,
                description,
                access,
            });
        }
    }
}

/// Reject rather than echo URLs that could carry credentials or arbitrary data.
fn safe_url(value: &str) -> Option<Url> {
    let mut url = Url::parse(value).ok()?;
    if !url.path().ends_with('/') {
        url.set_path(&format!("{}/", url.path()));
    }
    (matches!(url.scheme(), "http" | "https")
        && url.host_str().is_some()
        && url.username().is_empty()
        && url.password().is_none()
        && url.query().is_none()
        && url.fragment().is_none())
    .then_some(url)
}

fn string<'a>(value: &'a toml::Value, section: &str, key: &str) -> Option<&'a str> {
    value.get(section)?.get(key)?.as_str()
}

fn enabled(value: &toml::Value, section: &str, key: &str, default: bool) -> bool {
    value
        .get(section)
        .and_then(|v| v.get(key))
        .and_then(toml::Value::as_bool)
        .unwrap_or(default)
}

fn merge(base: &mut toml::Value, overlay: toml::Value) {
    match (base, overlay) {
        (toml::Value::Table(base), toml::Value::Table(overlay)) => {
            for (key, value) in overlay {
                if let Some(old) = base.get_mut(&key) {
                    merge(old, value);
                } else {
                    base.insert(key, value);
                }
            }
        }
        (base, overlay) => *base = overlay,
    }
}

fn config_base(value: &toml::Value) -> Option<Url> {
    if let Some(public) = string(value, "auth", "public_base_url") {
        return safe_url(public);
    }
    let default_bind = mem_config::default_bind_addr();
    let mut socket: SocketAddr = string(value, "service", "bind_addr")
        .unwrap_or(&default_bind)
        .parse()
        .ok()?;
    if socket.ip().is_unspecified() {
        socket.set_ip(if socket.is_ipv4() {
            std::net::Ipv4Addr::LOCALHOST.into()
        } else {
            std::net::Ipv6Addr::LOCALHOST.into()
        });
    }
    safe_url(&format!("http://{socket}/"))
}

fn collect(report: &mut Report, label: &str, value: &toml::Value) {
    let Some(base) = config_base(value) else {
        report.notices.push(format!(
            "{label}: no safe configured HTTP address; omitted."
        ));
        return;
    };
    let login = string(value, "auth", "mode") == Some("multi_user");
    let access = if login {
        "Sign in; current project permissions apply."
    } else {
        "Local reads; Open administration requests a loopback browser session."
    };
    let mut add = |path: &str, category, description, access| {
        if let Ok(url) = base.join(path)
            && url.origin() == base.origin()
            && safe_url(url.as_str()).is_some()
        {
            report.add(label, category, url.to_string(), description, access);
        }
    };
    add(
        "",
        "web",
        "Memory Web UI: search, memories, jobs and federation. More > Access: Users, Projects, Devices, Company (administrator permissions required).",
        access,
    );
    if login
        && value
            .get("auth")
            .and_then(|a| a.get("oidc"))
            .and_then(|o| o.get("issuer_url"))
            .and_then(toml::Value::as_str)
            .is_some_and(|s| !s.is_empty())
    {
        add(
            "v1/auth/login",
            "web",
            "Browser sign-in through the configured identity provider.",
            "Identity-provider login.",
        );
    }
    add(
        "v1/openapi.yaml",
        "api",
        "Complete HTTP API specification (YAML; not a graphical documentation UI).",
        "Service access policy applies.",
    );
    add(
        "healthz",
        "api",
        "Service and database health (JSON).",
        "Health endpoint; listing does not perform a check.",
    );
    if enabled(value, "mcp", "enabled", true) && enabled(value, "mcp", "http_enabled", true) {
        let path = string(value, "mcp", "http_path").unwrap_or("/mcp");
        // MCP is a same-service path, never an alternate origin or a token URL.
        if path.starts_with('/') && !path.starts_with("//") && !path.contains(['?', '#']) {
            add(
                path.trim_start_matches('/'),
                "api",
                "HTTP MCP endpoint for agent tools; not a browser UI.",
                "Configured MCP authentication and disclosure policies apply.",
            );
        }
    }
}

fn load(path: &std::path::Path) -> Option<toml::Value> {
    fs::read_to_string(path).ok()?.parse().ok()
}

/// Read only endpoint settings, without dotenv's process-environment mutation.
fn endpoint_env(value: &mut toml::Value, path: &std::path::Path, selected: bool) {
    let file = fs::read_to_string(mem_config::env_path_for_config(path)).unwrap_or_default();
    for (section, key, name, boolean) in [
        (
            "service",
            "bind_addr",
            "MEMORY_LAYER__SERVICE__BIND_ADDR",
            false,
        ),
        (
            "auth",
            "public_base_url",
            "MEMORY_LAYER__AUTH__PUBLIC_BASE_URL",
            false,
        ),
        ("auth", "mode", "MEMORY_LAYER__AUTH__MODE", false),
        ("mcp", "enabled", "MEMORY_LAYER__MCP__ENABLED", true),
        (
            "mcp",
            "http_enabled",
            "MEMORY_LAYER__MCP__HTTP_ENABLED",
            true,
        ),
        ("mcp", "http_path", "MEMORY_LAYER__MCP__HTTP_PATH", false),
    ] {
        let from_file = file
            .lines()
            .filter_map(|line| line.trim().split_once('='))
            .filter(|(k, _)| k.trim() == name)
            .map(|(_, v)| v.trim().trim_matches(['\'', '"']).to_owned())
            .next_back();
        let setting = if selected {
            std::env::var(name).ok().or(from_file)
        } else {
            from_file
        };
        if let Some(setting) = setting {
            let parsed = if boolean {
                let Ok(flag) = setting.parse::<bool>() else {
                    continue;
                };
                toml::Value::Boolean(flag)
            } else {
                toml::Value::String(setting)
            };
            if let Some(table) = value.as_table_mut() {
                let target = table
                    .entry(section)
                    .or_insert_with(|| toml::Value::Table(Default::default()));
                if let Some(target) = target.as_table_mut() {
                    target.insert(key.into(), parsed);
                }
            }
        }
    }
}

fn inventory(
    args: &UrlsArgs,
    explicit: Option<PathBuf>,
    area: Option<String>,
    routing: Option<PathBuf>,
) -> Report {
    let mut report = Report {
        schema_version: 1,
        configured_only: true,
        ..Report::default()
    };
    let mut candidates = Vec::new();
    let routing = mem_config::discover_area_routing_path(routing, Profile::Prod);
    if let Some(path) = routing {
        match AreaRouting::read(&path) {
            Ok(routes) => for (name, target) in routes.areas {
                if area.as_ref().is_some_and(|wanted| wanted != &name) { continue; }
                if let Some(config) = target.config { candidates.push((name, config, false)); }
                else { report.notices.push(format!("{name}: remote-only; no local Web URL configured. Use its owning host or approved remote expertise.")); }
            },
            Err(_) => report.notices.push("Area routing is missing, unreadable or invalid; other configurations are still listed.".into()),
        }
    }
    if let Some(path) = explicit {
        candidates.push(("selected".into(), path, Profile::detect() == Profile::Dev));
    }
    if area.is_none() {
        if let Some(path) = mem_config::discover_global_config_path() {
            candidates.push(("installed".into(), path, false));
        }
        if let Some(path) = mem_config::discover_repo_config_path() {
            candidates.push(("repository".into(), path.clone(), false));
            if mem_config::dev_overlay_path_for_base(&path).is_some_and(|p| p.is_file()) {
                candidates.push(("repository".into(), path, true));
            }
        }
    }
    let mut seen = BTreeSet::new();
    for (mut label, path, dev) in candidates {
        if !seen.insert((label.clone(), path.clone(), dev)) {
            continue;
        }
        let Some(mut value) = load(&path) else {
            report.notices.push(format!(
                "{label}: configuration is missing, unreadable or invalid; omitted."
            ));
            continue;
        };
        if dev {
            label.push_str(" (development)");
            if let Some(overlay) =
                mem_config::dev_overlay_path_for_base(&path).and_then(|p| load(&p))
            {
                merge(&mut value, overlay);
            } else {
                report.notices.push(format!(
                    "{label}: development overlay unavailable; omitted rather than guessing."
                ));
                continue;
            }
        }
        if let Some(filter) = &area
            && label.starts_with("selected")
            && string(&value, "area", "name") != Some(filter)
        {
            continue;
        }
        endpoint_env(&mut value, &path, label.starts_with("selected"));
        collect(&mut report, &label, &value);
    }
    if let Some(path) = &args.federation_config {
        let gateway = fs::read_to_string(path)
            .ok()
            .and_then(|s| serde_json::from_str::<serde_json::Value>(&s).ok())
            .and_then(|v| {
                v.get("gateway")
                    .and_then(serde_json::Value::as_str)
                    .and_then(safe_url)
            });
        if let Some(url) = gateway {
            report.add("configured federation participant", "federation", url.to_string(), "Private federation gateway; reviewed publication and expertise exchange, not a browser UI.", "Enrolled instance credentials and independent space/project policies required.");
        } else {
            report
                .notices
                .push("Participant configuration has no safe gateway URL; omitted.".into());
        }
    }
    report.entries.sort_by_key(|e| {
        (
            match e.category {
                "web" => 0,
                "api" => 1,
                _ => 2,
            },
            e.url.clone(),
            e.description,
        )
    });
    if report.entries.is_empty() {
        report.notices.push("No matching configured URLs found. Use --config or --area-routing to select existing configuration.".into());
    }
    report
}

pub(super) fn handle(
    args: &UrlsArgs,
    explicit: Option<PathBuf>,
    area: Option<String>,
    routing: Option<PathBuf>,
) -> Result<()> {
    let report = inventory(args, explicit, area, routing);
    if args.json {
        println!("{}", serde_json::to_string_pretty(&report)?);
    } else {
        println!("Configured URLs — reachability and authorization have not been checked.");
        for (category, title) in [
            ("web", "Web interfaces"),
            ("api", "API and agent interfaces"),
            ("federation", "Federation infrastructure"),
        ] {
            if report.entries.iter().any(|e| e.category == category) {
                println!("\n{title}");
            }
            for entry in report.entries.iter().filter(|e| e.category == category) {
                println!(
                    "  [{}] {}\n    {}\n    {}",
                    entry
                        .installations
                        .iter()
                        .cloned()
                        .collect::<Vec<_>>()
                        .join(", "),
                    entry.url,
                    entry.description,
                    entry.access
                );
            }
        }
        for notice in &report.notices {
            println!("\nNote: {notice}");
        }
    }
    Ok(())
}

#[cfg(test)]
mod tests {
    use super::*;

    fn config(extra: &str) -> toml::Value {
        format!("[service]\nbind_addr = '[::]:4040'\napi_token = 'do-not-print'\n{extra}")
            .parse()
            .unwrap()
    }

    #[test]
    fn ui_first_with_descriptions_and_json_safe_metadata() {
        let mut report = Report::default();
        collect(&mut report, "workshop", &config(""));
        assert_eq!(report.entries[0].category, "web");
        assert_eq!(report.entries[0].url, "http://[::1]:4040/");
        assert!(report.entries[0].description.contains("More > Access"));
        assert!(report.entries.iter().any(|e| e.url.ends_with("/mcp")));
        let json = serde_json::to_string(&report).unwrap();
        assert!(!json.contains("do-not-print"));
        assert!(!json.contains("api_token"));
    }

    #[test]
    fn public_address_and_enabled_oidc_login_take_precedence() {
        let mut report = Report::default();
        collect(
            &mut report,
            "installed",
            &config(
                "[auth]\nmode='multi_user'\npublic_base_url='https://memory.example.test/base'\n[auth.oidc]\nissuer_url='https://identity.example.test'",
            ),
        );
        assert!(
            report
                .entries
                .iter()
                .all(|e| e.url.starts_with("https://memory.example.test/base/"))
        );
        assert!(
            report
                .entries
                .iter()
                .any(|e| e.url.ends_with("v1/auth/login"))
        );
    }

    #[test]
    fn disabled_login_and_http_mcp_are_not_advertised() {
        for extra in ["[mcp]\nenabled=false", "[mcp]\nhttp_enabled=false"] {
            let mut report = Report::default();
            collect(&mut report, "area", &config(extra));
            assert!(
                !report
                    .entries
                    .iter()
                    .any(|e| e.url.contains("mcp") || e.url.contains("auth/login"))
            );
        }
    }

    #[test]
    fn duplicate_urls_retain_all_installation_labels() {
        let mut report = Report::default();
        collect(&mut report, "repository", &config(""));
        let count = report.entries.len();
        collect(&mut report, "selected", &config(""));
        assert_eq!(report.entries.len(), count);
        assert_eq!(report.entries[0].installations.len(), 2);
    }

    #[test]
    fn sensitive_and_non_http_urls_are_rejected_without_echoing() {
        for url in [
            "https://user:secret@example.test",
            "https://example.test/?token=secret",
            "https://example.test/#secret",
            "postgresql://user:secret@localhost/db",
            "file:///secret",
        ] {
            assert!(safe_url(url).is_none());
            let mut report = Report::default();
            collect(
                &mut report,
                "area",
                &config(&format!("[auth]\npublic_base_url='{url}'")),
            );
            assert!(report.entries.is_empty());
            assert!(!serde_json::to_string(&report).unwrap().contains("secret"));
        }
    }

    #[test]
    fn wildcard_ipv4_is_a_loopback_link() {
        let value: toml::Value = "[service]\nbind_addr='0.0.0.0:4040'".parse().unwrap();
        assert_eq!(
            config_base(&value).unwrap().as_str(),
            "http://127.0.0.1:4040/"
        );
    }

    #[test]
    fn invalid_mcp_paths_never_advertise_another_origin_or_token() {
        for path in [
            "//evil.test/mcp",
            "/https://evil.test/mcp",
            "/mcp?token=secret",
            "relative",
        ] {
            let mut report = Report::default();
            collect(
                &mut report,
                "area",
                &config(&format!("[mcp]\nhttp_path='{path}'")),
            );
            assert!(
                !report
                    .entries
                    .iter()
                    .any(|e| e.description.starts_with("HTTP MCP"))
            );
        }
    }

    #[test]
    fn omitted_bind_uses_the_runtime_configuration_default() {
        let value: toml::Value = "[service]".parse().unwrap();
        assert_eq!(
            config_base(&value).unwrap(),
            safe_url(&format!("http://{}/", mem_config::default_bind_addr())).unwrap()
        );
    }

    #[test]
    fn development_overlay_changes_only_its_candidate() {
        let mut base = config("");
        let untouched = base.clone();
        merge(
            &mut base,
            "[service]\nbind_addr='127.0.0.1:4250'\n[mcp]\nhttp_enabled=false"
                .parse()
                .unwrap(),
        );
        assert_eq!(
            config_base(&base).unwrap().as_str(),
            "http://127.0.0.1:4250/"
        );
        assert_eq!(
            config_base(&untouched).unwrap().as_str(),
            "http://[::1]:4040/"
        );
        assert!(!enabled(&base, "mcp", "http_enabled", true));
    }

    #[test]
    fn routing_filters_remote_only_and_missing_configs_without_service_access() {
        let dir = std::env::temp_dir().join(format!("memory-urls-{}", uuid::Uuid::new_v4()));
        fs::create_dir(&dir).unwrap();
        let config_path = dir.join("workshop.toml");
        fs::write(&config_path, "[service]\nbind_addr='127.0.0.1:1'").unwrap();
        let route = dir.join("routing.json");
        fs::write(&route, serde_json::to_vec(&serde_json::json!({
            "version":1,"default_area":"general","areas": {
                "general":{"instance_id":uuid::Uuid::new_v4(),"config":dir.join("missing.toml")},
                "workshop":{"instance_id":uuid::Uuid::new_v4(),"config":config_path},
                "trust":{"instance_id":uuid::Uuid::new_v4(),"config":null}
            },"routes":[]
        })).unwrap()).unwrap();
        #[cfg(unix)]
        {
            use std::os::unix::fs::PermissionsExt;
            fs::set_permissions(&route, fs::Permissions::from_mode(0o600)).unwrap();
        }
        let args = UrlsArgs {
            json: true,
            federation_config: None,
        };
        let report = inventory(&args, None, Some("workshop".into()), Some(route.clone()));
        assert!(!report.entries.is_empty());
        assert!(
            report
                .entries
                .iter()
                .all(|e| e.installations.contains("workshop"))
        );
        let trust = inventory(&args, None, Some("trust".into()), Some(route.clone()));
        assert!(trust.entries.is_empty());
        assert!(
            trust
                .notices
                .iter()
                .any(|n| n.contains("trust: remote-only"))
        );
        let missing = inventory(&args, None, Some("general".into()), Some(route));
        assert!(missing.entries.is_empty());
        assert!(
            missing
                .notices
                .iter()
                .any(|n| n.contains("configuration is missing"))
        );
        fs::remove_dir_all(dir).unwrap();
    }

    #[test]
    fn gateway_inventory_never_reads_or_prints_private_credentials() {
        let dir = std::env::temp_dir().join(format!("memory-urls-{}", uuid::Uuid::new_v4()));
        fs::create_dir(&dir).unwrap();
        let participant = dir.join("participant.json");
        fs::write(&participant, r#"{"gateway":"https://gateway.example.test","token_file":"/private/secret","pds":"https://private-pds.example.test","database_url":"postgresql://secret"}"#).unwrap();
        let report = inventory(
            &UrlsArgs {
                json: true,
                federation_config: Some(participant),
            },
            None,
            Some("no-such-area".into()),
            Some(dir.join("missing.json")),
        );
        assert_eq!(report.entries.len(), 1);
        let output = serde_json::to_string(&report).unwrap();
        assert!(output.contains("gateway.example.test"));
        assert!(!output.contains("secret") && !output.contains("private-pds"));
        fs::remove_dir_all(dir).unwrap();
    }
}
