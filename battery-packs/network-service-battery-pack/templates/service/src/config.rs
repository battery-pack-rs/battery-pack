//! Configuration from CLI args and environment variables.

use std::net::SocketAddr;
use std::path::PathBuf;

use clap::Parser;

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

    /// Directory for rolling log and metric files. When unset, logs go to stderr and metrics to
    /// stdout.
    #[arg(long, env = "TELEMETRY_DIR")]
    pub telemetry_dir: Option<PathBuf>,

    /// Base URL of a downstream service to forward items to. When omitted, items are kept in memory.
    #[arg(long, env = "DOWNSTREAM_URL")]
    pub downstream_url: Option<String>,
}
