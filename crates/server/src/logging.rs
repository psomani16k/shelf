use std::{net::SocketAddr, time::Instant};

use axum::{
    body::HttpBody,
    extract::{ConnectInfo, OriginalUri, Request},
    http::{HeaderMap, header},
    middleware::Next,
    response::Response,
};
use tracing::{Instrument, info, info_span};
use tracing_subscriber::EnvFilter;

/// One JSON object per line on stdout. Level comes from `RUST_LOG`, default `info`.
pub fn init() {
    let filter = EnvFilter::try_from_default_env().unwrap_or_else(|_| EnvFilter::new("info"));
    tracing_subscriber::fmt()
        .json()
        .flatten_event(true)
        .with_current_span(true)
        .with_span_list(false)
        .with_env_filter(filter)
        .init();
}

/// Writes the `access` line for each request, and runs the request inside a span carrying its
/// `request_id`, so every log line it causes can be matched to it.
pub async fn access_log(
    ConnectInfo(peer): ConnectInfo<SocketAddr>,
    OriginalUri(uri): OriginalUri,
    req: Request,
    next: Next,
) -> Response {
    let start = Instant::now();
    let request_id = header_str(req.headers(), "x-request-id").to_owned();
    let method = req.method().clone();
    let user_agent = header_str(req.headers(), header::USER_AGENT.as_str()).to_owned();
    let req_bytes = content_length(req.headers());

    let span = info_span!("request", request_id = %request_id);
    let response = next.run(req).instrument(span).await;

    let resp_bytes = response
        .body()
        .size_hint()
        .exact()
        .unwrap_or_else(|| content_length(response.headers()));
    info!(
        target: "access",
        request_id,
        method = %method,
        path = uri.path(),
        status = response.status().as_u16(),
        latency_ms = start.elapsed().as_secs_f64() * 1000.0,
        req_bytes,
        resp_bytes,
        // The direct peer: behind a reverse proxy this is the proxy until trusted proxies are configured.
        client_ip = %peer.ip(),
        user_agent,
    );
    response
}

fn header_str<'a>(headers: &'a HeaderMap, name: &str) -> &'a str {
    headers
        .get(name)
        .and_then(|v| v.to_str().ok())
        .unwrap_or("")
}

fn content_length(headers: &HeaderMap) -> u64 {
    header_str(headers, header::CONTENT_LENGTH.as_str())
        .parse()
        .unwrap_or(0)
}
