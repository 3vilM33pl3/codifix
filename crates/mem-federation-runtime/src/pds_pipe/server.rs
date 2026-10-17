// SPDX-License-Identifier: AGPL-3.0-or-later
//! ACLs exist before accepting the first client. The child has private stdio,
//! never a PDS TCP listener. One request executes at a time; ambiguous child
//! failure stops the broker rather than resending an immutable mutation.
use super::{PdsNamedPipe, environment_bytes, native, wire};
use crate::{Result, RuntimeError};
use serde::{Deserialize, Serialize};
use sha2::{Digest, Sha256};
use std::{
    mem::size_of,
    os::windows::io::{FromRawHandle, OwnedHandle},
    path::{Path, PathBuf},
    process::Stdio,
    ptr::null_mut,
    time::Duration,
};
use tokio::{
    io::AsyncWriteExt,
    net::windows::named_pipe::{NamedPipeServer, ServerOptions},
    process::Command,
};
use windows_sys::Win32::{
    Foundation::LocalFree,
    Security::{
        Authorization::{ConvertStringSecurityDescriptorToSecurityDescriptorW, SDDL_REVISION_1},
        SECURITY_ATTRIBUTES,
    },
    System::Threading::{GetCurrentProcessId, OpenProcess, PROCESS_QUERY_LIMITED_INFORMATION},
};

#[derive(Deserialize)]
#[serde(deny_unknown_fields)]
struct Config {
    schema_version: u32,
    pipe: PdsNamedPipe,
    node: PathBuf,
    node_sha256: String,
    entry: PathBuf,
    entry_sha256: String,
    helper_sha256: String,
    #[serde(default)]
    environment: Option<EnvironmentFile>,
}

#[derive(Deserialize)]
#[serde(deny_unknown_fields)]
struct EnvironmentFile {
    path: PathBuf,
    sha256: String,
}

fn read_config(path: &Path) -> Result<Config> {
    let config: Config = serde_json::from_slice(&crate::private_files::read(path, 64 * 1024)?)
        .map_err(|_| RuntimeError::Configuration)?;
    if config.schema_version != 1 {
        return Err(RuntimeError::Configuration);
    }
    config.pipe.validate()?;
    Ok(config)
}

#[derive(Serialize)]
pub struct PdsPipeStatus {
    schema_version: u32,
    pub server_sid: String,
    client_sid: String,
    pipe_name: String,
    readiness_scope: &'static str,
    observed_at: String,
}

/// Administrator-only service-aware read. No environment bytes, bearer or PDS
/// repository contents enter this unauthenticated public-XRPC health probe.
pub async fn check_status(path: &Path, service_name: &str) -> Result<PdsPipeStatus> {
    use mem_platform_files::windows::{ServiceInput, ServiceReader};
    let reader =
        ServiceReader::for_named_service(service_name).map_err(|_| RuntimeError::Unauthorized)?;
    let bytes = ServiceInput::open(path, 64 * 1024, &reader)
        .and_then(|input| input.read(64 * 1024))
        .map_err(|_| RuntimeError::Configuration)?;
    let config: Config = serde_json::from_slice(&bytes).map_err(|_| RuntimeError::Configuration)?;
    if config.schema_version != 1 {
        return Err(RuntimeError::Configuration);
    }
    config.pipe.validate()?;
    let request = reqwest::Client::new()
        .get("http://memory-pds.invalid/xrpc/com.atproto.server.describeServer")
        .build()
        .map_err(|_| RuntimeError::Configuration)?;
    let response = native::send(&config.pipe, request).await?;
    if !response.status().is_success() {
        return Err(RuntimeError::Unavailable);
    }
    let bytes = response
        .bytes()
        .await
        .map_err(|_| RuntimeError::Unavailable)?;
    if bytes.len() > 64 * 1024 {
        return Err(RuntimeError::InvalidResponse);
    }
    let value: serde_json::Value =
        serde_json::from_slice(&bytes).map_err(|_| RuntimeError::InvalidResponse)?;
    if !value.get("availableUserDomains").is_some_and(|domains| {
        domains.as_array().is_some_and(|domains| {
            domains.len() <= 100
                && domains
                    .iter()
                    .all(|domain| domain.as_str().is_some_and(|domain| domain.len() <= 253))
        })
    }) {
        return Err(RuntimeError::InvalidResponse);
    }
    Ok(PdsPipeStatus {
        schema_version: 1,
        server_sid: config.pipe.server_sid,
        client_sid: config.pipe.client_sid,
        pipe_name: config.pipe.name,
        readiness_scope: "verified_pipe_peer_and_public_xrpc_only",
        observed_at: chrono::Utc::now().to_rfc3339(),
    })
}

fn verify(path: &Path, expected: &str, limit: u64) -> Result<()> {
    if !path.is_absolute() || expected.len() != 64 {
        return Err(RuntimeError::Configuration);
    }
    let bytes = crate::private_files::read(path, limit)?;
    if format!("{:x}", Sha256::digest(bytes)) != expected {
        return Err(RuntimeError::Configuration);
    }
    Ok(())
}

