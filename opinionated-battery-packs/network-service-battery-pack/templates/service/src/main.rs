{% if dial9 %}
// dial9's allocator wraps the real allocator to add opt-in heap profiling. The hook is a
// passthrough until the memory profiler is installed in main.
{% if allocator == "jemalloc" %}
#[cfg(not(target_env = "msvc"))]
#[global_allocator]
static ALLOC: dial9_tokio_telemetry::memory_profiling::Dial9Allocator<tikv_jemallocator::Jemalloc> =
    dial9_tokio_telemetry::memory_profiling::Dial9Allocator::new(tikv_jemallocator::Jemalloc);
{% elif allocator == "mimalloc" %}
#[global_allocator]
static ALLOC: dial9_tokio_telemetry::memory_profiling::Dial9Allocator<mimalloc::MiMalloc> =
    dial9_tokio_telemetry::memory_profiling::Dial9Allocator::new(mimalloc::MiMalloc);
{% else %}
#[global_allocator]
static ALLOC: dial9_tokio_telemetry::memory_profiling::Dial9Allocator =
    dial9_tokio_telemetry::memory_profiling::Dial9Allocator::system();
{% endif %}
{% else %}
{% if allocator == "jemalloc" %}
// jemalloc reduces fragmentation and allocator contention under the multi-threaded
// runtime. It does not build under MSVC.
#[cfg(not(target_env = "msvc"))]
#[global_allocator]
static ALLOC: tikv_jemallocator::Jemalloc = tikv_jemallocator::Jemalloc;
{% elif allocator == "mimalloc" %}
#[global_allocator]
static ALLOC: mimalloc::MiMalloc = mimalloc::MiMalloc;
{% endif %}
{% endif %}
use clap::Parser;
{% if dial9 %}
use dial9_tokio_telemetry::{self as dial9, Dial9Config};
{% endif %}

use {{ crate_name }}::config::Config;
use {{ crate_name }}::telemetry;
{% if dial9 %}

// from_env reads the DIAL9_* knobs documented at
// https://docs.rs/dial9-tokio-telemetry/latest/dial9_tokio_telemetry/struct.Dial9Config.html#method.from_env
// (and set in dial9.env).
#[dial9::main(config = Dial9Config::from_env)]
{% else %}
#[tokio::main]
{% endif %}
async fn main() -> std::process::ExitCode {
    let config = Config::parse();
    // In-flight logs and metrics are flushed when this guard drops, on exit.
    let _telemetry = telemetry::init_telemetry(&config);
    {% if dial9 %}
    let _memory_profiler = if std::env::var("DIAL9_MEMORY_PROFILE_ENABLED").as_deref() == Ok("true") {
        dial9::memory_profiling::MemoryProfiler::with_defaults()
            .install(dial9::telemetry::TelemetryHandle::current())
            .inspect_err(|e| tracing::warn!("failed to install memory profiler: {e:#}"))
            .ok()
    } else {
        None
    };
    {% endif %}

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
