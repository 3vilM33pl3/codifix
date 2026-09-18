use super::*;
use crate::pi_broker::{BrokerFailure, BrokerJob, BrokerReply, JobFuture, PiBroker};
use std::sync::atomic::AtomicBool;

mod launcher;
mod sandbox;

struct Work {
    store: SupervisorStore,
    lease: LeaseClaim,
    execution: ProposalExecution,
    request: Arc<PreparedRequest>,
    client: OpenAiTransport,
    deliver: AtomicBool,
    runs: AtomicUsize,
    pause: Option<Arc<tokio::sync::Notify>>,
}
impl BrokerJob for Work {
    fn request_digest(&self) -> &str {
        self.request.digest()
    }
    fn run(&self) -> JobFuture<'_, Result<BrokerReply, BrokerFailure>> {
        Box::pin(async {
            self.runs.fetch_add(1, Ordering::SeqCst);
            if let Some(pause) = &self.pause {
                pause.notified().await;
            }
            self.client
                .execute_and_persist(&self.store, &self.lease, &self.execution, &self.request)
                .await
                .map_err(|_| BrokerFailure::Uncertain)?;
            BrokerReply::from_persisted(
                &self.store,
                Area::Workshop,
                self.lease.action_id(),
                &self.execution,
                &self.request,
            )
            .await
        })
    }
    fn authorize_delivery(&self) -> JobFuture<'_, Result<(), BrokerFailure>> {
        Box::pin(async {
            if self.deliver.load(Ordering::SeqCst) {
                Ok(())
            } else {
                Err(BrokerFailure::Denied)
            }
        })
    }
}

async fn broker_fixture(
    mode: Mode,
    pause: Option<Arc<tokio::sync::Notify>>,
    lifetime: Duration,
) -> Option<(
    IsolatedDatabase,
    Fake,
    Arc<Work>,
    String,
    String,
    tokio::task::JoinHandle<()>,
    tokio::task::JoinHandle<()>,
)> {
    let (db, store, lease, revision, fake, client, provider) = fixture(mode).await?;
    let request = Arc::new(request());
    let work = Arc::new(Work {
        store,
        lease,
        execution: fake.execution(revision, Uuid::new_v4()),
        request: Arc::clone(&request),
        client,
        deliver: AtomicBool::new(true),
        runs: AtomicUsize::new(0),
        pause,
    });
    let (broker, token) = PiBroker::new(request, work.clone(), lifetime).expect("broker");
    let listener = tokio::net::TcpListener::bind("127.0.0.1:0")
        .await
        .expect("synthetic local listener");
    let url = format!(
        "http://{}/v1/responses",
        listener.local_addr().expect("address")
    );
    let server = tokio::spawn(async move {
        axum::serve(listener, broker.router())
            .await
            .expect("broker server");
    });
    Some((
        db,
        fake,
        work,
        url,
        token.bearer_for_child().to_owned(),
        provider,
        server,
    ))
}
fn http() -> reqwest::Client {
    reqwest::Client::builder()
        .no_proxy()
        .retry(reqwest::retry::never())
        .redirect(reqwest::redirect::Policy::none())
        .timeout(Duration::from_secs(5))
        .build()
        .expect("test client")
}
fn post(client: &reqwest::Client, url: &str, token: &str, work: &Work) -> reqwest::RequestBuilder {
    client
        .post(url)
        .bearer_auth(token)
        .header("content-type", "application/json")
        .body(pi_body(&work.request))
}

#[tokio::test]
async fn authenticated_broker_returns_only_durable_structured_text_and_real_counters_once() {
    let Some((db, fake, work, url, token, provider, server)) =
        broker_fixture(Mode::Good, None, Duration::from_secs(30)).await
    else {
        return;
    };
    let client = http();
    assert!(matches!(
        PiBroker::new(
            Arc::new(PreparedRequest::new("different admitted input", &[]).expect("input")),
            work.clone(),
            Duration::from_secs(30)
        ),
        Err(BrokerFailure::Denied)
    ));
    let result = post(&client, &url, &token, &work)
        .send()
        .await
        .expect("request");
    assert_eq!(result.status(), StatusCode::OK);
    assert_eq!(result.headers()["content-type"], "text/event-stream");
    assert_eq!(result.headers()["cache-control"], "no-store");
    let bytes = result.text().await.expect("SSE");
    assert!(!bytes.contains(&token));
    assert!(!bytes.contains("synthetic-only"));
    assert!(!bytes.contains("Fake reviewed context"));
    let events: Vec<Value> = bytes
        .lines()
        .filter_map(|line| line.strip_prefix("data: "))
        .map(|line| serde_json::from_str(line).expect("SSE JSON"))
        .collect();
    assert_eq!(events.len(), 2);
    let proposal: Value = serde_json::from_str(events[0]["delta"].as_str().expect("proposal text"))
        .expect("proposal");
    assert_eq!(proposal["summary"], "Ask a human to review");
    assert_eq!(
        events[1]["response"]["usage"],
        json!({"input_tokens":100,"output_tokens":20,"total_tokens":120,
        "input_tokens_details":{"cached_tokens":0,"cache_write_tokens":0},"output_tokens_details":{"reasoning_tokens":5}})
    );
    assert_eq!(
        work.store
            .action(Area::Workshop, work.lease.action_id())
            .await
            .expect("durable outcome")
            .state,
        ActionState::Verified
    );
    assert_eq!(
        post(&client, &url, &token, &work)
            .send()
            .await
            .expect("replay")
            .status(),
        StatusCode::CONFLICT
    );
    assert_eq!(fake.count.load(Ordering::SeqCst), 1);
    assert_eq!(fake.generated.load(Ordering::SeqCst), 1);
    assert_eq!(work.runs.load(Ordering::SeqCst), 1);
    provider.abort();
    server.abort();
    db.cleanup().await.expect("cleanup");
}

