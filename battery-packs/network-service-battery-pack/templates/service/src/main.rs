{% include "snippets/allocator.rs" %}
use clap::Parser;

use {{ crate_name }}::config::Config;
use {{ crate_name }}::telemetry;
{% if dial9 %}

fn dial9_config() -> dial9_tokio_telemetry::Dial9Config {
    dial9_tokio_telemetry::Dial9Config::from_env()
}

#[dial9_tokio_telemetry::main(config = dial9_config)]
{% else %}

#[tokio::main]
{% endif %}
async fn main() -> std::process::ExitCode {
    let config = Config::parse();
    // In-flight logs and metrics are flushed when this guard drops, on exit.
    let _telemetry = telemetry::init_telemetry(&config);

    let code: u8 = match {{ crate_name }}::run(config).await {
        Ok(()) => 0,
        Err(error) => {
            tracing::error!("service exited with error: {error:#}");
            1
        }
    };
    {{ crate_name }}::metrics::record_process_exit(code);
    std::process::ExitCode::from(code)
}
