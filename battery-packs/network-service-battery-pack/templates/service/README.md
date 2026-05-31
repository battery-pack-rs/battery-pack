# {{ project_name }}

{{ description }}

## Run

{%- if downstream == "redis" %}
Start Redis with Docker or Podman, then run the service:

```bash
docker run --rm -p 6379:6379 redis:7-alpine   # or: podman run --rm -p 6379:6379 redis:7-alpine
cargo run
```

To run without Redis, use the in-memory store:

```bash
cargo run -- --in-memory
```
{%- elif downstream == "http-service" %}
Point the service at the downstream and run it:

```bash
cargo run -- --downstream-url http://127.0.0.1:3001
```
{%- else %}
```bash
cargo run
```
{%- endif %}

The service binds `127.0.0.1:3000` by default. Logs (JSON) go to stderr and metrics (JSON) to stdout.

## Endpoints

```bash
curl localhost:3000/health
{%- if downstream != "none" %}
curl -X PUT localhost:3000/items/greeting --data 'hello'
curl localhost:3000/items/greeting
{%- endif %}
```

## Test

```bash
cargo test
```
{%- if downstream == "redis" %}

`redis_round_trip` starts a real Redis through testcontainers. It needs a Docker or
Podman-compatible runtime (`DOCKER_HOST` selects which) and skips with a warning when none
is reachable.
{%- endif %}
{%- if benchmarks %}

## Benchmark

```bash
cargo bench
```
{%- endif %}
{%- if dial9 %}

## Debugging with dial9

This service records a Tokio flight recorder trace when `DIAL9_ENABLED=true`. Inspect traces
with the dial9 viewer:

```bash
cargo install dial9
dial9 serve --local-dir /tmp/dial9-traces
```
{%- endif %}
