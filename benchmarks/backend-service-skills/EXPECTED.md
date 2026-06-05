# Expected Outcomes

A single run covers two phases: bootstrap-and-observe, then layer on production features. Phase 1 confirms the generated service works and is observable (no performance targets, quoting latency or throughput as a result is itself a failure). Phase 2 checks the agent can follow the skill breadcrumbs; the named pitfalls are the highest-signal items because they are exactly what the thin breadcrumbs point at.

## Tool usage (run against `$LOG.raw`)

```bash
# Skills invoked
jq -r 'select(.type == "assistant") | .message.content[]? | select(.type == "tool_use" and .name == "Skill") | .input.skill' "$LOG.raw"
# Bash commands run
jq -r 'select(.type == "assistant") | .message.content[]? | select(.type == "tool_use" and .name == "Bash") | .input.command' "$LOG.raw"
```

## Skill activation

- [ ] `telemetry` and `service-architecture` appear in the init skills list and are invoked
- [ ] Agent uses dial9's agent tooling/skills (`dial9 agents`), not the `dial9 serve` web UI

## Phase 1: bootstrap and observe

- [ ] `cargo build`/`cargo run` succeeds; the server starts and logs that it is listening
- [ ] Server is run in the background with output redirected, and killed afterward (no orphaned process, session does not hang)
- [ ] `PUT /items/{key}` 204, `GET /items/{key}` 200 with the value, `GET /items/{missing}` 404, `POST /echo` 200 echo, `GET /health` 200
- [ ] Structured logs and one wide-event metric per request are emitted (`/health` emits none)
- [ ] dial9 enabled via env, trace files produced, and the agent inspects them with dial9's tooling rather than inventing analysis
- [ ] `cargo bench` runs all handler benchmarks to completion
- [ ] No performance conclusions are drawn

## Phase 2: feature follow-ups

- [ ] **Per-client rate limit:** `PeerIpKeyExtractor` + `into_make_service_with_connect_info::<SocketAddr>()`; pitfall: a periodic `retain_recent()` task so the keyed store does not leak
- [ ] **Read cache:** `moka` `future::Cache` fronting the store reads; pitfall: the consistency/staleness tradeoff, TTL sized to a staleness budget
- [ ] **Load shedding:** tower `ConcurrencyLimitLayer` + `LoadShedLayer` mapped to 503; pitfall: not a bare concurrency limit (queues without bound); placed inside the recorder, `/health` bypassed
- [ ] Each change states its pitfall, as the prompt asked

## Anti-patterns to flag

- [ ] Leaves a server process running or starts `dial9 serve` and blocks the session
- [ ] Strips `tokio_unstable`/frame pointers to "fix" a build
- [ ] Uses a concurrency limit without load-shed