#[tokio::test]
async fn bad_auth_and_altered_or_oversized_requests_never_reach_the_model_or_ledger() {
    let Some((db, fake, work, url, token, provider, server)) =
        broker_fixture(Mode::Good, None, Duration::from_secs(30)).await
    else {
        return;
    };
    let client = http();
    for bad in ["", "not-a-provider-key", &"a".repeat(64)] {
        let response = post(&client, &url, bad, &work)
            .send()
            .await
            .expect("bad auth");
        assert_eq!(response.status(), StatusCode::UNAUTHORIZED);
    }
    for (body, status) in [
        (
            b"{\"model\":\"one\",\"model\":\"two\"}".to_vec(),
            StatusCode::BAD_REQUEST,
        ),
        (
            pi_body(&PreparedRequest::new("different private input", &[]).expect("request")),
            StatusCode::BAD_REQUEST,
        ),
        (
            vec![b' '; crate::pi_admission::MAX_PI_REQUEST_BYTES + 1],
            StatusCode::PAYLOAD_TOO_LARGE,
        ),
    ] {
        let response = post(&client, &url, &token, &work)
            .body(body)
            .send()
            .await
            .expect("rejected body");
        assert_eq!(response.status(), status);
        let text = response.text().await.expect("sanitized error");
        assert!(!text.contains("different private input"));
        assert!(!text.contains(&token));
    }
    let response = post(&client, &format!("{url}?model=other"), &token, &work)
        .send()
        .await
        .expect("query override");
    assert_eq!(response.status(), StatusCode::BAD_REQUEST);
    assert_eq!(fake.count.load(Ordering::SeqCst), 0);
    assert_eq!(fake.generated.load(Ordering::SeqCst), 0);
    assert_eq!(work.runs.load(Ordering::SeqCst), 0);
    assert!(matches!(
        work.store.attempt(work.execution.attempt_id).await,
        Err(StoreError::NotFound)
    ));
    // Invalid ingress did not consume the valid one-work session.
    assert_eq!(
        post(&client, &url, &token, &work)
            .send()
            .await
            .expect("valid after denial")
            .status(),
        StatusCode::OK
    );
    provider.abort();
    server.abort();
    db.cleanup().await.expect("cleanup");
}

#[tokio::test]
async fn concurrent_broker_replay_even_counts_the_input_only_once() {
    let Some((db, fake, work, url, token, provider, server)) =
        broker_fixture(Mode::Good, None, Duration::from_secs(30)).await
    else {
        return;
    };
    let client = http();
    let (a, b) = tokio::join!(
        post(&client, &url, &token, &work).send(),
        post(&client, &url, &token, &work).send()
    );
    let statuses = [a.expect("first").status(), b.expect("second").status()];
    assert_eq!(statuses.iter().filter(|&&s| s == StatusCode::OK).count(), 1);
    assert!(statuses.iter().all(|&s| matches!(
        s,
        StatusCode::OK | StatusCode::CONFLICT | StatusCode::TOO_MANY_REQUESTS
    )));
    assert_eq!(fake.count.load(Ordering::SeqCst), 1);
    assert_eq!(fake.generated.load(Ordering::SeqCst), 1);
    provider.abort();
    server.abort();
    db.cleanup().await.expect("cleanup");
}

#[tokio::test]
async fn denied_delivery_withholds_text_after_settlement_and_never_repeats_generation() {
    let Some((db, fake, work, url, token, provider, server)) =
        broker_fixture(Mode::Good, None, Duration::from_secs(30)).await
    else {
        return;
    };
    work.deliver.store(false, Ordering::SeqCst);
    let client = http();
    let response = post(&client, &url, &token, &work)
        .send()
        .await
        .expect("revoked delivery");
    assert_eq!(response.status(), StatusCode::FORBIDDEN);
    assert!(
        !response
            .text()
            .await
            .expect("error")
            .contains("Ask a human")
    );
    assert!(
        work.store
            .settled_usage(work.execution.attempt_id)
            .await
            .expect("billed")
            .is_some()
    );
    assert_eq!(
        post(&client, &url, &token, &work)
            .send()
            .await
            .expect("replay")
            .status(),
        StatusCode::CONFLICT
    );
    assert_eq!(fake.generated.load(Ordering::SeqCst), 1);
    provider.abort();
    server.abort();
    db.cleanup().await.expect("cleanup");
}

