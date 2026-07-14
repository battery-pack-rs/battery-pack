# Creating a Battery Pack

A battery pack is a normal Rust crate published on crates.io. It has no real code — just a `Cargo.toml` that curates dependencies, plus documentation and optionally templates.

## Scaffolding

```bash
cargo bp new battery-pack --name my-battery-pack
```

This creates a battery pack project from the built-in template with the right structure, a starter README, and license files.

## Dependencies are recommendations

The crates in your `[dependencies]` are what gets recommended to users. When someone runs `cargo bp add my-pack`, these crates are added to *their* `Cargo.toml`:

```toml
[dependencies]
anyhow = "1"
thiserror = "2"

[dev-dependencies]
expect-test = "1.5"
```

The section they live in determines the default dependency kind for users — `[dev-dependencies]` become dev-deps in the user's crate, and so on.

## Features are named groups

Use Cargo's `[features]` to organize crates into toggleable groups:

```toml
[dependencies]
clap = { version = "4", features = ["derive"] }
dialoguer = "0.11"
indicatif = { version = "0.17", optional = true }
console = { version = "0.15", optional = true }

[features]
default = ["clap", "dialoguer"]
indicators = ["indicatif", "console"]
```

The `default` feature determines what a user gets with a plain `cargo bp add`. Crates marked `optional = true` are only installed when the user enables a feature that includes them (e.g., `cargo bp add cli -F indicators`).

If you don't define a `default` feature, all non-optional crates are included by default.

A feature can also augment Cargo features on a crate using `dep/feature` syntax:

```toml
[features]
tokio-full = ["tokio/full"]
```

## Auto-generated documentation

Every battery pack has a `build.rs` that generates documentation at compile time:

```rust
fn main() {
    battery_pack::build::generate_docs().unwrap();
}
```

This reads your `Cargo.toml`, `README.md`, and `docs.handlebars.md`, then renders them into a `docs.md` that becomes the crate's docs.rs page. The default template is:

```handlebars
\{{readme}}

\{{crate-table}}
```

`{{readme}}` inlines your README. `{{crate-table}}` auto-generates a table of your curated crates grouped by category, split by dependency kind, with links to crates.io. You never need to maintain a crate list by hand — it's derived from your `Cargo.toml`.

The `src/lib.rs` just includes the generated output:

```rust
#![doc = include_str!(concat!(env!("OUT_DIR"), "/docs.md"))]
```

See [Documentation and Examples](./docs-and-examples.md) for more on customizing the docs template and adding runnable examples.

## Getting fancy with metadata

Battery packs support additional metadata in `[package.metadata.battery-pack.*]` for richer behavior:

- **[Hidden Dependencies](./creating/hidden.md)** — hide internal crates (like `battery-pack` itself) from the user-facing picker and docs
- **[Categories](./creating/categories.md)** — group items thematically and express "pick at most one" constraints (e.g., choose one HAL, one allocator)
- **[Templates](./creating/templates.md)** — scaffold new projects or merge config files into existing ones
