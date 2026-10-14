// SPDX-License-Identifier: AGPL-3.0-or-later
//! Exact local subscription reads; no remote/model transport or raw error bodies.
use super::ApiClient;
use crate::commands::output::service_url;
use anyhow::{Result, ensure};
#[cfg(any(feature = "tui", test))]
use mem_record::LaptopCacheSearchResponse;
use mem_record::{
    LaptopCacheHistoryResponse, LaptopCacheList, LaptopCacheProject, LaptopCacheSearchRequest,
};
use reqwest::{Client, header::CACHE_CONTROL};
use uuid::Uuid;

pub(crate) fn cache_scope_matches(a: &LaptopCacheProject, b: &LaptopCacheProject) -> bool {
    a.subscription_id == b.subscription_id
        && a.device_id == b.device_id
        && a.company_id == b.company_id
        && a.area == b.area
        && a.owner_instance_id == b.owner_instance_id
        && a.project_id == b.project_id
        && a.space_id == b.space_id
}
fn valid_project(p: &LaptopCacheProject) -> bool {
    p.read_only && matches!(p.area.as_str(), "workshop" | "homelab" | "general")
}
#[cfg(any(feature = "tui", test))]
fn valid_question(q: &str, limit: u32) -> bool {
    !q.trim().is_empty()
        && q.len() <= 4096
        && (1..=32).contains(&limit)
        && !q.chars().any(|c| c.is_control() && !c.is_whitespace())
}
impl ApiClient {
    async fn cache_operation<T: serde::de::DeserializeOwned>(
        &self,
        path: &str,
        request: Option<&LaptopCacheSearchRequest>,
    ) -> Result<T> {
        ensure!(
            self.config
                .service
                .bind_addr
                .parse::<std::net::SocketAddr>()
                .is_ok_and(|a| a.ip().is_loopback()),
            "Synchronized cache requires a literal local loopback service"
        );
        let client = Client::builder()
            .no_proxy()
            .redirect(reqwest::redirect::Policy::none())
            .timeout(std::time::Duration::from_secs(30))
            .build()?;
        let headers = self.local_session_headers(&client, true).await?;
        let builder = if let Some(request) = request {
            client.post(service_url(&self.config, path)).json(request)
        } else {
            client.get(service_url(&self.config, path))
        };
        let mut response = builder
            .headers(headers)
            .header(CACHE_CONTROL, "no-store")
            .send()
            .await
            .map_err(|_| {
                anyhow::anyhow!("Local synchronized cache unavailable; no remote search attempted")
            })?;
        ensure!(
            response.status().is_success(),
            "{}",
            match response.status().as_u16() {
                401 => "Signed out: authenticated local owner required",
                403 => "Forbidden: synchronized cache requires the local loopback owner",
                404 => "Synchronized cache is not enabled or subscription/build unavailable",
                400 => "Invalid local question or result bounds",
                _ => "Local synchronized cache unavailable; response details suppressed",
            }
        );
        let mut bytes = Vec::new();
        while let Some(chunk) = response
            .chunk()
            .await
            .map_err(|_| anyhow::anyhow!("Unreadable local cache response"))?
        {
            ensure!(
                bytes.len() + chunk.len() <= 256 * 1024,
                "Oversized local cache response"
            );
            bytes.extend_from_slice(&chunk);
        }
        serde_json::from_slice(&bytes).map_err(|_| anyhow::anyhow!("Invalid local cache response"))
    }
    pub(crate) async fn laptop_cache(&self) -> Result<LaptopCacheList> {
        let value: LaptopCacheList = self.cache_operation("/v1/onboarding/cache", None).await?;
        let mut ids = std::collections::HashSet::new();
        ensure!(
            value.schema_version == 1
                && value
                    .projects
                    .iter()
                    .all(|p| valid_project(p) && ids.insert(p.subscription_id)),
            "Invalid local cache inventory"
        );
        Ok(value)
    }
    #[cfg(any(feature = "tui", test))]
    pub(crate) async fn laptop_cache_search(
        &self,
        project: &LaptopCacheProject,
        question: String,
        limit: u32,
    ) -> Result<LaptopCacheSearchResponse> {
        ensure!(
            valid_project(project) && valid_question(&question, limit),
            "Question requires 1–4096 UTF-8 bytes and result limit 1–32"
        );
        let request = LaptopCacheSearchRequest { question, limit };
        let value: LaptopCacheSearchResponse = self
            .cache_operation(
                &format!("/v1/onboarding/cache/{}/search", project.subscription_id),
                Some(&request),
            )
            .await?;
        let hex = |s: &str| s.len() == 64 && s.bytes().all(|c| c.is_ascii_hexdigit());
        ensure!(
            value.schema_version == 1
                && valid_project(&value.project)
                && cache_scope_matches(project, &value.project)
                && value.hits.len() <= limit as usize
                && value.hits.iter().all(|h| h.read_only
                    && hex(&h.digest)
                    && h.evidence.iter().all(|e| hex(&e.digest))),
            "Local cache result scope/provenance mismatch"
        );
        Ok(value)
    }
    pub(crate) async fn laptop_cache_history(
        &self,
        project: &LaptopCacheProject,
        canonical: Uuid,
        after_version: u32,
        limit: u32,
    ) -> Result<LaptopCacheHistoryResponse> {
        ensure!(
            valid_project(project) && !canonical.is_nil() && (1..=32).contains(&limit),
            "exact selected cache scope and bounded history required"
        );
        let value: LaptopCacheHistoryResponse = self.cache_operation(&format!("/v1/onboarding/cache/{}/history/{canonical}?after_version={after_version}&limit={limit}", project.subscription_id), None).await?;
        ensure!(
            value.schema_version == 1
                && value.read_only
                && value.canonical_id == canonical
                && cache_scope_matches(project, &value.project)
                && value.versions.len() <= limit as usize
                && value.versions.iter().all(|v| !v.id.is_nil()
                    && v.version > after_version
                    && v.digest.len() == 64
                    && v.digest.bytes().all(|c| c.is_ascii_hexdigit()))
                && value
                    .versions
                    .windows(2)
                    .all(|v| v[0].version < v[1].version)
                && value
                    .next_after_version
                    .is_none_or(|next| value.versions.last().is_some_and(|v| v.version == next)),
            "cache history identity/provenance mismatch"
        );
        Ok(value)
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    #[test]
    fn cache_question_is_bounded_utf8_and_never_accepts_control_input() {
        assert!(valid_question("exact\nquestion", 8));
        for q in [
            " ".to_owned(),
            "é".repeat(2049),
            "hidden\u{1b}command".into(),
        ] {
            assert!(!valid_question(&q, 8));
        }
        assert!(!valid_question("q", 33));
    }
    #[tokio::test]
    async fn cache_never_sends_owner_credential_to_nonloopback() {
        let config = serde_json::from_value(serde_json::json!({"service":{"bind_addr":"10.0.0.1:4040","api_token":"synthetic"},"database":{"url":"postgresql://unused/unused"}})).unwrap();
        let api = ApiClient::new(Client::new(), config);
        assert!(
            api.laptop_cache()
                .await
                .unwrap_err()
                .to_string()
                .contains("loopback")
        );
    }
    async fn serve(
        listener: tokio::net::TcpListener,
        responses: Vec<(u16, String, String)>,
    ) -> Vec<String> {
        use tokio::io::{AsyncReadExt, AsyncWriteExt};
        let mut requests = vec![];
        for (status, headers, body) in responses {
            let (mut socket, _) = listener.accept().await.unwrap();
            let mut bytes = vec![];
            loop {
                let mut buffer = [0; 4096];
                let n = socket.read(&mut buffer).await.unwrap();
                assert!(n > 0);
                bytes.extend_from_slice(&buffer[..n]);
                if let Some(start) = bytes.windows(4).position(|w| w == b"\r\n\r\n") {
                    let head = String::from_utf8_lossy(&bytes[..start]).to_lowercase();
                    let length = head
                        .lines()
                        .find_map(|l| {
                            l.strip_prefix("content-length: ")
                                .and_then(|n| n.parse::<usize>().ok())
                        })
                        .unwrap_or(0);
                    if bytes.len() >= start + 4 + length {
                        break;
                    }
                }
            }
            requests.push(String::from_utf8(bytes).unwrap());
            socket.write_all(format!("HTTP/1.1 {status} Synthetic\r\nContent-Type: application/json\r\nContent-Length: {}\r\nConnection: close\r\n{headers}\r\n{body}", body.len()).as_bytes()).await.unwrap();
        }
        requests
    }
    #[tokio::test]
    async fn cache_local_session_sends_only_exact_subscription_query_and_suppresses_denied_body() {
        let listener = tokio::net::TcpListener::bind("127.0.0.1:0").await.unwrap();
        let address = listener.local_addr().unwrap();
        let id = Uuid::new_v4();
        let p = LaptopCacheProject {
            subscription_id: id,
            device_id: Uuid::new_v4(),
            company_id: Uuid::new_v4(),
            area: "workshop".into(),
            owner_instance_id: Uuid::new_v4(),
            project_id: Uuid::new_v4(),
            space_id: Uuid::new_v4(),
            cursor: 0,
            observed_at: None,
            initialized: true,
            read_only: true,
            uploads: Default::default(),
            sync_error: None,
        };
        let owner = Uuid::new_v5(&Uuid::NAMESPACE_OID, b"memory-layer-local-owner");
        let list = serde_json::json!({"schema_version":1,"area":"workshop","instance_id":Uuid::new_v4(),"projects":[p]}).to_string();
        let result = serde_json::json!({"schema_version":1,"project":p,"hits":[]}).to_string();
        let server = tokio::spawn(serve(listener, vec![
            (200, String::new(), serde_json::json!({"authenticated":true,"mode":"single_user","read_only":true,"principal":{"id":owner,"kind":"internal","display_name":"synthetic","global_role":"admin"}}).to_string()),
            (200, "Set-Cookie: memory_session=synthetic-session; HttpOnly\r\nSet-Cookie: memory_csrf=synthetic-csrf\r\n".into(), "{}".into()),
            (200, String::new(), list), (200, String::new(), result),
            (403, String::new(), "PRIVATE_STORAGE_DETAILS".into()),
        ]));
        let config = serde_json::from_value(serde_json::json!({"service":{"bind_addr":address.to_string(),"api_token":"synthetic-owner"},"database":{"url":"postgresql://unused/unused"}})).unwrap();
        let mut inherited = reqwest::header::HeaderMap::new();
        inherited.insert(
            "x-company-secret",
            "synthetic-not-forwarded".parse().unwrap(),
        );
        let api = ApiClient::new(
            Client::builder()
                .default_headers(inherited)
                .build()
                .unwrap(),
            config,
        );
        api.laptop_cache().await.unwrap();
        api.laptop_cache_search(&p, "Exact question".into(), 8)
            .await
            .unwrap();
        let error = api.laptop_cache().await.unwrap_err().to_string();
        assert!(error.contains("Forbidden"));
        assert!(!error.contains("PRIVATE_STORAGE_DETAILS"));
        let requests = server.await.unwrap();
        assert_eq!(requests.len(), 5);
        assert!(requests[2].starts_with("GET /v1/onboarding/cache "));
        assert!(requests[3].starts_with(&format!("POST /v1/onboarding/cache/{id}/search ")));
        let body: serde_json::Value =
            serde_json::from_str(requests[3].split_once("\r\n\r\n").unwrap().1).unwrap();
        assert_eq!(
            body,
            serde_json::json!({"question":"Exact question","limit":8})
        );
        for request in &requests[2..] {
            assert!(request.contains("memory_session=synthetic-session"));
            assert!(request.contains("x-csrf-token: synthetic-csrf"));
            assert!(!request.contains("x-api-token"));
            assert!(!request.contains("x-company-secret"));
        }
    }
}
