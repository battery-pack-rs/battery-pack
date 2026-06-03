{% if benchmarks %}
//! Benchmarks the router end to end via `tower::oneshot`, without binding a socket.

use axum::body::Body;
use criterion::{Criterion, criterion_group, criterion_main};
use http::Request;
use tower::ServiceExt;

use {{ crate_name }}::store::Store;
use {{ crate_name }}::routes::{self, AppState};

fn state() -> AppState {
    // Set BENCH_DOWNSTREAM_URL to benchmark against a real downstream; unset uses the in-memory store.
    let store = Store::new(std::env::var("BENCH_DOWNSTREAM_URL").ok().as_deref())
        .expect("build benchmark store");
    AppState { store }
}

fn bench_route(
    c: &mut Criterion,
    rt: &tokio::runtime::Runtime,
    app: &axum::Router,
    name: &str,
    make_request: impl Fn() -> Request<Body>,
) {
    c.bench_function(name, |b| {
        b.to_async(rt).iter(|| {
            let app = app.clone();
            let request = make_request();
            async move { app.oneshot(request).await.unwrap() }
        })
    });
}

fn handlers(c: &mut Criterion) {
    let rt = tokio::runtime::Builder::new_multi_thread()
        .enable_all()
        .build()
        .unwrap();
    let app = routes::router(state());

    // /health bypasses the middleware stack; the rest exercise it. set_item/get_item also hit the
    // store, so they reflect BENCH_DOWNSTREAM_URL (in-memory when unset, the second server when set).
    // set_item runs first, so get_item benchmarks a hit rather than a miss.
    bench_route(c, &rt, &app, "health", || {
        Request::get("/health").body(Body::empty()).unwrap()
    });
    bench_route(c, &rt, &app, "set_item", || {
        Request::put("/items/bench").body(Body::from("value")).unwrap()
    });
    bench_route(c, &rt, &app, "get_item", || {
        Request::get("/items/bench").body(Body::empty()).unwrap()
    });
    bench_route(c, &rt, &app, "echo", || {
        Request::post("/echo")
            .header("content-type", "application/json")
            .body(Body::from(r#"{"message":"hi"}"#))
            .unwrap()
    });
}

criterion_group!(benches, handlers);
criterion_main!(benches);
{% endif %}
