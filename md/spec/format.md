# Battery Pack Format

This section specifies the structure of a battery pack crate.

## Crate structure

r[format.crate.name]
A battery pack crate's name MUST end in `-battery-pack`
(e.g., `error-battery-pack`, `cli-battery-pack`).

r[format.crate.keyword]
A battery pack crate MUST include `battery-pack` in its `keywords`
so that `cargo bp list` can discover it via the crates.io API.

r[format.crate.lib]
A battery pack crate's `lib.rs` SHOULD contain only a doc include directive.
There is no functional code in a battery pack.

r[format.crate.no-code]
A battery pack crate MUST NOT contain functional Rust code
(beyond the doc include and build.rs for doc generation).
It exists purely as a metadata and documentation vehicle.

r[format.crate.repository]
A battery pack crate SHOULD set the `repository` field in its
`[package]` section. The repository URL is used to link to
examples and templates in `cargo bp show` and the TUI.
`cargo bp validate` MUST warn if the repository URL is not set.

## Dependencies as curation

r[format.deps.source-of-truth]
The battery pack's dependency sections (`[dependencies]`,
`[dev-dependencies]`, `[build-dependencies]`) are the source of truth
for which crates the battery pack curates and their recommended
versions and features.

r[format.deps.kind-mapping]
The dependency section a crate appears in determines the default
dependency kind for users. A `[dependencies]` entry defaults to a
regular dependency, `[dev-dependencies]` to a dev-dependency, and
`[build-dependencies]` to a build-dependency.

r[format.deps.version-features]
Each dependency entry specifies the recommended version and Cargo features.
These are used by `cargo bp` when adding the crate to a user's project.

## Features

r[format.features.grouping]
Cargo `[features]` in the battery pack define named groups of crates.
Each feature lists the crate names it includes.

r[format.features.optional-required]
Any dependency listed in a `[features]` entry MUST be declared with
`optional = true` in its dependency section. This is a Cargo
requirement: feature names that match dependency names implicitly
enable that dependency, which Cargo only allows for optional deps.

r[format.features.default]
The `default` feature determines which crates are installed when
a user runs `cargo bp add <pack>` without additional flags. If no
`default` feature is defined, all non-optional crates are
considered part of the default set.

r[format.features.dev-build-always]
`[dev-dependencies]` and `[build-dependencies]` are always included
regardless of feature selection.

r[format.features.optional]
Crates marked `optional = true` in the dependency section are not
part of the default installation. They are available through named
features or individual selection.

r[format.features.additive]
Features are additive. Enabling a feature adds its crates on top of
whatever is already enabled. Features never remove crates.

r[format.features.augment]
A feature MAY augment the Cargo features of a crate that is already
included via another feature or the default set. Augmentation
uses Cargo's native `dep/feature` syntax in `[features]`
(e.g., `tokio-full = ["tokio/full"]`). No custom metadata is
required. When augmenting, the specified Cargo features are
unioned with the existing set.

## Categories

Categories group related items (features, dependencies, or templates) so that
`cargo bp add` can present them together, optionally as a set of mutually
exclusive alternatives.

r[format.categories.definition]
A `[package.metadata.battery-pack.categories.<name>]` table declares a category.
It MAY contain a `title` (display name in the picker header), a `description`
(explanatory text), and a `pick` field (the selection mode):

```toml
[package.metadata.battery-pack.categories.hal]
title = "Hardware Abstraction Layer"
description = "Pick the HAL for your target chip family"
pick = "at-most-one"
```

r[format.categories.pick]
The `pick` field MUST be either `"at-most-one"` or `"any"`. It defaults to
`"any"` when omitted. An `at-most-one` category allows selecting no more than
one of its members; an `any` category places no constraint on how many members
are selected.

r[format.categories.defined]
Every category name referenced in an item's `categories` list MUST have a
matching `[package.metadata.battery-pack.categories.<name>]` entry. A reference
to an undefined category is an error (`format.categories.defined`).

r[format.categories.empty]
A category that is declared but that no item references SHOULD be removed.
`cargo bp validate` MUST warn about an empty category
(`format.categories.empty`).

r[format.categories.pick-missing-title]
A category with `pick = "at-most-one"` SHOULD define a `title` for use as the
picker section header. `cargo bp validate` MUST warn when an `at-most-one`
category has no `title` (`format.categories.pick-missing-title`).

## Feature metadata

r[format.features.metadata]
A `[package.metadata.battery-pack.features.<name>]` table annotates a Cargo
feature. It MAY contain a `description` (shown next to the item in the picker)
and a `categories` list (the categories the feature belongs to, default `[]`).
Both fields are optional, and a feature with no metadata entry behaves exactly
as it does without this feature:

```toml
[package.metadata.battery-pack.features.stm32f4]
description = "STM32F4xx family"
categories = ["hal"]
```

r[format.features.unknown-feature]
A `[package.metadata.battery-pack.features.<name>]` entry whose `<name>` is not
a key in `[features]` is an error (`format.features.unknown-feature`).

r[format.features.exclusive-conflict]
Two or more features that belong to the same `at-most-one` category MUST NOT
both appear in the `[features]` `default` array. Doing so is an error
(`format.features.exclusive-conflict`), because it would install a conflicting
default set.