fn create(policy: &PdsNamedPipe, first: bool) -> Result<NamedPipeServer> {
    let sddl: Vec<u16> = format!(
        "O:{}D:P(A;;GA;;;{})(A;;0x0012008b;;;{})(A;;GA;;;SY)(A;;GA;;;BA)",
        policy.server_sid, policy.server_sid, policy.client_sid
    )
    .encode_utf16()
    .chain(Some(0))
    .collect();
    let mut descriptor = null_mut();
    // SAFETY: SIDs were parsed before interpolation, and output is Windows-owned.
    if unsafe {
        ConvertStringSecurityDescriptorToSecurityDescriptorW(
            sddl.as_ptr(),
            SDDL_REVISION_1,
            &mut descriptor,
            null_mut(),
        )
    } == 0
    {
        return Err(RuntimeError::Configuration);
    }
    let mut attrs = SECURITY_ATTRIBUTES {
        nLength: size_of::<SECURITY_ATTRIBUTES>() as u32,
        lpSecurityDescriptor: descriptor,
        bInheritHandle: 0,
    };
    // SAFETY: descriptor/attributes remain live until CreateNamedPipe returns;
    // bInheritHandle=0 prevents leaking the listener into the Node child.
    let server = unsafe {
        ServerOptions::new()
            .first_pipe_instance(first)
            .reject_remote_clients(true)
            .max_instances(2)
            .create_with_security_attributes_raw(
                &policy.name,
                (&mut attrs as *mut SECURITY_ATTRIBUTES).cast(),
            )
    };
    unsafe {
        LocalFree(descriptor);
    }
    server.map_err(|_| RuntimeError::Unavailable)
}

pub async fn serve(path: &Path) -> Result<()> {
    let config = read_config(path)?;
    let raw = unsafe { OpenProcess(PROCESS_QUERY_LIMITED_INFORMATION, 0, GetCurrentProcessId()) };
    if raw.is_null() {
        return Err(RuntimeError::Configuration);
    }
    let process = unsafe { OwnedHandle::from_raw_handle(raw) };
    if native::token_sid(&process)? != config.pipe.server_sid {
        return Err(RuntimeError::Configuration);
    }
    verify(&config.node, &config.node_sha256, 128 * 1024 * 1024)?;
    verify(&config.entry, &config.entry_sha256, 64 * 1024)?;
    let parent = config.entry.parent().ok_or(RuntimeError::Configuration)?;
    verify(
        &parent.join("http-stdio.mjs"),
        &config.helper_sha256,
        64 * 1024,
    )?;
    // The installer must pin the complete upstream dependency tree as well.
    // Package version and Node version are checked before the child's READY.
    let mut listener = create(&config.pipe, true)?;
    let mut command = Command::new(&config.node);
    command
        .arg(&config.entry)
        .current_dir(parent)
        .env_clear()
        .kill_on_drop(true)
        .stdin(Stdio::piped())
        .stdout(Stdio::piped())
        .stderr(Stdio::piped());
    for (name, value) in std::env::vars_os() {
        let key = name.to_string_lossy().to_ascii_uppercase();
        if (config.environment.is_none() && key.starts_with("PDS_"))
            || matches!(
                key.as_str(),
                "PATH" | "SYSTEMROOT" | "TEMP" | "TMP" | "NODE_EXTRA_CA_CERTS" | "LOG_LEVEL"
            )
        {
            command.env(name, value);
        }
    }
    if let Some(environment) = &config.environment {
        let bytes = crate::private_files::read(&environment.path, 64 * 1024)?;
        for (name, value) in environment_bytes(&bytes, &environment.sha256)? {
            command.env(name, value);
        }
    }
    let mut child = command.spawn().map_err(|_| RuntimeError::Unavailable)?;
    let mut stdin = child.stdin.take().ok_or(RuntimeError::Unavailable)?;
    let mut stdout = child.stdout.take().ok_or(RuntimeError::Unavailable)?;
    let mut stderr = child.stderr.take().ok_or(RuntimeError::Unavailable)?;
    let drain = tokio::spawn(async move {
        let _ = tokio::io::copy(&mut stderr, &mut tokio::io::sink()).await;
    });
    let outcome = async {
        let ready = tokio::time::timeout(Duration::from_secs(120), wire::read_frame(&mut stdout)).await.map_err(|_| RuntimeError::Unavailable)??;
        if ready != wire::READY { return Err(RuntimeError::InvalidResponse); }
        eprintln!("native private PDS pipe ready");
        loop {
            tokio::select! {
                signal = tokio::signal::ctrl_c() => { signal.map_err(|_| RuntimeError::Unavailable)?; return Ok(()); }
                _ = child.wait() => return Err(RuntimeError::Unavailable),
                connected = listener.connect() => connected.map_err(|_| RuntimeError::Unavailable)?,
            }
            // Keep a protected listening instance available during execution.
            let next = create(&config.pipe, false)?;
            let mut connection = std::mem::replace(&mut listener, next);
            tokio::time::timeout(Duration::from_secs(25), async {
                let request = wire::read_request(&mut connection).await?;
                wire::write_frame(&mut stdin, &request).await?;
                let response = wire::read_frame(&mut stdout).await?;
                connection.write_all(&response).await.map_err(|_| RuntimeError::Unavailable)?;
                // Keep the server handle alive until its client consumes the
                // response and closes; dropping unread pipe bytes loses them.
                let mut byte = [0u8; 1];
                let _ = tokio::io::AsyncReadExt::read(&mut connection, &mut byte).await;
                Ok::<(), RuntimeError>(())
            }).await.map_err(|_| RuntimeError::Unavailable)??;
        }
    }.await;
    drop(stdin);
    if tokio::time::timeout(Duration::from_secs(5), child.wait())
        .await
        .is_err()
    {
        let _ = child.start_kill();
        let _ = child.wait().await;
    }
    drain.abort();
    outcome
}
