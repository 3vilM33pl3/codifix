// SPDX-License-Identifier: AGPL-3.0-or-later
use crate::{Result, RuntimeError};
use reqwest::{Client, Response, Url};
use serde::de::DeserializeOwned;
use std::time::Duration;

/// An administrator-pinned origin, never a URL from a record/DID. Cleartext is
/// restricted to explicit disposable fixtures; production requires HTTPS.
#[derive(Clone)]
pub struct Endpoint {
    pub(crate) base: Url,
    pub(crate) client: Client,
    pipe: Option<crate::PdsNamedPipe>,
}
impl Endpoint {
    pub(crate) fn get(&self, url: Url) -> PdsRequest<'_> {
        PdsRequest {
            endpoint: self,
            builder: self.client.get(url),
        }
    }
    pub(crate) fn post(&self, url: Url) -> PdsRequest<'_> {
        PdsRequest {
            endpoint: self,
            builder: self.client.post(url),
        }
    }
    pub fn origin(&self) -> &str {
        self.base.as_str().trim_end_matches('/')
    }
    pub fn new(origin: &str, disposable_cleartext: bool) -> Result<Self> {
        Self::with_ca(origin, disposable_cleartext, None)
    }
    pub fn with_ca(origin: &str, disposable_cleartext: bool, ca: Option<&[u8]>) -> Result<Self> {
        let base = Url::parse(origin).map_err(|_| RuntimeError::Configuration)?;
        if base.host_str().is_none()
            || !base.username().is_empty()
            || base.password().is_some()
            || base.query().is_some()
            || base.fragment().is_some()
            || base.path() != "/"
            || !(base.scheme() == "https" || (disposable_cleartext && base.scheme() == "http"))
        {
            return Err(RuntimeError::Configuration);
        }
        let mut builder = Client::builder()
            .no_proxy()
            .redirect(reqwest::redirect::Policy::none())
            .connect_timeout(Duration::from_secs(5))
            .timeout(Duration::from_secs(20));
        if let Some(ca) = ca {
            if ca.len() > 16 * 1024 {
                return Err(RuntimeError::Configuration);
            }
            builder = builder.tls_built_in_root_certs(false).add_root_certificate(
                reqwest::Certificate::from_pem(ca).map_err(|_| RuntimeError::Configuration)?,
            );
        }
        let client = builder.build().map_err(|_| RuntimeError::Configuration)?;
        Ok(Self {
            base,
            client,
            pipe: None,
        })
    }
    /// Keep the HTTPS origin as the logical repository/credential identity, but
    /// dispatch PDS HTTP bytes exclusively over a protected Windows pipe.
    /// Unsupported hosts fail closed; this never falls back to TCP.
    pub fn with_pds_named_pipe(mut self, pipe: crate::PdsNamedPipe) -> Result<Self> {
        pipe.validate()?;
        if !cfg!(windows) || self.base.scheme() != "https" {
            return Err(RuntimeError::Configuration);
        }
        self.pipe = Some(pipe);
        Ok(self)
    }
    pub(crate) async fn send(&self, builder: reqwest::RequestBuilder) -> Result<Response> {
        let request = builder.build().map_err(|_| RuntimeError::Configuration)?;
        if request.url().origin() != self.base.origin() {
            return Err(RuntimeError::Configuration);
        }
        if let Some(pipe) = &self.pipe {
            #[cfg(windows)]
            return crate::pds_pipe::send(pipe, request).await;
            #[cfg(not(windows))]
            {
                let _ = pipe;
                return Err(RuntimeError::Configuration);
            }
        }
        self.client
            .execute(request)
            .await
            .map_err(|_| RuntimeError::Unavailable)
    }
    pub(crate) fn url(&self, path: &str) -> Result<Url> {
        self.base
            .join(path)
            .map_err(|_| RuntimeError::Configuration)
    }
}
/// PDS-only builder. Other endpoints continue using the existing HTTPS client.
pub(crate) struct PdsRequest<'a> {
    endpoint: &'a Endpoint,
    builder: reqwest::RequestBuilder,
}
impl PdsRequest<'_> {
    pub(crate) fn bearer_auth(mut self, token: &str) -> Self {
        self.builder = self.builder.bearer_auth(token);
        self
    }
    pub(crate) fn query<T: serde::Serialize + ?Sized>(mut self, value: &T) -> Self {
        self.builder = self.builder.query(value);
        self
    }
    pub(crate) fn json<T: serde::Serialize + ?Sized>(mut self, value: &T) -> Self {
        self.builder = self.builder.json(value);
        self
    }
    pub(crate) async fn send(self) -> Result<Response> {
        self.endpoint.send(self.builder).await
    }
}
pub(crate) async fn json<T: DeserializeOwned>(response: Response, limit: usize) -> Result<T> {
    serde_json::from_slice(&bytes(response, limit).await?)
        .map_err(|_| RuntimeError::InvalidResponse)
}
pub(crate) async fn bytes(mut response: Response, limit: usize) -> Result<Vec<u8>> {
    if response.content_length().is_some_and(|n| n > limit as u64) {
        return Err(RuntimeError::InvalidResponse);
    }
    let mut bytes = Vec::new();
    while let Some(chunk) = response
        .chunk()
        .await
        .map_err(|_| RuntimeError::Unavailable)?
    {
        if bytes.len() + chunk.len() > limit {
            return Err(RuntimeError::InvalidResponse);
        }
        bytes.extend_from_slice(&chunk);
    }
    Ok(bytes)
}

#[cfg(test)]
mod tests {
    use super::*;
    #[tokio::test]
    async fn pds_dispatch_preserves_bytes_and_rejects_cross_origin() {
        let listener = tokio::net::TcpListener::bind("127.0.0.1:0")
            .await
            .expect("synthetic test fixture");
        let endpoint = Endpoint::new(
            &format!(
                "http://{}",
                listener.local_addr().expect("synthetic test fixture")
            ),
            true,
        )
        .expect("synthetic test fixture");
        let task = tokio::spawn(async move {
            axum::serve(
                listener,
                axum::Router::new().route(
                    "/xrpc/fixture",
                    axum::routing::get(|| async { vec![0u8, 255, 1, 128] }),
                ),
            )
            .await
            .expect("synthetic test fixture");
        });
        let response = endpoint
            .get(
                endpoint
                    .url("/xrpc/fixture")
                    .expect("synthetic test fixture"),
            )
            .bearer_auth("synthetic-only")
            .send()
            .await
            .expect("synthetic test fixture");
        assert_eq!(
            bytes(response, 4).await.expect("synthetic test fixture"),
            [0, 255, 1, 128]
        );
        assert!(
            endpoint
                .get(Url::parse("https://different.invalid/").expect("synthetic test fixture"))
                .send()
                .await
                .is_err()
        );
        task.abort();
    }
    #[cfg(not(windows))]
    #[test]
    fn named_pipe_never_becomes_tcp_fallback_on_other_hosts() {
        let endpoint =
            Endpoint::new("https://pds.internal", false).expect("synthetic test fixture");
        assert!(
            endpoint
                .with_pds_named_pipe(crate::PdsNamedPipe {
                    name: r"\\.\pipe\memory-pds-test".into(),
                    server_sid: "S-1-5-21-1-2-3-1001".into(),
                    client_sid: "S-1-5-21-1-2-3-1002".into(),
                })
                .is_err()
        );
    }
}
