//! Native one-work AppContainer launcher. No Docker, TCP broker, inherited
//! provider credentials or network capability reaches either driver or Pi.
use super::*;
use crate::{pi_admission::PiAdmission, pi_broker::PiBroker, private_files};
use axum::{
    body::{Body, to_bytes},
    http::Request,
};
use mem_platform::windows_process::{CodeFile, ProcessSpec};
use mem_platform_files::windows::create_private_directory;
use mem_supervisor_pi::{
    ProcessDisposition,
    sandbox::{ChildInput, ChildReport},
};
use std::{
    ffi::c_void,
    fs::File,
    mem::size_of,
    os::windows::io::{AsHandle, FromRawHandle, OwnedHandle},
    ptr::null_mut,
    time::{Duration, Instant},
};
use tokio::{
    io::{AsyncReadExt, AsyncWriteExt},
    net::windows::named_pipe::{NamedPipeServer, ServerOptions},
    time::timeout,
};
use tower::ServiceExt;
use uuid::Uuid;
use windows_sys::Win32::{
    Foundation::LocalFree,
    Security::{
        Authorization::{
            ConvertSidToStringSidW, ConvertStringSecurityDescriptorToSecurityDescriptorW,
            SDDL_REVISION_1,
        },
        GetTokenInformation, SECURITY_ATTRIBUTES, TOKEN_QUERY, TOKEN_USER, TokenUser,
    },
    System::{
        Pipes::CreatePipe,
        Threading::{GetCurrentProcess, OpenProcessToken},
    },
};

#[derive(Deserialize)]
#[serde(deny_unknown_fields)]
struct BrokerRequest {
    schema_version: u32,
    method: String,
    path: String,
    headers: Vec<(String, String)>,
    body: String,
}
#[derive(Serialize)]
struct BrokerResponse {
    schema_version: u32,
    status: u16,
    headers: Vec<(String, String)>,
    body: String,
}

fn pipe() -> Result<(File, File), LaunchError> {
    let mut read = null_mut();
    let mut write = null_mut();
    let sa = SECURITY_ATTRIBUTES {
        nLength: size_of::<SECURITY_ATTRIBUTES>() as u32,
        lpSecurityDescriptor: null_mut(),
        bInheritHandle: 0,
    };
    if unsafe { CreatePipe(&mut read, &mut write, &sa, 4096) } == 0 {
        return Err(LaunchError::Files);
    }
    Ok(unsafe { (File::from_raw_handle(read), File::from_raw_handle(write)) })
}

fn current_sid() -> Result<String, LaunchError> {
    let mut token = null_mut();
    if unsafe { OpenProcessToken(GetCurrentProcess(), TOKEN_QUERY, &mut token) } == 0 {
        return Err(LaunchError::Files);
    }
    let token = unsafe { OwnedHandle::from_raw_handle(token) };
    use std::os::windows::io::AsRawHandle;
    let mut size = 0;
    unsafe {
        GetTokenInformation(token.as_raw_handle(), TokenUser, null_mut(), 0, &mut size);
    }
    if size == 0 || size > 65536 {
        return Err(LaunchError::Files);
    }
    let mut bytes = vec![0usize; (size as usize).div_ceil(size_of::<usize>())];
    if unsafe {
        GetTokenInformation(
            token.as_raw_handle(),
            TokenUser,
            bytes.as_mut_ptr().cast(),
            size,
            &mut size,
        )
    } == 0
    {
        return Err(LaunchError::Files);
    }
    let user = unsafe { &*bytes.as_ptr().cast::<TOKEN_USER>() };
    let mut text = null_mut();
    if unsafe { ConvertSidToStringSidW(user.User.Sid, &mut text) } == 0 {
        return Err(LaunchError::Files);
    }
    let mut len = 0;
    while len < 256 && unsafe { *text.add(len) } != 0 {
        len += 1;
    }
    let result = if len < 256 {
        String::from_utf16(unsafe { std::slice::from_raw_parts(text, len) })
            .map_err(|_| LaunchError::Files)
    } else {
        Err(LaunchError::Files)
    };
    unsafe {
        LocalFree(text.cast());
    }
    result
}

