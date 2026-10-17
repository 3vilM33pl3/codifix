// SPDX-License-Identifier: AGPL-3.0-or-later
use super::ApiClient;
use crate::commands::output::service_url;
use anyhow::{Result, bail, ensure};
use codifix_record::{
    SupervisorQuestionList, SupervisorQuestionPrepare, SupervisorQuestionPreview,
    SupervisorQuestionRequest, SupervisorQuestionStatus,
};
use reqwest::{Client, Method, Url, header::CACHE_CONTROL};
use uuid::Uuid;

impl ApiClient {
    async fn question_operation<T: serde::de::DeserializeOwned>(
        &self,
        project: &str,
        suffix: Option<&str>,
        after: Option<Uuid>,
        body: Option<serde_json::Value>,
    ) -> Result<T> {
        let client = Client::builder()
            .no_proxy()
            .redirect(reqwest::redirect::Policy::none())
            .timeout(std::time::Duration::from_secs(30))
            .build()?;
        let mut path = Url::parse("http://localhost/")?;
        {
            let mut segments = path
                .path_segments_mut()
                .map_err(|_| anyhow::anyhow!("Invalid URL"))?;
            segments.extend(["v1", "projects", project, "supervisor", "questions"]);
            if let Some(suffix) = suffix {
                segments.push(suffix);
            }
        }
        let headers = self.local_session_headers(&client, false).await?;
        let mut request = client
            .request(
                if body.is_some() {
                    Method::POST
                } else {
                    Method::GET
                },
                service_url(&self.config, path.path()),
            )
            .headers(headers)
            .header(CACHE_CONTROL, "no-store");
        if let Some(body) = body {
            request = request.json(&body);
        }
        if let Some(after) = after {
            request = request.query(&[("after", after.to_string())]);
        }
        let response = request.send().await.map_err(|_| anyhow::anyhow!("Question outcome unconfirmed; inspect status or retry the same request ID and payload"))?;
        match response.status().as_u16() {
            200..=202 => response.json().await.map_err(|_| {
                anyhow::anyhow!("Unreadable question receipt; inspect the same request ID")
            }),
            401 | 403 => bail!(
                "Question access denied; current project permissions and revocable authentication required"
            ),
            404 => bail!("Question or interaction profile unavailable"),
            409 => bail!(
                "Question conflicts with current policy, sources or prior request; do not change an existing request ID"
            ),
            429 => bail!("Question capacity or budget exhausted; inspect status before retrying"),
            _ => bail!(
                "Question outcome unconfirmed; inspect status or retry the exact request; response details suppressed"
            ),
        }
    }
    pub(crate) async fn supervisor_question_prepare(
        &self,
        project: &str,
        question: &str,
    ) -> Result<SupervisorQuestionPreview> {
        ensure!(
            !question.trim().is_empty() && question.len() <= 4096,
            "Question must contain 1–4096 UTF-8 bytes"
        );
        let result: SupervisorQuestionPreview = self
            .question_operation(
                project,
                Some("prepare"),
                None,
                Some(serde_json::to_value(SupervisorQuestionPrepare {
                    question: question.into(),
                })?),
            )
            .await?;
        ensure!(
            result.question == question,
            "Question preview mismatch; no question submitted"
        );
        Ok(result)
    }
    pub(crate) async fn supervisor_question_submit(
        &self,
        project: &str,
        request: &SupervisorQuestionRequest,
    ) -> Result<SupervisorQuestionStatus> {
        let result: SupervisorQuestionStatus = self
            .question_operation(project, None, None, Some(serde_json::to_value(request)?))
            .await?;
        ensure!(
            result.request_id == request.request_id,
            "Question receipt mismatch; inspect the original request ID"
        );
        Ok(result)
    }
    pub(crate) async fn supervisor_question(
        &self,
        project: &str,
        id: Uuid,
    ) -> Result<SupervisorQuestionStatus> {
        let result: SupervisorQuestionStatus = self
            .question_operation(project, Some(&id.to_string()), None, None)
            .await?;
        ensure!(result.request_id == id, "Question status mismatch");
        Ok(result)
    }
    pub(crate) async fn supervisor_questions(
        &self,
        project: &str,
        after: Option<Uuid>,
    ) -> Result<SupervisorQuestionList> {
        self.question_operation(project, None, after, None).await
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use tokio::io::{AsyncReadExt, AsyncWriteExt};

    async fn serve(
        listener: tokio::net::TcpListener,
        responses: Vec<(u16, String, String)>,
    ) -> Vec<String> {
        let mut requests = vec![];
        for (status, headers, body) in responses {
            let (mut socket, _) = listener.accept().await.unwrap();
            let mut bytes = vec![];
            loop {
                let mut chunk = [0; 4096];
                let n = socket.read(&mut chunk).await.unwrap();
                assert!(n > 0);
                bytes.extend_from_slice(&chunk[..n]);
                if let Some(start) = bytes.windows(4).position(|w| w == b"\r\n\r\n") {
                    let head = String::from_utf8_lossy(&bytes[..start]).to_lowercase();
                    let len = head
                        .lines()
                        .find_map(|line| {
                            line.strip_prefix("content-length: ")
                                .and_then(|n| n.parse::<usize>().ok())
                        })
                        .unwrap_or(0);
                    if bytes.len() >= start + 4 + len {
                        break;
                    }
                }
            }
            requests.push(String::from_utf8(bytes).unwrap());
            socket.write_all(format!("HTTP/1.1 {status} Test\r\nContent-Type: application/json\r\nContent-Length: {}\r\nConnection: close\r\n{headers}\r\n{body}", body.len()).as_bytes()).await.unwrap();
        }
        requests
    }
    fn auth(kind: &str, mode: &str) -> String {
        serde_json::json!({"authenticated":true,"mode":mode,"read_only":false,
            "principal":{"id":Uuid::new_v5(&Uuid::NAMESPACE_OID,b"memory-layer-local-owner"),"kind":kind,"display_name":"synthetic","global_role":"admin"}}).to_string()
    }
    fn api(address: std::net::SocketAddr) -> ApiClient {
        let config = serde_json::from_value(serde_json::json!({"service":{"bind_addr":address.to_string(),"api_token":"synthetic-owner"},"database":{"url":"postgresql://unused/unused"}})).unwrap();
        ApiClient::new(Client::new(), config)
    }
    #[tokio::test]
    async fn supervisor_question_local_session_is_shared_private_revocable_and_not_reminted() {
        let listener = tokio::net::TcpListener::bind("127.0.0.1:0").await.unwrap();
        let api = api(listener.local_addr().unwrap());
        let clone = api.clone();
        let server = tokio::spawn(serve(listener, vec![
            (200, String::new(), auth("internal", "single_user")),
            (200, "Set-Cookie: memory_session=synthetic-session; Path=/; HttpOnly\r\nSet-Cookie: memory_csrf=synthetic-csrf; Path=/\r\n".into(), "{}".into()),
            (200, String::new(), "{\"questions\":[],\"next_after\":null}".into()),
            (403, String::new(), "PRIVATE_DB_DETAILS".into()),
            (403, String::new(), "PRIVATE_DB_DETAILS".into()),
        ]));
        api.supervisor_questions("memory", None).await.unwrap();
        for _ in 0..2 {
            let error = clone
                .supervisor_questions("memory", None)
                .await
                .err()
                .unwrap()
                .to_string();
            assert!(error.contains("access denied"));
            assert!(!error.contains("PRIVATE_DB_DETAILS"));
        }
        let requests = server.await.unwrap();
        assert!(requests[0].starts_with("GET /v1/auth/me "));
        assert!(requests[0].contains("x-api-token: synthetic-owner"));
        assert!(requests[1].starts_with("POST /v1/auth/session/bootstrap "));
        for request in &requests[2..] {
            assert!(request.starts_with("GET /v1/projects/memory/supervisor/questions "));
            assert!(request.contains("memory_session=synthetic-session"));
            assert!(request.contains("x-csrf-token: synthetic-csrf"));
            assert!(request.contains("origin: http://127.0.0.1:"));
            assert!(!request.contains("x-api-token"));
        }
    }
    #[tokio::test]
    async fn supervisor_question_service_tokens_are_not_exchanged_for_owner_sessions() {
        let listener = tokio::net::TcpListener::bind("127.0.0.1:0").await.unwrap();
        let api = api(listener.local_addr().unwrap());
        let server = tokio::spawn(serve(
            listener,
            vec![
                (200, String::new(), auth("service_token", "single_user")),
                (
                    200,
                    String::new(),
                    "{\"questions\":[],\"next_after\":null}".into(),
                ),
            ],
        ));
        api.supervisor_questions("memory", None).await.unwrap();
        let requests = server.await.unwrap();
        assert!(requests[1].starts_with("GET /v1/projects/memory/supervisor/questions "));
        assert!(requests[1].contains("x-api-token: synthetic-owner"));
        assert!(!requests[1].contains("cookie:"));
    }
    #[tokio::test]
    async fn supervisor_question_auth_redirect_is_never_followed_or_bootstrapped() {
        let listener = tokio::net::TcpListener::bind("127.0.0.1:0").await.unwrap();
        let api = api(listener.local_addr().unwrap());
        let server = tokio::spawn(serve(
            listener,
            vec![(
                302,
                "Location: http://127.0.0.1:1/forbidden\r\n".into(),
                String::new(),
            )],
        ));
        let error = api
            .supervisor_questions("memory", None)
            .await
            .err()
            .unwrap()
            .to_string();
        assert_eq!(error, "Local authentication unavailable");
        assert_eq!(server.await.unwrap().len(), 1);
    }
    #[tokio::test]
    async fn supervisor_question_submission_sends_exact_consent_and_binds_receipt() {
        let listener = tokio::net::TcpListener::bind("127.0.0.1:0").await.unwrap();
        let api = api(listener.local_addr().unwrap());
        let request = SupervisorQuestionRequest {
            request_id: Uuid::new_v4(),
            question: "Exact private question".into(),
            profile_digest: "a".repeat(64),
            request_digest: "b".repeat(64),
        };
        let server = tokio::spawn(serve(listener, vec![
            (200, String::new(), auth("service_token", "multi_user")),
            (200, String::new(), serde_json::json!({"request_id":Uuid::new_v4(),"action_id":Uuid::new_v4(),"state":"queued","expires_at":"2026-09-19T10:00:00Z","proposal_available":false}).to_string()),
        ]));
        let error = api
            .supervisor_question_submit("memory", &request)
            .await
            .err()
            .unwrap()
            .to_string();
        assert!(error.contains("receipt mismatch"));
        let requests = server.await.unwrap();
        assert!(requests[1].starts_with("POST /v1/projects/memory/supervisor/questions "));
        let sent: serde_json::Value =
            serde_json::from_str(requests[1].split_once("\r\n\r\n").unwrap().1).unwrap();
        assert_eq!(sent, serde_json::to_value(request).unwrap());
    }
}
