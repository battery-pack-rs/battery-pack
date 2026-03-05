# cli-battery-pack

A [battery pack](https://crates.io/crates/battery-pack) for building CLI applications in Rust.

## What's Included

| Crate | What it does |
|-------|-------------|
| [anyhow](https://crates.io/crates/anyhow) | Ergonomic error handling for applications |
| [clap](https://crates.io/crates/clap) | Command-line argument parsing with derive macros |
| [dialoguer](https://crates.io/crates/dialoguer) | Interactive prompts and user input |

### Optional features

- **indicators** — `indicatif` for progress bars, `console` for terminal styling
- **search** — `regex` for pattern matching, `ignore` for gitignore-aware file walking

## Quick Start

```sh
cargo bp add cli
```

Want progress bars too?

```sh
cargo bp add cli -F indicators
```

## License

Licensed under either of:

- Apache License, Version 2.0 ([LICENSE-APACHE](LICENSE-APACHE) or <http://www.apache.org/licenses/LICENSE-2.0>)
- MIT license ([LICENSE-MIT](LICENSE-MIT) or <http://opensource.org/licenses/MIT>)

at your option.
