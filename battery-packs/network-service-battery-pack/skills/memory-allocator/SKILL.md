---
name: memory-allocator
description: Choosing and profiling the global allocator (jemalloc, mimalloc, system) in the network-service scaffold
---

# Memory Allocator

The `allocator` choice at generation time sets the `#[global_allocator]` in `src/main.rs`. It is a build-time decision, not a runtime knob.

## Which one

- **jemalloc** (`tikv-jemallocator`, the default): per-thread arenas cut lock contention and keep fragmentation low when many Tokio workers allocate at once, the common server pattern, which tends to give steadier RSS and tail latency under sustained churn. The cost is a higher idle baseline and a larger binary. It does not build on MSVC, so both the dependency and the `#[global_allocator]` static are gated with `cfg(not(target_env = "msvc"))`.
- **mimalloc**: similar anti-fragmentation goals with very fast small-allocation paths and often a smaller footprint than jemalloc. A strong pick for allocation-heavy workloads or when you need a Windows-MSVC target.
- **system**: the platform allocator (glibc on Linux). No extra dependency and the simplest build, but glibc's per-thread arenas can grow RSS under heavy multi-threaded churn. Use it for low-concurrency services, when you want zero added dependencies, or as a baseline when isolating an allocator-sensitive bug.

Measure before switching: the best choice is workload-dependent, throughput, memory footprint, and tail latency trade against each other, and jemalloc is a safe default for a multi-threaded service.

## Heap profiling with dial9

When the `dial9` feature is on, the global allocator is wrapped in `Dial9Allocator`, which is a passthrough until a memory profiler is installed in `main`. Set `DIAL9_MEMORY_PROFILE_ENABLED=true` to install it at startup and capture allocation samples. dial9's own tooling reads the resulting profile; this skill does not cover that analysis.

## Invariants

- Keep the `cfg(not(target_env = "msvc"))` gate on both the jemalloc dependency and its static, or Windows builds break.
- There is exactly one `#[global_allocator]`. When `dial9` is on, the allocator must be the `Dial9Allocator` wrapper around your chosen allocator, not the bare allocator, or heap profiling silently records nothing.

## References

- jemalloc: https://docs.rs/tikv-jemallocator, mimalloc: https://docs.rs/mimalloc
- Profiling analysis and the viewer: see the dial9 references in the `telemetry` skill.
