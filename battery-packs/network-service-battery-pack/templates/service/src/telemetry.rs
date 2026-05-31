//! Tracing and metrics initialization.

use std::path::Path;

use metrique::ServiceMetrics;
use metrique::json::Json;
use metrique::writer::sink::AttachHandle;
use metrique::writer::{AttachGlobalEntrySinkExt, EntryIoStreamExt, FormatExt};
use metrique_util::{AttachGlobalEntrySinkTokioMetricsExt, TokioRuntimeMetricsConfig};
use tracing_appender::non_blocking::WorkerGuard;
use tracing_appender::rolling::{RollingFileAppender, Rotation};
use tracing_subscriber::prelude::*;

use crate::config::Config;
use crate::metrics::Globals;

/// Keeps telemetry alive. Dropping it flushes buffered logs and detaches the metric sink, so it
/// must outlive request handling.
#[must_use = "dropping the guard stops log flushing and detaches the metric sink"]
pub struct TelemetryGuard {
    _log: WorkerGuard,
    _metrics: AttachHandle,
}

/// Installs logs and metrics in one call. With `telemetry_dir` set, both roll into files there;
/// otherwise logs go to stderr and metrics to stdout, on separate streams.
pub fn init_telemetry(config: &Config) -> TelemetryGuard {
    TelemetryGuard {
        _log: init_tracing(&config.log_level, config.telemetry_dir.as_deref()),
        _metrics: init_metrics(&config.service_name, config.telemetry_dir.as_deref()),
    }
}

fn init_tracing(log_level: &str, telemetry_dir: Option<&Path>) -> WorkerGuard {
    // Only the writer differs by destination; `non_blocking` erases its type, so both arms yield
    // the same handle and the rest of the setup is shared.
    let (writer, guard) = match telemetry_dir {
        Some(dir) => tracing_appender::non_blocking(RollingFileAppender::new(
            Rotation::HOURLY,
            dir,
            "application.log",
        )),
        None => tracing_appender::non_blocking(std::io::stderr()),
    };
    let registry = tracing_subscriber::registry()
        .with(tracing_subscriber::EnvFilter::new(log_level))
        .with(tracing_subscriber::fmt::layer().json().with_writer(writer));
    {% if dial9 %}
    // Filter aggressively: dial9 correlates with other signals and does not need every span.
    // Unfiltered SDK spans can exceed 100k events/s.
    let dial9 = dial9_tokio_telemetry::tracing_layer::Dial9TokioLayer::new().with_filter(
        tracing_subscriber::filter::Targets::new()
            .with_target("{{ crate_name }}", tracing::Level::TRACE)
            .with_default(tracing::Level::ERROR),
    );
    registry.with(dial9).init();
    {% else %}
    registry.init();
    {% endif %}
    guard
}

fn init_metrics(service_name: &str, telemetry_dir: Option<&Path>) -> AttachHandle {
    let globals = Globals {
        service_name: service_name.to_string(),
    };
    // The stream type depends on the writer, so build it (with the shared globals) inside each arm.
    let handle = match telemetry_dir {
        Some(dir) => ServiceMetrics::attach_to_stream(
            Json::new()
                .output_to_makewriter(RollingFileAppender::new(
                    Rotation::MINUTELY,
                    dir,
                    "metrics.log",
                ))
                .merge_globals(globals),
        ),
        None => ServiceMetrics::attach_to_stream(
            Json::new()
                .output_to_makewriter(|| std::io::stdout().lock())
                .merge_globals(globals),
        ),
    };
    ServiceMetrics::subscribe_tokio_runtime_metrics(TokioRuntimeMetricsConfig::default());
    handle
}
