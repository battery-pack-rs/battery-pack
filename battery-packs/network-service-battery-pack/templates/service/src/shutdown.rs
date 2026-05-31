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

/// Resolves once, on the first SIGINT or SIGTERM. Returns the trigger so the caller
/// can measure drain time and record the reason after the server has stopped.
pub async fn shutdown_signal() -> ShutdownReason {
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

/// Waits for a shutdown signal, triggers graceful drain through `drain_tx`, and waits up to
/// `SHUTDOWN_DRAIN_TIMEOUT` for the server task to finish. Records the shutdown metric, including
/// whether the drain completed in time.
pub async fn drain_on_signal(
    server: tokio::task::JoinHandle<()>,
    drain_tx: tokio::sync::oneshot::Sender<()>,
) {
    let reason = shutdown_signal().await;
    tracing::info!(reason = reason.as_str(), "draining");
    let start = Instant::now();
    let _ = drain_tx.send(());

    // Graceful shutdown waits for in-flight requests. A flood of slow requests
    // can still out shutdown, so cap the wait and force exit if it elapses.
    let drained = tokio::time::timeout(SHUTDOWN_DRAIN_TIMEOUT, server).await.is_ok();
    if !drained {
        tracing::warn!(
            timeout_secs = SHUTDOWN_DRAIN_TIMEOUT.as_secs(),
            "drain timed out, forcing shutdown"
        );
    }
    crate::metrics::record_shutdown(reason.as_str(), drained, start.elapsed());
}
