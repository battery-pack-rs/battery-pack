---
name: service-architecture
description: Request lifecycle, Tower layer ordering, and scaling choices for the network-service scaffold
---

# Service Architecture

How a request flows through this service, why the middleware stack is ordered the way it is, and how to scale it. The stack lives in `src/routes.rs::router`.

## Why axum, and when to drop to hyper

The scaffold uses axum: its extractors and `Router` make handlers and middleware cheap to write, at a small per-request cost from cloning state and running extractors. Drop to raw hyper (the `hyper-util` examples) only when you need what axum hides: custom connection lifecycle, protocol upgrades, or the last allocation off a hot path.

## The layer stack (read this before reordering it)

Layers wrap the router, so the last one added is the outermost and a request flows through them top down. The order is deliberate. Several layers are optional features, so a given service may not have all of them; the list shows the full stack with every feature enabled:

1. **SetRequestId / PropagateRequestId** (outermost): honor an incoming `x-request-id` or mint one, and echo it back. It runs first so everything inside has a correlation id.
2. **TraceLayer**: opens the request span carrying that id, so all handler logs correlate.
3. **telemetry_middleware**: opens the wide-event metric. It is the outermost *status* recorder, so it captures the final status even when an inner layer (not the handler) produced it.
4. **rate limit (GovernorLayer)** (when enabled): inside the recorder, so a rejected 429 is still counted as a request.
5. **CatchPanic** (when enabled): converts a handler panic into a 500 inside the recorder, so the 500 is recorded and the panic does not unwind through the metric guard.
6. **OnEarlyDrop** (when enabled): diagnostic only, logs a warning if the response future or body is dropped before completion. It does not alter the status.
7. **timeout / body-timeout** (when enabled): bound total request time (408) and slow-body reads.
8. **DefaultBodyLimit** (innermost): rejects oversized bodies with 413 before they are buffered into memory.

Anything that can *produce a status* (rate limit, panic, timeout, body limit) sits inside `telemetry_middleware` so the status is recorded, while request-id and tracing sit outside it so the id and span exist when it runs. `/health` is mounted on a separate router with none of this, so probes are not metered or rate-limited.

## Scaling the rate limit

The scaffold ships a single global token bucket (`GlobalKeyExtractor`). To limit per client instead:

- Swap `GlobalKeyExtractor` for `PeerIpKeyExtractor`.
- Serve with `into_make_service_with_connect_info::<SocketAddr>()` so the extractor can read the peer address from request extensions (tests then need a `SocketAddr` injected).
- Run a periodic `governor_conf.limiter().retain_recent()` task. The keyed store grows one entry per key and is never auto-evicted, so without this it leaks memory under a wide key space.

No new dependency or feature is needed: `PeerIpKeyExtractor` ships with the `tower_governor` the `rate-limit` feature already pulls, so this is a code change.

Behind a load balancer the peer IP is the balancer, so key off a trusted forwarded-for header instead of the socket address.

## Optional additions

These are deliberately not scaffolded; they are workload-specific and easy to misuse.

- **Read caching** (the pack's `cache` feature, `moka`): use its `future::Cache` in front of the HTTP forwarder backend. A cache changes the service's consistency contract; its failure mode is silent stale reads. Size the TTL against an explicit staleness budget.
- **Load shedding** (the pack's `load-shed` feature, tower's limit and load-shed layers): the scaffold already *measures* concurrency via the always-on `IN_FLIGHT` counter. To *bound* it, add `ConcurrencyLimitLayer` paired with `LoadShedLayer` (a bare concurrency limit queues requests without bound; the pair sheds with a 503). Size the cap off observed `IN_FLIGHT` rather than a guess. Place it inside the recorder so the 503 is counted, but outside the rate limit and timeout.

## Invariants

- Do not move a status-producing layer (rate limit, catch-panic, timeout, body limit) outside `telemetry_middleware`, or that status stops being recorded.
- Keep request-id and tracing outside `telemetry_middleware`; it reads the id they set.
- Keep `DefaultBodyLimit` innermost so oversized bodies are rejected before buffering.
- A new route needs a matching arm in `classify_operation` (`src/middleware.rs`) or it records as `Other`.
- `/health` stays on the bypass router: no metrics, no rate limit.

## References

- axum: https://docs.rs/axum, hyper-util examples: https://github.com/hyperium/hyper-util
- tower_governor: https://docs.rs/tower_governor, moka: https://docs.rs/moka, tower (load-shed, concurrency-limit): https://docs.rs/tower
- Observability of the stack: see the `telemetry` skill.
