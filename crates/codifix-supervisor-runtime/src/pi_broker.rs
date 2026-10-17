//! One-work authenticated Pi ingress. No automatic listener, credential lookup,
//! database selection, source consent, sandbox, scheduler or tool execution.
//!
//! A trusted service implements `BrokerJob` and owns the exact reviewed request,
//! durable lease/attempt and current disclosure checks. No identifiers, URLs,
//! model settings or credentials supplied by Pi are forwarded to that job.
use crate::{
    model::PreparedRequest,
    pi_admission::PiAdmission,
    transport::{DisclosureGuard, DisclosurePhase, OpenAiTransport, ProposalExecution},
};
use axum::{
    Router,
    body::{Body, to_bytes},
    extract::{Request, State},
    http::{StatusCode, header},
    response::{IntoResponse, Response},
    routing::post,
};
use codifix_supervisor::{Area, DEFAULT_MODEL};
use codifix_supervisor_store::{LeaseClaim, SupervisorStore};
use serde_json::json;
use sha2::{Digest, Sha256};
use std::{
    future::Future,
    pin::Pin,
    sync::{
        Arc,
        atomic::{AtomicBool, Ordering},
    },
    time::Duration,
};
use subtle::ConstantTimeEq;
use tokio::{
    sync::Semaphore,
    time::{Instant, timeout_at},
};
use uuid::Uuid;

pub type JobFuture<'a, T> = Pin<Box<dyn Future<Output = T> + Send + 'a>>;

/// Trusted service implementation, NOT an HTTP request contract. Implementations
/// must check current disclosure before counting/generation and fence proposal
/// publication with current source/project authority. `run` may be called once;
/// `authorize_delivery` must recheck current disclosure after durable completion.
pub trait BrokerJob: Send + Sync + 'static {
    fn request_digest(&self) -> &str;
    fn run(&self) -> JobFuture<'_, Result<BrokerReply, BrokerFailure>>;
    fn authorize_delivery(&self) -> JobFuture<'_, Result<(), BrokerFailure>>;
}

/// Concrete host job: requires a service-supplied current disclosure guard and
/// durable authorization/claim. None of these fields originate from Pi input.
pub struct GuardedBrokerJob {
    pub store: SupervisorStore,
    pub lease: LeaseClaim,
    pub execution: ProposalExecution,
    pub request: Arc<PreparedRequest>,
    pub transport: OpenAiTransport,
    pub disclosure: Arc<dyn DisclosureGuard>,
}

impl BrokerJob for GuardedBrokerJob {
    fn request_digest(&self) -> &str {
        self.request.digest()
    }
    fn run(&self) -> JobFuture<'_, Result<BrokerReply, BrokerFailure>> {
        Box::pin(async {
            self.transport
                .execute_guarded_and_persist(
                    &self.store,
                    &self.lease,
                    &self.execution,
                    &self.request,
                    self.disclosure.as_ref(),
                )
                .await
                .map_err(|error| match error {
                    crate::transport::TransportError::Authority => BrokerFailure::Denied,
                    _ => BrokerFailure::Uncertain,
                })?;
            BrokerReply::from_persisted(
                &self.store,
                self.lease.area(),
                self.lease.action_id(),
                &self.execution,
                &self.request,
            )
            .await
        })
    }
    fn authorize_delivery(&self) -> JobFuture<'_, Result<(), BrokerFailure>> {
        Box::pin(async {
            self.disclosure
                .authorize(DisclosurePhase::Deliver, &self.execution, &self.request)
                .await
                .map_err(|_| BrokerFailure::Denied)
        })
    }
}

/// Opaque credential for one exact job, not a provider credential. Intentionally
/// no Debug/Serialize/Clone. The caller supplies it only to the isolated child.
pub struct BrokerToken(String);
impl BrokerToken {
    pub fn bearer_for_child(&self) -> &str {
        &self.0
    }
}

/// A response can be built only by recovering the exact persisted proposal and
/// authoritative settlement counters. Caller still authorizes current reads.
/// No public constructor from arbitrary text, counters or provider output.
pub struct BrokerReply {
    wire: String,
}

