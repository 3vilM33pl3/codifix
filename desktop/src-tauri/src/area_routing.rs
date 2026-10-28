//! Operator routing stays native. Only public area identity/origin reaches the renderer.
use crate::{Session, Transport};
use codifix_config::{discover_area_routing_path, AreaIdentityConfig, AreaRouting, Profile};
use serde::{Deserialize, Serialize};
use std::path::Path;
use tauri::State;
use url::Url;

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq)]
pub(crate) struct Binding {
    pub name: String,
    pub instance_id: uuid::Uuid,
    pub endpoint: String,
}
#[derive(Serialize)]
pub(crate) struct Choice {
    name: String,
    instance_id: uuid::Uuid,
    binding: Option<Binding>,
    error: Option<String>,
}
#[derive(Default)]
pub(crate) enum Selection {
    #[default]
    Custom,
    Unavailable,
    Area(Binding),
}
fn routing() -> Result<Option<AreaRouting>, String> {
    discover_area_routing_path(None, Profile::Prod)
        .map(|path| AreaRouting::read(&path).map_err(|e| e.to_string()))
        .transpose()
}
#[derive(Deserialize)]
struct PublicConfig {
    area: Option<AreaIdentityConfig>,
    auth: Option<PublicAuth>,
}
#[derive(Deserialize)]
struct PublicAuth {
    public_base_url: Option<String>,
}
fn resolve(
    routing: &AreaRouting,
    area: Option<&str>,
    repository: Option<&Path>,
    project: Option<&str>,
) -> Result<Binding, String> {
    let resolved = routing
        .resolve(area, repository, project)
        .map_err(|e| e.to_string())?;
    // Deliberately do not load AppConfig, dotenv, token, or provider configuration.
    let metadata = std::fs::metadata(&resolved.config).map_err(|_| "Area config unavailable")?;
    if metadata.len() > 1024 * 1024 {
        return Err("Area config exceeds size limit".into());
    }
    #[cfg(unix)]
    {
        use std::os::unix::fs::{MetadataExt, PermissionsExt};
        let owner = std::process::Command::new("id")
            .arg("-u")
            .output()
            .map_err(|_| "Cannot determine area config owner")?;
        let uid = String::from_utf8_lossy(&owner.stdout)
            .trim()
            .parse::<u32>()
            .map_err(|_| "Cannot determine area config owner")?;
        if !metadata.is_file()
            || metadata.permissions().mode() & 0o022 != 0
            || !owner.status.success()
            || (metadata.uid() != 0 && metadata.uid() != uid)
        {
            return Err("Area client config must belong to this user or root and must not be group/world writable".into());
        }
    }
    #[cfg(not(windows))]
    let text = std::fs::read_to_string(&resolved.config).map_err(|_| "Area config unavailable")?;
    #[cfg(windows)]
    let text = String::from_utf8(codifix_platform::private_windows::Input::open(&resolved.config, 1024 * 1024)
        .and_then(|input| input.read(1024 * 1024))
        .map_err(|_| "Area client config requires a protected Windows file with private ACLs and no reparse points or hard links")?)
        .map_err(|_| "Invalid area client configuration")?;
    let config: PublicConfig =
        toml::from_str(&text).map_err(|_| "Invalid area client configuration")?;
    if config.area.as_ref() != Some(&resolved.identity) {
        return Err("Area client config identity does not match routing".into());
    }
    let origin = config
        .auth
        .and_then(|a| a.public_base_url)
        .ok_or("Area client config requires auth.public_base_url; no fallback was attempted")?;
    let origin = crate::endpoint(&origin)?;
    if origin.scheme() != "https" {
        return Err("Area browser origin must use HTTPS".into());
    }
    Ok(Binding {
        name: resolved.identity.name,
        instance_id: resolved.identity.instance_id,
        endpoint: origin.origin().ascii_serialization(),
    })
}
#[tauri::command]
pub(crate) fn list_areas() -> Result<Vec<Choice>, String> {
    let Some(routing) = routing()? else {
        return Ok(vec![]);
    };
    Ok(routing
        .areas
        .iter()
        .map(|(name, target)| {
            let result = resolve(&routing, Some(name), None, None);
            Choice {
                name: name.clone(),
                instance_id: target.instance_id,
                binding: result.as_ref().ok().cloned(),
                error: result.err(),
            }
        })
        .collect())
}
#[tauri::command]
pub(crate) fn resolve_area(
    area: Option<String>,
    repository: Option<String>,
    project: Option<String>,
) -> Result<Option<Binding>, String> {
    routing()?
        .map(|r| {
            resolve(
                &r,
                area.as_deref(),
                repository.as_deref().map(Path::new),
                project.as_deref(),
            )
        })
        .transpose()
}
#[tauri::command]
pub(crate) fn select_area(
    area: Option<String>,
    repository: Option<String>,
    project: Option<String>,
    state: State<'_, Transport>,
) -> Result<Option<Binding>, String> {
    {
        let mut c = state.0.lock().map_err(|_| "Connection unavailable")?;
        c.generation += 1;
        state
            .1
            .store(c.generation, std::sync::atomic::Ordering::SeqCst);
        c.session = None;
        c.pending = None;
        c.selection = Selection::Unavailable;
    }
    let binding = resolve_area(area, repository, project)?;
    let mut c = state.0.lock().map_err(|_| "Connection unavailable")?;
    c.selection = binding
        .clone()
        .map(Selection::Area)
        .unwrap_or(Selection::Custom);
    Ok(binding)
}
#[tauri::command]
pub(crate) fn select_custom_service(state: State<'_, Transport>) -> Result<(), String> {
    let mut c = state.0.lock().map_err(|_| "Connection unavailable")?;
    c.generation += 1;
    state
        .1
        .store(c.generation, std::sync::atomic::Ordering::SeqCst);
    c.session = None;
    c.pending = None;
    c.selection = Selection::Custom;
    Ok(())
}
pub(crate) fn selected(state: &Transport, origin: &Url) -> Result<Option<Binding>, String> {
    let c = state.0.lock().map_err(|_| "Connection unavailable")?;
    match &c.selection {
        Selection::Custom => Ok(None),
        Selection::Unavailable => Err("Selected area is unavailable. Select an available area or explicitly choose a custom service.".into()),
        Selection::Area(binding) if binding.endpoint == origin.origin().ascii_serialization() => Ok(Some(binding.clone())),
        Selection::Area(_) => Err("Service address does not match the selected area's configured browser origin".into()),
    }
}
pub(crate) async fn verify(origin: &Url, binding: Option<&Binding>) -> Result<(), String> {
    if let Some(binding) = binding {
        if binding.endpoint != origin.origin().ascii_serialization() {
            return Err("Area origin mismatch".into());
        }
        let health = Session::new(origin.clone(), None)?
            .request_unchecked("/healthz", reqwest::Method::GET, None)
            .await?;
        if health["area_instance_id"]
            .as_str()
            .and_then(|s| uuid::Uuid::parse_str(s).ok())
            != Some(binding.instance_id)
        {
            return Err(
                "Service area identity does not match routing. No credentials were sent.".into(),
            );
        }
    }
    Ok(())
}

