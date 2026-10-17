// SPDX-License-Identifier: AGPL-3.0-or-later
use super::PdsNamedPipe;
use crate::{Result, RuntimeError};
use http_body_util::{BodyExt, Full};
use hyper::body::Bytes;
use hyper_util::rt::TokioIo;
use std::{
    ffi::c_void,
    mem::size_of,
    os::windows::{
        ffi::OsStrExt,
        io::{AsRawHandle, FromRawHandle, OwnedHandle},
    },
    ptr::{addr_of_mut, null_mut},
    time::Duration,
};
use windows_sys::Win32::{
    Foundation::{INVALID_HANDLE_VALUE, LocalFree},
    Security::{
        ACCESS_ALLOWED_ACE, ACE_HEADER,
        Authorization::{ConvertSidToStringSidW, GetSecurityInfo, SE_FILE_OBJECT},
        DACL_SECURITY_INFORMATION, GetAce, GetTokenInformation, INHERIT_ONLY_ACE,
        OWNER_SECURITY_INFORMATION, TOKEN_QUERY, TOKEN_USER, TokenUser,
    },
    Storage::FileSystem::{
        CreateFileW, FILE_FLAG_OVERLAPPED, FILE_GENERIC_READ, FILE_WRITE_DATA, OPEN_EXISTING,
        SECURITY_IDENTIFICATION, SECURITY_SQOS_PRESENT,
    },
    System::{
        Pipes::GetNamedPipeServerProcessId,
        Threading::{OpenProcess, OpenProcessToken, PROCESS_QUERY_LIMITED_INFORMATION},
    },
};

struct Allocation(*mut c_void);
impl Drop for Allocation {
    fn drop(&mut self) {
        unsafe {
            LocalFree(self.0);
        }
    }
}

fn sid(value: *mut c_void) -> Result<String> {
    if value.is_null() {
        return Err(RuntimeError::Unauthorized);
    }
    let mut text = null_mut();
    // SAFETY: value belongs to a live, Windows-validated security descriptor/token.
    if unsafe { ConvertSidToStringSidW(value, &mut text) } == 0 {
        return Err(RuntimeError::Unauthorized);
    }
    let _allocation = Allocation(text.cast());
    let mut length = 0;
    // SAFETY: ConvertSidToStringSidW returned a NUL-terminated allocation.
    unsafe {
        while *text.add(length) != 0 {
            length += 1;
        }
        String::from_utf16(std::slice::from_raw_parts(text, length))
            .map_err(|_| RuntimeError::Unauthorized)
    }
}

pub(super) fn token_sid(process: &OwnedHandle) -> Result<String> {
    let mut raw = null_mut();
    // SAFETY: process handle lives through call; successful call transfers token.
    if unsafe { OpenProcessToken(process.as_raw_handle(), TOKEN_QUERY, &mut raw) } == 0 {
        return Err(RuntimeError::Unauthorized);
    }
    let token = unsafe { OwnedHandle::from_raw_handle(raw) };
    let mut length = 0;
    unsafe {
        GetTokenInformation(token.as_raw_handle(), TokenUser, null_mut(), 0, &mut length);
    }
    if length < size_of::<TOKEN_USER>() as u32 || length > 65_536 {
        return Err(RuntimeError::Unauthorized);
    }
    let mut buffer = vec![0usize; (length as usize).div_ceil(size_of::<usize>())];
    // SAFETY: aligned buffer meets the Windows-reported allocation requirement.
    if unsafe {
        GetTokenInformation(
            token.as_raw_handle(),
            TokenUser,
            buffer.as_mut_ptr().cast(),
            length,
            &mut length,
        )
    } == 0
    {
        return Err(RuntimeError::Unauthorized);
    }
    sid(unsafe { (*buffer.as_ptr().cast::<TOKEN_USER>()).User.Sid })
}

