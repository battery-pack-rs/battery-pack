# Manifest Manipulation

This section specifies how `cargo bp` reads and modifies Cargo.toml files.

## Battery pack state (`battery-pack.toml`)

> **Note:** The `battery-pack.toml` format is subject to change in future
> versions. The file includes a `version` field to support forward
> compatibility.

r[manifest.state.location]
Battery pack state (installed packs, active features, managed
dependencies) is stored in a `battery-pack.toml` file next to the
crate's `Cargo.toml`. Each crate in a workspace has its own
`battery-pack.toml`.

r[manifest.state.format]
The file uses the following structure:

```toml
version = 1

[[battery-pack]]
name = "cli"
features = ["default", "indicators"]

[[battery-pack.managed-deps]]
name = "clap"
version = "4.5"

[[battery-pack.managed-deps]]
name = "dialoguer"
version = "0.11"
```

r[manifest.state.version]
The `version` field MUST be present and set to `1`. Tools MUST
reject files with a version higher than they support.

r[manifest.state.name]
The `name` field uses the short form of the battery pack name
(e.g., `"cli"` for `cli-battery-pack`).

## Battery pack discovery

r[manifest.register.location]
Installed battery packs are discovered by scanning
`[build-dependencies]` in the crate's `Cargo.toml` for entries
whose names end in `-battery-pack` or equal `"battery-pack"`.

## Active features

r[manifest.features.storage]
The active features for a battery pack are stored in the
`features` array of the corresponding `[[battery-pack]]` entry
in `battery-pack.toml`. When no `battery-pack.toml` exists or
the pack is not listed, the `default` feature is implicitly active.

## Dependency management

r[manifest.deps.add]
When adding a crate, `cargo bp` MUST add it to the correct dependency
section (`[dependencies]`, `[dev-dependencies]`, or `[build-dependencies]`)
based on the battery pack's Cargo.toml, unless overridden by the user.

r[manifest.deps.version-features]
Each dependency entry MUST include the version and Cargo features
as specified by the battery pack.

r[manifest.deps.workspace]
In a workspace, `cargo bp` MUST add crate entries to
`[workspace.dependencies]` in the workspace root and reference
them as `crate = { workspace = true }` in the crate's dependency section.

r[manifest.deps.no-workspace]
In a non-workspace project, `cargo bp` MUST add crate entries
directly to the crate's dependency section with full version and features.

r[manifest.deps.existing]
If a dependency already exists in the user's Cargo.toml, `cargo bp`
MUST NOT overwrite user customizations (additional features, version overrides).
It MUST only add missing features and warn about version mismatches.

r[manifest.deps.remove]
When a user disables a crate via the TUI, `cargo bp` MUST remove
it from the appropriate dependency section. If using workspace
dependencies, the `workspace.dependencies` entry SHOULD be preserved
(other crates in the workspace may use it).

## Managed dependencies in templates

r[manifest.managed.marker]
A template's Cargo.toml MAY use `bp-managed = true` on a dependency
instead of hardcoding a version. This signals that the version should
be resolved at template generation time from the battery pack's own spec.

```toml
[dependencies]
clap.bp-managed = true

[build-dependencies]
cli-battery-pack.bp-managed = true
```

r[manifest.managed.conflict]
A dependency MUST NOT have both `bp-managed = true` and any other keys
(`version`, `features`, `default-features`, etc.). `cargo bp` MUST
error if `bp-managed` is combined with other dependency keys.

r[manifest.managed.resolution]
When generating a project from a template, `cargo bp` MUST resolve
each `bp-managed` dependency by replacing the entire entry with the
version and Cargo features from the battery pack's spec. Specs are
discovered from the crate root's workspace first. If a referenced
battery pack is not found locally (e.g. a cross-pack reference after
downloading from crates.io), `cargo bp` MUST fetch its spec from the
registry. Battery pack crates in `[build-dependencies]` get the
battery pack's own version.

r[manifest.managed.no-partial]
Partial overrides are not supported. A `bp-managed` dependency cannot
selectively manage only the version or only the features. The spec
controls both. To customize features or pin a specific version, use
an explicit dependency entry instead of `bp-managed = true`. If you
have a use case for partial overrides, please [open an issue](https://github.com/battery-pack-rs/battery-pack/issues).

r[manifest.managed.explicit-override]
A template MAY use an explicit version instead of `bp-managed = true`
to pin a specific version or specify custom features. Explicit
dependencies are left as-is and not modified during resolution.

## Cross-pack merging

r[manifest.merge.version]
When multiple battery packs recommend the same crate, `cargo bp`
MUST use the newest version. This applies even across major versions —
the highest version always wins.

r[manifest.merge.features]
When multiple battery packs recommend the same crate with different
Cargo features, `cargo bp` MUST union (merge) all the features.

r[manifest.merge.dep-kind]
When multiple battery packs recommend the same crate with different
dependency kinds, `cargo bp` MUST resolve as follows:
- If any pack lists the crate in `[dependencies]`, it MUST be added
  as a regular dependency (the widest scope).
- If one pack lists it in `[dev-dependencies]` and another in
  `[build-dependencies]`, it MUST be added to both sections.

## Sync behavior

r[manifest.sync.version-bump]
During sync, `cargo bp` MUST update a dependency's version to the
battery pack's recommended version only when the user's version is
older. If the user's version is equal to or newer than the
recommended version, it MUST be left unchanged.

r[manifest.sync.feature-add]
During sync, `cargo bp` MUST add any Cargo features that the
battery pack specifies but that are missing from the user's
dependency entry. Existing user features MUST be preserved —
sync MUST NOT remove Cargo features.

## TOML formatting

r[manifest.toml.preserve]
`cargo bp` MUST preserve existing TOML formatting, comments,
and ordering when modifying Cargo.toml files.

r[manifest.toml.style]
New entries added by `cargo bp` SHOULD follow the existing
formatting style of the file (inline tables vs. multi-line, etc.).