#[cfg(test)]
mod tests {
    use super::*;
    use codifix_config::{AreaRoute, AreaTarget};
    use std::collections::BTreeMap;
    use uuid::Uuid;
    struct Fixture {
        root: std::path::PathBuf,
        routing: AreaRouting,
    }
    impl Fixture {
        fn new() -> Self {
            let root = std::env::temp_dir().join(format!("desktop-area-{}", Uuid::new_v4()));
            std::fs::create_dir_all(&root).unwrap();
            let mut areas = BTreeMap::new();
            for name in ["general", "workshop", "homelab"] {
                let instance_id = Uuid::new_v4();
                let config = root.join(format!("{name}.toml"));
                std::fs::write(&config, format!("[area]\nname = '{name}'\ninstance_id = '{instance_id}'\n[auth]\npublic_base_url = 'https://{name}.example'\n[client]\ntoken = 'must-not-be-used'\n")).unwrap();
                #[cfg(unix)]
                {
                    use std::os::unix::fs::PermissionsExt;
                    std::fs::set_permissions(&config, std::fs::Permissions::from_mode(0o600))
                        .unwrap();
                }
                areas.insert(
                    name.into(),
                    AreaTarget {
                        instance_id,
                        config: Some(config),
                    },
                );
            }
            Self {
                root,
                routing: AreaRouting {
                    version: 1,
                    default_area: "general".into(),
                    areas,
                    routes: vec![],
                },
            }
        }
    }
    impl Drop for Fixture {
        fn drop(&mut self) {
            let _ = std::fs::remove_dir_all(&self.root);
        }
    }
    #[test]
    fn exact_repository_precedes_project_only_and_slug_does_not_guess_area() {
        let mut f = Fixture::new();
        let a = f.root.join("a");
        let b = f.root.join("b");
        std::fs::create_dir_all(&a).unwrap();
        std::fs::create_dir_all(&b).unwrap();
        f.routing.routes = vec![
            AreaRoute {
                area: "workshop".into(),
                repository: Some(a.clone()),
                project: Some("shared".into()),
            },
            AreaRoute {
                area: "homelab".into(),
                repository: Some(b.clone()),
                project: Some("shared".into()),
            },
            AreaRoute {
                area: "homelab".into(),
                repository: None,
                project: Some("shared".into()),
            },
        ];
        assert_eq!(
            resolve(&f.routing, None, Some(&a), Some("shared"))
                .unwrap()
                .name,
            "workshop"
        );
        assert_eq!(
            resolve(&f.routing, None, Some(&b), Some("shared"))
                .unwrap()
                .name,
            "homelab"
        );
        assert_eq!(
            resolve(&f.routing, Some("general"), Some(&b), Some("shared"))
                .unwrap()
                .name,
            "general"
        );
        assert_eq!(
            resolve(&f.routing, None, None, Some("homelab"))
                .unwrap()
                .name,
            "general"
        );
        let public =
            serde_json::to_string(&resolve(&f.routing, Some("homelab"), None, None).unwrap())
                .unwrap();
        assert!(!public.contains("must-not-be-used") && !public.contains("token"));
    }
    #[test]
    fn unavailable_remote_mismatched_or_missing_origin_never_falls_back() {
        let mut f = Fixture::new();
        let config = f.routing.areas["homelab"].config.clone().unwrap();
        std::fs::write(&config, "[area]\nname='workshop'\ninstance_id='11111111-1111-4111-8111-111111111111'\n[auth]\npublic_base_url='https://workshop.example'\n").unwrap();
        assert!(resolve(&f.routing, Some("homelab"), None, None)
            .unwrap_err()
            .contains("identity"));
        std::fs::write(
            &config,
            format!(
                "[area]\nname='homelab'\ninstance_id='{}'",
                f.routing.areas["homelab"].instance_id
            ),
        )
        .unwrap();
        assert!(resolve(&f.routing, Some("homelab"), None, None)
            .unwrap_err()
            .contains("public_base_url"));
        std::fs::remove_file(&config).unwrap();
        assert!(resolve(&f.routing, Some("homelab"), None, None)
            .unwrap_err()
            .contains("unavailable"));
        f.routing.areas.get_mut("homelab").unwrap().config = None;
        assert!(resolve(&f.routing, Some("homelab"), None, None)
            .unwrap_err()
            .contains("remote-only"));
        assert_eq!(
            resolve(&f.routing, Some("workshop"), None, None)
                .unwrap()
                .name,
            "workshop"
        );
    }
    #[test]
    fn mismatched_health_never_receives_credentials_and_stale_generation_never_sends_auth() {
        use std::io::{Read, Write};
        use std::sync::{
            atomic::{AtomicU64, Ordering},
            Arc,
        };
        for stale in [false, true] {
            let listener = std::net::TcpListener::bind("127.0.0.1:0").unwrap();
            let origin =
                crate::endpoint(&format!("http://{}", listener.local_addr().unwrap())).unwrap();
            let identity = Uuid::new_v4();
            let epoch = Arc::new(AtomicU64::new(1));
            let server_epoch = epoch.clone();
            let server = std::thread::spawn(move || {
                let (mut socket, _) = listener.accept().unwrap();
                socket
                    .set_read_timeout(Some(std::time::Duration::from_secs(5)))
                    .unwrap();
                let mut bytes = [0; 8192];
                let size = socket.read(&mut bytes).unwrap();
                let request = String::from_utf8_lossy(&bytes[..size]).to_lowercase();
                assert!(request.starts_with("get /healthz "));
                assert!(
                    !request.contains("authorization")
                        && !request.contains("cookie")
                        && !request.contains("secret-test")
                );
                if stale {
                    server_epoch.store(2, Ordering::SeqCst);
                }
                let body = serde_json::json!({"area_instance_id": if stale { identity } else { Uuid::new_v4() }}).to_string();
                socket.write_all(format!("HTTP/1.1 200 OK\r\nContent-Length: {}\r\nConnection: close\r\n\r\n{body}", body.len()).as_bytes()).unwrap();
                listener.set_nonblocking(true).unwrap();
                std::thread::sleep(std::time::Duration::from_millis(80));
                assert!(
                    listener.accept().is_err(),
                    "authenticated request followed invalid/stale health"
                );
            });
            tauri::async_runtime::block_on(async {
                let mut session = Session::new(origin.clone(), Some("secret-test".into())).unwrap();
                session.area = Some(Binding {
                    name: "homelab".into(),
                    instance_id: identity,
                    endpoint: origin.origin().ascii_serialization(),
                });
                session.fence = Some((epoch, 1));
                let error = session
                    .request("/v1/auth/me", reqwest::Method::GET, None)
                    .await
                    .unwrap_err();
                assert!(
                    error.contains(if stale {
                        "Connection changed"
                    } else {
                        "area identity"
                    }),
                    "{error}"
                );
            });
            server.join().unwrap();
        }
    }
}