/// ACL is applied at pipe creation, before connecting/duplicating its client.
/// No AppContainer SID can open the name; only the admitted inherited handle
/// reaches the sandbox. Same-user/admin control is the operator boundary.
async fn broker_pipe() -> Result<(NamedPipeServer, File), LaunchError> {
    let sid = current_sid()?;
    let sddl: Vec<u16> = format!("O:{sid}D:P(A;;GA;;;{sid})(A;;GA;;;SY)(A;;GA;;;BA)")
        .encode_utf16()
        .chain(Some(0))
        .collect();
    let mut descriptor = null_mut();
    if unsafe {
        ConvertStringSecurityDescriptorToSecurityDescriptorW(
            sddl.as_ptr(),
            SDDL_REVISION_1,
            &mut descriptor,
            null_mut(),
        )
    } == 0
    {
        return Err(LaunchError::Files);
    }
    let mut sa = SECURITY_ATTRIBUTES {
        nLength: size_of::<SECURITY_ATTRIBUTES>() as u32,
        lpSecurityDescriptor: descriptor,
        bInheritHandle: 0,
    };
    let name = format!(r"\\.\pipe\memory-pi-work-{}", Uuid::new_v4());
    let result = unsafe {
        ServerOptions::new()
            .first_pipe_instance(true)
            .reject_remote_clients(true)
            .max_instances(1)
            .create_with_security_attributes_raw(
                &name,
                (&mut sa as *mut SECURITY_ATTRIBUTES).cast::<c_void>(),
            )
    };
    unsafe {
        LocalFree(descriptor);
    }
    let server = result.map_err(|_| LaunchError::Files)?;
    let client = File::options()
        .read(true)
        .write(true)
        .open(name)
        .map_err(|_| LaunchError::Files)?;
    timeout(Duration::from_secs(2), server.connect())
        .await
        .map_err(|_| LaunchError::Files)?
        .map_err(|_| LaunchError::Files)?;
    Ok((server, client))
}

async fn serve_one(mut pipe: NamedPipeServer, router: axum::Router) -> Result<(), LaunchError> {
    let length = pipe.read_u32().await.map_err(|_| LaunchError::Child)? as usize;
    if length == 0 || length > 4 * 1024 * 1024 {
        return Err(LaunchError::Child);
    }
    let mut bytes = vec![0; length];
    pipe.read_exact(&mut bytes)
        .await
        .map_err(|_| LaunchError::Child)?;
    let payload: BrokerRequest = serde_json::from_slice(&bytes).map_err(|_| LaunchError::Child)?;
    if payload.schema_version != 1
        || payload.method != "POST"
        || payload.path != "/v1/responses"
        || payload.body.len() > 512 * 1024
        || payload.headers.len() > 100
        || payload
            .headers
            .iter()
            .any(|(key, value)| key.len() > 128 || value.len() > 4096)
    {
        return Err(LaunchError::Child);
    }
    let mut request = Request::builder().method("POST").uri("/v1/responses");
    for (key, value) in payload.headers {
        request = request.header(key, value);
    }
    let request = request
        .body(Body::from(payload.body))
        .map_err(|_| LaunchError::Child)?;
    let response = router
        .oneshot(request)
        .await
        .map_err(|_| LaunchError::Child)?;
    let status = response.status().as_u16();
    let headers = response
        .headers()
        .iter()
        .map(|(key, value)| {
            Ok((
                key.to_string(),
                value.to_str().map_err(|_| LaunchError::Child)?.to_owned(),
            ))
        })
        .collect::<Result<Vec<_>, LaunchError>>()?;
    let body = String::from_utf8(
        to_bytes(response.into_body(), 256 * 1024)
            .await
            .map_err(|_| LaunchError::Child)?
            .to_vec(),
    )
    .map_err(|_| LaunchError::Child)?;
    let bytes = serde_json::to_vec(&BrokerResponse {
        schema_version: 1,
        status,
        headers,
        body,
    })
    .map_err(|_| LaunchError::Child)?;
    if bytes.len() > 512 * 1024 {
        return Err(LaunchError::Child);
    }
    pipe.write_u32(bytes.len() as u32)
        .await
        .map_err(|_| LaunchError::Child)?;
    pipe.write_all(&bytes)
        .await
        .map_err(|_| LaunchError::Child)?;
    pipe.flush().await.map_err(|_| LaunchError::Child)
}

/// Native rehearsal diagnostics are deliberately absent from production builds.
/// Only fixed phase names and typed enum metadata can enter this protected file;
/// never child stdout/stderr, prompts, bearer tokens or provider responses.
#[cfg(test)]
fn record_failed_child_report(
    run_dir: &std::path::Path,
    report: &ChildReport,
) -> Result<(), LaunchError> {
    use std::io::Write;
    let bytes = serde_json::to_vec(&serde_json::json!({
        "schema_version": 1,
        "phase": "driver_report_rejected",
        "process": report.process,
        "failure": report.failure,
    }))
    .map_err(|_| LaunchError::Files)?;
    let mut file = mem_platform_files::windows::create_private_file(
        &run_dir.join("rehearsal-driver-report.json"),
    )
    .map_err(|_| LaunchError::Files)?;
    file.write_all(&bytes).map_err(|_| LaunchError::Files)?;
    file.sync_all().map_err(|_| LaunchError::Files)
}

