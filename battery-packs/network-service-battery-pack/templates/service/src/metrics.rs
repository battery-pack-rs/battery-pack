//! Wide-event metrics for the service.
//!
//! One [`RequestMetrics`] record is emitted per request. The middleware owns the parent and
//! opens [`HandlerMetrics`] as a slot the handler fills via the `HandlerMetricsHandle` extractor.

use std::time::{Duration, SystemTime};

use metrique::Slot;
use metrique::ServiceMetrics;
use metrique::timers::{Stopwatch, Timer};
use metrique::unit::{Byte, Millisecond};
use metrique::unit_of_work::metrics;
use metrique::writer::Entry;
use metrique::writer::value::ToString;

/// Properties attached to every emitted record.
#[derive(Entry)]
#[entry]
pub struct Globals {
    pub service_name: String,
}

/// The operation a request maps to.
#[metrics(value(string))]
#[derive(Clone, Copy)]
pub enum Operation {
    GetItem,
    SetItem,
    Echo,
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
#[metrics(subfield_owned)]
#[derive(Default)]
pub struct HandlerMetrics {
    /// Bytes of the value read or written, set by the handler.
    #[metrics(unit = Byte)]
    pub payload_bytes: usize,
    /// `None` for operations that make no downstream call.
    pub downstream_success: Option<bool>,
    /// Whether a lookup found the key. `None` for operations that do not look one up.
    pub found: Option<bool>,
    pub downstream_error_kind: Option<ErrorKind>,
    /// The downstream failure chain, recorded as a property for debugging.
    pub downstream_error: Option<String>,
    /// Duration of the downstream call, if started.
    #[metrics(unit = Millisecond)]
    pub downstream_duration: Stopwatch,
}

/// Per-request wide event. The middleware owns the guard and flushes it on drop.
#[metrics(rename_all = "PascalCase")]
pub struct RequestMetrics {
    pub request_id: String,
    pub operation: Operation,
    /// Request path, recorded as a property (high-cardinality: it contains the key).
    pub path: String,
    /// The start time of the request (from when our router saw it)
    #[metrics(timestamp)]
    pub timestamp: SystemTime,
    /// The full foreground duration of the request
    #[metrics(unit = Millisecond)]
    pub duration: Timer,
    /// True unless the service itself failed (5xx). A 4xx is a client problem, still a success.
    pub success: bool,
    /// If the request was a 4xx error
    pub client_error: bool,
    /// If the request was a 5xx error
    pub server_error: bool,
    /// Rendered via `ToString` so it is recorded as a string property, not a numeric metric.
    #[metrics(format = ToString)]
    pub status_code: u16,
    pub error_kind: Option<ErrorKind>,
    #[metrics(flatten)]
    pub handler: Slot<HandlerMetrics>,
}

impl RequestMetrics {
    /// Opens a record bound to the global sink. The duration timer starts now, so it must be
    /// called at the middleware boundary, not inside the handler.
    pub fn init(request_id: String, operation: Operation) -> RequestMetricsGuard {
        RequestMetrics {
            request_id,
            operation,
            path: String::new(),
            timestamp: SystemTime::now(),
            duration: Timer::start_now(),
            success: false,
            client_error: false,
            server_error: false,
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