fn validate_pipe(
    pipe: &tokio::net::windows::named_pipe::NamedPipeClient,
    policy: &PdsNamedPipe,
) -> Result<()> {
    let mut pid = 0;
    if unsafe { GetNamedPipeServerProcessId(pipe.as_raw_handle(), &mut pid) } == 0 || pid == 0 {
        return Err(RuntimeError::Unauthorized);
    }
    let raw = unsafe { OpenProcess(PROCESS_QUERY_LIMITED_INFORMATION, 0, pid) };
    if raw.is_null() {
        return Err(RuntimeError::Unauthorized);
    }
    let process = unsafe { OwnedHandle::from_raw_handle(raw) };
    if token_sid(&process)? != policy.server_sid {
        return Err(RuntimeError::Unauthorized);
    }
    let mut owner = null_mut();
    let mut dacl = null_mut();
    let mut descriptor = null_mut();
    // SAFETY: handle remains connected; returned allocation owns the ACL and SID.
    let status = unsafe {
        GetSecurityInfo(
            pipe.as_raw_handle(),
            SE_FILE_OBJECT,
            OWNER_SECURITY_INFORMATION | DACL_SECURITY_INFORMATION,
            &mut owner,
            null_mut(),
            &mut dacl,
            null_mut(),
            &mut descriptor,
        )
    };
    if status != 0 {
        return Err(RuntimeError::Unauthorized);
    }
    let _allocation = Allocation(descriptor);
    if dacl.is_null() || sid(owner)? != policy.server_sid {
        return Err(RuntimeError::Unauthorized);
    }
    for index in 0..u32::from(unsafe { (*dacl).AceCount }) {
        let mut raw = null_mut();
        if unsafe { GetAce(dacl, index, &mut raw) } == 0 {
            return Err(RuntimeError::Unauthorized);
        }
        let header = unsafe { &*raw.cast::<ACE_HEADER>() };
        if u32::from(header.AceFlags) & INHERIT_ONLY_ACE != 0 || header.AceType == 1 {
            continue;
        }
        if header.AceType != 0 || usize::from(header.AceSize) < size_of::<ACCESS_ALLOWED_ACE>() {
            return Err(RuntimeError::Unauthorized);
        }
        let ace = unsafe { &mut *raw.cast::<ACCESS_ALLOWED_ACE>() };
        let allowed = sid(addr_of_mut!(ace.SidStart).cast())?;
        if ace.Mask != 0
            && allowed != policy.server_sid
            && allowed != policy.client_sid
            && allowed != "S-1-5-18"
            && allowed != "S-1-5-32-544"
        {
            return Err(RuntimeError::Unauthorized);
        }
    }
    // Repeat process identity observation after descriptor inspection.
    let mut after = 0;
    if unsafe { GetNamedPipeServerProcessId(pipe.as_raw_handle(), &mut after) } == 0 || after != pid
    {
        return Err(RuntimeError::Unauthorized);
    }
    Ok(())
}

struct Connection(tokio::task::JoinHandle<()>);
impl Drop for Connection {
    fn drop(&mut self) {
        self.0.abort();
    }
}

