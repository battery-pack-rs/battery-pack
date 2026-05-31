//! Configuration from CLI args and environment variables.

use std::net::SocketAddr;
{% if metrics_output == "disk" %}
use std::path::PathBuf;
{% endif %}
use std::time::Duration;

use clap::Parser;

/// Parses a millisecond count into a `Duration`.
fn parse_millis(s: &str) -> Result<Duration, std::num::ParseIntError> {
    s.parse().map(Duration::from_millis)
}

#[derive(Debug, Clone, Parser)]
pub struct Config {
    /// Address the HTTP server binds to.
    #[arg(long, env = "BIND_ADDR", default_value = "127.0.0.1:3000")]
    pub bind_addr: SocketAddr,

    /// `tracing` filter directive, e.g. `info,{{ crate_name }}=debug`.
    #[arg(long, env = "RUST_LOG", default_value = "info,{{ crate_name }}=debug")]
    pub log_level: String,

    /// Service name attached to every metric record.
    #[arg(long, env = "SERVICE_NAME", default_value = "{{ project_name }}")]
    pub service_name: String,
    {% if metrics_output == "disk" %}

    /// Directory for rolling log and metric files.
    #[arg(long, env = "TELEMETRY_DIR", default_value = "./telemetry")]
    pub telemetry_dir: PathBuf,
    {% endif %}

    /// How long to let in-flight requests drain before forcing shutdown, in milliseconds.
    #[arg(long, env = "SHUTDOWN_DRAIN_TIMEOUT_MS", value_parser = parse_millis, default_value = "30000")]
    pub shutdown_drain_timeout: Duration,
    {% if downstream == "redis" %}

    /// Redis connection URL. Use `--in-memory` to skip Redis entirely.
    #[arg(long, env = "REDIS_URL", default_value = "redis://127.0.0.1:6379")]
    pub redis_url: String,

    /// Back the store with an in-memory map instead of Redis (for local runs and tests).
    #[arg(long, env = "IN_MEMORY", default_value_t = false)]
    pub in_memory: bool,
    {% elif downstream == "http-service" %}

    /// Base URL of the downstream HTTP service.
    #[arg(long, env = "DOWNSTREAM_URL", default_value = "http://127.0.0.1:3001")]
    pub downstream_url: String,

    /// Per-request timeout for the downstream call, in milliseconds.
    #[arg(long, env = "DOWNSTREAM_TIMEOUT_MS", value_parser = parse_millis, default_value = "1000")]
    pub downstream_timeout: Duration,
    {% endif %}
}
