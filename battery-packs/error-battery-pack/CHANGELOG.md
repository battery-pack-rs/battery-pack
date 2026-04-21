# Changelog

All notable changes to this project will be documented in this file.

The format is based on [Keep a Changelog](https://keepachangelog.com/en/1.0.0/),
and this project adheres to [Semantic Versioning](https://semver.org/spec/v2.0.0.html).

## [Unreleased]

## [0.6.0](https://github.com/battery-pack-rs/battery-pack/compare/error-battery-pack-v0.5.3...error-battery-pack-v0.6.0) - 2026-04-21

### Added

- implement dynamic shell completions using clap_complete for CLI commands and arguments. ([#99](https://github.com/battery-pack-rs/battery-pack/pull/99))

### Other

- Remove build.rs hooks, add cargo bp check for drift detection

## [0.5.3](https://github.com/battery-pack-rs/battery-pack/compare/error-battery-pack-v0.5.2...error-battery-pack-v0.5.3) - 2026-03-13

### Other

- refactor bphelper-cli and narrow battery-pack dependency ([#48](https://github.com/battery-pack-rs/battery-pack/pull/48))

## [0.5.2](https://github.com/battery-pack-rs/battery-pack/compare/error-battery-pack-v0.5.1...error-battery-pack-v0.5.2) - 2026-03-05

### Other

- *(files)* move battery-packs into their own directory to make filesystem better

## [0.5.1](https://github.com/battery-pack-rs/battery-pack/compare/error-battery-pack-v0.5.0...error-battery-pack-v0.5.1) - 2026-03-03

### Added

- *(cli + error + logging)* expose validate() function

## [0.5.0](https://github.com/battery-pack-rs/battery-pack/compare/error-battery-pack-v0.4.0...error-battery-pack-v0.5.0) - 2026-03-03

### Other

- *(error-battery-pack)* migrate to battery pack format spec
- update error-battery-pack to build with current codebase
