# Contributing to Battery Pack

This section covers how to contribute to the `cargo-bp` tool itself.

If you're looking to create your own battery pack crate, see
[Creating a Battery Pack](../creating.md) instead.

## Development setup

Clone the repository and build:

```bash
git clone https://github.com/battery-pack-rs/battery-pack.git
cd battery-pack
cargo build --workspace
```

Run the test suite:

```bash
cargo test --all --workspace
```

## Repository structure

- `src/battery-pack/` — the `cargo-bp` CLI and its helper crates
- `battery-packs/` — first-party battery packs (cli, error, ci, etc.)
- `md/` — this documentation (built with mdbook)
- `md/spec/` — the formal specification
- `md/rfds/` — design documents (Requests for Discussion)

## Guides

- [Ratatui testing guide](./ratatui-testing-guide.md) — how to write snapshot tests for TUI components
