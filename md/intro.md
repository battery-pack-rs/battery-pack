# What is a Battery Pack?

Rust is famously "batteries not included" — the standard library is small,
and for most real-world tasks you need to pull in crates from the ecosystem.
But which crates? With what features? In what combination?

A **battery pack** answers those questions. It's a curated collection of crates
for a specific domain — error handling, CLI tooling, async programming, web services —
assembled by someone who's thought carefully about what works well together.

## Curation, not abstraction

Battery packs don't wrap or re-export crates. You use the real crates directly,
with their real APIs, their real docs, their real proc macros. A battery pack
just tells `cargo bp` which crates to install and how to configure them.

Think of a battery pack like a shopping list written by an expert.
You don't have to use everything on the list, and you can always add your own items.
But the list gives you a solid starting point.

## A quick example

Say you're building a CLI tool and you want the `cli-battery-pack`:

```bash
cargo bp add cli
```

This adds `clap` and `dialoguer` to your `[dependencies]` — the battery pack's defaults.
Your code uses them directly:

```rust
use clap::Parser;
use dialoguer::Input;

#[derive(Parser)]
struct Cli {
    #[arg(short, long)]
    name: String,
}
```

Want progress bars too? The `cli-battery-pack` has an `indicators` feature:

```bash
cargo bp add cli -F indicators
```

Now you also have `indicatif` and `console` in your dependencies.

## How it works

Under the hood, a battery pack is a crate published on crates.io.
It has no real code — just a Cargo.toml listing curated dependencies,
documentation, examples, and optionally templates for bootstrapping new projects.

When you run `cargo bp add`, the CLI:

1. Downloads the battery pack's Cargo.toml from crates.io
2. Reads its dependencies and features
3. Adds the selected crates to *your* Cargo.toml as real dependencies
4. Records which battery pack they came from (in `[package.metadata]`)

The battery pack itself is never compiled as part of your project.
It's purely a source of truth for `cargo bp` to read.

## The TUI

Running `cargo bp` with no arguments opens an interactive terminal interface
where you can:

- Browse and search available battery packs
- Add or remove battery packs from your project
- Toggle individual crates on and off
- Choose whether each crate is a runtime, dev, or build dependency
- Create new projects from battery pack templates

The subcommands (`cargo bp add`, `cargo bp status`, etc.) are there for
scripting and quick one-off operations, but the TUI is the primary experience.

## Templates

Some battery packs include project templates that scaffold a new Rust project with the recommended crates and structure already in place. See [Getting Started](./getting-started.md) to create your first project from a template, or [Templates](./templates.md#creating-a-new-project-from-a-template) for the full set of options.

## What's next

- **[Getting Started](./getting-started.md)** walks you through installing the CLI and using your first battery pack
- **[Using Battery Packs](./using.md)** covers the full range of `cargo bp` functionality
- If you want to create your own battery pack, see the **[Author's Guide](./creating.md)**
