{% if server_framework == "axum" %}
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

/// Open a per-request metric, and passes a slot via extensions for the handler
/// to write additional fields to. The request metric will be flushed whether
/// or not the handler is reached.
pub async fn telemetry_layer(mut req: Request, next: Next) -> Response {
    let request_id = Uuid::now_v7().to_string();
    let operation = classify_operation(req.method(), req.uri().path());
    let span = tracing::info_span!("request", %request_id);
    let mut metrics = RequestMetrics::open(request_id.clone(), operation);

    let slot = metrics.handler.open(OnParentDrop::Discard).expect("slot opened once");
    req.extensions_mut()
        .insert(HandlerMetricsHandle(Arc::new(Mutex::new(Some(slot)))));

    let mut response = next.run(req).instrument(span).await;

    let status = response.status();
    metrics.status_code = status.as_u16();
    metrics.success = status.is_success();
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
            .expect("HandlerMetricsGuard taken twice, or telemetry_layer middleware not installed");
        Ok(HandlerMetricsGuard(guard))
    }
}

fn classify_operation(method: &Method, path: &str) -> Operation {
    match path {
        "/health" => Operation::Health,
        "/echo" => Operation::Echo,
        _ if method == Method::GET => Operation::GetItem,
        _ => Operation::SetItem,
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

{% endif %}
