{% if benchmarks %}
//! Benchmarks the router end to end via `tower::oneshot`, without binding a socket.

use axum::body::Body;
use criterion::{Criterion, criterion_group, criterion_main};
use http::Request;
use tower::ServiceExt;

use {{ crate_name }}::downstream::Store;
use {{ crate_name }}::routes::{self, AppState};

fn state() -> AppState {
    AppState {
        store: Store::InMemory(Default::default()),
    }
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
{% endif %}
