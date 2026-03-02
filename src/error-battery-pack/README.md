# error-battery-pack

Error handling done well. A [battery pack](https://crates.io/crates/battery-pack) that curates the essential error handling crates for Rust.

## What's Included

| Crate | What it does |
|-------|-------------|
| [anyhow](https://crates.io/crates/anyhow) | Ergonomic error handling for applications — `Result<T>`, `.context()`, error chaining |
| [thiserror](https://crates.io/crates/thiserror) | Derive macros for defining custom error types in libraries |

## Quick Start

```sh
cargo bp add error
```

This adds `anyhow` and `thiserror` to your `[dependencies]` and sets up build-time validation.

## When to Use Which

- **anyhow** — Use in application code (binaries, CLI tools, servers) where you want to propagate errors with context and don't need callers to match on specific variants.
- **thiserror** — Use in library code where callers need to inspect and match on specific error variants.

They compose naturally: library functions return `Result<T, MyError>` (thiserror), and application code wraps them with `anyhow::Result<T>` adding `.context()`.

## Examples

Run the included examples to see the patterns in action:

```sh
# Basic anyhow usage with .context()
cargo run --example basic -p error-battery-pack

# Custom error types with thiserror + anyhow interop
cargo run --example custom-errors -p error-battery-pack

# Multi-layer error context chains
cargo run --example context-chain -p error-battery-pack
```

## License

Licensed under either of:

- Apache License, Version 2.0 ([LICENSE-APACHE](LICENSE-APACHE) or <http://www.apache.org/licenses/LICENSE-2.0>)
- MIT license ([LICENSE-MIT](LICENSE-MIT) or <http://opensource.org/licenses/MIT>)

at your option.
