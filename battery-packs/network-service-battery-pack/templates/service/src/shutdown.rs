{% include "snippets/shutdown.rs" %}

use std::time::{Duration, Instant};

/// Waits for a shutdown signal, triggers graceful drain through `drain_tx`, and waits up to
/// `timeout` for the server task to finish. Records the shutdown metric, including whether the
/// drain completed in time.
pub async fn drain_on_signal(
    server: tokio::task::JoinHandle<()>,
    drain_tx: tokio::sync::oneshot::Sender<()>,
    timeout: Duration,
) {
    let reason = shutdown_signal().await;
    tracing::info!(reason = reason.as_str(), "draining");
    let start = Instant::now();
    let _ = drain_tx.send(());

    // Graceful shutdown waits for in-flight requests. A flood of slow requests
    // can still out shutdown, so cap the wait and force exit if it elapses.
    let drained = tokio::time::timeout(timeout, server).await.is_ok();
    if !drained {
        tracing::warn!(timeout_secs = timeout.as_secs(), "drain timed out, forcing shutdown");
    }
    crate::metrics::record_shutdown(reason.as_str(), drained, start.elapsed());
}