impl BrokerReply {
    pub async fn from_persisted(
        store: &SupervisorStore,
        area: Area,
        action: Uuid,
        execution: &ProposalExecution,
        request: &PreparedRequest,
    ) -> Result<Self, BrokerFailure> {
        let binding = store
            .instance_binding()
            .await
            .map_err(|_| BrokerFailure::Failed)?
            .ok_or(BrokerFailure::Denied)?;
        if binding.area != area || binding.instance_id != execution.instance_id {
            return Err(BrokerFailure::Denied);
        }
        let proposal = store
            .private_proposal(area, execution.project_id, action)
            .await
            .map_err(|_| BrokerFailure::Denied)?;
        let digest = proposal
            .request_digest
            .iter()
            .map(|n| format!("{n:02x}"))
            .collect::<String>();
        if proposal.attempt_id != execution.attempt_id
            || proposal.policy_revision != execution.policy_revision
            || digest != request.digest()
            || proposal.evidence_digests != request.evidence_digests()
        {
            return Err(BrokerFailure::Denied);
        }
        let usage = store
            .settled_usage(execution.attempt_id)
            .await
            .map_err(|_| BrokerFailure::Failed)?
            .ok_or(BrokerFailure::Uncertain)?;
        let text = serde_json::to_string(&proposal.proposal).map_err(|_| BrokerFailure::Failed)?;
        // Broker-issued response identity, not a claimed original OpenAI ID.
        let response_id = format!("memory-supervisor-{}", execution.attempt_id);
        let message_id = format!("memory-proposal-{action}");
        let events = [
            json!({"type":"response.output_text.delta","item_id":message_id,"content_index":0,"delta":text}),
            json!({"type":"response.completed","response":{
                "id":response_id,"object":"response","model":DEFAULT_MODEL,
                "status":"completed","service_tier":"default","error":null,"incomplete_details":null,
                "output":[{"type":"message","id":message_id,"role":"assistant","status":"completed",
                    "content":[{"type":"output_text","text":text,"annotations":[]}]}],
                "usage":{"input_tokens":usage.input_tokens,"output_tokens":usage.output_tokens,
                    "total_tokens":usage.total_tokens,
                    "input_tokens_details":{"cached_tokens":usage.cached_tokens,"cache_write_tokens":usage.cache_write_tokens},
                    "output_tokens_details":{"reasoning_tokens":usage.reasoning_tokens}}
            }}),
        ];
        let wire = events
            .iter()
            .map(|event| format!("data: {event}\n\n"))
            .collect::<String>();
        if wire.len() > crate::model::MAX_RESPONSE_BYTES {
            return Err(BrokerFailure::Failed);
        }
        Ok(Self { wire })
    }
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, thiserror::Error)]
pub enum BrokerFailure {
    #[error("current disclosure or work authority denied")]
    Denied,
    #[error("work deadline expired")]
    Expired,
    #[error("work failed; inspect the metadata journal")]
    Failed,
    #[error("provider outcome or usage is uncertain; do not resend")]
    Uncertain,
}

struct Session {
    token_digest: [u8; 32],
    request: Arc<PreparedRequest>,
    job: Arc<dyn BrokerJob>,
    expires: Instant,
    consumed: AtomicBool,
    reading: Semaphore,
}

/// A router for ONE exact job. Creating it neither starts a listener nor proves
/// sandbox/egress isolation. Never mount it on the ordinary Codifix service or a
/// public interface; a supervisor-owned private Unix socket is the intended
/// transport. The future sandbox relay must expose only this router to Pi.
pub struct PiBroker {
    session: Arc<Session>,
}

impl PiBroker {
    pub fn new(
        request: Arc<PreparedRequest>,
        job: Arc<dyn BrokerJob>,
        lifetime: Duration,
    ) -> Result<(Self, BrokerToken), BrokerFailure> {
        if job.request_digest() != request.digest() {
            return Err(BrokerFailure::Denied);
        }
        if lifetime.is_zero() || lifetime > Duration::from_secs(120) {
            return Err(BrokerFailure::Expired);
        }
        let token = BrokerToken(format!(
            "{}{}",
            Uuid::new_v4().simple(),
            Uuid::new_v4().simple()
        ));
        let session = Session {
            token_digest: Sha256::digest(token.0.as_bytes()).into(),
            request,
            job,
            expires: Instant::now() + lifetime,
            consumed: AtomicBool::new(false),
            reading: Semaphore::new(1),
        };
        Ok((
            Self {
                session: Arc::new(session),
            },
            token,
        ))
    }

    pub fn router(&self) -> Router {
        Router::new()
            .route("/v1/responses", post(handle))
            .with_state(Arc::clone(&self.session))
    }
}

