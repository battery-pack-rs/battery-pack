# battery-pack

The `battery-pack` crate provides two things:

1. **The `cargo bp` CLI** for working with battery packs
2. **Common infrastructure** for authoring battery packs

## What's a Battery Pack?

A battery pack bundles everything you need to get started in an area: curated crates, documentation, examples, and templates.

Think of it like an addition to the standard library targeting a particular use case, like building a CLI tool or web server.

## Installing the CLI

```bash
cargo install battery-pack
# or
cargo binstall battery-pack
```

## Using the CLI

```bash
# Create a new project from a battery pack template
cargo bp new cli

# Add a battery pack to your project
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
