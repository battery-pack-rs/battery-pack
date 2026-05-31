---
name: memory-allocator
description: Choosing and profiling the global allocator (jemalloc, mimalloc, system) in the network-service scaffold
---

# Memory Allocator

The `allocator` choice at generation time sets the `#[global_allocator]` in `src/main.rs`. It is a build-time decision, not a runtime knob.

## Which one

- **jemalloc** (`tikv-jemallocator`, the default): lower fragmentation and less allocator contention under the multi-threaded Tokio runtime, where many worker threads allocate concurrently. It does not build on MSVC, so both the dependency and the `#[global_allocator]` static are gated with `cfg(not(target_env = "msvc"))`.
- **mimalloc**: a reasonable alternative with similar goals; pick it if you have measured it winning for your workload or you need a Windows-MSVC target.
- **system**: the platform default. Choose it when you want zero extra dependencies or are debugging an allocator-sensitive issue and want a baseline.

Do not swap allocators to "optimize memory" without measuring: the right choice is workload-dependent, and the default is a safe starting point.

## Heap profiling with dial9

When the `dial9` feature is on, the global allocator is wrapped in `Dial9Allocator`, which is a passthrough until a memory profiler is installed in `main`. Set `DIAL9_MEMORY_PROFILE_ENABLED=true` to install it at startup and capture allocation samples. The install is programmatic (not via `Dial9Config::from_env`, which does not yet wire the profiler). dial9's own tooling reads the resulting profile; this skill does not cover that analysis.

## Invariants

- Keep the `cfg(not(target_env = "msvc"))` gate on both the jemalloc dependency and its static, or Windows builds break.
- There is exactly one `#[global_allocator]`. When `dial9` is on, the allocator must be the `Dial9Allocator` wrapper around your chosen allocator, not the bare allocator, or heap profiling silently records nothing.

## References

- jemalloc: https://docs.rs/tikv-jemallocator, mimalloc: https://docs.rs/mimalloc
- Profiling analysis and the viewer: see the dial9 references in the `telemetry` skill.
