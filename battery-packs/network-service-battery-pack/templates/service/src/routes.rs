//! Axum router, application state, and request handlers.

use axum::Router;
use axum::Json;
use axum::extract::{Path, State};
use axum::routing::{get, post};
use http::StatusCode;
use serde::{Deserialize, Serialize};

use crate::config::Config;
use crate::downstream::Store;
use crate::middleware::{HandlerMetricsGuard, telemetry_middleware};
{% if rate_limit %}
use tower_governor::GovernorLayer;
use tower_governor::governor::GovernorConfigBuilder;
use tower_governor::key_extractor::GlobalKeyExtractor;
{% endif %}

{% if tower_timeout %}
/// Requests slower than this are aborted with 408 so a stalled handler cannot pin a connection.
const REQUEST_TIMEOUT: std::time::Duration = std::time::Duration::from_secs(15);
{% endif %}
{% if rate_limit %}
/// Global ingress cap: one shared bucket allows bursts up to `RATE_LIMIT_BURST`, refilled at
/// `RATE_LIMIT_PER_SECOND` per second. See the service-architecture skill to switch to per-client.
const RATE_LIMIT_BURST: u32 = 50;
const RATE_LIMIT_PER_SECOND: u64 = 20;
{% endif %}

/// State information shared across routes.
///
/// Fields must be cheaply clonable: axum clones this for every request.
#[derive(Clone)]
pub struct AppState {
    pub store: Store,
}

pub fn build_state(config: &Config) -> anyhow::Result<AppState> {
    let store = Store::new(config.downstream_url.as_deref())?;
    Ok(AppState { store })
}

pub fn router(state: AppState) -> Router {
    {% if rate_limit %}
    let governor = GovernorConfigBuilder::default()
        .key_extractor(GlobalKeyExtractor)
        .per_second(RATE_LIMIT_PER_SECOND)
        .burst_size(RATE_LIMIT_BURST)
        .finish()
        .expect("valid rate-limit config");
    {% endif %}
    // Instrumented routes carry the full middleware stack.
    let app = Router::new()
        .route("/items/{key}", get(get_item).put(set_item))
        .route("/echo", post(echo))
        {% if tower_timeout %}
        .layer(tower_http::timeout::TimeoutLayer::with_status_code(
            StatusCode::REQUEST_TIMEOUT,
            REQUEST_TIMEOUT,
        ))
        {% endif %}
        {% if tower_on_early_drop %}
        .layer(
            tower_http::on_early_drop::OnEarlyDropLayer::builder()
                .on_future_drop(|_req: &axum::extract::Request| {
                    || tracing::warn!("response future dropped before completion")
                })
                .on_body_drop(tower_http::on_early_drop::OnBodyDropFn::new(
                    |_req: &axum::extract::Request| {
                        |_parts: &http::response::Parts| {
                            || tracing::warn!("response body dropped before completion")
                        }
                    },
                )),
        )
        {% endif %}
        {% if tower_catch_panic %}
        // turn panics into 500 responses
        .layer(tower_http::catch_panic::CatchPanicLayer::new())
        {% endif %}
        {% if rate_limit %}
        // Applied inside telemetry_middleware so a rejected (429) request is still recorded as a metric.
        .layer(GovernorLayer::new(governor))
        {% endif %}
        // telemetry_middleware is applied last so it is the outermost layer and records the final status
        // even when an inner layer (timeout, catch-panic) produced the response.
        .layer(axum::middleware::from_fn(telemetry_middleware));

    // /health bypasses the middleware stack
    Router::new()
        .route("/health", get(health))
        .merge(app)
        .with_state(state)
}

async fn health() -> &'static str {
    "ok"
}

#[tracing::instrument(skip(state, metrics), fields(key = %key))]
async fn get_item(
    State(state): State<AppState>,
    Path(key): Path<String>,
    mut metrics: HandlerMetricsGuard,
) -> Result<String, StatusCode> {
    let result = {
        let _timing = metrics.downstream_duration.start();
        state.store.get(&key).await
    };
    match result {
        Ok(Some(value)) => {
            metrics.downstream_success = Some(true);
            metrics.found = Some(true);
            metrics.payload_bytes = value.len();
            tracing::debug!(found = true, "get_item");
            Ok(value)
        }
        Ok(None) => {
            metrics.downstream_success = Some(true);
            metrics.found = Some(false);
            tracing::debug!(found = false, "get_item");
            Err(StatusCode::NOT_FOUND)
        }
        Err(e) => {
            metrics.downstream_error_kind = Some(e.kind());
            let err = anyhow::anyhow!(e);
            metrics.downstream_error = Some(format!("{err:#}"));
            tracing::error!("downstream call failed: {err:#}");
            Err(StatusCode::BAD_GATEWAY)
        }
    }
}

#[tracing::instrument(skip(state, metrics, body), fields(key = %key))]
async fn set_item(
    State(state): State<AppState>,
    Path(key): Path<String>,
    mut metrics: HandlerMetricsGuard,
    body: String,
) -> Result<StatusCode, StatusCode> {
    metrics.payload_bytes = body.len();
    let result = {
        let _timing = metrics.downstream_duration.start();
        state.store.set(&key, &body).await
    };
    match result {
        Ok(()) => {
            metrics.downstream_success = Some(true);
            Ok(StatusCode::NO_CONTENT)
        }
        Err(e) => {
            metrics.downstream_error_kind = Some(e.kind());
            let err = anyhow::anyhow!(e);
            metrics.downstream_error = Some(format!("{err:#}"));
            tracing::error!("downstream call failed: {err:#}");
            Err(StatusCode::BAD_GATEWAY)
        }
    }
}

