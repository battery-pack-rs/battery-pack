# battery-pack

Curated crate bundles with docs, templates, and agentic skills.

📖 **[Read the book](https://battery-pack-rs.github.io/battery-pack)**

## What's a Battery Pack?

A battery pack bundles everything you need to get started in an area: curated crates, documentation, examples, and templates.

Think of it like an addition to the standard library targeting a particular use case, like building a CLI tool or web server.

## Installing the CLI

Install [`cargo-bp`](https://crates.io/crates/cargo-bp):

```bash
cargo install cargo-bp
# or
cargo binstall cargo-bp
```

## Using the CLI

```bash
# Create a new project from a battery pack template
cargo bp new cli

# Add a battery pack to an existing project
cargo bp add cli

# Show info about a battery pack
cargo bp show cli
```

## Authoring Battery Packs

The battery-pack crate is also a battery pack itself.

```bash
# Create a new project from a battery pack template
cargo bp new battery-pack
```