pub(super) async fn run(
    config: SandboxConfig,
    request: Arc<PreparedRequest>,
    job: Arc<dyn BrokerJob>,
    control: RunControl,
) -> Result<[u8; 32], LaunchError> {
    if control.stopped() {
        return Err(LaunchError::Stopped);
    }
    let pi_hash = config
        .windows_artifacts
        .as_ref()
        .ok_or(LaunchError::Artifact)?
        .validate()
        .map_err(|_| LaunchError::Artifact)?;
    let driver_hash = private_files::digest(&config.driver_sha256)?;
    let runtime = config.private_runtime.clone();
    // No automatic deletion if cleanup fails; this private operation directory
    // is the recovery journal/evidence location and contains no provider key.
    let run_dir = runtime.join(format!("pi-{}", Uuid::new_v4()));
    let code = run_dir.join("code");
    let scratch = run_dir.join("scratch");
    let _runtime = private_files::directory(&runtime, true)?;
    create_private_directory(&run_dir).map_err(|_| LaunchError::Files)?;
    create_private_directory(&code).map_err(|_| LaunchError::Files)?;
    create_private_directory(&scratch).map_err(|_| LaunchError::Files)?;
    let pi = code.join("pi.exe");
    let driver = code.join("memory-supervisor-sandbox.exe");
    private_files::copy(
        &config.pi,
        &pi,
        &config
            .windows_artifacts
            .as_ref()
            .ok_or(LaunchError::Artifact)?
            .binary_sha256,
    )?;
    private_files::copy(&config.driver, &driver, &config.driver_sha256)?;
    let (stdin_read, stdin_write) = pipe()?;
    let (stdout_read, stdout_write) = pipe()?;
    let (stderr_read, stderr_write) = pipe()?;
    let (broker_server, broker_client) = broker_pipe().await?;
    let job = Arc::new(ControlledJob {
        inner: job,
        control: control.clone(),
    });
    let (broker, token) = PiBroker::new(Arc::clone(&request), job, Duration::from_secs(120))
        .map_err(|_| LaunchError::Unavailable)?;
    let admission = PiAdmission::new(&request);
    let input = serde_json::to_vec(&ChildInput {
        system: admission.system_prompt().into(),
        prompt: admission.prompt().into(),
        broker_token: token.bearer_for_child().into(),
    })
    .map_err(|_| LaunchError::Files)?;
    let mut child = mem_platform::windows_process::spawn(ProcessSpec {
        executable: CodeFile {
            path: driver,
            sha256: driver_hash,
        },
        runtime_files: vec![CodeFile {
            path: pi,
            sha256: pi_hash,
        }],
        scratch,
        arguments: vec![
            "--windows-pi-sha256".into(),
            config
                .windows_artifacts
                .as_ref()
                .ok_or(LaunchError::Artifact)?
                .binary_sha256
                .clone()
                .into(),
        ],
        stdin: stdin_read.as_handle(),
        stdout: stdout_write.as_handle(),
        stderr: stderr_write.as_handle(),
        broker: broker_client.as_handle(),
    })
    .map_err(|_| LaunchError::Unavailable)?;
    drop((stdin_read, stdout_write, stderr_write, broker_client));
    let ingress = tokio::spawn(serve_one(broker_server, broker.router()));
    let mut stdin = tokio::fs::File::from_std(stdin_write);
    let mut stdout = tokio::fs::File::from_std(stdout_read).take(8193);
    let mut stderr = tokio::fs::File::from_std(stderr_read).take(8193);
    let operation = async {
        stdin
            .write_all(&input)
            .await
            .map_err(|_| LaunchError::Child)?;
        drop(stdin);
        let mut out = Vec::new();
        let mut err = Vec::new();
        let (a, b) = tokio::join!(stdout.read_to_end(&mut out), stderr.read_to_end(&mut err));
        a.map_err(|_| LaunchError::Child)?;
        b.map_err(|_| LaunchError::Child)?;
        if out.len() > 8192 || err.len() > 8192 {
            return Err(LaunchError::Child);
        }
        let report: ChildReport = serde_json::from_slice(&out).map_err(|_| LaunchError::Child)?;
        if report.failure.is_some() || report.process != ProcessDisposition::Reaped {
            #[cfg(test)]
            if record_failed_child_report(&run_dir, &report).is_err() {
                eprintln!("protected child-report metadata unavailable; retaining original Child failure");
            }
            return Err(LaunchError::Child);
        }
        report.proposal_sha256.ok_or(LaunchError::Child)
    };
    let result = tokio::select! {
        result=timeout(Duration::from_secs(125),operation)=>result.unwrap_or(Err(LaunchError::Deadline)),
        _=control.cancelled()=>Err(LaunchError::Stopped),
    };
    ingress.abort();
    let _ = ingress.await;
    if result.is_err() {
        child.terminate().map_err(|_| LaunchError::Cleanup)?;
    }
    let until = Instant::now() + Duration::from_secs(5);
    let exit = loop {
        match child.try_wait().map_err(|_| LaunchError::Cleanup)? {
            Some(exit) => break exit,
            None if Instant::now() < until => tokio::time::sleep(Duration::from_millis(20)).await,
            None => {
                child.terminate().map_err(|_| LaunchError::Cleanup)?;
                return Err(LaunchError::Cleanup);
            }
        }
    };
    // try_wait restores all grants, validates the code and deletes the profile
    // only after the complete job (driver AND Pi) is empty.
    if exit != 0 {
        return Err(LaunchError::Child);
    }
    // The sandbox guard deliberately pins its code and directories without
    // delete sharing. Cleanup has been verified above; release those handles
    // before removing this completed operation's private directory.
    drop(child);
    if result.is_ok() {
        std::fs::remove_dir_all(&run_dir).map_err(|_| LaunchError::Cleanup)?;
    }
    result
}

