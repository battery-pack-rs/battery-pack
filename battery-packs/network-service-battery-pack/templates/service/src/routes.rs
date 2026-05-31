{% if server_framework == "axum" %}
//! Axum router, application state, and request handlers.

use std::time::Duration;

use axum::Router;
use axum::extract::{Path, State};
use axum::routing::get;
use http::StatusCode;
use metrique::timers::Timer;
{% if downstream != "none" %}
use anyhow::Context;
{% endif %}

use crate::config::Config;
{% if downstream != "none" %}
use crate::downstream::Store;
{% endif %}
use crate::middleware::{HandlerMetricsGuard, telemetry_layer};

#[derive(Clone)]
pub struct AppState {
    {% if downstream != "none" %}
    pub store: Store,
    {% endif %}
}

{% if downstream == "redis" %}
pub async fn build_state(config: &Config) -> anyhow::Result<AppState> {
    let store = if config.in_memory {
        Store::in_memory()
    } else {
        Store::connect(&config.redis_url)
            .await
            .context("connect to redis")?
    };
    Ok(AppState { store })
}
{% elif downstream == "http-service" %}
pub async fn build_state(config: &Config) -> anyhow::Result<AppState> {
    let store = Store::connect(&config.downstream_url, config.downstream_timeout)
        .context("build downstream client")?;
    Ok(AppState { store })
}
{% else %}
pub async fn build_state(_config: &Config) -> anyhow::Result<AppState> {
    Ok(AppState {})
}
{% endif %}

