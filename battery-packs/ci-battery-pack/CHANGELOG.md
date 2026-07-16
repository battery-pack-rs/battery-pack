# Changelog

All notable changes to this project will be documented in this file.

The format is based on [Keep a Changelog](https://keepachangelog.com/en/1.0.0/),
and this project adheres to [Semantic Versioning](https://semver.org/spec/v2.0.0.html).

## [Unreleased]

## [0.1.6](https://github.com/battery-pack-rs/battery-pack/compare/ci-battery-pack-v0.1.5...ci-battery-pack-v0.1.6) - 2026-07-16

### Added

- *(packs)* add category metadata to cli, ci, and backend-service packs (Phase 8)
- *(ci)* add security scanning template
- *(ci)* default generated dependency policy
- *(ci)* document generated audit policy
- *(ci)* add optional dependency policy template

### Fixed

- *(ci)* refresh dependency policy snapshots
- *(ci)* remove path filter from dependency-policy pull_request trigger
- *(ci)* warn on wildcard dependencies in policy template
- *(ci)* run generated audit workflow on config changes

### Other

- Fix mdBook links and add link checking
- *(ci)* drop action ref comments from snapshots
- *(ci)* clarify unresolved action redaction
- *(ci)* simplify snapshot normalization
- *(ci)* normalize workflow snapshot pins
- allow NCSA license in dependency policy
- *(ci)* clarify audit issue prompt
- *(ci)* align skill template commands
- *(ci)* simplify optional feature guidance
- *(ci)* tighten security scanning text
- *(ci)* keep audit issue option out of feature table
- add repository license policy

### Security

- fix rustsec vulnerabilities

## [0.1.5](https://github.com/battery-pack-rs/battery-pack/compare/ci-battery-pack-v0.1.4...ci-battery-pack-v0.1.5) - 2026-06-03

### Added

- *(ci-battery-pack)* add optional cross-platform testing job ([#136](https://github.com/battery-pack-rs/battery-pack/pull/136))

### Fixed

- *(test)* make snapshot tests cross-platform for `.exe` ([#138](https://github.com/battery-pack-rs/battery-pack/pull/138))

### Other

- *(battery-pack)* migrate templates to `battery-pack.toml`, drop unused dep ([#142](https://github.com/battery-pack-rs/battery-pack/pull/142))

## [0.1.4](https://github.com/battery-pack-rs/battery-pack/compare/ci-battery-pack-v0.1.3...ci-battery-pack-v0.1.4) - 2026-04-30

### Added

- rename validate_templates to validate
- *(ci-battery-pack)* add validate test, merged snapshot tests for all templates

### Fixed

- rename template Cargo.toml to _Cargo.toml, flip assertions

### Other

- Merge pull request #120 from jlizen/feat/defines-in-show
- *(ci-battery-pack)* replace file list and contains tests with merged snapshots
- *(ci-battery-pack)* expand hints
- *(ci)* restructure README for template merging ([#115](https://github.com/battery-pack-rs/battery-pack/pull/115))

## [0.1.3](https://github.com/battery-pack-rs/battery-pack/compare/ci-battery-pack-v0.1.2...ci-battery-pack-v0.1.3) - 2026-04-22

### Added

- *(ci)* add post-merge hints to CI templates

## [0.1.2](https://github.com/battery-pack-rs/battery-pack/compare/ci-battery-pack-v0.1.1...ci-battery-pack-v0.1.2) - 2026-04-21

### Other

- *(ci)* Clarify defaults/all flag, expand SARIF docs, fix mdbook duplicate guard ([#110](https://github.com/battery-pack-rs/battery-pack/pull/110))

## [0.1.1](https://github.com/battery-pack-rs/battery-pack/compare/ci-battery-pack-v0.1.0...ci-battery-pack-v0.1.1) - 2026-04-21

### Other

- updated the following local packages: battery-pack, battery-pack, battery-pack
