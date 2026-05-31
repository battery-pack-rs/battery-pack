{%- if benchmarks %}
//! Benchmarks the router end to end via `tower::oneshot`, without binding a socket.

use axum::body::Body;
use criterion::{Criterion, criterion_group, criterion_main};
use http::Request;
use tower::ServiceExt;

{%- if downstream != "none" %}
use {{ crate_name }}::downstream::Store;
{%- endif %}
use {{ crate_name }}::routes::{self, AppState};

fn state() -> AppState {
    {%- if downstream == "redis" %}
    AppState {
        store: Store::in_memory(),
    }
    {%- elif downstream == "http-service" %}
    AppState {
        store: Store::connect("http://127.0.0.1:0", std::time::Duration::from_millis(1000))
            .expect("build client"),
    }
    {%- else %}
    AppState {}
    {%- endif %}
}

fn health(c: &mut Criterion) {
    let rt = tokio::runtime::Builder::new_multi_thread()
        .enable_all()
        .build()
        .unwrap();
    let app = routes::router(state());
    c.bench_function("health", |b| {
        b.to_async(&rt).iter(|| {
            let app = app.clone();
            async move {
                app.oneshot(Request::get("/health").body(Body::empty()).unwrap())
                    .await
                    .unwrap()
            }
        })
    });
}

criterion_group!(benches, health);
criterion_main!(benches);
{%- endif %}