async fn handle(State(session): State<Arc<Session>>, request: Request) -> Response {
    let mut auth = request.headers().get_all(header::AUTHORIZATION).iter();
    let supplied = auth
        .next()
        .and_then(|v| v.to_str().ok())
        .and_then(|v| v.strip_prefix("Bearer "));
    let authenticated = supplied
        .filter(|s| s.len() == 64)
        .map(|s| {
            let digest: [u8; 32] = Sha256::digest(s.as_bytes()).into();
            bool::from(session.token_digest.ct_eq(&digest))
        })
        .unwrap_or(false);
    if !authenticated || auth.next().is_some() {
        return error(StatusCode::UNAUTHORIZED, "unauthorized");
    }
    if Instant::now() >= session.expires {
        return error(StatusCode::GONE, "expired");
    }
    if session.consumed.load(Ordering::Acquire) {
        return error(StatusCode::CONFLICT, "already_consumed");
    }
    if request.uri().query().is_some()
        || request
            .headers()
            .get_all(header::CONTENT_TYPE)
            .iter()
            .count()
            != 1
        || request
            .headers()
            .get(header::CONTENT_TYPE)
            .and_then(|v| v.to_str().ok())
            != Some("application/json")
        || request.headers().contains_key(header::CONTENT_ENCODING)
    {
        return error(StatusCode::BAD_REQUEST, "invalid_request");
    }
    let Ok(reading) = session.reading.try_acquire() else {
        return error(StatusCode::TOO_MANY_REQUESTS, "busy");
    };
    let deadline = session.expires.min(Instant::now() + Duration::from_secs(2));
    let body = match timeout_at(
        deadline,
        to_bytes(
            request.into_body(),
            crate::pi_admission::MAX_PI_REQUEST_BYTES,
        ),
    )
    .await
    {
        Ok(Ok(body)) => body,
        Ok(Err(_)) => return error(StatusCode::PAYLOAD_TOO_LARGE, "body_bounds"),
        Err(_) => return error(StatusCode::REQUEST_TIMEOUT, "body_timeout"),
    };
    if PiAdmission::new(&session.request).validate(&body).is_err() {
        return error(StatusCode::BAD_REQUEST, "input_mismatch");
    }
    if Instant::now() >= session.expires {
        return error(StatusCode::GONE, "expired");
    }
    if session.consumed.swap(true, Ordering::AcqRel) {
        return error(StatusCode::CONFLICT, "already_consumed");
    }
    drop(reading);
    // Own work independently of this HTTP connection: disconnect must not cancel
    // settlement/persistence and turn a paid result into an automatic retry.
    // Runtime shutdown/deadline still leaves any begun attempt conservatively
    // in-flight/uncertain, recoverable by the durable supervisor journal.
    let job_session = Arc::clone(&session);
    let work =
        tokio::spawn(async move { timeout_at(job_session.expires, job_session.job.run()).await });
    let reply = match work.await {
        Ok(Ok(Ok(reply))) => reply,
        Ok(Ok(Err(reason))) => return failure(reason),
        _ => return failure(BrokerFailure::Uncertain),
    };
    match timeout_at(session.expires, session.job.authorize_delivery()).await {
        Ok(Ok(())) if Instant::now() < session.expires => {}
        Ok(Err(reason)) => return failure(reason),
        _ => return failure(BrokerFailure::Expired),
    }
    let mut response = Response::new(Body::from(reply.wire));
    response.headers_mut().insert(
        header::CONTENT_TYPE,
        "text/event-stream".parse().expect("static header"),
    );
    response.headers_mut().insert(
        header::CACHE_CONTROL,
        "no-store".parse().expect("static header"),
    );
    response
}

fn failure(reason: BrokerFailure) -> Response {
    let (status, code) = match reason {
        BrokerFailure::Denied => (StatusCode::FORBIDDEN, "denied"),
        BrokerFailure::Expired => (StatusCode::GONE, "expired"),
        BrokerFailure::Failed => (StatusCode::BAD_GATEWAY, "failed"),
        BrokerFailure::Uncertain => (StatusCode::BAD_GATEWAY, "uncertain_do_not_resend"),
    };
    error(status, code)
}

fn error(status: StatusCode, code: &'static str) -> Response {
    (
        status,
        [(header::CACHE_CONTROL, "no-store")],
        axum::Json(json!({"error":{"code":code,"message":code}})),
    )
        .into_response()
}
