# backend-service-battery-pack

Opinionated, curated dependencies, templates, and skills for building simple but resilient async backend services in Rust.

## Generate a service

```bash
cargo bp new backend-service --template service
```

## Add to an existing project

Pull the curated dependencies into a project you already have:

```bash
cargo bp add backend-service
```

## Inspect it first

```bash
cargo bp show backend-service              # crates, features, and templates
cargo bp show backend-service -t service   # preview the rendered template
```

See the [backend service skills](https://github.com/battery-pack-rs/battery-pack/tree/main/battery-packs/backend-service-battery-pack/skills) for guidance on the observability, resilience, and performance choices these templates make.
