# Templates

Templates let users scaffold new projects with `cargo bp new` or merge config files into existing projects with `cargo bp add <pack> -t <name>`.

## Structure

A template lives in a subdirectory under `templates/`:

```
templates/
└── default/
    ├── bp-template.toml
    ├── _Cargo.toml
    └── src/
        └── main.rs
```

> **Note:** Template `Cargo.toml` files must be named `_Cargo.toml`. `cargo package` treats any subdirectory containing a `Cargo.toml` as a separate crate and excludes it. The template engine maps `_Cargo.toml` back to `Cargo.toml` in the output.

Register templates in your `Cargo.toml`:

```toml
[package.metadata.battery.templates]
default = { path = "templates/default", description = "A basic starting point" }
subcmds = { path = "templates/subcmds", description = "Multi-command CLI" }
```

If you have multiple templates, users choose with `--template`:

```bash
cargo bp new my-pack --template subcmds
```

## Placeholders

The `bp-template.toml` configures template variables using [MiniJinja](https://github.com/mitsuhiko/minijinja) syntax:

```toml
[placeholders.description]
type = "string"
prompt = "What does this project do?"
default = "A new project"
```

Placeholder names must use snake_case (`my_value`, not `my-value`) because MiniJinja treats `-` as minus.

### Types

```toml
# String (default)
[placeholders.description]
type = "string"
prompt = "Project description"
default = "A new project"

# Bool — yes/no prompt, defaults to false
[placeholders.benchmarks]
type = "bool"
prompt = "Include benchmarks?"

# Select — arrow-key selection, requires explicit default
[placeholders.ci_platform]
type = "select"
prompt = "CI platform"
options = ["github", "none"]
default = "github"
```

Bool values work naturally in templates: `{% if benchmarks %}`. On the command line, bare `-d benchmarks` implies `=true`.

### Built-in variables

These are always available (no declaration needed):

- `{{ project_name }}` — the project name from `--name`
- `{{ crate_name }}` — derived from `project_name` with `-` replaced by `_`

### Built-in functions

- `{{ pin_github_action("actions/checkout", "v6") }}` — resolves a GitHub Action tag to a SHA-pinned reference at generation time
- `{{ rust_stable_version() }}` — returns the current stable Rust version

### Category-linked placeholders

A `select` placeholder can derive its options from a [category](./categories.md) instead of a hardcoded list:

```toml
[placeholders.allocator]
type = "select"
prompt = "Global allocator"
options.category = "allocator"
default = "jemalloc"
```

If the user already made a selection in the `cargo bp add` picker, this placeholder is pre-filled.

## Managed dependencies

Use `bp-managed = true` in your template's `_Cargo.toml` instead of hardcoding versions:

```toml
[dependencies]
clap.bp-managed = true

[build-dependencies]
cli-battery-pack.bp-managed = true
```

When someone generates a project, `cargo bp` resolves actual versions from your battery pack's spec. You never need to update template files when you bump dependency versions.

You can override features or add keys alongside `bp-managed`:

```toml
# Managed version, explicit features:
clap = { bp-managed = true, features = ["derive", "env"] }

# Managed version with optional:
serde = { bp-managed = true, optional = true }
```

The only key that conflicts with `bp-managed` is `version`.

## Merge-friendly templates

Templates applied to existing projects with `cargo bp add <pack> -t <name>` handle file conflicts by type:

- **`Cargo.toml`** — dependencies merged (versions upgraded if behind, features unioned)
- **Other `.toml`** — new sections/keys added, existing ones left alone
- **`.yml` / `.yaml`** — top-level keys merged; `jobs`, `on`, `permissions` deep-merged
- **Everything else** — user prompted to skip or overwrite

Tips for merge-friendly templates:

- Keep template `Cargo.toml` minimal — only what the template needs
- Use `bp-managed = true` so versions stay current
- Use unique filenames for workflows (e.g., `typos.yml` not `ci.yml`)

### Hints

For steps that can't be automated, declare hints:

```toml
[[hints]]
message = "Add `mod errors;` to your lib.rs or main.rs"

[[hints]]
message = "Run `cargo install cargo-fuzz` if you haven't already"
```

Hints are printed after the merge summary (only for `cargo bp add -t`, not `cargo bp new`).

### Including files from outside the template

```toml
[[files]]
src = "LICENSE-MIT"       # relative to crate root
dest = "LICENSE-MIT"      # relative to generated project
```

## Validating templates

`cargo bp validate` generates each template into a temp directory, runs `cargo check` and `cargo test`, and reports failures. Add this to your CI:

```rust
#[cfg(test)]
mod tests {
    #[test]
    fn validate() {
        ::battery_pack::testing::validate(env!("CARGO_MANIFEST_DIR")).unwrap();
    }
}
```

Placeholders should have `default` values so validation can generate templates non-interactively.
