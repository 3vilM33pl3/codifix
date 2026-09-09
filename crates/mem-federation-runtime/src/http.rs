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
}
impl Endpoint {
    pub fn new(origin: &str, disposable_cleartext: bool) -> Result<Self> {
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
        let client = Client::builder()
            .no_proxy()
            .redirect(reqwest::redirect::Policy::none())
            .connect_timeout(Duration::from_secs(5))
            .timeout(Duration::from_secs(20))
            .build()
            .map_err(|_| RuntimeError::Configuration)?;
        Ok(Self { base, client })
    }
    pub(crate) fn url(&self, path: &str) -> Result<Url> {
        self.base
            .join(path)
            .map_err(|_| RuntimeError::Configuration)
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
