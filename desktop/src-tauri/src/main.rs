#![cfg_attr(not(debug_assertions), windows_subsystem = "windows")]
use reqwest::{
    cookie::{CookieStore, Jar},
    Client, Method,
};
use serde::Serialize;
use serde_json::Value;
use std::{
    sync::{Arc, Mutex},
    time::Duration,
};
use tauri::State;
use tauri_plugin_dialog::DialogExt;
use url::Url;
const MAX_RESPONSE: usize = 8 * 1024 * 1024;
#[derive(Clone)]
struct Session {
    endpoint: Url,
    client: Client,
    jar: Arc<Jar>,
    token: Option<String>,
}
#[derive(Default)]
struct Connection {
    generation: u64,
    session: Option<Session>,
}
#[derive(Default)]
struct Transport(Mutex<Connection>);
fn endpoint(value: &str) -> Result<Url, String> {
    let u = Url::parse(value).map_err(|_| "Invalid service URL")?;
    let local = is_local(&u);
    if !matches!(u.scheme(), "http" | "https")
        || (u.scheme() == "http" && !local)
        || !u.username().is_empty()
        || u.password().is_some()
        || u.query().is_some()
        || u.fragment().is_some()
        || u.path() != "/"
        || u.host_str().is_none()
    {
        return Err("Use a service origin: loopback HTTP or HTTPS, without credentials, path, query or fragment".into());
    }
    Ok(u)
}
fn is_local(u: &Url) -> bool {
    u.host_str().is_some_and(|h| {
        h == "localhost"
            || h.trim_matches(['[', ']'])
                .parse::<std::net::IpAddr>()
                .is_ok_and(|ip| ip.is_loopback())
    })
}
fn uuid(s: &str) -> bool {
    s.len() == 36
        && s.chars().enumerate().all(|(i, c)| {
            if [8, 13, 18, 23].contains(&i) {
                c == '-'
            } else {
                c.is_ascii_hexdigit()
            }
        })
}
fn allowed(path: &str, method: &str) -> bool {
    if path.contains(['#', '\\', '%']) || !path.starts_with("/v1/") {
        return false;
    }
    let raw = path.split('?').next().unwrap_or("");
    let p: Vec<_> = raw.split('/').collect();
    if method == "GET" && matches!(raw, "/v1/auth/me" | "/v1/agents") {
        return true;
    }
    if p.len() >= 4 && p[2] == "memory" && uuid(p[3]) {
        return method == "GET" && (p.len() == 4 || p.len() == 5 && p[4] == "history");
    }
    if p.len() < 5
        || p[2] != "projects"
        || p[3].is_empty()
        || !p[3]
            .chars()
            .all(|c| c.is_ascii_alphanumeric() || matches!(c, '-' | '_' | '.'))
        || matches!(p[3], "." | "..")
    {
        return false;
    }
    match (method, p.len(), p[4]) {
        (
            "GET",
            5,
            "overview"
            | "memories"
            | "replacement-proposals"
            | "activities"
            | "jobs"
            | "graph"
            | "memory-graph",
        ) => true,
        ("GET", 6, "graph") => p[5] == "status",
        ("POST", 5, "resume") => true,
        ("GET", 6, "jobs") => uuid(p[5]),
        ("POST", 7, "jobs") => uuid(p[5]) && p[6] == "retry",
        ("POST", 7, "replacement-proposals") => uuid(p[5]) && matches!(p[6], "approve" | "reject"),
        _ => false,
    }
}
impl Session {
    async fn request(
        &self,
        path: &str,
        method: Method,
        body: Option<Value>,
    ) -> Result<Value, String> {
        let url = self.endpoint.join(path).map_err(|_| "Invalid API path")?;
        let mut request = self
            .client
            .request(method.clone(), url)
            .header("origin", self.endpoint.origin().ascii_serialization());
        if let Some(token) = &self.token {
            request = request.bearer_auth(token);
        }
        if method != Method::GET {
            if let Some(cookies) = self.jar.cookies(&self.endpoint) {
                if let Ok(cookies) = cookies.to_str() {
                    if let Some(csrf) = cookies
                        .split(';')
                        .map(str::trim)
                        .find_map(|c| c.strip_prefix("codifix_csrf="))
                    {
                        request = request.header("x-csrf-token", csrf);
                    }
                }
            }
        }
        if let Some(body) = body {
            request = request.json(&body);
        }
        let mut response = request
            .send()
            .await
            .map_err(|_| "Service connection failed; outcome may be unconfirmed")?;
        let status = response.status();
        let mut bytes = Vec::new();
        while let Some(chunk) = response
            .chunk()
            .await
            .map_err(|_| "Response interrupted; outcome unconfirmed")?
        {
            if bytes.len() + chunk.len() > MAX_RESPONSE {
                return Err("Service response exceeds size limit".into());
            }
            bytes.extend_from_slice(&chunk);
        }
        if !status.is_success() {
            return Err(format!(
                "Service returned HTTP {}. Refresh before retrying a write.",
                status.as_u16()
            ));
        }
        serde_json::from_slice(&bytes).map_err(|_| "Service returned invalid JSON".into())
    }
}
#[tauri::command]
async fn connect_service(
    endpoint: String,
    token: Option<String>,
    state: State<'_, Transport>,
) -> Result<Value, String> {
    let endpoint = crate::endpoint(&endpoint)?;
    if token
        .as_ref()
        .is_some_and(|t| t.is_empty() || t.contains(['\r', '\n']))
    {
        return Err("Invalid token".into());
    }
    let generation = {
        let mut c = state.0.lock().map_err(|_| "Connection unavailable")?;
        c.generation += 1;
        c.session = None;
        c.generation
    };
    let jar = Arc::new(Jar::default());
    let client = Client::builder()
        .cookie_provider(jar.clone())
        .redirect(reqwest::redirect::Policy::none())
        .timeout(Duration::from_secs(30))
        .connect_timeout(Duration::from_secs(10))
        .build()
        .map_err(|_| "Cannot create service client")?;
    let session = Session {
        endpoint,
        client,
        jar,
        token,
    };
    let auth = session.request("/v1/auth/me", Method::GET, None).await?;
    if session.token.is_none()
        && auth["mode"] == "single_user"
        && auth["read_only"] == false
        && is_local(&session.endpoint)
    {
        session
            .request("/v1/auth/session/bootstrap", Method::POST, None)
            .await?;
    }
    let auth = session.request("/v1/auth/me", Method::GET, None).await?;
    let mut c = state.0.lock().map_err(|_| "Connection unavailable")?;
    if c.generation != generation {
        return Err("Connection changed; discard this response".into());
    }
    c.session = Some(session);
    Ok(auth)
}
#[tauri::command]
fn disconnect_service(state: State<'_, Transport>) -> Result<(), String> {
    let mut c = state.0.lock().map_err(|_| "Connection unavailable")?;
    c.generation += 1;
    c.session = None;
    Ok(())
}
#[tauri::command]
async fn api_request(
    path: String,
    method: String,
    body: Option<Value>,
    state: State<'_, Transport>,
) -> Result<Value, String> {
    if !allowed(&path, &method) {
        return Err("API operation is not allowed".into());
    }
    let (generation, session) = {
        let c = state.0.lock().map_err(|_| "Connection unavailable")?;
        (
            c.generation,
            c.session.clone().ok_or("Connect to a service first")?,
        )
    };
    let result = session
        .request(
            &path,
            if method == "GET" {
                Method::GET
            } else {
                Method::POST
            },
            body,
        )
        .await;
    if state
        .0
        .lock()
        .map_err(|_| "Connection unavailable")?
        .generation
        != generation
    {
        return Err("Connection changed; discard this response".into());
    }
    result
}
#[derive(Serialize)]
struct ProjectChoice {
    path: String,
    slug: Option<String>,
}
#[tauri::command]
async fn choose_project(app: tauri::AppHandle) -> Result<Option<ProjectChoice>, String> {
    let (tx, rx) = tokio::sync::oneshot::channel();
    app.dialog().file().pick_folder(move |p| {
        let _ = tx.send(p);
    });
    let Some(file) = rx.await.map_err(|_| "Folder selection cancelled")? else {
        return Ok(None);
    };
    let path = file.into_path().map_err(|_| "Choose a local folder")?;
    let config = path.join(".codifix/project.toml");
    let slug = if config.exists() {
        let metadata =
            std::fs::metadata(&config).map_err(|_| "Cannot read project configuration")?;
        if metadata.len() > 65536 {
            return Err("Project configuration exceeds size limit".into());
        }
        let text =
            std::fs::read_to_string(config).map_err(|_| "Cannot read project configuration")?;
        let value: toml::Value =
            toml::from_str(&text).map_err(|_| "Invalid project configuration")?;
        value
            .get("slug")
            .and_then(toml::Value::as_str)
            .map(str::to_owned)
    } else {
        None
    };
    Ok(Some(ProjectChoice {
        path: path.to_string_lossy().into_owned(),
        slug,
    }))
}
#[tauri::command]
fn open_service(
    section: String,
    endpoint: Option<String>,
    state: State<'_, Transport>,
) -> Result<(), String> {
    // Login can be opened even after a 401, without pretending that browser
    // cookies authenticate the native transport. No arbitrary path is accepted.
    let origin = if let Some(value) = endpoint {
        if section != "login" {
            return Err("An explicit endpoint is only allowed for login".into());
        }
        crate::endpoint(&value)?
    } else {
        let c = state.0.lock().map_err(|_| "Connection unavailable")?;
        c.session
            .as_ref()
            .ok_or("Connect to a service first")?
            .endpoint
            .clone()
    };
    let path = match section.as_str() {
        "home" | "help" => "/",
        "login" => "/v1/auth/login?return_to=%2F",
        _ => return Err("Unknown service section".into()),
    };
    open::that(
        origin
            .join(path)
            .map_err(|_| "Invalid service URL")?
            .as_str(),
    )
    .map_err(|_| "Cannot open browser".into())
}

