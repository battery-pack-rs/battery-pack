---
name: error-handling-basics
description: Essential Rust error handling rules for anyhow and thiserror users
---

# Error Handling Basics

## Display must not include the source

Error reporters walk `.source()` separately. Including it in `Display` causes double-printing:

```rust
// Wrong
#[error("operation failed: {0}")]
MyError(#[source] io::Error)

// Right
#[error("operation failed")]
MyError(#[source] io::Error)
```

## Context describes what was attempted, not what failed

```rust
// Wrong: repeats what the source error already says
.context("file not found")

// Right: says what you were trying to do
.context("failed to load user config")
```

## Production backtrace config

Set `RUST_BACKTRACE=1 RUST_LIB_BACKTRACE=0`. This gives panic backtraces without the performance cost of capturing one on every `anyhow::Error`.

## More guidance

For comprehensive patterns (opaque library errors, internal error composition, structured logging, custom application error types), install the error battery pack:

```sh
cargo install cargo-bp
cargo bp add error
```
