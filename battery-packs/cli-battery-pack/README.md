# cli-battery-pack

A [battery pack](https://crates.io/crates/battery-pack) for building CLI applications in Rust.

## What's Included

| Crate | What it does |
|-------|-------------|
| [anstream](https://crates.io/crates/anstream) | Auto-detecting stream for terminal color support |
| [anstyle](https://crates.io/crates/anstyle) | ANSI text styling |
| [anstyle-hyperlink](https://crates.io/crates/anstyle-hyperlink) | ANSI hyperlink support |
| [supports-hyperlinks](https://crates.io/crates/supports-hyperlinks) | Detect terminal hyperlink support |
| [anyhow](https://crates.io/crates/anyhow) | Ergonomic error handling for applications |
| [clap](https://crates.io/crates/clap) | Command-line argument parsing with derive macros |
| [colorchoice-clap](https://crates.io/crates/colorchoice-clap) | Clap argument for controlling color output |
| [wild](https://crates.io/crates/wild) | Glob argument expansion (for Windows compatibility) |
| [dialoguer](https://crates.io/crates/dialoguer) | Interactive prompts and user input |
| [human-panic](https://crates.io/crates/human-panic) | Human-friendly panic messages |

### Dev dependencies

| Crate | What it does |
|-------|-------------|
| [snapbox](https://crates.io/crates/snapbox) | Snapshot testing for CLI commands |

### Optional features

- **indicators**: `indicatif` for progress bars, `console` for terminal styling
- **search**: `regex` for pattern matching, `ignore` for gitignore-aware file walking
- **config**: `etcetera` for platform-native configuration directories

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
