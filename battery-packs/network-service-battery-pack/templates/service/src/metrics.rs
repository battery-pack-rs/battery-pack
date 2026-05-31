//! Wide-event metrics for the service.
//!
//! One [`RequestMetrics`] record is emitted per request. The middleware owns the parent and
//! opens [`HandlerMetrics`] as a slot the handler fills via the `HandlerMetricsHandle` extractor.

use std::time::{Duration, SystemTime};

use metrique::Slot;
use metrique::ServiceMetrics;
use metrique::timers::Timer;
use metrique::unit::{Byte, Millisecond};
use metrique::unit_of_work::metrics;
use metrique::writer::Entry;

/// Properties attached to every emitted record.
#[derive(Entry)]
#[entry]
pub struct Globals {
    pub service_name: String,
}

/// The operation a request maps to. `value(string)` keeps metric cardinality bounded.
#[metrics(value(string))]
#[derive(Clone, Copy)]
pub enum Operation {
    GetItem,
    SetItem,
    Echo,
    Health,
}

/// Why a request or downstream call failed.
#[metrics(value(string))]
#[derive(Clone, Copy)]
pub enum ErrorKind {
    Timeout,
    Downstream,
    InvalidRequest,
    Internal,
}

/// Everything below the middleware records here. The middleware opens it as a slot and the
/// handler writes to it through the `HandlerMetricsHandle` extractor.
#[metrics(subfield)]
#[derive(Default)]
pub struct HandlerMetrics {
    /// Bytes of the value read or written, set by the handler.
    #[metrics(unit = Byte)]
    pub payload_bytes: usize,
    pub downstream_success: bool,
    pub downstream_error_kind: Option<ErrorKind>,
    /// `None` when no downstream call was made.
    #[metrics(unit = Millisecond)]
    pub downstream_latency: Option<Duration>,
}

/// Per-request wide event. The middleware owns the guard and flushes it on drop.
#[metrics(rename_all = "PascalCase")]
pub struct RequestMetrics {
    pub request_id: String,
    pub operation: Operation,
    #[metrics(timestamp)]
    pub timestamp: SystemTime,
    #[metrics(unit = Millisecond)]
    pub duration: Timer,
    pub success: bool,
    pub status_code: u16,
    pub error_kind: Option<ErrorKind>,
    #[metrics(flatten)]
    pub handler: Slot<HandlerMetrics>,
}

impl RequestMetrics {
    /// Opens a record bound to the global sink. The duration timer starts now, so it must be
    /// called at the middleware boundary, not inside the handler.
    pub fn open(request_id: String, operation: Operation) -> RequestMetricsGuard {
        RequestMetrics {
            request_id,
            operation,
            timestamp: SystemTime::now(),
            duration: Timer::start_now(),
            success: false,
            status_code: 0,
            error_kind: None,
            handler: Slot::default(),
        }
        .append_on_drop(ServiceMetrics::sink_or_discard())
    }
}

/// Emitted once when the process drains and exits.
#[metrics(rename_all = "PascalCase")]
pub struct ShutdownMetrics {
    #[metrics(timestamp)]
    pub timestamp: SystemTime,
    pub reason: &'static str,
    pub drained: bool,
    #[metrics(unit = Millisecond)]
    pub drain_duration: Duration,
}

/// Records the shutdown metric as the process drains.
pub fn record_shutdown(reason: &'static str, drained: bool, drain_duration: Duration) {
    ShutdownMetrics {
        timestamp: SystemTime::now(),
        reason,
        drained,
        drain_duration,
    }
    .append_on_drop(ServiceMetrics::sink_or_discard());
}

/// Emitted once as the process exits, carrying the exit code (0 = success).
#[metrics(rename_all = "PascalCase")]
pub struct ProcessExitMetrics {
    #[metrics(timestamp)]
    pub timestamp: SystemTime,
    pub exit_code: u8,
}

/// Records the process-exit metric just before the binary returns its exit code.
pub fn record_process_exit(exit_code: u8) {
    ProcessExitMetrics {
        timestamp: SystemTime::now(),
        exit_code,
    }
    .append_on_drop(ServiceMetrics::sink_or_discard());
}
