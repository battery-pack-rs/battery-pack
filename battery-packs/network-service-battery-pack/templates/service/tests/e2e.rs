{%- if downstream != "none" %}
//! Integration tests against a real downstream. Fast, mock-backed tests live as unit tests
//! in `src/routes.rs`; these exercise an actual dependency.

use axum::body::{Body, to_bytes};
use http::{Request, StatusCode};
use tower::ServiceExt;

use {{ crate_name }}::downstream::Store;
use {{ crate_name }}::routes::{self, AppState};

{%- if downstream == "redis" %}
/// Probes for a Docker- or Podman-compatible runtime.
fn container_runtime_available() -> bool {
    ["docker", "podman"].iter().any(|bin| {
        std::process::Command::new(bin)
            .arg("info")
            .output()
            .map(|out| out.status.success())
            .unwrap_or(false)
    })
}

#[tokio::test]
async fn redis_round_trip() {
    if !container_runtime_available() {
        eprintln!("skipping redis_round_trip: no docker or podman runtime available");
        return;
    }

    use testcontainers::runners::AsyncRunner;
    use testcontainers_modules::redis::Redis;

    let node = Redis::default().start().await.expect("start redis");
    let port = node.get_host_port_ipv4(6379).await.expect("redis port");
    let store = Store::connect(&format!("redis://127.0.0.1:{port}"))
        .await
        .expect("connect to redis");
    round_trip(store).await;
}
{%- elif downstream == "http-service" %}
#[tokio::test]
async fn http_downstream_round_trip() {
    // Set DOWNSTREAM_URL to a running service that serves GET/PUT on `/<key>`.
    let Ok(url) = std::env::var("DOWNSTREAM_URL") else {
        eprintln!("skipping real_downstream_round_trip: set DOWNSTREAM_URL to run");
        return;
    };
    let store = Store::connect(&url, std::time::Duration::from_millis(2000)).expect("build client");
    round_trip(store).await;
}
{%- endif %}

async fn round_trip(store: Store) {
    let app = routes::router(AppState { store });

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
{%- endif %}
