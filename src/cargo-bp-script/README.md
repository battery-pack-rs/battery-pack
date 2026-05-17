# cargo-bp-script

Schema and process runner for scripting `cargo bp` commands.

This crate provides:

- **Strongly-typed schema** for the JSON output of `cargo bp` subcommands
  that support `--json`. This lets agents and tools consume `cargo bp`
  output without parsing free-form text.
- **A small "commons" runner** that spawns `cargo bp` as a subprocess,
  captures its stdout, and parses the JSON into the schema types.

Currently supports:

- `cargo bp status --json` → [`StatusReport`]

## Consuming output

```rust,no_run
use cargo_bp_script::StatusCommand;

let report = StatusCommand::new()
    .cwd("/path/to/my/project")
    .run()?;

for pack in &report.packs {
    println!("{} {}", pack.short_name, pack.version);
    for warning in &pack.warnings {
        println!(
            "  {}: {} → {} recommended",
            warning.crate_name, warning.current_version, warning.recommended_version,
        );
    }
}
# Ok::<(), cargo_bp_script::Error>(())
```

## Producing output

The schema types use a `new(required)` + chainable `with_*` builder
pattern, so additional fields can be added in future schema versions
without breaking producers:

```rust
use cargo_bp_script::{
    DependencyWarning, InstalledPackStatus, ProjectInfo, StatusReport,
};

let report = StatusReport::new(ProjectInfo::new("Cargo.toml"))
    .with_pack(
        InstalledPackStatus::new("cli", "cli-battery-pack", "0.3.0")
            .with_active_feature("default")
            .with_warning(DependencyWarning::new("clap", "4.4.0", "4.5.0")),
    );
```

## Schema versioning

The top-level [`StatusReport::schema_version`] field is bumped on
breaking changes to the JSON layout. The current value is exposed
as the [`SCHEMA_VERSION`] constant.