pub fn router(state: AppState) -> Router {
    let app = Router::new()
        .route("/health", get(health))
        {% if downstream != "none" %}
        .route("/items/{key}", get(get_item).put(set_item))
        {% else %}
        .route("/echo", axum::routing::post(echo))
        {% endif %}
        .with_state(state);

    // `record_metrics` is applied last so it is the outermost layer and records the final
    // status even when an inner layer (timeout, catch-panic) produced the response.
    app
        {% if tower_timeout %}
        .layer(tower_http::timeout::TimeoutLayer::with_status_code(
            StatusCode::REQUEST_TIMEOUT,
            Duration::from_secs(15),
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
        // turn panics into 500s
        .layer(tower_http::catch_panic::CatchPanicLayer::new())
        {% endif %}
        .layer(axum::middleware::from_fn(telemetry_layer))
}

async fn health() -> &'static str {
    "ok"
}

{% if downstream != "none" %}
#[tracing::instrument(skip(state, metrics), fields(key = %key))]
async fn get_item(
    State(state): State<AppState>,
    Path(key): Path<String>,
    mut metrics: HandlerMetricsGuard,
) -> Result<String, StatusCode> {
    let mut timer = Timer::start_now();
    let result = state.store.get(&key).await;
    metrics.downstream_latency = Some(timer.stop());
    match result {
        Ok(Some(value)) => {
            metrics.downstream_success = true;
            metrics.payload_bytes = value.len();
            tracing::info!(found = true, "get_item");
            Ok(value)
        }
        Ok(None) => {
            metrics.downstream_success = true;
            tracing::info!(found = false, "get_item");
            Err(StatusCode::NOT_FOUND)
        }
        Err(e) => {
            metrics.downstream_error_kind = Some(e.kind());
            tracing::error!("downstream call failed: {:#}", anyhow::anyhow!(e));
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
    let mut timer = Timer::start_now();
    let result = state.store.set(&key, &body).await;
    metrics.downstream_latency = Some(timer.stop());
    match result {
        Ok(()) => {
            metrics.downstream_success = true;
            Ok(StatusCode::NO_CONTENT)
        }
        Err(e) => {
            metrics.downstream_error_kind = Some(e.kind());
            tracing::error!("downstream call failed: {:#}", anyhow::anyhow!(e));
            Err(StatusCode::BAD_GATEWAY)
        }
    }
}
{% else %}
/// Echoes the request body back.
#[tracing::instrument(skip_all)]
async fn echo(mut metrics: HandlerMetricsGuard, body: String) -> String {
    metrics.payload_bytes = body.len();
    body
}
{% endif %}

#[cfg(test)]
mod tests {
    use super::*;
    use axum::body::{Body, to_bytes};
    use http::Request;
    use tower::ServiceExt;
    {% if downstream == "http-service" %}
    use httpmock::prelude::*;
    {% endif %}

    {% if downstream == "redis" %}
    fn test_state() -> AppState {
        AppState {
            store: Store::in_memory(),
        }
    }
    {% elif downstream == "http-service" %}
    fn test_state() -> AppState {
        AppState {
            store: Store::connect("http://127.0.0.1:0", Duration::from_millis(1000)).expect("client"),
        }
    }
    {% else %}
    fn test_state() -> AppState {
        AppState {}
    }
    {% endif %}

    #[tokio::test]
    async fn health_ok() {
        let resp = router(test_state())
            .oneshot(Request::get("/health").body(Body::empty()).unwrap())
            .await
            .unwrap();
        assert_eq!(resp.status(), StatusCode::OK);
    }

    /// Installs a thread-local test sink, drives one request, and returns the single emitted
    /// metric entry. `#[tokio::test]` runs on a current-thread runtime, so the thread-local sink
    /// set here is the one the middleware records into.
    async fn capture_metric(app: axum::Router, req: Request<Body>) -> metrique::test_util::TestEntry {
        let metrique::test_util::TestEntrySink { inspector, sink } =
            metrique::test_util::test_entry_sink();
        let _guard = metrique::ServiceMetrics::set_test_sink(sink);
        app.oneshot(req).await.unwrap();
        let mut entries = inspector.entries();
        assert_eq!(entries.len(), 1, "one record per request");
        entries.pop().unwrap()
    }

    #[tokio::test]
    async fn health_emits_metric() {
        let m = capture_metric(
            router(test_state()),
            Request::get("/health").body(Body::empty()).unwrap(),
        )
        .await;
        assert_eq!(m.values["Operation"], "Health");
        assert!(m.metrics["Success"].as_bool());
        assert_eq!(m.metrics["StatusCode"].as_u64(), 200);
    }

    {% if downstream != "none" %}
    #[tracing_test::traced_test]
    #[tokio::test]
    async fn get_item_is_instrumented() {
        // Outcome depends on the downstream; we only assert the handler's span/event fired.
        let _ = router(test_state())
            .oneshot(Request::get("/items/abc").body(Body::empty()).unwrap())
            .await
            .unwrap();
        assert!(logs_contain("get_item"));
    }
    {% endif %}

    {% if downstream == "redis" %}
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
        assert_eq!(m.metrics["StatusCode"].as_u64(), 404);
        assert_eq!(m.values["ErrorKind"], "InvalidRequest");
    }
    {% elif downstream == "http-service" %}
    #[tokio::test]
    async fn get_found() {
        let server = MockServer::start_async().await;
        let mock = server
            .mock_async(|when, then| {
                when.method(GET).path("/k");
                then.status(200).body("v");
            })
            .await;
        let store = Store::connect(&server.base_url(), Duration::from_millis(1000)).expect("client");

        let got = router(AppState { store })
            .oneshot(Request::get("/items/k").body(Body::empty()).unwrap())
            .await
            .unwrap();
        assert_eq!(got.status(), StatusCode::OK);
        let body = to_bytes(got.into_body(), usize::MAX).await.unwrap();
        assert_eq!(&body[..], b"v");
        mock.assert_async().await;
    }

    #[tokio::test]
    async fn get_not_found() {
        let server = MockServer::start_async().await;
        server
            .mock_async(|when, then| {
                when.method(GET).path("/missing");
                then.status(404);
            })
            .await;
        let store = Store::connect(&server.base_url(), Duration::from_millis(1000)).expect("client");

        let got = router(AppState { store })
            .oneshot(Request::get("/items/missing").body(Body::empty()).unwrap())
            .await
            .unwrap();
        assert_eq!(got.status(), StatusCode::NOT_FOUND);
    }

    #[tokio::test]
    async fn found_records_downstream_success() {
        let server = MockServer::start_async().await;
        server
            .mock_async(|when, then| {
                when.method(GET).path("/k");
                then.status(200).body("vv");
            })
            .await;
        let store = Store::connect(&server.base_url(), Duration::from_millis(1000)).expect("client");
        let m = capture_metric(
            router(AppState { store }),
            Request::get("/items/k").body(Body::empty()).unwrap(),
        )
        .await;
        assert!(m.metrics["DownstreamSuccess"].as_bool());
        assert_eq!(m.metrics["PayloadBytes"].as_u64(), 2);
    }

    #[tokio::test]
    async fn downstream_5xx_records_downstream_error() {
        let server = MockServer::start_async().await;
        server
            .mock_async(|when, then| {
                when.method(GET).path("/boom");
                then.status(500);
            })
            .await;
        let store = Store::connect(&server.base_url(), Duration::from_millis(1000)).expect("client");
        let m = capture_metric(
            router(AppState { store }),
            Request::get("/items/boom").body(Body::empty()).unwrap(),
        )
        .await;
        assert_eq!(m.metrics["StatusCode"].as_u64(), 502);
        assert_eq!(m.values["ErrorKind"], "Downstream");
        assert_eq!(m.values["DownstreamErrorKind"], "Downstream");
        assert!(!m.metrics["DownstreamSuccess"].as_bool());
    }
    {% else %}
    #[tokio::test]
    async fn echo_returns_body() {
        let got = router(test_state())
            .oneshot(Request::post("/echo").body(Body::from("hello")).unwrap())
            .await
            .unwrap();
        assert_eq!(got.status(), StatusCode::OK);
        let body = to_bytes(got.into_body(), usize::MAX).await.unwrap();
        assert_eq!(&body[..], b"hello");
    }

    #[tokio::test]
    async fn echo_records_payload() {
        let m = capture_metric(
            router(test_state()),
            Request::post("/echo").body(Body::from("hello")).unwrap(),
        )
        .await;
        assert_eq!(m.values["Operation"], "Echo");
        assert!(m.metrics["Success"].as_bool());
        assert_eq!(m.metrics["PayloadBytes"].as_u64(), 5);
    }
    {% endif %}
}
{% endif %}
