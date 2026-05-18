---
name: application-errors
description: Propagating and formatting errors in Rust applications with anyhow
---

# Application Errors

Propagating and formatting errors in binaries, CLI tools, and services.

> **Prerequisites:** This skill references examples from the error battery pack.
> ```sh
> cargo install cargo-bp   # install the battery-pack CLI
> cargo bp add error       # add anyhow + thiserror to your project
> ```

## main() Return Type

For CLIs and tools, return `anyhow::Result` from `main()` to get automatic error printing:

```rust
fn main() -> anyhow::Result<()> {
    let config = load_config()?;
    run(config)
}
```

For services or when you want control over error formatting, handle errors explicitly:

```rust
fn main() {
    if let Err(e) = run() {
        eprintln!("{e:?}");  // full chain with backtrace
        std::process::exit(1);
    }
}
```

## Context Chains with anyhow

When propagating errors, attach context describing what operation was being attempted (not what went wrong, since the source error already says that):

```rust
use anyhow::{Context, Result};

fn read_config(path: &str) -> Result<Config> {
    let content = std::fs::read_to_string(path)
        .with_context(|| format!("failed to read config from \"{path}\""))?;

    let config: Config = toml::from_str(&content)
        .context("failed to parse config")?;

    Ok(config)
}

fn start_server() -> Result<()> {
    let config = read_config("server.toml")
        .context("failed to initialize server")?;
    // ...
    Ok(())
}
```

This produces:

```text
failed to initialize server

Caused by:
    0: failed to read config from "server.toml"
    1: No such file or directory (os error 2)
```

`.with_context(|| ...)` is the lazy variant for expensive format strings. Both also work on `Option<T>`, converting `None` into an error.

```sh
cargo run --example context-chain -p error-battery-pack
```

## Formatting for Logs vs Terminals

| Formatter | Output |
|-----------|--------|
| `{:#}` | Single line, colon-separated: `outer: middle: root cause` |
| `{:?}` | Multi-line "Caused by:" chain, plus backtrace if captured |

Use `{:#}` for structured log fields. Use `{:?}` for terminal output.

For structured logging with `tracing`, walk the chain explicitly:

```rust
fn error_chain(error: &anyhow::Error) -> Vec<String> {
    error.chain().map(|e| e.to_string()).collect()
}

tracing::error!(
    error = %err,
    causes = ?error_chain(&err),
    "request failed"
);
```

## Backtrace Configuration

| Variable | Effect |
|----------|--------|
| `RUST_BACKTRACE=1` | Capture backtraces on panic |
| `RUST_LIB_BACKTRACE=1` | Capture backtraces inside `anyhow::Error` at creation |
| `RUST_LIB_BACKTRACE=0` | Disable anyhow backtraces even when `RUST_BACKTRACE=1` |

**For production services, set `RUST_BACKTRACE=1 RUST_LIB_BACKTRACE=0`.** Backtrace capture can be expensive under contention, and under high error rates the overhead can cause cascading latency spikes.

## When to Reach for Custom Types

`anyhow` is the right default, but define a custom error type when you need to branch on the error programmatically:

```rust
use thiserror::Error;

#[derive(Error, Debug)]
pub enum AppError {
    #[error("not found: {0}")]
    NotFound(String),

    #[error("invalid input")]
    BadRequest(#[source] anyhow::Error),

    #[error("internal error")]
    Internal(#[from] anyhow::Error),
}

impl AppError {
    pub fn status_code(&self) -> u16 {
        match self {
            Self::NotFound(_) => 404,
            Self::BadRequest(_) => 400,
            Self::Internal(_) => 500,
        }
    }
}

// Wire into your HTTP framework's error response trait
// (e.g., axum's IntoResponse, actix's ResponseError)
```

This composes with `anyhow`: library layers use `anyhow::Result` with `.context()`, and the outer handler maps into `AppError` at the boundary. `#[from] anyhow::Error` acts as a catch-all for unexpected failures.

For errors that cross crate boundaries or need semver stability, see the **library-errors** skill.
