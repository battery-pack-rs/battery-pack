# embedded-battery-pack

A [battery pack](https://crates.io/crates/battery-pack) for embedded Rust — curates HALs, concurrency frameworks, drivers, and no\_std utilities from the [awesome-embedded-rust](https://github.com/rust-embedded/awesome-embedded-rust) ecosystem.

## Quick start

```bash
cargo bp new embedded
```

This scaffolds a project with your chosen HAL, concurrency model, and peripherals already wired up.

To add embedded crates to an existing project instead:

```bash
cargo bp add embedded -F stm32f4 -F embassy -F panic-probe -F defmt-logging
```

## Learn more

This battery pack is inspired by and draws from the [awesome-embedded-rust](https://github.com/rust-embedded/awesome-embedded-rust) list. For more about the embedded Rust ecosystem and how to get involved, visit the [Embedded Rust Working Group](https://rust-embedded.org/).