pub(crate) async fn send(
    policy: &PdsNamedPipe,
    request: reqwest::Request,
) -> Result<reqwest::Response> {
    tokio::time::timeout(Duration::from_secs(20), async {
        let name: Vec<u16> = std::ffi::OsStr::new(&policy.name)
            .encode_wide()
            .chain(Some(0))
            .collect();
        // GENERIC_WRITE includes FILE_CREATE_PIPE_INSTANCE. Ask only for data
        // write access so the gateway cannot create competing server instances.
        let raw = unsafe {
            CreateFileW(
                name.as_ptr(),
                FILE_GENERIC_READ | FILE_WRITE_DATA,
                0,
                null_mut(),
                OPEN_EXISTING,
                FILE_FLAG_OVERLAPPED | SECURITY_SQOS_PRESENT | SECURITY_IDENTIFICATION,
                null_mut(),
            )
        };
        if raw == INVALID_HANDLE_VALUE {
            return Err(RuntimeError::Unavailable);
        }
        // SAFETY: CreateFile transferred a fresh overlapped handle; Tokio owns
        // and closes it from this point, including registration failure.
        let pipe =
            unsafe { tokio::net::windows::named_pipe::NamedPipeClient::from_raw_handle(raw) }
                .map_err(|_| RuntimeError::Unavailable)?;
        // No authentication bytes are written until peer identity and DACL pass.
        validate_pipe(&pipe, policy)?;
        let mut path = request.url().path().to_owned();
        if let Some(query) = request.url().query() {
            path.push('?');
            path.push_str(query);
        }
        let payload = match request.body() {
            None => Bytes::new(),
            Some(body) => {
                Bytes::copy_from_slice(body.as_bytes().ok_or(RuntimeError::Configuration)?)
            }
        };
        if payload.len() > codifix_atproto_verify::MAX_CAR_BYTES {
            return Err(RuntimeError::Configuration);
        }
        let mut outgoing = hyper::Request::builder()
            .method(request.method())
            .uri(path)
            .body(Full::new(payload))
            .map_err(|_| RuntimeError::Configuration)?;
        *outgoing.headers_mut() = request.headers().clone();
        let authority = request
            .url()
            .as_str()
            .split_once("://")
            .and_then(|(_, tail)| tail.split('/').next())
            .ok_or(RuntimeError::Configuration)?;
        outgoing.headers_mut().insert(
            hyper::header::HOST,
            authority.parse().map_err(|_| RuntimeError::Configuration)?,
        );
        let (mut sender, connection) = hyper::client::conn::http1::handshake(TokioIo::new(pipe))
            .await
            .map_err(|_| RuntimeError::Unavailable)?;
        let _connection = Connection(tokio::spawn(async move {
            let _ = connection.await;
        }));
        let response = sender
            .send_request(outgoing)
            .await
            .map_err(|_| RuntimeError::Unavailable)?;
        let (parts, mut body) = response.into_parts();
        let mut bytes = Vec::new();
        while let Some(frame) = body.frame().await {
            let frame = frame.map_err(|_| RuntimeError::Unavailable)?;
            if let Ok(data) = frame.into_data() {
                if bytes.len().saturating_add(data.len()) > codifix_atproto_verify::MAX_CAR_BYTES {
                    return Err(RuntimeError::InvalidResponse);
                }
                bytes.extend_from_slice(&data);
            }
        }
        Ok(reqwest::Response::from(hyper::Response::from_parts(
            parts,
            reqwest::Body::from(bytes),
        )))
    })
    .await
    .map_err(|_| RuntimeError::Unavailable)?
}

#[cfg(test)]
mod tests {
    use super::*;
    use tokio::io::{AsyncReadExt, AsyncWriteExt};
    use windows_sys::Win32::{
        Security::{
            Authorization::{
                ConvertStringSecurityDescriptorToSecurityDescriptorW, SDDL_REVISION_1,
            },
            SECURITY_ATTRIBUTES,
        },
        System::Threading::GetCurrentProcessId,
    };

