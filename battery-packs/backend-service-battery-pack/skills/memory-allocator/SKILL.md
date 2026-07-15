---
name: memory-allocator
description: Choosing and profiling the global allocator (jemalloc, mimalloc, system) in the backend-service `service` template
---

# Memory Allocator

The `allocator` choice at generation time sets the `#[global_allocator]` in `src/main.rs`. It is a build-time decision, not a runtime knob.

## Which one

- **jemalloc** (`tikv-jemallocator`, the default): a mature, arena-based allocator with strong fragmentation control and steady RSS under sustained multi-threaded load, plus deep runtime tuning (`MALLOC_CONF`) and built-in heap profiling. The cost is a higher idle baseline and a larger binary. It does not build on MSVC, so both the dependency and the `#[global_allocator]` static are gated with `cfg(not(target_env = "msvc"))`.
- **mimalloc**: newer and smaller, with very fast small-allocation paths and typically lower memory footprint, but far fewer tuning knobs and less of a track record under sustained server load. A strong pick for allocation-heavy workloads or a Windows-MSVC target.
- **system**: the platform allocator (glibc on Linux). No extra dependency and the simplest build, but glibc's per-thread arenas can grow RSS under heavy multi-threaded churn. Use it for low-concurrency services, when you want zero added dependencies, or as a baseline when isolating an allocator-sensitive bug.

jemalloc is the default; switch only after measuring your workload.

## Heap profiling with dial9

When the `dial9` feature is on, the global allocator is wrapped in `Dial9Allocator`, which is a passthrough until a memory profiler is installed in `main`. Set `DIAL9_MEMORY_PROFILE_ENABLED=true` to install it at startup and capture allocation samples. dial9's own tooling reads the resulting profile; this skill does not cover that analysis.

## Invariants

- Keep the `cfg(not(target_env = "msvc"))` gate on both the jemalloc dependency and its static, or Windows builds break.
- There is exactly one `#[global_allocator]`. When `dial9` is on, the allocator must be the `Dial9Allocator` wrapper around your chosen allocator, not the bare allocator, or heap profiling silently records nothing.

## References

- jemalloc: https://docs.rs/tikv-jemallocator, mimalloc: https://docs.rs/mimalloc
- Profiling analysis and the viewer: see the dial9 references in the `telemetry` skill.
