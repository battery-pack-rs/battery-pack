//! Tracing and metrics initialization.

use metrique::ServiceMetrics;
use metrique::json::Json;
use metrique::writer::sink::AttachHandle;
use metrique::writer::{AttachGlobalEntrySinkExt, EntryIoStreamExt, FormatExt};
use metrique_util::{AttachGlobalEntrySinkTokioMetricsExt, TokioRuntimeMetricsConfig};
use tracing_appender::non_blocking::WorkerGuard;
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

/// Installs logs and metrics in one call.
{%- if metrics_output == "disk" %}
/// Both roll into `telemetry_dir` as separate files (`application.log`, `metrics.log`).
{%- else %}
/// Metrics go to stdout and logs to stderr.
{%- endif %}
pub fn init_telemetry(config: &Config) -> TelemetryGuard {
    TelemetryGuard {
        _log: init_tracing(&config.log_level{% if metrics_output == "disk" %}, &config.telemetry_dir{% endif %}),
        _metrics: init_metrics(&config.service_name{% if metrics_output == "disk" %}, &config.telemetry_dir{% endif %}),
    }
}

fn init_tracing(log_level: &str{% if metrics_output == "disk" %}, dir: &std::path::Path{% endif %}) -> WorkerGuard {
    {%- if metrics_output == "disk" %}
    let (writer, guard) = tracing_appender::non_blocking(
        tracing_appender::rolling::RollingFileAppender::new(
            tracing_appender::rolling::Rotation::HOURLY,
            dir,
            "application.log",
        ),
    );
    {%- else %}
    let (writer, guard) = tracing_appender::non_blocking(std::io::stderr());
    {%- endif %}
    let registry = tracing_subscriber::registry()
        .with(tracing_subscriber::EnvFilter::new(log_level))
        .with(tracing_subscriber::fmt::layer().json().with_writer(writer));
    {%- if dial9 %}
    // Filter aggressively: dial9 correlates with other signals and does not need every span.
    // Unfiltered SDK spans can exceed 100k events/s.
    let dial9 = dial9_tokio_telemetry::tracing_layer::Dial9TokioLayer::new().with_filter(
        tracing_subscriber::filter::Targets::new()
            .with_target("{{ crate_name }}", tracing::Level::TRACE)
            .with_default(tracing::Level::ERROR),
    );
    registry.with(dial9).init();
    {%- else %}
    registry.init();
    {%- endif %}
    guard
}

fn init_metrics(service_name: &str{% if metrics_output == "disk" %}, dir: &std::path::Path{% endif %}) -> AttachHandle {
    let handle = ServiceMetrics::attach_to_stream(
        Json::new()
            {%- if metrics_output == "disk" %}
            .output_to_makewriter(tracing_appender::rolling::RollingFileAppender::new(
                tracing_appender::rolling::Rotation::MINUTELY,
                dir,
                "metrics.log",
            ))
            {%- else %}
            .output_to_makewriter(|| std::io::stdout().lock())
            {%- endif %}
            .merge_globals(Globals {
                service_name: service_name.to_string(),
            }),
    );
    ServiceMetrics::subscribe_tokio_runtime_metrics(TokioRuntimeMetricsConfig::default());
    handle
}
