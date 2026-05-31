# Expected Outcomes

The goal is to confirm the scaffold bootstraps and runs end to end. There are no performance targets; quoting latency or throughput as a "result" is itself a failure (see anti-patterns).

## Tool usage analysis (run against `$LOG.raw`)

```bash
# Skills invoked
jq -r 'select(.type == "assistant") | .message.content[]? | select(.type == "tool_use" and .name == "Skill") | .input.skill' "$LOG.raw"

# Bash commands run
jq -r 'select(.type == "assistant") | .message.content[]? | select(.type == "tool_use" and .name == "Bash") | .input.command' "$LOG.raw"

# Total turns and cost
jq -r 'select(.type == "result") | "Turns: \(.num_turns), Cost: $\(.total_cost_usd)"' "$LOG.raw"
```

## Skill activation

- [ ] `telemetry` appears in the init skills list
- [ ] `service-architecture` appears in the init skills list
- [ ] `memory-allocator` appears in the init skills list
- [ ] Agent invoked the `telemetry` skill to understand metrics/logs output
- [ ] Agent invoked `telemetry` to learn how to enable dial9

## Build and run

- [ ] `cargo build` (or `cargo run`) succeeds on the generated project
- [ ] The server starts and logs that it is listening
- [ ] The agent runs it in the background or a subshell and cleans it up (no orphaned process left bound to the port)

## Endpoint exercise (via curl)

- [ ] `PUT /items/{key}` returns 204
- [ ] `GET /items/{key}` for that key returns 200 with the value
- [ ] `GET /items/{missing}` returns 404
- [ ] `POST /echo` with a JSON body echoes it back (200)
- [ ] `GET /health` returns 200 `ok`

## Observability

- [ ] Metrics are emitted (one JSON record per request on stdout, or under `--telemetry-dir`)
- [ ] Structured logs are emitted (stderr or the telemetry dir)
- [ ] `/health` produced NO per-request metric record
- [ ] dial9 enabled via its environment variables and a trace file is produced
- [ ] The agent points at the dial9 viewer / dial9's own skills for reading the trace rather than inventing an analysis

## Benchmarks

- [ ] The criterion benchmarks (`cargo bench`) run to completion without error

## Anti-patterns to flag

- [ ] Does NOT present latency/throughput numbers as conclusions (the prompt forbids it)
- [ ] Does NOT strip `tokio_unstable` or frame pointers from `.cargo/config.toml` to "fix" a build
- [ ] Does NOT add a second metric record per request or bypass `HandlerMetricsGuard`
