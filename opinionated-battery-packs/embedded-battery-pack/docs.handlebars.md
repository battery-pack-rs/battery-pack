# embedded-battery-pack

Opinionated battery pack for embedded Rust — curates HALs, concurrency
frameworks, drivers, and no_std utilities from the
[awesome-embedded-rust](https://github.com/rust-embedded/awesome-embedded-rust)
ecosystem.

## Quick start

```bash
cargo bp add embedded -F stm32f4 -F embassy -F panic-probe -F defmt-logging
```

## Categories

{{#each categories}}
### {{this.title}}

{{this.description}}

| Feature | Description |
|---------|-------------|
{{#each this.items}}
| `{{this.name}}` | {{this.description}} |
{{/each}}

{{/each}}

## Template

| Name | Description |
|------|-------------|
{{#each templates}}
| `{{this.name}}` | {{this.description}} |
{{/each}}
