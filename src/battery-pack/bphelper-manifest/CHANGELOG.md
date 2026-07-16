# Changelog

All notable changes to this project will be documented in this file.

The format is based on [Keep a Changelog](https://keepachangelog.com/en/1.0.0/),
and this project adheres to [Semantic Versioning](https://semver.org/spec/v2.0.0.html).

## [Unreleased]

## [0.6.0](https://github.com/battery-pack-rs/battery-pack/compare/bphelper-manifest-v0.5.6...bphelper-manifest-v0.6.0) - 2026-07-16

### Added

- *(cli)* wire categories into picker and validate exclusive -F picks (Phase 4+5)
- *(manifest)* parse and validate category metadata (Phase 1+2)

### Fixed

- address post-implementation review findings (Phase 9)
- address adversarial review findings in categories impl
- *(manifest)* include implicit-feature optional deps under ActiveFeatures::All
- spelling :)
- spelling and formatting

### Other

- *(manifest)* restrict resolve_all() to test-only visibility
- Update src/battery-pack/bphelper-manifest/src/lib.rs
- ActiveFeatures enum + resolver hardening
- add a cargo-metadata oracle test
- switch feature lists to a parsed FeatureRef
- Merge branch 'main' of https://github.com/battery-pack-rs/battery-pack into fix/issue-13-cargo-metadata

## [0.5.6](https://github.com/battery-pack-rs/battery-pack/compare/bphelper-manifest-v0.5.5...bphelper-manifest-v0.5.6) - 2026-06-03

### Fixed

- *(manifest)* include non-optional base deps when features are active ([#142](https://github.com/battery-pack-rs/battery-pack/pull/142))

### Other

- *(templates)* trim `.rs` whitespace, run rustfmt after render, and assert `cargo fmt --check` on validate ([#142](https://github.com/battery-pack-rs/battery-pack/pull/142))

## [0.5.5](https://github.com/battery-pack-rs/battery-pack/compare/bphelper-manifest-v0.5.4...bphelper-manifest-v0.5.5) - 2026-04-30

### Added

- *(bp-managed)* allow features and other keys alongside bp-managed
- *(manifest)* add format.templates.cargo-toml validation rule

### Fixed

- rename template Cargo.toml to _Cargo.toml, flip assertions

## [0.5.4](https://github.com/battery-pack-rs/battery-pack/compare/bphelper-manifest-v0.5.3...bphelper-manifest-v0.5.4) - 2026-04-13

### Other

- refactor test: change tests to use snapbox instead of expect-test ([#80](https://github.com/battery-pack-rs/battery-pack/pull/80))

## [0.5.3](https://github.com/battery-pack-rs/battery-pack/compare/bphelper-manifest-v0.5.2...bphelper-manifest-v0.5.3) - 2026-04-03

### Fixed

- include dev/build deps in feature resolution ([#76](https://github.com/battery-pack-rs/battery-pack/pull/76))

## [0.5.2](https://github.com/battery-pack-rs/battery-pack/compare/bphelper-manifest-v0.5.1...bphelper-manifest-v0.5.2) - 2026-03-12

### Added

- with_template uses bp-managed, move discovery to bphelper-manifest

### Other

- add managed-battery-pack fixture with bp-managed deps

## [0.5.1](https://github.com/battery-pack-rs/battery-pack/compare/bphelper-manifest-v0.5.0...bphelper-manifest-v0.5.1) - 2026-03-05

### Added

- replace cargo-generate with MiniJinja template engine

### Fixed

- accept exact name "battery-pack" in validate_spec ([#40](https://github.com/battery-pack-rs/battery-pack/pull/40))

## [0.5.0](https://github.com/battery-pack-rs/battery-pack/compare/bphelper-manifest-v0.4.1...bphelper-manifest-v0.5.0) - 2026-03-02

### Added

- implement cargo bp status with version warnings
- implement cross-pack crate merging
- add cargo bp validate and rewrite spec/manifest layer

### Fixed

- fix a lot of clippy lints
- correct pre-existing test failures in bphelper-manifest
- metadata location abstraction + dep-kind routing + hidden filtering

### Other

- review fixes — merge non-additive spec rules, fix bugs, dedup
- eliminate CargoManifest, reuse BatteryPackSpec from bphelper-manifest
- sync behavior — add [impl] tags + tests
- add tracey [impl] tags for format and cli spec rules
- clean up cargo bp add TUI and interactive picker
