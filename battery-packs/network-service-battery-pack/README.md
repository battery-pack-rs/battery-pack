# network-service-battery-pack

Curated dependencies, templates, and skills for building simple but resilient async network services in Rust.

## Generate a service

```bash
cargo bp new network-service --template service
```

## Add to an existing project

Pull the curated dependencies into a project you already have:

```bash
cargo bp add network-service
```

## Inspect it first

```bash
cargo bp show network-service              # crates, features, and templates
cargo bp show network-service -t service   # preview the rendered template
```

See [skills/](skills/) for guidance on the observability, resilience, and performance choices these templates make.
