// SPDX-License-Identifier: AGPL-3.0-or-later
use crate::{Result, RuntimeError};
use tokio::io::{AsyncRead, AsyncReadExt, AsyncWrite, AsyncWriteExt};
pub(super) const MAX_FRAME: usize = codifix_atproto_verify::MAX_CAR_BYTES + 64 * 1024;
pub(super) const READY: &[u8] = b"memory-pds-stdio-v1";

pub(super) async fn read_frame(reader: &mut (impl AsyncRead + Unpin)) -> Result<Vec<u8>> {
    let length = reader
        .read_u32()
        .await
        .map_err(|_| RuntimeError::Unavailable)? as usize;
    if length == 0 || length > MAX_FRAME {
        return Err(RuntimeError::InvalidResponse);
    }
    let mut bytes = vec![0; length];
    reader
        .read_exact(&mut bytes)
        .await
        .map_err(|_| RuntimeError::Unavailable)?;
    Ok(bytes)
}
pub(super) async fn write_frame(
    writer: &mut (impl AsyncWrite + Unpin),
    bytes: &[u8],
) -> Result<()> {
    if bytes.is_empty() || bytes.len() > MAX_FRAME {
        return Err(RuntimeError::InvalidResponse);
    }
    writer
        .write_u32(bytes.len() as u32)
        .await
        .map_err(|_| RuntimeError::Unavailable)?;
    writer
        .write_all(bytes)
        .await
        .map_err(|_| RuntimeError::Unavailable)?;
    writer.flush().await.map_err(|_| RuntimeError::Unavailable)
}

/// Gateway requests use bounded, in-memory HTTP/1 bodies. Reject chunking and
/// ambiguous lengths instead of introducing a second HTTP normalization layer.
pub(super) async fn read_request(reader: &mut (impl AsyncRead + Unpin)) -> Result<Vec<u8>> {
    let mut bytes = Vec::new();
    while !bytes.ends_with(b"\r\n\r\n") {
        if bytes.len() >= 16 * 1024 {
            return Err(RuntimeError::InvalidResponse);
        }
        bytes.push(
            reader
                .read_u8()
                .await
                .map_err(|_| RuntimeError::Unavailable)?,
        );
    }
    let head = std::str::from_utf8(&bytes).map_err(|_| RuntimeError::InvalidResponse)?;
    let mut lines = head.split("\r\n");
    let request = lines.next().ok_or(RuntimeError::InvalidResponse)?;
    if !(request.starts_with("GET /xrpc/") || request.starts_with("POST /xrpc/"))
        || !request.ends_with(" HTTP/1.1")
    {
        return Err(RuntimeError::InvalidResponse);
    }
    let mut content_length = None;
    for line in lines.filter(|line| !line.is_empty()) {
        let (key, value) = line.split_once(':').ok_or(RuntimeError::InvalidResponse)?;
        if key.eq_ignore_ascii_case("transfer-encoding") {
            return Err(RuntimeError::InvalidResponse);
        }
        if key.eq_ignore_ascii_case("content-length") {
            if content_length.is_some() {
                return Err(RuntimeError::InvalidResponse);
            }
            let value = value.trim();
            if value.is_empty() || !value.bytes().all(|b| b.is_ascii_digit()) {
                return Err(RuntimeError::InvalidResponse);
            }
            content_length = Some(
                value
                    .parse::<usize>()
                    .map_err(|_| RuntimeError::InvalidResponse)?,
            );
        }
    }
    let size = content_length.unwrap_or(0);
    if size > codifix_atproto_verify::MAX_CAR_BYTES {
        return Err(RuntimeError::InvalidResponse);
    }
    let offset = bytes.len();
    bytes.resize(offset + size, 0);
    reader
        .read_exact(&mut bytes[offset..])
        .await
        .map_err(|_| RuntimeError::Unavailable)?;
    Ok(bytes)
}

#[cfg(test)]
mod tests {
    use super::*;
    #[tokio::test]
    async fn bounded_framing_and_opaque_http_body() {
        let request = b"POST /xrpc/fixture HTTP/1.1\r\nHost: fixture\r\nContent-Length: 4\r\n\r\n\x00\xff\x80\x01";
        assert_eq!(
            read_request(&mut request.as_slice())
                .await
                .expect("bounded fixture"),
            request
        );
        let mut framed = Vec::new();
        write_frame(&mut framed, request)
            .await
            .expect("frame fixture");
        assert_eq!(
            read_frame(&mut framed.as_slice())
                .await
                .expect("frame fixture"),
            request
        );
        assert_eq!(READY, b"memory-pds-stdio-v1");
    }
    #[tokio::test]
    async fn ambiguous_chunked_and_unbounded_messages_are_rejected() {
        for mut bytes in [
            b"POST /xrpc/fixture HTTP/1.1\r\nContent-Length: 0\r\nContent-Length: 0\r\n\r\n"
                .as_slice(),
            b"POST /xrpc/fixture HTTP/1.1\r\nTransfer-Encoding: chunked\r\n\r\n".as_slice(),
            b"POST /xrpc/fixture HTTP/1.1\r\nContent-Length: 9999999999\r\n\r\n".as_slice(),
            b"GET /outside HTTP/1.1\r\n\r\n".as_slice(),
        ] {
            assert!(read_request(&mut bytes).await.is_err());
        }
        assert!(read_frame(&mut [255u8; 4].as_slice()).await.is_err());
    }
}
