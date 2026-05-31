# Expected Outcomes

The skills give breadcrumbs, not recipes, so the bar is: did the agent reach a correct implementation of each feature, and did it name and avoid the specific pitfall the skill flagged? The pitfalls are the highest-signal checks: they are exactly what the thin breadcrumb is pointing at.

## Skill activation

- [ ] `service-architecture` appears in the init skills list and is invoked
- [ ] Agent read the referenced crates' docs (moka, tower_governor, tower) rather than guessing APIs

## Per-client rate limiting

- [ ] Swaps `GlobalKeyExtractor` for `PeerIpKeyExtractor`
- [ ] Serves with `into_make_service_with_connect_info::<SocketAddr>()` so the peer address is available
- [ ] **Pitfall:** adds a periodic `retain_recent()` cleanup task (the keyed store grows one entry per key and is never auto-evicted)
- [ ] Notes that behind a load balancer the key should come from a trusted forwarded-for header, not the socket peer

## Read-through cache

- [ ] Uses `moka` (`future::Cache`)
- [ ] Applies the cache to the HTTP forwarder path only, NOT the in-memory store
- [ ] **Pitfall:** acknowledges the consistency/staleness tradeoff and sizes the TTL against a staleness budget; invalidates or writes through on `set`
- [ ] Adds `cache_hit` / `cache_miss` to the metrics

## Load shedding

- [ ] Uses tower's `ConcurrencyLimitLayer` paired with `LoadShedLayer`
- [ ] **Pitfall:** does NOT use a bare concurrency limit (which queues unboundedly); the pair is what sheds
- [ ] Maps the shed error to 503 (e.g. via `HandleErrorLayer`)
- [ ] Places it inside `telemetry_middleware` so the 503 is recorded, and keeps `/health` bypassing it
- [ ] Sizes the concurrency cap off the observed `IN_FLIGHT` counter rather than a guessed constant

## General

- [ ] Each change compiles conceptually (correct types, feature flags, layer placement)
- [ ] The agent states the one decisive pitfall per feature, as the prompt asked

## Anti-patterns to flag

- [ ] Does NOT cache the in-memory store
- [ ] Does NOT add a concurrency limit without load-shed
- [ ] Does NOT move a status-producing layer outside the telemetry recorder
- [ ] Does NOT restate large chunks of the crates' APIs as if from memory without reading the docs
