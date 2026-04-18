# Changelog

All notable changes to this project will be documented in this file.

The format is based on [Keep a Changelog](https://keepachangelog.com/en/1.0.0/),
and this project adheres to [Semantic Versioning](https://semver.org/spec/v2.0.0.html).

## [Unreleased]

## [0.4.13](https://github.com/battery-pack-rs/battery-pack/compare/cargo-bp-v0.4.12...cargo-bp-v0.4.13) - 2026-04-18

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

## [0.4.12](https://github.com/battery-pack-rs/battery-pack/compare/cargo-bp-v0.4.11...cargo-bp-v0.4.12) - 2026-04-17

### Other

- give cargo-bp its own README, refine battery-pack README ([#86](https://github.com/battery-pack-rs/battery-pack/pull/86))
