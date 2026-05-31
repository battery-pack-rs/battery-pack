# Ideas for future-work

- OTEL/OTLP metrics: once metrique ships an OpenTelemetry formatter, offer it alongside `Json::new()` / `Emf` in the metrics init
- metrique queue self-metrics (metrique_queue_overflows / queue_len / idle_percent): Currently these only surface via the metrics-rs bridge, which is not good enough to depend on yet. Revisit once improved: https://github.com/awslabs/metrique/issues/205. 
- metrique sysinfo integration: https://github.com/awslabs/metrique/issues/255
- Redis, postgres, kafka, grpc
- Async-processor template (`templates/processor/`): a long-running consumer that pulls work from an input source and processes it (probably kafka, redis lists, maybe postgres?)
- Additional serialization formats beyond JSON (protobuf, maybe postcard, maybe flatc)
