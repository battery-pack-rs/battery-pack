# Changelog

All notable changes to this project will be documented in this file.

The format is based on [Keep a Changelog](https://keepachangelog.com/en/1.0.0/),
and this project adheres to [Semantic Versioning](https://semver.org/spec/v2.0.0.html).

## [Unreleased]

## [0.6.0](https://github.com/battery-pack-rs/battery-pack/compare/cli-battery-pack-v0.5.1...cli-battery-pack-v0.6.0) - 2026-04-21

### Added

- implement dynamic shell completions using clap_complete for CLI commands and arguments. ([#99](https://github.com/battery-pack-rs/battery-pack/pull/99))

### Fixed

- Propagate bp-managed errors and show full validation output

### Other

- Remove build.rs hooks, add cargo bp check for drift detection

## [0.5.1](https://github.com/battery-pack-rs/battery-pack/compare/cli-battery-pack-v0.5.0...cli-battery-pack-v0.5.1) - 2026-04-13

### Other

- *(deps)* upgrade ratatui to 0.30 and enable snapbox term-svg ([#81](https://github.com/battery-pack-rs/battery-pack/pull/81))

## [0.5.0](https://github.com/battery-pack-rs/battery-pack/compare/cli-battery-pack-v0.4.4...cli-battery-pack-v0.5.0) - 2026-04-03

### Fixed

- *(cli-battery-pack)* move snapbox to dev-dependencies, update README ([#77](https://github.com/battery-pack-rs/battery-pack/pull/77))

## [0.4.4](https://github.com/battery-pack-rs/battery-pack/compare/cli-battery-pack-v0.4.3...cli-battery-pack-v0.4.4) - 2026-04-02

### Added

- *(cli)* Expand capabilities ([#67](https://github.com/battery-pack-rs/battery-pack/pull/67))

## [0.4.3](https://github.com/battery-pack-rs/battery-pack/compare/cli-battery-pack-v0.4.2...cli-battery-pack-v0.4.3) - 2026-03-13

### Other

- refactor bphelper-cli and narrow battery-pack dependency ([#48](https://github.com/battery-pack-rs/battery-pack/pull/48))

## [0.4.2](https://github.com/battery-pack-rs/battery-pack/compare/cli-battery-pack-v0.4.1...cli-battery-pack-v0.4.2) - 2026-03-05

### Other

- *(files)* move battery-packs into their own directory to make filesystem better

## [0.4.1](https://github.com/battery-pack-rs/battery-pack/compare/cli-battery-pack-v0.4.0...cli-battery-pack-v0.4.1) - 2026-03-03

### Added

- *(cli + error + logging)* expose validate() function
