# Hidden Dependencies

Some crates in your battery pack are internal tooling — not something users would want to install. Every battery pack should at minimum hide the `battery-pack` build dependency (used for doc generation):

```toml
[package.metadata.battery-pack]
hidden = ["battery-pack"]
```

Hidden crates don't appear in the TUI picker, in `cargo bp show` output, or in the auto-generated docs.

## Adding more hidden crates

Any internal plumbing crates should be hidden:

```toml
[package.metadata.battery-pack]
hidden = ["battery-pack", "bphelper-manifest", "snapbox"]
```

## Globs

You can use glob patterns:

```toml
[package.metadata.battery-pack]
hidden = ["serde*"]
```

## Hiding everything

If your battery pack is purely templates (no curated crates for users to pick), hide all dependencies:

```toml
[package.metadata.battery-pack]
hidden = ["*"]
```
