//! Public, static operator shell; every private request uses the existing bearer API.
//!
//! Wiring: export `pub mod operator;` and add
//! `.route("/operator", axum::routing::get(crate::operator::shell))` to the service
//! router. No additional configuration, privilege, database query, external script
//! route, or token embedding is needed. Serve through the normal HTTPS origin.
//!
//! The HTML keeps its manually entered bearer only in the current page's JS
//! closure. Disconnect clears rendered private data and aborts pending fetches.
//! CSP hashes bind the fixed inline script/style; changes automatically produce
//! matching hashes. The browser integration test is explicitly opt-in and uses
//! only a loopback mock API, a synthetic token and locally installed Playwright.
use axum::{
    http::{HeaderValue, StatusCode, header},
    response::{Html, IntoResponse, Response},
};
use base64::{Engine, engine::general_purpose::STANDARD};
use sha2::{Digest, Sha256};

const PAGE: &str = include_str!("operator.html");

fn inline_hash(start: &str, end: &str) -> String {
    // PAGE is a compile-time literal owned by this module, never request data.
    let source = PAGE
        .split_once(start)
        .and_then(|(_, rest)| rest.split_once(end))
        .map(|(source, _)| source)
        .unwrap_or_default();
    STANDARD.encode(Sha256::digest(source.as_bytes()))
}

/// Static shell only. Existing task/control endpoints retain all authorization.
pub async fn shell() -> Response {
    let policy = format!(
        "default-src 'none'; script-src 'sha256-{}'; style-src 'sha256-{}'; connect-src 'self'; img-src 'none'; font-src 'none'; object-src 'none'; base-uri 'none'; form-action 'none'; frame-ancestors 'none'",
        inline_hash("<script>", "</script>"),
        inline_hash("<style>", "</style>")
    );
    // Fail closed if the fixed policy ever becomes an invalid header.
    let Ok(policy) = HeaderValue::from_str(&policy) else {
        return (
            StatusCode::INTERNAL_SERVER_ERROR,
            "Operator policy unavailable.",
        )
            .into_response();
    };
    let mut response = Html(PAGE).into_response();
    response
        .headers_mut()
        .insert(header::CONTENT_SECURITY_POLICY, policy);
    response
        .headers_mut()
        .insert(header::CACHE_CONTROL, HeaderValue::from_static("no-store"));
    response.headers_mut().insert(
        header::REFERRER_POLICY,
        HeaderValue::from_static("no-referrer"),
    );
    response.headers_mut().insert(
        header::X_CONTENT_TYPE_OPTIONS,
        HeaderValue::from_static("nosniff"),
    );
    response
}