    fn fixture(
        broad: bool,
    ) -> (
        PdsNamedPipe,
        tokio::net::windows::named_pipe::NamedPipeServer,
    ) {
        let raw =
            unsafe { OpenProcess(PROCESS_QUERY_LIMITED_INFORMATION, 0, GetCurrentProcessId()) };
        assert!(!raw.is_null());
        let process = unsafe { OwnedHandle::from_raw_handle(raw) };
        let principal = token_sid(&process).expect("synthetic test fixture");
        let policy = PdsNamedPipe {
            name: format!(r"\\.\pipe\memory-pds-test-{}", uuid::Uuid::new_v4()),
            server_sid: principal.clone(),
            client_sid: principal.clone(),
        };
        policy.validate().expect("synthetic test fixture");
        let extra = if broad { "(A;;GR;;;WD)" } else { "" };
        let sddl: Vec<u16> = format!("O:{principal}D:P(A;;GA;;;{principal}){extra}")
            .encode_utf16()
            .chain(Some(0))
            .collect();
        let mut descriptor = null_mut();
        assert_ne!(
            unsafe {
                ConvertStringSecurityDescriptorToSecurityDescriptorW(
                    sddl.as_ptr(),
                    SDDL_REVISION_1,
                    &mut descriptor,
                    null_mut(),
                )
            },
            0
        );
        let _allocation = Allocation(descriptor);
        let mut attrs = SECURITY_ATTRIBUTES {
            nLength: size_of::<SECURITY_ATTRIBUTES>() as u32,
            lpSecurityDescriptor: descriptor,
            bInheritHandle: 0,
        };
        let server = unsafe {
            tokio::net::windows::named_pipe::ServerOptions::new()
                .first_pipe_instance(true)
                .reject_remote_clients(true)
                .create_with_security_attributes_raw(
                    &policy.name,
                    (&mut attrs as *mut SECURITY_ATTRIBUTES).cast(),
                )
        }
        .expect("synthetic test fixture");
        (policy, server)
    }

    #[tokio::test]
    async fn protected_pipe_preserves_http_binary_bytes() {
        let (policy, mut server) = fixture(false);
        let task = tokio::spawn(async move {
            server.connect().await.expect("synthetic test fixture");
            let mut headers = Vec::new();
            let mut byte = [0u8; 1];
            while !headers.ends_with(b"\r\n\r\n") {
                server
                    .read_exact(&mut byte)
                    .await
                    .expect("synthetic test fixture");
                headers.extend_from_slice(&byte);
                assert!(headers.len() < 4096);
            }
            let text = String::from_utf8(headers)
                .expect("synthetic test fixture")
                .to_ascii_lowercase();
            assert!(text.starts_with("get /xrpc/fixture?did=fixture http/1.1"));
            assert!(text.contains("authorization: bearer synthetic-only"));
            server.write_all(b"HTTP/1.1 200 OK\r\nContent-Length: 4\r\nConnection: close\r\n\r\n\x00\xff\x80\x01").await.expect("synthetic test fixture");
            let _ = server.read(&mut byte).await;
        });
        let endpoint = crate::Endpoint::new("https://pds.internal", false)
            .expect("synthetic test fixture")
            .with_pds_named_pipe(policy)
            .expect("synthetic test fixture");
        let response = endpoint
            .get(
                endpoint
                    .url("/xrpc/fixture?did=fixture")
                    .expect("synthetic test fixture"),
            )
            .bearer_auth("synthetic-only")
            .send()
            .await
            .expect("synthetic test fixture");
        assert_eq!(
            crate::http::bytes(response, 4)
                .await
                .expect("synthetic test fixture"),
            [0, 255, 128, 1]
        );
        task.await.expect("synthetic test fixture");
    }

    #[tokio::test]
    async fn broad_pipe_acl_is_rejected_before_authentication_bytes() {
        let (policy, mut server) = fixture(true);
        let task = tokio::spawn(async move {
            server.connect().await.expect("synthetic test fixture");
            let mut bytes = [0u8; 1];
            assert_eq!(server.read(&mut bytes).await.unwrap_or(0), 0);
        });
        let endpoint = crate::Endpoint::new("https://pds.internal", false)
            .expect("synthetic test fixture")
            .with_pds_named_pipe(policy)
            .expect("synthetic test fixture");
        assert!(matches!(
            endpoint
                .get(
                    endpoint
                        .url("/xrpc/fixture")
                        .expect("synthetic test fixture")
                )
                .bearer_auth("synthetic-only")
                .send()
                .await,
            Err(RuntimeError::Unauthorized)
        ));
        tokio::time::timeout(Duration::from_secs(5), task)
            .await
            .expect("synthetic test fixture")
            .expect("synthetic test fixture");
    }
}
