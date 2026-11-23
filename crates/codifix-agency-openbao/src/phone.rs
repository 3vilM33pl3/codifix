//! Human custodian. Only the scoped browser supplies this role's share.
use crate::{service::Acknowledgement, transport, *};
use axum::{
    Json, Router,
    body::Bytes,
    extract::{DefaultBodyLimit, State},
    http::{HeaderMap, StatusCode},
    response::{Html, IntoResponse, Response},
    routing::{get, post},
};
use std::sync::Arc;
use zeroize::Zeroizing;

#[derive(Clone, Copy, Debug, Deserialize, Serialize, PartialEq, Eq)]
#[serde(rename_all = "snake_case")]
pub enum Stage {
    Waiting,
    InvalidShare,
    Submitting,
    Accepted,
    Declined,
    Unconfirmed,
}

#[derive(Clone, Debug, Deserialize, Serialize)]
pub struct Pending {
    pub submission: Submission,
    pub stage: Stage,
}

fn load(c: &CustodianConfig) -> Result<CustodianState> {
    let state: CustodianState = protected::load(&c.state_file)?;
    if state.binding != c.target.binding()? {
        return Err(Error::TargetChanged);
    }
    Ok(state)
}

fn http_error(e: Error) -> StatusCode {
    match e {
        Error::Authentication => StatusCode::UNAUTHORIZED,
        Error::InvalidShare => StatusCode::UNPROCESSABLE_ENTITY,
        _ => StatusCode::CONFLICT,
    }
}

async fn request(
    State(c): State<Arc<CustodianConfig>>,
    headers: HeaderMap,
    Json(sub): Json<Submission>,
) -> Response {
    let action = async {
        transport::authenticate(&c.controller_token_file, &headers)?;
        let _lock = protected::StateLock::acquire(&c.state_file)?;
        let mut state = load(&c)?;
        sub.ceremony.validate(&c.target, &c.roster, now())?;
        if state.last_sequence < sub.ceremony.sequence {
            sub.check_progress(
                &transport::verified_status(&c.target).await?,
                &c.custodian_id,
            )?;
            state.last_sequence = sub.ceremony.sequence;
            state.last_challenge = Some(sub.ceremony.challenge.clone());
            state.phone = Some(Pending {
                submission: sub.clone(),
                stage: Stage::Waiting,
            });
            protected::save(&c.state_file, &state)?;
        }
        let pending = state.phone.ok_or(Error::Ceremony)?;
        if pending.submission != sub {
            return Err(Error::Ceremony);
        }
        Ok(match pending.stage {
            Stage::Accepted => Json(Acknowledgement {
                custodian_id: c.custodian_id.clone(),
                challenge: sub.ceremony.challenge,
                sequence: sub.ceremony.sequence,
            })
            .into_response(),
            Stage::Waiting | Stage::InvalidShare | Stage::Submitting => {
                (StatusCode::ACCEPTED, Json(pending.stage)).into_response()
            }
            Stage::Declined => StatusCode::GONE.into_response(),
            Stage::Unconfirmed => StatusCode::CONFLICT.into_response(),
        })
    }
    .await;
    action.unwrap_or_else(|e| http_error(e).into_response())
}

fn authorize(c: &CustodianConfig, headers: &HeaderMap) -> Result<()> {
    transport::authenticate(
        c.phone_token_file.as_ref().ok_or(Error::Configuration)?,
        headers,
    )
}

async fn status(
    State(c): State<Arc<CustodianConfig>>,
    headers: HeaderMap,
) -> std::result::Result<Json<serde_json::Value>, StatusCode> {
    authorize(&c, &headers).map_err(http_error)?;
    let pending = load(&c)
        .map_err(http_error)?
        .phone
        .ok_or(StatusCode::NOT_FOUND)?;
    let ceremony = pending.submission.ceremony;
    Ok(Json(serde_json::json!({
        "installation": c.target.expected.installation_id, "participant":c.custodian_id,
        "attempt":ceremony.challenge, "expires_at":ceremony.expires_at, "stage":pending.stage,
        "accepted_before":pending.submission.expected_progress, "threshold":c.target.expected.threshold,
    })))
}

#[derive(Deserialize)]
#[serde(deny_unknown_fields)]
struct Decision<'a> {
    attempt: &'a str,
    decision: &'a str,
    share: Option<&'a str>,
}

async fn decide(
    State(c): State<Arc<CustodianConfig>>,
    headers: HeaderMap,
    body: Bytes,
) -> StatusCode {
    let result = async {
        authorize(&c, &headers)?;
        let input = Zeroizing::new(body.to_vec());
        drop(body);
        let decision: Decision<'_> = serde_json::from_slice(&input).map_err(|_| Error::Ceremony)?;
        let _lock = protected::StateLock::acquire(&c.state_file)?;
        let mut state = load(&c)?;
        let pending = state.phone.as_mut().ok_or(Error::Ceremony)?;
        if pending.submission.ceremony.challenge != decision.attempt
            || !matches!(pending.stage, Stage::Waiting | Stage::InvalidShare)
        {
            return Err(Error::Ceremony);
        }
        pending
            .submission
            .ceremony
            .validate(&c.target, &c.roster, now())?;
        if decision.decision == "decline" && decision.share.is_none() {
            pending.stage = Stage::Declined;
            protected::save(&c.state_file, &state)?;
            return Ok(());
        }
        if decision.decision != "contribute" {
            return Err(Error::Ceremony);
        }
        let share = decision.share.ok_or(Error::InvalidShare)?.trim();
        let submission = pending.submission.clone();
        pending.stage = Stage::Submitting;
        protected::save(&c.state_file, &state)?;
        let result = transport::submit_share(&c, &submission, share).await;
        state.phone.as_mut().ok_or(Error::Ceremony)?.stage = match result {
            Ok(()) => Stage::Accepted,
            Err(Error::InvalidShare) => Stage::InvalidShare,
            Err(_) => Stage::Unconfirmed,
        };
        protected::save(&c.state_file, &state)?;
        result
    }
    .await;
    result.map_or_else(http_error, |_| StatusCode::NO_CONTENT)
}

pub(crate) fn router(c: CustodianConfig) -> Result<Router> {
    let phone = transport::token(c.phone_token_file.as_ref().ok_or(Error::Configuration)?)?;
    if *phone == *transport::token(&c.controller_token_file)?
        || *phone == *transport::token(&c.target.attestor_token_file)?
    {
        return Err(Error::Configuration);
    }
    Ok(Router::new()
        .route("/v1/ceremony",post(request))
        .route("/unseal",get(|| async { Html(include_str!("phone.html")) }))
        .route("/unseal/status",get(status)).route("/unseal/decision",post(decide))
        .layer(axum::middleware::map_response(|mut response: Response| async {
            let headers = response.headers_mut();
            headers.insert("cache-control","no-store".parse().expect("static header"));
            headers.insert("referrer-policy","no-referrer".parse().expect("static header"));
            headers.insert("x-content-type-options","nosniff".parse().expect("static header"));
            headers.insert("content-security-policy","default-src 'none'; script-src 'self'; style-src 'unsafe-inline'; connect-src 'self'; frame-ancestors 'none'; base-uri 'none'; form-action 'none'".parse().expect("static header"));
            response
        }))
        .route("/unseal/phone.js", get(|| async { ([("content-type","application/javascript"), ("cache-control","no-store")],include_str!("phone.js")) }))
        .layer(DefaultBodyLimit::max(8192)).with_state(Arc::new(c)))
}
