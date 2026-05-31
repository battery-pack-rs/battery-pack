# {{ project_name }}

{{ description }}

## Run

```bash
cargo run
```

With no `--downstream-url`, items are stored in memory, so this works with no dependencies. Point it at a downstream to forward items there instead:

```bash
cargo run -- --downstream-url http://127.0.0.1:3001
```

A second instance can be that downstream. Use `--port` (the `PORT` env var works too) to move each one:

```bash
# terminal 1 — backing store (in-memory):
cargo run -- --port 3001

# terminal 2 — forwarder:
cargo run -- --port 3000 --downstream-url http://127.0.0.1:3001
```

The service binds `127.0.0.1:3000` by default; `--port` overrides just the port. Logs go to stderr and metrics to stdout. Set `--telemetry-dir <dir>` to roll both into files there instead.

## Endpoints

```bash
curl localhost:3000/health
curl -X PUT localhost:3000/items/greeting --data 'hello'
curl localhost:3000/items/greeting
curl -X POST localhost:3000/echo -H 'content-type: application/json' --data '{"message":"hi"}'
```

{%- if benchmarks %}

## Benchmark

```bash
cargo bench
```

Benchmarks hit each handler in-process over the in-memory store. Set `BENCH_DOWNSTREAM_URL` to instead measure the forwarding path against a running instance:

```bash
BENCH_DOWNSTREAM_URL=http://127.0.0.1:3001 cargo bench
```
{%- endif %}
{%- if dial9 %}

## Debugging with dial9

Run with the prod-ready flight recorder enabled (Tokio telemetry, cpu stack traces, heap profiling, and more) by enabling Dial9 environment variables.
Locally, you can do this by sourcing by sourcing `dial9.env`:

```bash
set -a; source dial9.env; set +a
cargo run
```

Then inspect the traces with the dial9 viewer:

```bash
cargo install dial9
dial9 serve --local-dir /tmp/dial9-traces
```

Or analyze it with the agent toolkit:

```bash
dial9 agents
```

{%- endif %}
