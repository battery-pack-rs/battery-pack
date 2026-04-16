# Getting Started

## Install the CLI

```bash
cargo install cargo-bp
```

This gives you the `cargo bp` command.

## Create a new project from a template

Battery packs can include project templates. To start a new CLI application
using the `cli-battery-pack` template:

```bash
cargo bp new cli
```

You'll be prompted for a project name and directory.
The result is a ready-to-go Rust project with the battery pack's
recommended crates already in your `Cargo.toml`.

See [Using Battery Packs](./using.md#creating-a-new-project-from-a-template) for the full set of template options, including choosing specific templates, non-interactive mode, and previewing output.

## Add a battery pack to an existing project

If you already have a Rust project, you can add a battery pack to it:

```bash
cargo bp add error
```

This resolves `error` to `error-battery-pack`, downloads it from crates.io,
and adds its default crates to your project. For `error-battery-pack`,
that means `anyhow` and `thiserror`.

### What changed in your Cargo.toml

Before:

```toml
[package]
name = "my-app"
version = "0.1.0"
edition = "2024"

[dependencies]
```

After `cargo bp add error`:

```toml
[package]
name = "my-app"
version = "0.1.0"
edition = "2024"

[package.metadata.battery-pack]
error-battery-pack = "0.4.0"

[dependencies]
anyhow = "1"
thiserror = "2"
```

The `[package.metadata.battery-pack]` section records which battery packs
you've installed and their versions. The actual crates — `anyhow` and `thiserror` —
are real entries in `[dependencies]` that you use directly.

## Use the crates

There's nothing special about how you use the crates. They're real dependencies:

```rust
use anyhow::{Context, Result};
use thiserror::Error;

#[derive(Error, Debug)]
enum MyError {
    #[error("not found: {0}")]
    NotFound(String),
}

fn main() -> Result<()> {
    let config = std::fs::read_to_string("config.toml")
        .context("failed to read config")?;
    Ok(())
}
```

Proc macros, derive macros, attribute macros — everything works exactly
as if you'd added the crates by hand. Because you did, with help.

## Launch the TUI

For a richer experience, just run:

```bash
cargo bp
```

This opens an interactive terminal interface where you can browse available
battery packs, toggle individual crates, and manage your dependencies visually.
See [Using Battery Packs](./using.md) for the full tour.

## Check your battery pack status

To see which battery packs you have installed and whether anything is out of date:

```bash
cargo bp status
```

This shows your installed packs, their versions, and warnings if any
of your dependency versions are older than what the battery pack recommends.