#[cfg(test)]
mod tests {
    use super::*;
    use axum::{Router, routing::post};
    use serde_json::json;
    use std::sync::atomic::{AtomicUsize, Ordering};

    #[tokio::test]
    async fn inherited_broker_preserves_exact_request_and_response_bytes() {
        let (server, client) = broker_pipe().await.expect("protected connected pipe");
        let router = Router::new().route(
            "/v1/responses",
            post(|request: Request<Body>| async move {
                assert_eq!(
                    request.headers()["authorization"],
                    "Bearer synthetic-one-work"
                );
                assert_eq!(
                    &to_bytes(request.into_body(), 4096).await.expect("body")[..],
                    br#"{"text":"synthetic \\ path", "value":1}"#
                );
                axum::http::Response::builder()
                    .status(200)
                    .header("content-type", "text/event-stream")
                    .body(Body::from("data: synthetic\n\n"))
                    .expect("fake response")
            }),
        );
        let serving = tokio::spawn(serve_one(server, router));
        let mut client = tokio::fs::File::from_std(client);
        let request = json!({"schema_version":1,"method":"POST","path":"/v1/responses","headers":[["authorization","Bearer synthetic-one-work"],["content-type","application/json"]],"body":r#"{"text":"synthetic \\ path", "value":1}"#});
        let bytes = serde_json::to_vec(&request).expect("request");
        client.write_u32(bytes.len() as u32).await.expect("length");
        client.write_all(&bytes).await.expect("bytes");
        let length = client.read_u32().await.expect("reply length");
        assert!(length <= 512 * 1024);
        let mut bytes = vec![0; length as usize];
        client.read_exact(&mut bytes).await.expect("reply bytes");
        let reply: serde_json::Value = serde_json::from_slice(&bytes).expect("reply");
        assert_eq!(reply["status"], 200);
        assert_eq!(reply["body"], "data: synthetic\n\n");
        serving.await.expect("server joined").expect("one exchange");
    }

    #[tokio::test]
    async fn changed_broker_address_never_reaches_job() {
        let count = Arc::new(AtomicUsize::new(0));
        let observed = Arc::clone(&count);
        let router = Router::new().route(
            "/v1/responses",
            post(move || {
                let count = Arc::clone(&observed);
                async move {
                    count.fetch_add(1, Ordering::SeqCst);
                    "not invoked"
                }
            }),
        );
        let (server, client) = broker_pipe().await.expect("private pipe");
        let serving = tokio::spawn(serve_one(server, router));
        let mut client = tokio::fs::File::from_std(client);
        let bytes = serde_json::to_vec(&json!({"schema_version":1,"method":"POST","path":"https://other.invalid/v1/responses","headers":[],"body":"{}"})).expect("request");
        client.write_u32(bytes.len() as u32).await.expect("length");
        client.write_all(&bytes).await.expect("bytes");
        assert_eq!(
            serving.await.expect("server joined"),
            Err(LaunchError::Child)
        );
        assert_eq!(count.load(Ordering::SeqCst), 0);
    }

    #[tokio::test]
    async fn oversized_broker_prefix_rejected_before_body_allocation() {
        let (server, client) = broker_pipe().await.expect("private pipe");
        let serving = tokio::spawn(serve_one(server, Router::new()));
        let mut client = tokio::fs::File::from_std(client);
        client.write_u32(4 * 1024 * 1024 + 1).await.expect("prefix");
        assert_eq!(
            serving.await.expect("server joined"),
            Err(LaunchError::Child)
        );
    }
}