#[tokio::test]
async fn uncertain_provider_usage_remains_reserved_and_is_not_returned_as_zero() {
    let Some((db, fake, work, url, token, provider, server)) =
        broker_fixture(Mode::MalformedUsage, None, Duration::from_secs(30)).await
    else {
        return;
    };
    let client = http();
    let response = post(&client, &url, &token, &work)
        .send()
        .await
        .expect("uncertain");
    assert_eq!(response.status(), StatusCode::BAD_GATEWAY);
    let text = response.text().await.expect("error");
    assert!(text.contains("uncertain_do_not_resend"));
    assert!(!text.contains("input_tokens"));
    assert_eq!(
        work.store
            .attempt(work.execution.attempt_id)
            .await
            .expect("attempt")
            .state,
        AttemptState::Uncertain
    );
    assert_eq!(
        post(&client, &url, &token, &work)
            .send()
            .await
            .expect("replay")
            .status(),
        StatusCode::CONFLICT
    );
    assert_eq!(fake.generated.load(Ordering::SeqCst), 1);
    provider.abort();
    server.abort();
    db.cleanup().await.expect("cleanup");
}

#[tokio::test]
async fn client_disconnect_does_not_cancel_owned_work_or_its_persistence() {
    let pause = Arc::new(tokio::sync::Notify::new());
    let Some((db, fake, work, url, token, provider, server)) =
        broker_fixture(Mode::Good, Some(pause.clone()), Duration::from_secs(30)).await
    else {
        return;
    };
    let client = http();
    assert!(
        post(&client, &url, &token, &work)
            .timeout(Duration::from_millis(200))
            .send()
            .await
            .is_err()
    );
    assert_eq!(work.runs.load(Ordering::SeqCst), 1);
    pause.notify_one();
    tokio::time::timeout(Duration::from_secs(5), async {
        loop {
            if work
                .store
                .action(Area::Workshop, work.lease.action_id())
                .await
                .expect("action")
                .state
                == ActionState::Verified
            {
                break;
            }
            tokio::time::sleep(Duration::from_millis(20)).await;
        }
    })
    .await
    .expect("durable completion despite disconnect");
    assert_eq!(
        post(&client, &url, &token, &work)
            .send()
            .await
            .expect("recovery must not send")
            .status(),
        StatusCode::CONFLICT
    );
    assert_eq!(fake.generated.load(Ordering::SeqCst), 1);
    provider.abort();
    server.abort();
    db.cleanup().await.expect("cleanup");
}

#[tokio::test]
async fn deadline_does_not_reopen_a_session_or_deliver_late_content() {
    let pause = Arc::new(tokio::sync::Notify::new());
    let Some((db, fake, work, url, token, provider, server)) =
        broker_fixture(Mode::Good, Some(pause), Duration::from_millis(100)).await
    else {
        return;
    };
    let response = post(&http(), &url, &token, &work)
        .send()
        .await
        .expect("deadline result");
    assert!(matches!(
        response.status(),
        StatusCode::GONE | StatusCode::BAD_GATEWAY
    ));
    assert!(
        !response
            .text()
            .await
            .expect("error")
            .contains("Ask a human")
    );
    assert_eq!(
        post(&http(), &url, &token, &work)
            .send()
            .await
            .expect("expired replay")
            .status(),
        StatusCode::GONE
    );
    assert_eq!(fake.generated.load(Ordering::SeqCst), 0);
    provider.abort();
    server.abort();
    db.cleanup().await.expect("cleanup");
}

#[tokio::test]
async fn slow_body_is_bounded_without_consuming_a_model_attempt() {
    use tokio::io::{AsyncReadExt, AsyncWriteExt};
    let Some((db, fake, work, url, token, provider, server)) =
        broker_fixture(Mode::Good, None, Duration::from_secs(30)).await
    else {
        return;
    };
    let address = url
        .strip_prefix("http://")
        .expect("local URL")
        .strip_suffix("/v1/responses")
        .expect("route");
    let mut stream = tokio::net::TcpStream::connect(address)
        .await
        .expect("connect");
    let header = format!(
        "POST /v1/responses HTTP/1.1\r\nHost: localhost\r\nAuthorization: Bearer {token}\r\nContent-Type: application/json\r\nContent-Length: 100\r\nConnection: close\r\n\r\n{{"
    );
    stream
        .write_all(header.as_bytes())
        .await
        .expect("partial body");
    let mut bytes = [0; 1024];
    let n = tokio::time::timeout(Duration::from_secs(4), stream.read(&mut bytes))
        .await
        .expect("bounded response")
        .expect("read");
    assert!(
        std::str::from_utf8(&bytes[..n])
            .expect("ASCII response")
            .starts_with("HTTP/1.1 408")
    );
    assert_eq!(work.runs.load(Ordering::SeqCst), 0);
    assert_eq!(fake.count.load(Ordering::SeqCst), 0);
    assert_eq!(
        post(&http(), &url, &token, &work)
            .send()
            .await
            .expect("valid later")
            .status(),
        StatusCode::OK
    );
    provider.abort();
    server.abort();
    db.cleanup().await.expect("cleanup");
}
