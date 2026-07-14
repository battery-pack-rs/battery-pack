# Categories

Categories let you group related items and optionally constrain selection. Without categories, the picker shows a flat list of features and dependencies. With categories, items are grouped into labeled sections — and you can mark some as "pick at most one" to present alternatives.

## Defining a category

Categories are declared in `[package.metadata.battery-pack.categories.<name>]`:

```toml
[package.metadata.battery-pack.categories.allocator]
title = "Global Allocator"
description = "Pick a high-performance allocator (or use the system default)"
pick = "at-most-one"

[package.metadata.battery-pack.categories.middleware]
title = "HTTP Middleware Layers"
description = "Tower-HTTP middleware for your service"
```

Fields:

- **`title`** — display name shown in the picker and docs
- **`description`** (optional) — explanatory text shown under the title
- **`pick`** — `"at-most-one"` or `"any"` (default: `"any"`)

## Assigning items to categories

Features, dependencies, and templates can be assigned to one or more categories using per-item metadata:

```toml
[package.metadata.battery-pack.features.jemalloc]
description = "jemalloc (not available on MSVC)"
categories = ["allocator"]

[package.metadata.battery-pack.features.mimalloc-alloc]
description = "mimalloc (works everywhere including MSVC)"
categories = ["allocator"]

[package.metadata.battery-pack.dependencies.embedded-hal]
description = "Trait abstractions for embedded I/O"
categories = ["portable"]
```

The `description` is shown next to the item in the picker and in the generated docs.

## `at-most-one` vs `any`

Use `at-most-one` when alternatives are mutually exclusive:

- Which HAL for your chip family (stm32f4 *or* nrf52840, never both)
- Which async runtime (tokio *or* async-std)
- Which allocator (jemalloc *or* mimalloc)

Use `any` (the default) for thematic groupings where users might want multiple items:

- HTTP middleware layers (tracing *and* timeout *and* request-id)
- Portable embedded utilities (embedded-hal *and* heapless *and* defmt)

## How it looks

In the TUI, `at-most-one` categories render as radio buttons (select one deselects others). `any` categories render as checkboxes. In docs, categories become sections with tables.

On the command line, requesting two features from the same `at-most-one` category is an error:

```bash
cargo bp add embedded -F stm32f4 -F nrf52840
# error: features 'stm32f4' and 'nrf52840' are exclusive (category: hal)
```

## Full example

```toml
[package.metadata.battery-pack.categories.hal]
title = "Hardware Abstraction Layer"
description = "Pick the HAL for your target chip family"
pick = "at-most-one"

[package.metadata.battery-pack.categories.rtos]
title = "Concurrency Framework"
description = "Pick your async/RTOS model"
pick = "at-most-one"

[package.metadata.battery-pack.categories.portable]
title = "Portable Ecosystem"
description = "Works with any HAL"

[package.metadata.battery-pack.features.stm32f4]
description = "STM32F4xx family (Cortex-M4F)"
categories = ["hal"]

[package.metadata.battery-pack.features.nrf52840]
description = "Nordic nRF52840 (Cortex-M4F, BLE + USB)"
categories = ["hal"]

[package.metadata.battery-pack.features.embassy]
description = "Embassy — async/await runtime for embedded"
categories = ["rtos"]

[package.metadata.battery-pack.dependencies.embedded-hal]
description = "Trait abstractions for embedded I/O (v1.0)"
categories = ["portable"]
```

Items not assigned to any category appear in generic "Dependencies" / "Dev dependencies" sections.