fn main() {
    tauri::Builder::default()
        .manage(Transport::default())
        .plugin(tauri_plugin_dialog::init())
        .plugin(tauri_plugin_notification::init())
        .plugin(tauri_plugin_window_state::Builder::default().build())
        .invoke_handler(tauri::generate_handler![
            connect_service,
            disconnect_service,
            api_request,
            choose_project,
            open_service
        ])
        .run(tauri::generate_context!())
        .expect("Unable to start Codifix Desktop");
}
#[cfg(test)]
mod tests {
    use super::*;
    #[test]
    fn endpoints() {
        for s in [
            "http://127.0.0.1:8080",
            "http://localhost:8080",
            "https://example.com",
        ] {
            assert!(endpoint(s).is_ok());
        }
        for s in [
            "http://example.com",
            "https://u:p@example.com",
            "https://example.com/path",
            "https://example.com?q=x",
            "file:///tmp/a",
        ] {
            assert!(endpoint(s).is_err());
        }
    }
    #[test]
    fn paths() {
        assert!(allowed("/v1/projects/codifix/activities?limit=20", "GET"));
        assert!(allowed("/v1/projects/codifix/resume", "POST"));
        for p in [
            "/v1/auth/tokens",
            "/v1/projects/../overview",
            "/v1/projects/%2e%2e/overview",
            "https://bad/v1/auth/me",
            "/v1/auth/session/bootstrap",
        ] {
            assert!(!allowed(p, "POST"));
        }
    }
    #[test]
    fn cookie_session_sends_csrf_and_rejects_redirects() {
        use std::io::{Read, Write};
        let listener = std::net::TcpListener::bind("127.0.0.1:0").unwrap();
        let base = format!("http://{}/", listener.local_addr().unwrap());
        let server = std::thread::spawn(move || {
            for index in 0..3 {
                let (mut socket, _) = listener.accept().unwrap();
                socket
                    .set_read_timeout(Some(Duration::from_secs(5)))
                    .unwrap();
                let mut buffer = [0; 8192];
                let count = socket.read(&mut buffer).unwrap();
                let request = String::from_utf8_lossy(&buffer[..count]).to_lowercase();
                if index == 1 {
                    assert!(request.contains("x-csrf-token: testcsrf"));
                    assert!(request.contains("codifix_session=testsession"));
                }
                let response = if index == 0 {
                    "HTTP/1.1 200 OK\r\nContent-Length: 2\r\nSet-Cookie: codifix_csrf=testcsrf; Path=/\r\nSet-Cookie: codifix_session=testsession; Path=/; HttpOnly\r\nConnection: close\r\n\r\n{}"
                } else if index == 1 {
                    "HTTP/1.1 200 OK\r\nContent-Length: 2\r\nConnection: close\r\n\r\n{}"
                } else {
                    "HTTP/1.1 302 Found\r\nLocation: http://example.com/\r\nContent-Length: 0\r\nConnection: close\r\n\r\n"
                };
                socket.write_all(response.as_bytes()).unwrap();
            }
        });
        tauri::async_runtime::block_on(async {
            let jar = Arc::new(Jar::default());
            let session = Session {
                endpoint: endpoint(&base).unwrap(),
                client: Client::builder()
                    .cookie_provider(jar.clone())
                    .redirect(reqwest::redirect::Policy::none())
                    .timeout(Duration::from_secs(5))
                    .build()
                    .unwrap(),
                jar,
                token: None,
            };
            session
                .request("/v1/auth/me", Method::GET, None)
                .await
                .unwrap();
            session
                .request("/v1/projects/demo/resume", Method::POST, None)
                .await
                .unwrap();
            assert!(session
                .request("/v1/auth/me", Method::GET, None)
                .await
                .unwrap_err()
                .contains("302"));
        });
        server.join().unwrap();
    }
}
