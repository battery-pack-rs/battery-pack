//! Library surface: the service modules plus [`run`], the server orchestration. Tests and
//! benchmarks exercise [`routes::router`] directly; the binary supplies the process-global setup.

pub mod config;
{%- if downstream != "none" %}
pub mod downstream;
{%- endif %}
pub mod metrics;
pub mod middleware;
pub mod routes;
pub mod shutdown;
pub mod telemetry;

{%- if dial9 %}
use dial9_tokio_telemetry as dial9;
{%- endif %}

use anyhow::Context;

use crate::config::Config;

/// Serves until SIGINT/SIGTERM, then drains in-flight requests and records the shutdown metric.
/// The tracing and metric sinks are installed by the binary and outlive this call.
pub async fn run(config: Config) -> anyhow::Result<()> {
    let app = routes::router(routes::build_state(&config).await?);
    let listener = tokio::net::TcpListener::bind(config.bind_addr)
        .await
        .context("bind listener")?;
    tracing::info!(addr = %config.bind_addr, "listening");

    // Run the accept loop as its own task so the runtime schedules it like any other work
    //{% if dial9 %} and dial9 records it{% endif %}. Shutdown coordination lives in `shutdown`.
    let (drain_tx, drain_rx) = tokio::sync::oneshot::channel::<()>();
    let server = {% if dial9 %}dial9::spawn{% else %}tokio::spawn{% endif %}(async move {
        axum::serve(listener, app)
            .with_graceful_shutdown(async move {
                let _ = drain_rx.await;
            })
            .await
            .expect("server error");
    });

    shutdown::drain_on_signal(server, drain_tx, config.shutdown_drain_timeout).await;
    Ok(())
}
