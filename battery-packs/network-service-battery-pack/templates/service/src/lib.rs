//! Library surface: the service modules plus [`run`], the server orchestration. Tests and
//! benchmarks exercise [`routes::router`] directly; the binary supplies the process-global setup.

pub mod config;
pub mod store;
pub mod metrics;
pub mod middleware;
pub mod routes;
pub mod shutdown;
pub mod telemetry;

{% if dial9 %}
use dial9_tokio_telemetry as dial9;
{% endif %}

use anyhow::Context;

use crate::config::Config;

/// Serves until SIGINT/SIGTERM, then drains in-flight requests and records the shutdown metric.
/// The tracing and metric sinks are installed by the binary and outlive this call.
pub async fn run(config: Config) -> anyhow::Result<()> {
    tracing::info!(?config, "starting up");
    let state = routes::build_state(&config)?;
    let probe_store = state.store.clone();
    let app = routes::router(state);
    let addr = config.socket_addr();
    let listener = tokio::net::TcpListener::bind(addr)
        .await
        .context("bind listener")?;
    tracing::info!(%addr, "listening");

    // Smoke test that downstream is reachable. Only warn on unreachable, to avoid
    // cascading failure.
    {% if dial9 %}dial9::spawn{% else %}tokio::spawn{% endif %}(async move {
        if let Some(result) = probe_store.probe().await {
            match result {
                Ok(()) => tracing::debug!("downstream reachable at startup"),
                Err(e) => tracing::warn!(
                    "downstream unreachable at startup (non-fatal): {:#}",
                    anyhow::anyhow!(e)
                ),
            }
        }
    });

    // Run the server in a task so we hold its JoinHandle: that lets us trigger graceful shutdown
    // and cap how long we wait for in-flight requests to drain before forcing exit.
    let (drain_tx, drain_rx) = tokio::sync::oneshot::channel::<()>();
    let server = {% if dial9 %}dial9::spawn{% else %}tokio::spawn{% endif %}(async move {
        axum::serve(listener, app)
            .with_graceful_shutdown(async move {
                let _ = drain_rx.await;
            })
            .await
            .context("server error")
    });

    shutdown::drain_on_signal(server, drain_tx).await;
    Ok(())
}
