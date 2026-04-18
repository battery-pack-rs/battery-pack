# Changelog

All notable changes to this project will be documented in this file.

The format is based on [Keep a Changelog](https://keepachangelog.com/en/1.0.0/),
and this project adheres to [Semantic Versioning](https://semver.org/spec/v2.0.0.html).

## [Unreleased]

## [0.4.13](https://github.com/battery-pack-rs/battery-pack/compare/battery-pack-v0.4.12...battery-pack-v0.4.13) - 2026-04-18

### Added

- TUI context-awareness and one-shot exit behavior
- cargo bp show annotates installed crates and features
- cargo bp show displays features section
- add cargo bp rm command to remove battery packs
- reworked add picker with edit semantics and pre-selection
- cargo bp add with no args shows helpful message instead of TUI
- track managed-deps in battery pack metadata

### Fixed

- resolve cargo clippy warnings

### Other

- pacify the merciless cargo fmt
- pacify the merciless cargo fmt
- remove dead TUI add screen code
- cargo bp enable command
- write_bp_features_to_doc uses regular TOML table instead of inline table
- Merge pull request #87 from nikomatsakis/do-not-default-to-gui
- Fix detail view not scrolling when selection moves off screen

## [0.4.12](https://github.com/battery-pack-rs/battery-pack/compare/battery-pack-v0.4.11...battery-pack-v0.4.12) - 2026-04-17

### Added

- add battery-pack-cli binary crate for cargo install

### Other

- give cargo-bp its own README, refine battery-pack README ([#86](https://github.com/battery-pack-rs/battery-pack/pull/86))
- rename battery-pack-cli crate to cargo-bp

## [0.4.11](https://github.com/battery-pack-rs/battery-pack/compare/battery-pack-v0.4.10...battery-pack-v0.4.11) - 2026-04-13

### Other

- refactor test: change tests to use snapbox instead of expect-test ([#80](https://github.com/battery-pack-rs/battery-pack/pull/80))
- *(deps)* upgrade ratatui to 0.30 and enable snapbox term-svg ([#81](https://github.com/battery-pack-rs/battery-pack/pull/81))

## [0.4.10](https://github.com/battery-pack-rs/battery-pack/compare/battery-pack-v0.4.9...battery-pack-v0.4.10) - 2026-04-03

### Fixed

- include dev/build deps in feature resolution ([#76](https://github.com/battery-pack-rs/battery-pack/pull/76))
- *(cli-battery-pack)* move snapbox to dev-dependencies, update README ([#77](https://github.com/battery-pack-rs/battery-pack/pull/77))

## [0.4.9](https://github.com/battery-pack-rs/battery-pack/compare/battery-pack-v0.4.8...battery-pack-v0.4.9) - 2026-04-02

### Added

- add page and jump scrolling to preview ([#66](https://github.com/battery-pack-rs/battery-pack/pull/66))
- *(cli)* Expand capabilities ([#67](https://github.com/battery-pack-rs/battery-pack/pull/67))

### Fixed

- force validate_templates to use non_interactive mode ([#70](https://github.com/battery-pack-rs/battery-pack/pull/70))
- remove double panic hook from `tui.rs`

### Other

- update Cargo.lock dependencies

## [0.4.8](https://github.com/battery-pack-rs/battery-pack/compare/battery-pack-v0.4.7...battery-pack-v0.4.8) - 2026-03-13

### Other

- refactor bphelper-cli and narrow battery-pack dependency ([#48](https://github.com/battery-pack-rs/battery-pack/pull/48))

## [0.4.7](https://github.com/battery-pack-rs/battery-pack/compare/battery-pack-v0.4.6...battery-pack-v0.4.7) - 2026-03-12

### Added

- with_template uses bp-managed, move discovery to bphelper-manifest
- wire bp-managed resolution into template generation
- implement bp-managed dependency resolution

### Fixed

- reject any extra keys alongside bp-managed, not just version

### Other

- *(template)* Use dotted key syntax for `bp-managed` dependencies ([#56](https://github.com/battery-pack-rs/battery-pack/pull/56))
- remove unused resolve_bp_managed file-walking wrapper
- resolve bp-managed in all Cargo.toml files within project dir
- use expect-test snapshots for bp-managed resolution output
- verify preview resolves bp-managed deps
- move bp-managed resolution into shared render pipeline
- write bp metadata as inline tables instead of dotted sub-tables
- add managed-battery-pack fixture with bp-managed deps

## [0.4.6](https://github.com/battery-pack-rs/battery-pack/compare/battery-pack-v0.4.5...battery-pack-v0.4.6) - 2026-03-12

### Added

- template preview — render and display templates without generating a project ([#45](https://github.com/battery-pack-rs/battery-pack/pull/45))

### Fixed

- remove unused variables param from render_template_dir ([#46](https://github.com/battery-pack-rs/battery-pack/pull/46))

## [0.4.5](https://github.com/battery-pack-rs/battery-pack/compare/battery-pack-v0.4.4...battery-pack-v0.4.5) - 2026-03-05

### Added

- add with_template authoring template
- add --define flag to cargo bp new for setting placeholder values
- replace cargo-generate with MiniJinja template engine
- validate templates in cargo bp validate

### Fixed

- accept exact name "battery-pack" in validate_spec ([#40](https://github.com/battery-pack-rs/battery-pack/pull/40))

### Other

- remove stale hooks ignore from default template config
- tighten template engine visibility and improve bp-template.toml handling
- add unit tests for template engine core logic

## [0.4.4](https://github.com/battery-pack-rs/battery-pack/compare/battery-pack-v0.4.3...battery-pack-v0.4.4) - 2026-03-03

### Added

- *(battery-pack)* add validate() to authoring template

## [0.4.3](https://github.com/battery-pack-rs/battery-pack/compare/battery-pack-v0.4.2...battery-pack-v0.4.3) - 2026-03-02

### Added

- Add aliases for `List`, `Show`, and `Status` subcommands.
- *(tui)* handle Ctrl+C as quit
- --path flag for sync/status, bare `cargo bp` launches TUI
- error screen for network failures in TUI
- dep_kind cycling and feature-dependency toggle constraint
- implement docgen with bphelper-build crate and 14 tests
- implement cargo bp status with version warnings
- wire --crate-source through all discovery subcommands
- implement --crate-source flag for local workspace discovery
- add repository warning to validate, plus tests
- implement cross-pack crate merging
- add cli.validate.* spec paragraphs and integration tests
- add cargo bp validate and rewrite spec/manifest layer
- implement option 3 — sync-based battery packs with sets

### Fixed

- fix a lot of clippy lints
- *(tui)* restore terminal and cursor on error exit and panic
- propagate cargo bp sync errors instead of silently discarding
- correct pre-existing test failures in bphelper-manifest
- remove .clone() on Copy type, use BTreeSet for feature lookup
- metadata location abstraction + dep-kind routing + hidden filtering
- repair 5 invalid tracey references, coverage 39%→41%
- give clear error when cargo bp validate runs from workspace root
- handle empty parent path in find_workspace_manifest

### Other

- *(typos)* fix typos
- Merge pull request #4 from jlizen/fix-terminal-exit
- TUI polish — dedup render/test helpers, iterator for selectable_items
- extract CrateEntry::new constructor (2 copies)
- extract wait_for_enter helper (3 copies)
- extract list_nav helper for non-wrapping ListState movement
- TUI code review cleanup — dedup, idiom fixes, test helpers
- TUI code review cleanup — dedup, idiom fixes, test helpers
- tests andsuch
- review fixes — merge non-additive spec rules, fix bugs, dedup
- Add missing [verify] tags for spec coverage
- eliminate CargoManifest, reuse BatteryPackSpec from bphelper-manifest
- shared reqwest client via OnceLock
- deduplicate workspace ref and dep writing patterns
- single read-modify-write for workspace Cargo.toml in add_battery_pack
- add group2 add tests and list integration tests
- add [impl] tags + [verify] tests for 4 existing rules, fix 2 invalid refs
- sync behavior — add [impl] tags + tests
- TOML preservation round-trip tests
- add tracey [impl] tags for format and cli spec rules
- rename 'set' to 'feature' in CLI, remove error-battery-pack
- clean up cargo bp add TUI and interactive picker

## [0.3.0](https://github.com/battery-pack-rs/battery-pack/releases/tag/battery-pack-v0.3.0) - 2026-01-23

### Added

- show examples in `cargo bp show` with --path support
- auto-generate battery pack documentation from cargo metadata
- interactive template selection for `cargo bp new`
- add interactive TUI for `cargo bp list` and `cargo bp show`
- add search and show commands to cargo bp CLI
- cargo bp new downloads from crates.io CDN

### Other

- fmt, bump versions
- rename `cargo bp search` to `cargo bp list`
- update cargo-toml metadata
