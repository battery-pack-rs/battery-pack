//! Request-metrics middleware and the guard a handler uses to record its own metrics.

use std::sync::{Arc, Mutex};

use axum::extract::{FromRequestParts, Request};
use axum::middleware::Next;
use axum::response::Response;
use http::request::Parts;
use http::{HeaderValue, Method, StatusCode};
use metrique::{OnParentDrop, SlotGuard};
use tracing::Instrument;
use uuid::Uuid;

use crate::metrics::{ErrorKind, HandlerMetrics, Operation, RequestMetrics};

/// Injects request context and metrics for use by handlers, and processes
/// responses into more metrics.
pub async fn telemetry_middleware(mut req: Request, next: Next) -> Response {
    let request_id = Uuid::now_v7().to_string();
    let operation = classify_operation(req.method(), req.uri().path());
    let span = tracing::info_span!("request", %request_id);
    // path is captured now, before next.run consumes the request, so it survives a timeout.
    let mut metrics =
        RequestMetrics::init(request_id.clone(), operation, req.uri().path().to_string());

    let slot = metrics.handler.open(OnParentDrop::Discard).expect("slot opened more than once");
    req.extensions_mut()
        .insert(HandlerMetricsHandle(Arc::new(Mutex::new(Some(slot)))));

    let mut response = next.run(req).instrument(span).await;

    let status = response.status();
    metrics.status_code = status.as_u16();
    metrics.client_error = status.is_client_error();
    metrics.server_error = status.is_server_error();
    // A 4xx is the client's fault, not ours, so only a 5xx counts against success.
    metrics.success = !status.is_server_error();
    metrics.error_kind = classify_error(status);
    if let Ok(value) = HeaderValue::from_str(&request_id) {
        response.headers_mut().insert("x-request-id", value);
    }
    response
}

/// Holds slot guard for handler metrics before it is pulled out of request extensions.
#[derive(Clone)]
struct HandlerMetricsHandle(Arc<Mutex<Option<SlotGuard<HandlerMetrics>>>>);

/// Owned access to the request's [`HandlerMetrics`], obtained in a handler. Writes go to the
/// parent record. The values flush into the request's metric when this drops.
pub struct HandlerMetricsGuard(SlotGuard<HandlerMetrics>);

impl std::ops::Deref for HandlerMetricsGuard {
    type Target = HandlerMetrics;
    fn deref(&self) -> &HandlerMetrics {
        &self.0
    }
}

impl std::ops::DerefMut for HandlerMetricsGuard {
    fn deref_mut(&mut self) -> &mut HandlerMetrics {
        &mut self.0
    }
}

impl<S: Send + Sync> FromRequestParts<S> for HandlerMetricsGuard {
    type Rejection = std::convert::Infallible;

    async fn from_request_parts(parts: &mut Parts, _state: &S) -> Result<Self, Self::Rejection> {
        let guard = parts
            .extensions
            .get::<HandlerMetricsHandle>()
            .and_then(|h| h.0.lock().unwrap_or_else(|e| e.into_inner()).take())
            .expect("HandlerMetricsGuard taken twice, or telemetry_middleware not installed");
        Ok(HandlerMetricsGuard(guard))
    }
}

/// Maps a request to its operation. Unmatched routes record as `Other` rather than folding into an
/// existing operation, so a new endpoint shows up as unclassified until it is added here.
fn classify_operation(method: &Method, path: &str) -> Operation {
    match (method, path) {
        (&Method::POST, "/echo") => Operation::Echo,
        (&Method::GET, p) if p.starts_with("/items/") => Operation::GetItem,
        (&Method::PUT, p) if p.starts_with("/items/") => Operation::SetItem,
        _ => Operation::Other,
    }
}

fn classify_error(status: StatusCode) -> Option<ErrorKind> {
    if status.is_success() {
        return None;
    }
    Some(match status {
        StatusCode::REQUEST_TIMEOUT => ErrorKind::Timeout,
        StatusCode::BAD_GATEWAY | StatusCode::SERVICE_UNAVAILABLE | StatusCode::GATEWAY_TIMEOUT => {
            ErrorKind::Downstream
        }
        s if s.is_client_error() => ErrorKind::InvalidRequest,
        _ => ErrorKind::Internal,
    })
}

