# Changelog

All notable changes to this project will be documented in this file.

The format is based on [Keep a Changelog](https://keepachangelog.com/en/1.0.0/),
and this project adheres to [Semantic Versioning](https://semver.org/spec/v2.0.0.html).

## [Unreleased]

## [0.1.1](https://github.com/battery-pack-rs/battery-pack/compare/backend-service-battery-pack-v0.1.0...backend-service-battery-pack-v0.1.1) - 2026-06-03

### Added

- *(backend-service-battery-pack)* opinionated battery pack that scaffolds an async backend service (axum) with metrique wide-event metrics, structured tracing, optional dial9 flight-recorder integration, jemalloc/mimalloc allocator selection, graceful shutdown with drain metrics, an in-memory or HTTP-forwarding downstream, and a Tower middleware stack, plus telemetry, memory-allocator, and service-architecture skills ([#142](https://github.com/battery-pack-rs/battery-pack/pull/142))

### Other

- *(backend-service-battery-pack)* rename from network-service-battery-pack ([#142](https://github.com/battery-pack-rs/battery-pack/pull/142))
