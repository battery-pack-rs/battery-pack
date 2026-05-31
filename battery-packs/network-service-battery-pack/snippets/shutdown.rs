//! Graceful shutdown signal, shared across service types.

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
