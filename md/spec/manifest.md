# Manifest Manipulation

This section specifies how `cargo bp` reads and modifies Cargo.toml files.

## Battery pack registration

r[manifest.register.location]
Battery pack registrations are stored in a `[*.metadata.battery-pack]`
table, where `*` is either `package` or `workspace`.

r[manifest.register.format]
Each registration is a key-value pair where the key is the battery pack
crate name and the value is the version string:

```toml
[package.metadata.battery-pack]
error-battery-pack = "0.4.0"
cli-battery-pack = "0.3.0"
```

r[manifest.register.workspace-default]
When a workspace root exists, battery pack registrations MUST default
to `[workspace.metadata.battery-pack]` in the workspace root Cargo.toml.

r[manifest.register.package-level]
The user MAY choose to register a battery pack at the package level
using `[package.metadata.battery-pack]` in the crate's own Cargo.toml.
This is for per-crate battery packs in a workspace.

r[manifest.register.both-levels]
`cargo bp` MUST support reading registrations from both workspace
and package metadata. When both exist, package-level registrations
take precedence for that crate.

## Active features

r[manifest.features.storage]
The active features for a battery pack MUST be stored alongside
the registration in one of two forms:

- Full form: `cli-battery-pack = { features = ["default", "indicators"] }`
- Short form (when only the default feature is active): `cli-battery-pack = "0.3.0"`

The short form is equivalent to `{ features = ["default"] }`.
When the `features` key is absent, the `default` feature is
implicitly active.

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
clap = { bp-managed = true }

[build-dependencies]
cli-battery-pack = { bp-managed = true }
```

r[manifest.managed.conflict]
A dependency MUST NOT have both `bp-managed = true` and any other keys
(`version`, `features`, `default-features`, etc.). `cargo bp` MUST
error if `bp-managed` is combined with other dependency keys.

r[manifest.managed.resolution]
When generating a project from a template, `cargo bp` MUST resolve
each `bp-managed` dependency by replacing the entire entry with the
version and Cargo features from the battery pack's spec. The spec
is always the same crate that contains the template. When downloaded
from crates.io, this is the latest published version. When using a
local path, it is the local source tree. Battery pack crates in
`[build-dependencies]` get the battery pack's own version.

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
