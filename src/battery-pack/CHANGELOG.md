# Changelog

All notable changes to this project will be documented in this file.

The format is based on [Keep a Changelog](https://keepachangelog.com/en/1.0.0/),
and this project adheres to [Semantic Versioning](https://semver.org/spec/v2.0.0.html).

## [Unreleased]

## [0.3.1](https://github.com/battery-pack-rs/battery-pack/compare/battery-pack-v0.3.0...battery-pack-v0.3.1) - 2026-01-28

### Added

- add description placeholder to battery-pack template
- validate battery pack names must end with -battery-pack
- add default template for authoring battery packs

### Fixed

- update default template to use battery-pack correctly
- use generated docs
- special-case "battery-pack" in crate name resolution

### Other

- add README.md for battery-pack crate

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
