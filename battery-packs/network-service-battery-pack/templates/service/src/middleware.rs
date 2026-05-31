//! Request-metrics middleware and the guard a handler uses to record its own metrics.

use std::sync::{Arc, Mutex};

use axum::extract::{FromRequestParts, Request};
use axum::middleware::Next;
use axum::response::Response;
use http::request::Parts;
use http::{Method, StatusCode};
use metrique::{OnParentDrop, SlotGuard};
use tower_http::request_id::RequestId;

use crate::metrics::{ErrorKind, HandlerMetrics, Operation, RequestMetrics};

/// Records a wide-event metric per request, and exposes a [`HandlerMetricsGuard`] slot for handlers.
/// The request id and tracing span come from the tower-http layers wrapping this one.
pub async fn telemetry_middleware(mut req: Request, next: Next) -> Response {
    let request_id = req
        .extensions()
        .get::<RequestId>()
        .and_then(|id| id.header_value().to_str().ok())
        .unwrap_or_default()
        .to_string();
    let operation = classify_operation(req.method(), req.uri().path());
    // path is captured now, before next.run consumes the request, so it survives a timeout.
    let mut metrics =
        RequestMetrics::init(request_id, operation, req.uri().path().to_string());

    let slot = metrics.handler.open(OnParentDrop::Discard).expect("slot opened more than once");
    req.extensions_mut()
        .insert(HandlerMetricsHandle(Arc::new(Mutex::new(Some(slot)))));

    let response = next.run(req).await;

    let status = response.status();
    metrics.status_code = status.as_u16();
    metrics.client_error = status.is_client_error();
    metrics.server_error = status.is_server_error();
    // A 4xx is the client's fault, not ours, so only a 5xx counts against success.
    metrics.success = !status.is_server_error();
    metrics.error_kind = classify_error(status);
    response
}

/// Clonable into request extensions. The Mutex and Option let a handler take the guard exactly once.
#[derive(Clone)]
struct HandlerMetricsHandle(Arc<Mutex<Option<SlotGuard<HandlerMetrics>>>>);

/// Handler access to [`HandlerMetrics`]. Writes flush into the request's record on drop.
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

/// Unmatched routes record as `Other` so a new endpoint shows up as unclassified until added here.
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