## Dependency metadata

r[format.deps.metadata]
A `[package.metadata.battery-pack.dependencies.<name>]` table annotates a
dependency. It MAY contain a `description` and a `categories` list, with the
same meaning as feature metadata:

```toml
[package.metadata.battery-pack.dependencies.embedded-hal]
description = "Trait abstractions for embedded I/O"
categories = ["portable"]
```

r[format.dependencies.unknown-dep]
A `[package.metadata.battery-pack.dependencies.<name>]` entry whose `<name>`
does not appear in any dependency section is an error
(`format.dependencies.unknown-dep`).

## Hidden dependencies

r[format.hidden.metadata]
The `[package.metadata.battery-pack]` section MAY contain a `hidden`
key with a list of dependency names to hide from users.

r[format.hidden.effect]
Hidden dependencies do not appear in the TUI, `cargo bp show`,
or the auto-generated crate table. They cannot be installed by users
through `cargo bp`.

r[format.hidden.glob]
Entries in the `hidden` list MAY use glob patterns.
For example, `"serde*"` hides `serde`, `serde_json`, `serde_derive`, etc.

r[format.hidden.wildcard]
The value `"*"` hides all dependencies. This is useful for battery packs
that provide only templates and examples.

## Templates

r[format.templates.directory]
Templates are stored in subdirectories under `templates/` in the
battery pack crate.

r[format.templates.metadata]
Templates MUST be registered in `[package.metadata.battery.templates]`
with a `path` and `description`. An entry MAY also carry an optional
`categories` list, naming the categories the template belongs to (default
`[]`):

```toml
[package.metadata.battery.templates]
default = { path = "templates/default", description = "A basic starting point" }
fuzzing = { path = "templates/fuzzing", description = "cargo-fuzz scaffold", categories = ["quality"] }
```

r[format.templates.engine]
Templates use [MiniJinja](https://github.com/mitsuhiko/minijinja)
for rendering. Each template directory MAY contain a `bp-template.toml`
to configure placeholders and ignored paths.

r[format.templates.cargo-toml]
Template `Cargo.toml` files MUST be named `_Cargo.toml`. `cargo package`
treats any subdirectory containing a `Cargo.toml` as a separate crate
boundary and excludes it from the published tarball. The template engine
automatically maps `_Cargo.toml` back to `Cargo.toml` in the generated
output. `cargo bp validate` rejects templates containing `Cargo.toml`.

r[format.templates.cargo-toml-passthrough]
The `_Cargo.toml` → `Cargo.toml` mapping is suppressed for output paths
under `templates/`. This allows battery packs that scaffold other battery
packs (e.g. the `with_template` authoring template) to preserve
`_Cargo.toml` in their generated template directories.

r[format.templates.managed-deps]
Template `_Cargo.toml` files SHOULD use `bp-managed = true` on dependencies
instead of hardcoding versions. This ensures generated projects always
get the versions from the battery pack's current spec. See
[Managed dependencies in templates](./manifest.md#managed-dependencies-in-templates)
for details.

r[format.templates.config-excluded]
The root `bp-template.toml` is the engine's configuration file and
MUST NOT be included in generated output. A `bp-template.toml` nested
inside a subdirectory (e.g. a scaffolded inner template) MUST be
included in the output normally.

r[format.templates.ignore]
The `ignore` list in `bp-template.toml` specifies files and folders
to exclude from generated output entirely. Entries are matched by
exact name against any path component, so `ignore = ["hooks"]`
excludes a `hooks/` directory at any depth. Wildcards are not
supported.

r[format.templates.files]
The `[[files]]` array in `bp-template.toml` copies files from outside
the template directory into the generated project. Each entry has a
`src` path (relative to the crate root) and a `dest` path (relative
to the generated project root). Source files are rendered through the
template engine. Existing files from the template directory are not
overwritten.

r[format.templates.builtin-variables]
The template engine provides the following built-in variables:

- `project_name` — the project name passed via `--name`
- `crate_name` — derived from `project_name` by replacing `-` with `_`

These are available in all template files without declaring them as
placeholders.

r[format.templates.selection]
If a battery pack has multiple templates, `cargo bp new` MUST prompt
the user to select one (unless `--template` is specified).

r[format.templates.placeholder-defaults]
Template placeholders SHOULD define a `default` value in
`bp-template.toml` so that templates can be validated
non-interactively by `cargo bp validate`.

r[format.templates.placeholder-names]
Placeholder names MUST use snake_case. Names containing `-` are
rejected because MiniJinja parses `-` as the minus operator, making
such variables unreachable in template expressions.

## Examples

r[format.examples.standard]
Examples are standard Cargo examples in the `examples/` directory.
They follow normal Cargo conventions and are runnable with `cargo run --example`.

r[format.examples.browsable]
Examples MUST be listed in `cargo bp show` output and in the TUI's
detail view for the battery pack.

## Scaffolding

r[format.scaffold.template]
The `battery-pack` crate (the CLI itself) MUST include a built-in
template for authoring new battery packs. Running
`cargo bp new battery-pack` MUST create a new battery pack project
with the standard structure (Cargo.toml, README.md,
docs.handlebars.md, src/lib.rs, examples/, templates/).
