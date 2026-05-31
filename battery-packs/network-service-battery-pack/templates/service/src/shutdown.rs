//! Graceful shutdown signal and drain coordination.

use std::time::{Duration, Instant};

/// How long to let in-flight requests finish before forcing exit. Match this to your deployment's
/// termination grace period (e.g. Kubernetes `terminationGracePeriodSeconds`).
const SHUTDOWN_DRAIN_TIMEOUT: Duration = Duration::from_secs(30);

/// What triggered shutdown. Recorded on the shutdown metric.
#[derive(Clone, Copy)]
pub enum ShutdownReason {
    CtrlC,
    Sigterm,
}

impl ShutdownReason {
    pub fn as_str(self) -> &'static str {
        match self {
            ShutdownReason::CtrlC => "ctrl_c",
            ShutdownReason::Sigterm => "sigterm",
        }
    }
}

/// Resolves on the first SIGINT or SIGTERM, returning the trigger so the caller can record the reason.
pub(crate) async fn shutdown_signal() -> ShutdownReason {
    let ctrl_c = async {
        tokio::signal::ctrl_c()
            .await
            .expect("install SIGINT handler");
    };

    #[cfg(unix)]
    let terminate = async {
        tokio::signal::unix::signal(tokio::signal::unix::SignalKind::terminate())
            .expect("install SIGTERM handler")
            .recv()
            .await;
    };
    #[cfg(not(unix))]
    let terminate = std::future::pending::<()>();

    tokio::select! {
        biased;
        _ = ctrl_c => ShutdownReason::CtrlC,
        _ = terminate => ShutdownReason::Sigterm,
    }
}

/// Drains in-flight requests on a shutdown signal, capped at `SHUTDOWN_DRAIN_TIMEOUT`, then records
/// the shutdown metric (including whether the drain finished in time).
pub async fn drain_on_signal(
    server: tokio::task::JoinHandle<anyhow::Result<()>>,
    drain_tx: tokio::sync::oneshot::Sender<()>,
) {
    let reason = shutdown_signal().await;
    tracing::info!(reason = reason.as_str(), "draining");
    let start = Instant::now();
    let _ = drain_tx.send(());

    // Graceful shutdown waits for in-flight requests. A flood of slow requests can still
    // outlast our shutdown window, so cap the wait and force exit if it elapses. Only a clean
    // finish counts as drained: a server error or panic is a failure, not a graceful drain.
    let abort = server.abort_handle();
    let drained = match tokio::time::timeout(SHUTDOWN_DRAIN_TIMEOUT, server).await {
        Ok(Ok(Ok(()))) => true,
        Ok(Ok(Err(e))) => {
            tracing::error!("server stopped with an error: {e:#}");
            false
        }
        Ok(Err(join_error)) => {
            tracing::error!("server task panicked: {join_error}");
            false
        }
        Err(_elapsed) => {
            tracing::warn!(
                timeout_secs = SHUTDOWN_DRAIN_TIMEOUT.as_secs(),
                "drain timed out, forcing shutdown"
            );
            // Cancel rather than detach, so in-flight work does not outlive this call.
            abort.abort();
            false
        }
    };
    crate::metrics::record_shutdown(reason.as_str(), drained, start.elapsed());
}
