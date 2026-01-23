# What is a Battery Pack?

A **battery pack** bundles everything you need to get started in an area: a curated set of crates, coherent documentation, examples, templates, and agentic skills.

**Curation, not abstraction.** You use the real crates directly—the battery pack just makes them easier to discover and start with.

## Quick Start

```bash
# Install the CLI
cargo install battery-pack

# Create a new CLI app from cli-battery-pack templates
cargo bp new cli

# Or specify a template directly
cargo bp new cli --template simple --name myapp
```

## Using a Battery Pack

Add it as a dependency:

```bash
cargo bp add cli
```

This adds to your `Cargo.toml`:

```toml
[dependencies]
cli = { package = "cli-battery-pack", version = "0.1" }
```

Access the curated crates through the battery pack namespace:

```rust
use cli::clap::Parser;
use cli::console::style;
use cli::indicatif::ProgressBar;
```

## What's Inside a Battery Pack?

- **Facade crate** — re-exports curated crates through a single dependency
- **Templates** — cargo-generate templates for common starting points
- **Skills** (coming soon) — machine-readable guidance for AI agents and humans
- **Other battery packs** — battery packs can extend one another, composing focused building blocks

## Example: cli-battery-pack

The `cli-battery-pack` bundles together common crates for CLI applications, such as `clap` for argument parsing, `console` for terminal styling, and `indicatif` for progress bars.

To help you get started, it includes templates like `simple` and `subcmds`.

It also extends `error-battery-pack` and `logging-battery-pack`:

```rust
use cli::anyhow::Result;  // from error-battery-pack
use cli::tracing::info;   // from logging-battery-pack
```
