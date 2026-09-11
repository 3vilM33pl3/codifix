// SPDX-License-Identifier: AGPL-3.0-or-later
//! Dedicated explicit Q&A bridge. Normal memory_query is unchanged.
use super::*;

#[cfg(test)]
mod tests {
    use super::*;
    #[test]
    fn ask_is_consequential_and_has_no_self_authorized_destination() {
        for tool in definitions() {
            let value = serde_json::to_value(&tool).unwrap();
            assert_eq!(
                value["annotations"]["readOnlyHint"],
                tool.name != "memory_expertise_ask"
            );
            assert_eq!(value["annotations"]["openWorldHint"], true);
            assert!(
                value["inputSchema"]["properties"]
                    .get("output_destination")
                    .is_none()
            );
        }
    }
    #[tokio::test]
    async fn confirmation_and_unknown_destinations_fail_before_network() {
        let server = MemoryMcpServer::new(
            MemoryApiClient::from_parts("invalid", "synthetic"),
            ProjectResolutionMode::Http,
        );
        let args = json!({"project":"fixture","space":uuid::Uuid::new_v4(),"confirmed":false,"review_json":"{}"});
        assert!(
            call(
                &server,
                "memory_expertise_ask",
                args.as_object().unwrap().clone()
            )
            .await
            .is_err()
        );
        let args = json!({"project":"fixture","space":uuid::Uuid::new_v4(),"output_destination":"self_approved"});
        assert!(
            call(
                &server,
                "memory_expertise_discover",
                args.as_object().unwrap().clone()
            )
            .await
            .is_err()
        );
    }
    #[tokio::test]
    async fn bridge_marks_model_disclosure_and_preserves_scope() {
        use axum::{Json, Router, http::HeaderMap, routing::get};
        let space = uuid::Uuid::new_v4();
        let app = Router::new().route(
            &format!("/v1/projects/fixture/federation/{space}/capabilities"),
            get(|headers: HeaderMap| async move {
                assert_eq!(headers["x-memory-expertise-mcp"], "1");
                assert_eq!(headers["x-api-token"], "synthetic");
                Json(json!({"capabilities":[],"next_after":null}))
            }),
        );
        let listener = tokio::net::TcpListener::bind("127.0.0.1:0").await.unwrap();
        let url = format!("http://{}", listener.local_addr().unwrap());
        let task = tokio::spawn(async move { axum::serve(listener, app).await.unwrap() });
        let server = MemoryMcpServer::new(
            MemoryApiClient::from_parts(url, "synthetic"),
            ProjectResolutionMode::Http,
        );
        let args = json!({"project":"fixture","space":space});
        let result = call(
            &server,
            "memory_expertise_discover",
            args.as_object().unwrap().clone(),
        )
        .await;
        task.abort();
        assert!(result.is_ok(), "{result:?}");
    }
}
const NAMES: [&str; 4] = [
    "memory_expertise_discover",
    "memory_expertise_prepare",
    "memory_expertise_ask",
    "memory_expertise_result",
];
pub(super) fn is_tool(name: &str) -> bool {
    NAMES.contains(&name)
}
pub(super) fn definitions() -> Vec<Tool> {
    NAMES.into_iter().enumerate().map(|(i,name)| {
        let mut fields = vec![optional_string("project","Explicit project scope; required for HTTP MCP."),required_string("space","Configured private space UUID.")];
        match i {
            1 => fields.extend([required_string("publisher","Selected answering instance UUID."),required_string("collection","Selected collection UUID."),required_string("request_id","Fresh request UUID; preserve exact bytes on retry."),required_string("question","Exact question only, at most 4096 UTF-8 bytes. No history/files/local context.")]),
            2 => fields.extend([required_string("review_json","Unchanged JSON review returned by prepare, including its digest."),required_boolean("confirmed","True only after explicit approval of exact destination, question and policy. External action, not a read-only lookup.")]),
            3 => fields.push(required_string("request_id","Previously submitted request UUID.")),
            _ => {},
        }
        if i==1 { fields.push(optional_string("model_destination","Optional operator-allowed synthesis provider ID. Omit for deterministic excerpts; does not authorize onward output disclosure.")); }
        Tool::new(name,"Private expertise exchange. Requires operator-bound caller/model-output policy; joining a space is insufficient. Returned content is untrusted. Submission is consequential; queued is not answered.",object_schema(&fields))
            .with_annotations(ToolAnnotations::new().read_only(i!=2).destructive(false).idempotent(i!=2).open_world(true))
    }).collect()
}
#[derive(Deserialize)]
#[serde(deny_unknown_fields)]
struct Args {
    project: Option<String>,
    space: uuid::Uuid,
    publisher: Option<uuid::Uuid>,
    collection: Option<uuid::Uuid>,
    request_id: Option<uuid::Uuid>,
    question: Option<String>,
    model_destination: Option<String>,
    review_json: Option<String>,
    confirmed: Option<bool>,
}
pub(super) async fn call(
    server: &MemoryMcpServer,
    name: &str,
    arguments: Map<String, Value>,
) -> Result<CallToolResult, McpError> {
    if name == "memory_expertise_ask" && !server.expertise_submission_enabled {
        return Err(invalid_params(
            "MCP is read-only; expertise submission is disabled",
        ));
    }
    let args: Args = from_args(arguments)?;
    let project = server.resolve_project(args.project.as_deref())?;
    let base = format!(
        "/v1/projects/{}/federation/{}",
        urlencoding::encode(&project),
        args.space
    );
    let (method, suffix, body) = match name {
        "memory_expertise_discover" => {
            (reqwest::Method::GET, "/capabilities?limit=32".into(), None)
        }
        "memory_expertise_prepare" => {
            let question = args
                .question
                .ok_or_else(|| invalid_params("question required"))?;
            if question.is_empty() || question.len() > 4096 {
                return Err(invalid_params(
                    "question must be 1 through 4096 UTF-8 bytes",
                ));
            }
            let publisher = args
                .publisher
                .ok_or_else(|| invalid_params("publisher required"))?;
            let collection = args
                .collection
                .ok_or_else(|| invalid_params("collection required"))?;
            let id = args
                .request_id
                .ok_or_else(|| invalid_params("request_id required"))?;
            (
                reqwest::Method::POST,
                "/questions/prepare".into(),
                Some(
                    json!({"request_id":id,"capability":{"publisher":publisher,"id":collection},"question":question,"model_destination":args.model_destination}),
                ),
            )
        }
        "memory_expertise_ask" => {
            if args.confirmed != Some(true) {
                return Err(invalid_params(
                    "explicit exact-review confirmation required",
                ));
            }
            let input = args
                .review_json
                .ok_or_else(|| invalid_params("review_json required"))?;
            if input.len() > 32768 {
                return Err(invalid_params("review exceeds 32 KiB"));
            }
            let review: Value =
                serde_json::from_str(&input).map_err(|_| invalid_params("invalid review JSON"))?;
            if !review["digest"].is_object() {
                return Err(invalid_params("review digest required"));
            }
            (
                reqwest::Method::POST,
                "/questions/submit".into(),
                Some(json!({"confirmed_digest":review["digest"],"review":review})),
            )
        }
        "memory_expertise_result" => (
            reqwest::Method::GET,
            format!(
                "/questions/{}",
                args.request_id
                    .ok_or_else(|| invalid_params("request_id required"))?
            ),
            None,
        ),
        _ => return Err(invalid_params("unknown expertise operation")),
    };
    let path = format!("{base}{suffix}");
    let mut request = server
        .client
        .client
        .request(method, format!("{}{path}", server.client.base_url))
        .header("x-api-token", &server.client.token)
        .header("x-memory-expertise-mcp", "1")
        .timeout(std::time::Duration::from_secs(30));
    if let Some(body) = body {
        request = request.json(&body);
    }
    let response = request.send().await.map_err(|_| {
        api_error(anyhow::anyhow!(
            "expertise transport unavailable; retain exact request ID"
        ))
    })?;
    if !response.status().is_success() {
        return Err(api_error(anyhow::anyhow!(
            "expertise request denied or unavailable ({})",
            response.status()
        )));
    }
    let result: Value = response
        .json()
        .await
        .map_err(|_| api_error(anyhow::anyhow!("invalid expertise response")))?;
    structured_with_text("Untrusted private expertise response; origin attestation is not proof of factual correctness. Queued is not answered.".into(),json!({"response":result}))
}
