# Internal future-work

- OTEL/OTLP metrics: once metrique ships an OpenTelemetry formatter, offer it alongside `Json::new()` / `Emf` in the metrics init (CloudWatch supports OTEL metrics natively, public preview Apr 2026).
- metrique queue self-metrics (metrique_queue_overflows / queue_len / idle_percent): Currently these only surface via the metrics-rs bridge, which is not good enough to depend on yet. Revisit once improved: https://github.com/awslabs/metrique/issues/205. 
- metrique sysinfo integration: https://github.com/awslabs/metrique/issues/255
- Async-processor template (`templates/processor/`): a long-running consumer that pulls work from an input source and processes it, reusing the service template's allocator/telemetry/shutdown/downstream stack. Input-source variants to cover: HTTP polling, Redis BLPOP (needs a dedicated connection, not the shared ConnectionManager), Kafka, and Postgres (LISTEN/NOTIFY or skip-locked polling). Deferred from the initial release to keep the first pack focused on the request-response service.
- RedisJSON support for the redis downstream.
- Kafka downstream/input source.
- gRPC stack.
- Additional serialization formats beyond JSON (protobuf, maybe postcard, maybe flatc)
