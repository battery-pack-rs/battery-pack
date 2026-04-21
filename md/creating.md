# Creating a Battery Pack

A battery pack is a normal Rust crate published on crates.io.
It has no real code — just a Cargo.toml that curates dependencies,
plus documentation, examples, and optionally templates.

## Scaffolding

The fastest way to start is:

```bash
cargo bp new battery-pack --name my-battery-pack
```

This creates a new battery pack project from the built-in template,
complete with the right structure, a starter README, and license files.

## Anatomy of a battery pack

Here's what a battery pack looks like:

```
my-battery-pack/
├── Cargo.toml
├── README.md
├── docs.handlebars.md
├── src/
│   └── lib.rs
├── examples/
│   ├── basic.rs
│   └── advanced.rs
└── templates/
    └── default/
        ├── bp-template.toml
        ├── Cargo.toml
        └── src/
            └── main.rs
```

The important parts:

- **Cargo.toml** — defines the curated crates as dependencies
- **README.md** — your prose documentation
- **docs.handlebars.md** — template for auto-generated docs on docs.rs
- **src/lib.rs** — just a doc include (no real code)
- **examples/** — runnable examples showing the crates in action
- **templates/** — project templates for `cargo bp new`

## Defining crates

Your curated crates are just regular Cargo dependencies. The dependency
section they live in determines the default dependency kind for users:

```toml
[dependencies]
anyhow = "1"
thiserror = "2"

[dev-dependencies]
expect-test = "1.5"

[build-dependencies]
cc = "1"
```

When a user installs your battery pack:
- `anyhow` and `thiserror` default to regular dependencies
- `expect-test` defaults to a dev-dependency
- `cc` defaults to a build-dependency

Users can override these in the TUI.

## Features for grouping

Use Cargo's `[features]` to organize crates into groups:

```toml
[dev-dependencies]
clap = { version = "4", features = ["derive"] }
dialoguer = "0.11"
indicatif = { version = "0.17", optional = true }
console = { version = "0.15", optional = true }

[features]
default = ["clap", "dialoguer"]
indicators = ["indicatif", "console"]
fancy = ["clap", "indicatif", "console"]
```

### The default feature

The `default` feature determines which crates a user gets with a plain
`cargo bp add`. Crates not in `default` are available but not installed
unless the user explicitly enables them (e.g., `cargo bp add cli -F indicators`).

If you don't define a `default` feature, all non-optional crates
are included by default.

### Optional crates

Mark crates as `optional = true` if they shouldn't be part of the default
installation. These crates are available through named features or
individual selection in the TUI.

### Feature augmentation

A feature can add Cargo features to a crate, not just toggle it on.
This uses Cargo's native `dep/feature` syntax:

```toml
[dependencies]
tokio = { version = "1", features = ["macros", "rt"] }

[features]
default = ["tokio"]
tokio-full = ["tokio/full"]
```

Enabling `tokio-full` keeps `tokio` but adds the `full` feature on top
of `macros` and `rt`. Feature merging is always additive.

## Hidden dependencies

If your battery pack has dependencies that are internal tooling — not
something users would want to install — mark them as hidden. Every
battery pack should hide the `battery-pack` build dependency (used for
doc generation), along with any other internal crates:

```toml
[package.metadata.battery-pack]
hidden = ["battery-pack", "handlebars", "cargo-metadata"]
```

Hidden crates don't appear in the TUI or in `cargo bp show` output.

You can use globs:

```toml
[package.metadata.battery-pack]
hidden = ["serde*"]
```

Or hide everything (useful if your battery pack is purely templates and examples):

```toml
[package.metadata.battery-pack]
hidden = ["*"]
```

## The lib.rs

A battery pack's `lib.rs` is minimal — it just includes auto-generated documentation:

```rust
#![doc = include_str!(concat!(env!("OUT_DIR"), "/docs.md"))]
```

This makes the battery pack's documentation visible on docs.rs,
including an auto-generated table of all curated crates.
See [Documentation and Examples](./docs-and-examples.md) for details
on how the doc generation works.

## Templates

Templates let users bootstrap new projects with `cargo bp new`.
They use [MiniJinja](https://github.com/mitsuhiko/minijinja) templates
with a `bp-template.toml` configuration file.

A template lives in a subdirectory under `templates/`:

```
templates/
└── default/
    ├── bp-template.toml
    ├── Cargo.toml
    └── src/
        └── main.rs
```

The `bp-template.toml` configures template variables:

```toml
ignore = ["hooks"]

[placeholders.description]
type = "string"
prompt = "What does this project do?"
default = "A new project"
```

Placeholders should have `default` values so that `cargo bp validate`
can generate and check templates non-interactively. Placeholder names
must use snake_case (`description`, not `my-description`) because
MiniJinja treats `-` as the minus operator.

The template engine also provides built-in variables (no declaration needed):

- `{{ project_name }}` — the project name passed via `--name`
- `{{ crate_name }}` — derived from `project_name` with `-` replaced by `_`

### Built-in functions

Templates can call these functions in MiniJinja expressions:

- `{{ pin_github_action("actions/checkout", "v6") }}` — resolves a GitHub
  Action tag to a SHA-pinned reference at generation time (e.g.
  `actions/checkout@abc123 # v6.0.2`). Semver-aware: finds the latest
  version under the given major tag. Supports an optional subpath:
  `{{ pin_github_action("github/codeql-action", "v3", "upload-sarif") }}`.
- `{{ rust_stable_version() }}` — returns the current stable Rust version
  from `rustc --version` (e.g. `1.80.0`).

### Placeholder types

Placeholders support three types:

```toml
# String (default)
[placeholders.description]
type = "string"
prompt = "Project description"
default = "A new project"

# Bool — interactive: yes/no prompt, non-interactive: defaults to false
[placeholders.benchmarks]
type = "bool"
prompt = "Include benchmarks?"

# Select — interactive: arrow-key selection, requires explicit default
[placeholders.ci_platform]
type = "select"
prompt = "CI platform"
options = ["github", "none"]
default = "github"
```

Bool values are registered as actual booleans in MiniJinja, so
`{% if benchmarks %}` works naturally. Bare `-d benchmarks` on the
command line implies `=true`.

To include files from outside the template directory (e.g. shared
license files), use `[[files]]`:

```toml
[[files]]
src = "LICENSE-MIT"       # relative to crate root
dest = "LICENSE-MIT"      # relative to generated project
```

Register templates in your Cargo.toml metadata:

```toml
[package.metadata.battery.templates]
default = { path = "templates/default", description = "A basic starting point" }
subcmds = { path = "templates/subcmds", description = "Multi-command CLI" }
```

If you have multiple templates, users can choose:

```bash
cargo bp new my-pack --template subcmds
```

### Managed dependencies

Use `bp-managed = true` on dependencies in your template's Cargo.toml
instead of hardcoding versions. When someone generates a project from
your template, `cargo bp` resolves the actual versions from your
battery pack's spec:

```toml
[dependencies]
clap.bp-managed = true

[build-dependencies]
cli-battery-pack.bp-managed = true

[package.metadata.battery-pack]
cli-battery-pack = { features = ["default"] }
```

This way you don't need to update template files when you bump
dependency versions. The template always picks up the current spec.

`bp-managed = true` replaces the entire dependency entry with the
version and features from the spec. If you need to pin a specific
version or customize features for a dependency, use an explicit
entry instead:

```toml
# Managed: version and features come from the spec:
anyhow.bp-managed = true

# Explicit: left as-is during resolution:
clap = { version = "4", features = ["derive", "color"] }
```

### Validating templates

`cargo bp validate` automatically generates each template, runs
`cargo check` and `cargo test` on the result, and reports failures.
This catches broken templates before they reach users.

To run template validation in your CI tests, add a test in your `src/lib.rs`:

```rust
#[cfg(test)]
mod tests {
    #[test]
    fn validate_templates() {
        ::battery_pack::testing::validate_templates(env!("CARGO_MANIFEST_DIR")).unwrap();
    }
}
```

The built-in scaffolding template includes this test by default.