#[derive(Deserialize)]
pub struct EchoRequest {
    pub message: String,
}

#[derive(Serialize)]
pub struct EchoResponse {
    pub message: String,
}

/// Echoes a JSON body back. A malformed body is rejected with 422.
#[tracing::instrument(skip_all)]
async fn echo(mut metrics: HandlerMetricsGuard, Json(req): Json<EchoRequest>) -> Json<EchoResponse> {
    metrics.payload_bytes = req.message.len();
    Json(EchoResponse {
        message: req.message,
    })
}

#[cfg(test)]
mod tests {
    use super::*;
    use axum::body::{Body, to_bytes};
    use http::Request;
    use tower::ServiceExt;

    fn test_state() -> AppState {
        AppState {
            store: Store::InMemory(Default::default()),
        }
    }

    /// Installs a thread-local test sink, drives one request, and returns the single emitted
    /// metric entry. `#[tokio::test]` runs on a current-thread runtime, so the thread-local sink
    /// set here is the one the middleware records into.
    async fn capture_metric(
        app: axum::Router,
        req: Request<Body>,
    ) -> metrique::test_util::TestEntry {
        let metrique::test_util::TestEntrySink { inspector, sink } =
            metrique::test_util::test_entry_sink();
        let _guard = metrique::ServiceMetrics::set_test_sink(sink);
        app.oneshot(req).await.unwrap();
        let mut entries = inspector.entries();
        assert_eq!(entries.len(), 1, "one record per request");
        entries.pop().unwrap()
    }

    #[tokio::test]
    async fn health_ok() {
        let resp = router(test_state())
            .oneshot(Request::get("/health").body(Body::empty()).unwrap())
            .await
            .unwrap();
        assert_eq!(resp.status(), StatusCode::OK);
    }

    #[tokio::test]
    async fn health_emits_no_metric() {
        let metrique::test_util::TestEntrySink { inspector, sink } =
            metrique::test_util::test_entry_sink();
        let _guard = metrique::ServiceMetrics::set_test_sink(sink);
        router(test_state())
            .oneshot(Request::get("/health").body(Body::empty()).unwrap())
            .await
            .unwrap();
        assert!(inspector.entries().is_empty(), "/health bypasses the telemetry layer");
    }

    #[tracing_test::traced_test]
    #[tokio::test]
    async fn get_item_is_instrumented() {
        let _ = router(test_state())
            .oneshot(Request::get("/items/abc").body(Body::empty()).unwrap())
            .await
            .unwrap();
        assert!(logs_contain("get_item"));
    }

    #[tokio::test]
    async fn set_then_get_returns_value() {
        let app = router(test_state());
        let put = app
            .clone()
            .oneshot(Request::put("/items/k").body(Body::from("v")).unwrap())
            .await
            .unwrap();
        assert_eq!(put.status(), StatusCode::NO_CONTENT);

        let got = app
            .oneshot(Request::get("/items/k").body(Body::empty()).unwrap())
            .await
            .unwrap();
        assert_eq!(got.status(), StatusCode::OK);
        let body = to_bytes(got.into_body(), usize::MAX).await.unwrap();
        assert_eq!(&body[..], b"v");
    }

    #[tokio::test]
    async fn get_missing_is_not_found() {
        let got = router(test_state())
            .oneshot(Request::get("/items/nope").body(Body::empty()).unwrap())
            .await
            .unwrap();
        assert_eq!(got.status(), StatusCode::NOT_FOUND);
    }

    #[tokio::test]
    async fn set_records_payload_and_downstream_success() {
        let m = capture_metric(
            router(test_state()),
            Request::put("/items/k").body(Body::from("hello")).unwrap(),
        )
        .await;
        assert_eq!(m.values["Operation"], "SetItem");
        assert!(m.metrics["DownstreamSuccess"].as_bool());
        assert_eq!(m.metrics["PayloadBytes"].as_u64(), 5);
    }

    #[tokio::test]
    async fn missing_key_records_invalid_request() {
        let m = capture_metric(
            router(test_state()),
            Request::get("/items/nope").body(Body::empty()).unwrap(),
        )
        .await;
        assert_eq!(m.values["StatusCode"], "404");
        assert_eq!(m.values["ErrorKind"], "InvalidRequest");
        assert!(m.metrics["ClientError"].as_bool());
        assert!(!m.metrics["ServerError"].as_bool());
        assert!(m.metrics["Success"].as_bool()); // a 4xx is the client's fault, still a success
        assert_eq!(m.values["Path"], "/items/nope");
        assert!(!m.metrics["Found"].as_bool());
    }

    #[tokio::test]
    async fn echo_returns_json() {
        let got = router(test_state())
            .oneshot(
                Request::post("/echo")
                    .header("content-type", "application/json")
                    .body(Body::from(r#"{"message":"hi"}"#))
                    .unwrap(),
            )
            .await
            .unwrap();
        assert_eq!(got.status(), StatusCode::OK);
        let body = to_bytes(got.into_body(), usize::MAX).await.unwrap();
        assert_eq!(&body[..], br#"{"message":"hi"}"#);
    }

    #[tokio::test]
    async fn echo_records_payload() {
        let m = capture_metric(
            router(test_state()),
            Request::post("/echo")
                .header("content-type", "application/json")
                .body(Body::from(r#"{"message":"hello"}"#))
                .unwrap(),
        )
        .await;
        assert_eq!(m.values["Operation"], "Echo");
        assert!(m.metrics["Success"].as_bool());
        assert_eq!(m.metrics["PayloadBytes"].as_u64(), 5);
    }
}
