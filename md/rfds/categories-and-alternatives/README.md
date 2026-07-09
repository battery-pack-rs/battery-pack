# RFD: Categories and Exclusive Picks

## Summary

Extend battery packs with metadata to express **categories** (groupings of
related items) and **exclusive picks** (choose at most one from a group). This
enables battery packs like `embedded-battery-pack` that curate alternatives —
"here are 5 ways to do X, pick the one that fits your chip" — with first-class
UI support in `cargo bp add`.

## Motivation

Today, battery pack features are purely **additive**: every feature you enable
adds crates on top of what's already there. This works well for "kitchen sink"
packs like `cli-battery-pack` where you want clap *and* indicatif *and*
dialoguer together.

But some domains are about *choosing between alternatives*:

- **Embedded**: you pick *one* HAL crate for your chip family (stm32f4xx-hal
  *or* nrf52840-hal, never both)
- **Async runtimes**: you pick tokio *or* async-std *or* smol
- **TLS backends**: you pick rustls *or* native-tls
- **Allocators**: you pick jemalloc *or* mimalloc (already awkward in
  `backend-service-battery-pack` today)

The [awesome-embedded-rust] repository curates hundreds of crates organized by
vendor and category. A battery pack for this domain needs a way to say "here
is a category of alternatives; present them to the user and let them pick
one."

### Relationship to Cargo mutually-exclusive globals

There's an [active pre-RFC for mutually-exclusive global features][pre-rfc] in
Cargo itself. That proposal would give Cargo native understanding of choices
that are exclusive — making it impossible to compile two conflicting values in
one build graph. Cargo doesn't have this today, so we need to build something
on top. If Cargo grows first-class support, we would likely deprecate our
custom metadata in favor of reading Cargo's native declarations. But we don't
want to let perfect be the enemy of good and block on completion of the
pre-RFC.

Our design should be forward-compatible: if Cargo globals land, a battery pack
author could migrate from `[package.metadata.battery-pack.categories]` to
native `[globals]` declarations, and `cargo bp` would read the Cargo-native
format instead.

[awesome-embedded-rust]: https://github.com/rust-embedded/awesome-embedded-rust
[pre-rfc]: https://internals.rust-lang.org/t/pre-rfc-mutually-excusive-global-features/19618

## Design

The core model is simple:

1. **Any selectable item** (feature, dependency, or template) can have
   metadata — including a `description` and zero or more `categories`.
2. **Categories** have a title, a description, and a `pick` mode
   (`at-most-one` or `any`).

When displayed, items are grouped by category. `at-most-one` categories render
as radio buttons. Items not in any category appear in generic sections
("Features", "Dependencies", "Templates") as today.

### Item metadata: `[package.metadata.battery-pack.<kind>.<name>]`

Any selectable item can be annotated. The `<kind>` is `features`,
`dependencies`, or `templates`:

```toml
[package.metadata.battery-pack.features.stm32f4]
description = "STM32F4xx family"
categories = ["hal"]

[package.metadata.battery-pack.features.nrf52840]
description = "nRF52840 SoC"
categories = ["hal"]

[package.metadata.battery-pack.features.embassy]
description = "Embassy — async/await for embedded"
categories = ["rtos"]

[package.metadata.battery-pack.dependencies.embedded-hal]
description = "Trait abstractions for embedded I/O"
categories = ["portable"]
```

Fields:
- `description` — shown in the picker next to the item name
- `categories` — list of category names this item belongs to (default: `[]`)

Both fields are optional. An item with no metadata entry behaves exactly as
today.

### Category metadata: `[package.metadata.battery-pack.categories.<name>]`

```toml
[package.metadata.battery-pack.categories.hal]
title = "Hardware Abstraction Layer"
description = "Pick the HAL for your target chip family"
pick = "at-most-one"

[package.metadata.battery-pack.categories.rtos]
title = "Concurrency Framework"
description = "Pick your scheduling / concurrency approach"
pick = "at-most-one"

[package.metadata.battery-pack.categories.portable]
title = "Portable Utilities"
description = "Works with any HAL"
```

Fields:
- `title` — display name in the picker header
- `description` — explanatory text (optional)
- `pick` — `"at-most-one"` or `"any"` (default: `"any"`)

Categories are **pack-scoped**: two different battery packs can both define a
category named `"hal"` without conflict. Cross-pack category coordination is
out of scope (future work; would need a pack extension mechanism).

### Category-linked template placeholders

Today, template `bp-template.toml` files duplicate category information in
their `select` placeholders. For example, `backend-service-battery-pack`'s
service template has:

```toml
[placeholders.allocator]
type = "select"
prompt = "Global allocator"
options = ["jemalloc", "mimalloc", "system"]
default = "jemalloc"
```

These options are manually kept in sync with the `allocator` category's
features. Instead of a literal `options` array, the placeholder can reference
a category to derive its options automatically:

```toml
[placeholders.allocator]
type = "select"
prompt = "Global allocator"
options.category = "allocator"
default = "jemalloc"
```

The `options` field accepts either a literal array (`options = [...]`) or a
category reference (`options.category = "..."`). In serde terms, this is an
untagged enum.

Behavior:

- The options list is automatically the set of items in the named category.
- The template variable receives the chosen item name as its value
  (e.g., `{{ allocator }}` = `"jemalloc"`).
- If the user already made a selection in the `cargo bp add` picker, this
  placeholder is pre-filled and can be skipped during template generation.
- If the category has `pick = "at-most-one"` and no selection was made, the
  template prompts as usual.

The template still uses the value the same way:

```toml
{% if allocator == "jemalloc" %}
tikv-jemallocator.bp-managed = true
{% elif allocator == "mimalloc" %}
mimalloc.bp-managed = true
{% endif %}
```

The source of truth is the category definition, not a duplicated options list.
Adding a new feature to the category automatically makes it available as a
template option.

## UI: How `cargo bp add embedded` looks

### Interactive (TUI)

The picker renders categories as sections with radio-button (●/○) or checkbox
([x]/[ ]) semantics based on the `pick` constraint:

```
  embedded-battery-pack v0.1.0
  ─────────────────────────────────────────────────

  Hardware Abstraction Layer (pick at most one):
      ○ stm32f0     STM32F0xx family
      ○ stm32f1     STM32F1xx family
      ○ stm32f3     STM32F3xx family
      ● stm32f4     STM32F4xx family            ← selected
      ○ stm32f7     STM32F7xx family
      ○ nrf52840    nRF52840 SoC
      ○ esp32       ESP32 (no_std via esp-hal)
      ○ rp2040      RP2040 (Raspberry Pi Pico)

  Concurrency Framework (pick at most one):
      ○ rtic        RTIC — interrupt-driven concurrency
      ● embassy     Embassy — async/await for embedded

  Portable Utilities:
      [x] embedded-hal    Trait abstractions for embedded I/O
      [x] defmt           Efficient logging for constrained devices
      [ ] embedded-io     Read/Write traits for embedded
      [ ] heapless        Static-friendly data structures

  ─────────────────────────────────────────────────
  Space: toggle  ←/→: collapse/expand  Enter: confirm  q: cancel  p: preview
```

UX details:

- **`at-most-one` categories** use radio-button rendering (●/○). Selecting one
  deselects any other in the same category. Pressing Backspace clears the
  current selection entirely (returns to zero selections). It's possible to
  arrive at a state with multiple selections (e.g., the user previously ran
  `cargo add` manually). In that case the picker shows the current state
  honestly — multiple items selected — with a warning banner. Selecting a new
  radio item clears the others; the deselected crates are removed from
  `Cargo.toml` on confirm.
- **`any` categories** use checkboxes as today. Pressing `a` on a category
  header checks all items in that category (same as today's section toggle).
  For `at-most-one` categories, `a` is a no-op (can't select all).
- **Collapsing**: pressing left-arrow on a category header collapses it;
  right-arrow expands it.
- The header line says "(pick at most one)" for `at-most-one` categories.
- **Validation on Enter**: if an `at-most-one` category has more than one
  selection, the picker shows an inline error and refuses to confirm.

### Non-interactive (CLI flags)

```bash
# Pick a specific HAL and concurrency framework:
cargo bp add embedded -F stm32f4 -F embassy

# Enable a portable utility:
cargo bp add embedded -F stm32f4 -F embassy -F heapless

# Validation: this is an error (two exclusive HALs):
cargo bp add embedded -F stm32f4 -F nrf52840
# error: features `stm32f4` and `nrf52840` are exclusive (category: hal)
```

### `cargo bp show`

```
embedded-battery-pack v0.1.0
Curated hardware ecosystem for embedded Rust

Categories:
  hal — Hardware Abstraction Layer (pick at most one)
    stm32f0, stm32f1, stm32f3, stm32f4, stm32f7, nrf52840, esp32, rp2040
  rtos — Concurrency Framework (pick at most one)
    rtic, embassy
  portable — Portable Utilities
    embedded-hal, defmt, embedded-io, heapless

Templates:
  blinky    — Minimal blinky LED example for your chosen HAL
```

## Example: Full `embedded-battery-pack` Cargo.toml

```toml
[package]
name = "embedded-battery-pack"
version = "0.1.0"
edition = "2024"
description = "Curated hardware ecosystem for embedded Rust"
license = "MIT OR Apache-2.0"
keywords = ["battery-pack", "embedded", "hal", "no-std"]

# --- Category definitions ---

[package.metadata.battery-pack.categories.hal]
title = "Hardware Abstraction Layer"
description = "Pick the HAL for your target chip family"
pick = "at-most-one"

[package.metadata.battery-pack.categories.rtos]
title = "Concurrency Framework"
description = "Pick your scheduling / concurrency approach"
pick = "at-most-one"

[package.metadata.battery-pack.categories.portable]
title = "Portable Utilities"
description = "Works with any HAL"

# --- Feature metadata ---

[package.metadata.battery-pack.features.stm32f0]
description = "STM32F0xx family"
categories = ["hal"]

[package.metadata.battery-pack.features.stm32f1]
description = "STM32F1xx family"
categories = ["hal"]

[package.metadata.battery-pack.features.stm32f4]
description = "STM32F4xx family"
categories = ["hal"]

[package.metadata.battery-pack.features.nrf52840]
description = "nRF52840 SoC"
categories = ["hal"]

[package.metadata.battery-pack.features.esp32]
description = "ESP32 (no_std via esp-hal)"
categories = ["hal"]

[package.metadata.battery-pack.features.rp2040]
description = "RP2040 (Raspberry Pi Pico)"
categories = ["hal"]

[package.metadata.battery-pack.features.rtic]
description = "RTIC — interrupt-driven concurrency"
categories = ["rtos"]

[package.metadata.battery-pack.features.embassy]
description = "Embassy — async/await for embedded"
categories = ["rtos"]

# --- Dependency metadata ---

[package.metadata.battery-pack.dependencies.embedded-hal]
description = "Trait abstractions for embedded I/O"
categories = ["portable"]

[package.metadata.battery-pack.dependencies.defmt]
description = "Efficient logging for constrained devices"
categories = ["portable"]

[package.metadata.battery-pack.dependencies.embedded-io]
description = "Read/Write traits for embedded"
categories = ["portable"]

[package.metadata.battery-pack.dependencies.heapless]
description = "Static-friendly data structures"
categories = ["portable"]

# --- Hidden deps ---

[package.metadata.battery-pack]
hidden = ["battery-pack"]

# --- Dependencies ---

[dependencies]
# Portable ecosystem
embedded-hal = { version = "1", optional = true }
defmt = { version = "1", optional = true }
embedded-io = { version = "0.6", optional = true }
heapless = { version = "0.8", optional = true }
critical-section = { version = "1", optional = true }

# HALs
stm32f0xx-hal = { version = "0.18", optional = true }
stm32f1xx-hal = { version = "0.10", optional = true }
stm32f4xx-hal = { version = "0.22", features = ["rt"], optional = true }
nrf52840-hal = { version = "0.18", optional = true }
esp-hal = { version = "1", optional = true }
rp2040-hal = { version = "0.10", optional = true }

# Concurrency frameworks
rtic = { version = "2", optional = true }
embassy-executor = { version = "0.7", optional = true }
embassy-time = { version = "0.4", optional = true }

[build-dependencies]
battery-pack = { version = "0.7", features = ["build"] }

# --- Features ---

[features]
default = ["embedded-hal", "defmt", "critical-section"]

# HAL features (exclusive within category)
stm32f0 = ["stm32f0xx-hal", "embedded-hal"]
stm32f1 = ["stm32f1xx-hal", "embedded-hal"]
stm32f4 = ["stm32f4xx-hal", "embedded-hal"]
nrf52840 = ["nrf52840-hal", "embedded-hal"]
esp32 = ["esp-hal", "embedded-hal"]
rp2040 = ["rp2040-hal", "embedded-hal"]

# RTOS features (exclusive within category)
rtic = ["dep:rtic", "critical-section"]
embassy = ["embassy-executor", "embassy-time"]
```

## Error conditions

### Authoring errors (`cargo bp validate`)

| Rule | Condition | Message |
|------|-----------|---------|
| `format.categories.defined` | An item's `categories` list references an undefined category | `feature 'stm32f4' references undefined category 'hal'` |
| `format.features.exclusive-conflict` | Two or more features in the same `at-most-one` category are both in `default` | `features 'jemalloc' and 'mimalloc-alloc' are both in default but belong to at-most-one category 'allocator'` |
| `format.categories.empty` | A category is declared but nothing references it | warning: `category 'foo' is declared but has no members` |
| `format.categories.pick-missing-title` | A category has `pick = "at-most-one"` but no `title` | warning: `at-most-one category 'hal' should have a title for the picker UI` |
| `format.features.unknown-feature` | `[package.metadata.battery-pack.features.X]` where `X` is not in `[features]` | `feature metadata 'X' does not match any entry in [features]` |
| `format.dependencies.unknown-dep` | `[package.metadata.battery-pack.dependencies.X]` where `X` is not in any dependency section | `dependency metadata 'X' does not match any dependency` |
| `format.template.category-placeholder-mismatch` | A template placeholder uses `options.category` referencing an undefined category | `placeholder 'allocator' references undefined category 'allocator'` |

### Usage errors (`cargo bp add`)

| Context | Condition | Message |
|---------|-----------|---------|
| Non-interactive `-F` | Two features from the same `at-most-one` category | `error: features 'stm32f4' and 'nrf52840' are exclusive (category: hal)` |
| Non-interactive `-t` | Two templates from the same `at-most-one` category | `error: templates 'X' and 'Y' are exclusive (category: Z)` |
| Interactive (picker) | Enter with >1 selection in `at-most-one` category | Inline error: `"category 'hal' allows at most one selection"` |
| `--all-features` | Multiple exclusive selections | No error — bypasses exclusive checks |

### Edge case: pre-existing multi-selection

If the user previously installed items via `cargo add` that conflict with an
`at-most-one` constraint, the picker shows them honestly (multiple radio
buttons filled) with a warning banner:

```
  ⚠ Multiple selections in "Global Allocator" — pick one to resolve
```

The user must deselect down to one (or zero) before the picker will confirm.
Selecting a new item automatically deselects the others. On confirm, deselected
crates are removed from `Cargo.toml`.

## Impact on existing battery packs

### `backend-service-battery-pack`

Today the allocator choice (`jemalloc` vs `mimalloc-alloc`) is two independent
features with no expressed relationship — a user could enable both and get
linker errors. The tower-http middleware layers are a flat list that would
benefit from grouping.

```toml
[package.metadata.battery-pack.categories.allocator]
title = "Global Allocator"
description = "Pick a high-performance allocator (or use system default)"
pick = "at-most-one"

[package.metadata.battery-pack.categories.http-layers]
title = "HTTP Middleware Layers"
description = "Tower-HTTP middleware for your service"

[package.metadata.battery-pack.features.jemalloc]
description = "jemalloc (not available on MSVC)"
categories = ["allocator"]

[package.metadata.battery-pack.features.mimalloc-alloc]
description = "mimalloc (works everywhere including MSVC)"
categories = ["allocator"]

[package.metadata.battery-pack.features.http-trace]
description = "Request/response tracing spans"
categories = ["http-layers"]

[package.metadata.battery-pack.features.http-request-id]
description = "X-Request-Id propagation"
categories = ["http-layers"]

[package.metadata.battery-pack.features.http-timeout]
description = "Request timeout enforcement"
categories = ["http-layers"]

[package.metadata.battery-pack.features.http-catch-panic]
description = "Convert panics to 500 responses"
categories = ["http-layers"]
```

The picker renders allocators as radio buttons (picking one deselects the
other), and middleware layers as a checkbox group under a clear heading.

The service template's `bp-template.toml` also benefits — its allocator
placeholder uses `options.category` instead of a hardcoded list:

```toml
[placeholders.allocator]
type = "select"
options.category = "allocator"
prompt = "Global allocator"
default = "jemalloc"
```

### `ci-battery-pack`

This pack is template-heavy. Its 13 templates are currently a flat list.
Categories provide organizational grouping:

```toml
[package.metadata.battery-pack.categories.quality]
title = "Code Quality"
description = "Static analysis and testing tools"

[package.metadata.battery-pack.categories.docs]
title = "Documentation"
```

Templates declare their categories:

```toml
[package.metadata.battery.templates]
fuzzing = { path = "...", description = "cargo-fuzz scaffold + CI workflows", categories = ["quality"] }
mutation-testing = { path = "...", description = "Mutation testing with cargo-mutants", categories = ["quality"] }
spellcheck = { path = "...", description = "crate-ci/typos config + CI workflow", categories = ["quality"] }
clippy-sarif = { path = "...", description = "Clippy with GitHub PR annotations", categories = ["quality"] }
security-scanning = { path = "...", description = "RustSec audit workflow", categories = ["quality"] }
mdbook = { path = "...", description = "mdBook scaffold + GitHub Pages deployment", categories = ["docs"] }
```

All categories here use `pick = "any"` (the default) — purely organizational.
The picker shows items grouped under meaningful headings instead of a flat
list.

### `cli-battery-pack`

No exclusive choices — features are genuinely additive. Categories help
organize the picker:

```toml
[package.metadata.battery-pack.categories.output]
title = "Terminal Output"
description = "Color, hyperlinks, and progress display"

[package.metadata.battery-pack.categories.input]
title = "User Input"
description = "Argument parsing and interactive prompts"

[package.metadata.battery-pack.features.indicators]
description = "Progress bars and spinners (indicatif + console)"
categories = ["output"]

[package.metadata.battery-pack.features.search]
description = "Regex search with .gitignore-aware file walking"

[package.metadata.battery-pack.features.config]
description = "XDG/platform config directories (etcetera)"
categories = ["input"]
```

### `logging-battery-pack` / `error-battery-pack`

Too small to benefit. No change needed.

## Interaction with existing features

- Items without category annotations work exactly as before.
- A single battery pack can mix categorized and uncategorized items freely.
- `cargo bp add --all-features` skips validation for exclusive categories
  (since the user explicitly asked for everything — useful for CI builds
  that test all combinations).
- `battery-pack.toml` records which features were chosen, unchanged from
  today's format.

## Future work

- **Cargo globals.** If/when Cargo gets mutually-exclusive globals, we'd
  read Cargo's native format and deprecate custom metadata.

- **Conditional visibility / `requires`.** jlizen raised a use case where
  templates should only be visible if a certain feature is selected (e.g.,
  GitHub-specific templates only shown when `github` feature is active).
  This is a filtering mechanism orthogonal to categories.

- **Pack extension.** There's no mechanism for one battery pack to extend
  another (shared categories, feature forwarding, template inheritance).
  A single big pack works for v1; extension is a separate RFD.

- **Shared item identity across categories.** When an item belongs to multiple
  categories it appears as independent rows in the picker. Toggling it in one
  section doesn't live-update its copy in the other. The confirmed *result* is
  correct (decoded by name into a set), but the picker visually desyncs during
  interaction. Fixing this means adding an `Option<String>` identity to
  `SectionItem` and propagating state across entries sharing an id. This also
  introduces edge cases — e.g., an item in both an `at-most-one` and an `any`
  category could be checked in the `any` section, putting the `at-most-one`
  section into an invalid state. Needs its own design pass.

## Prior art

- **awesome-embedded-rust**: flat curated list organized by vendor, no
  tooling support
- **Cargo pre-RFC for mutually-exclusive globals**: build-system-level
  enforcement of exclusive choices
- **Gentoo USE flags**: global configuration flags with profile defaults
- **Homebrew formulae with conflicts**: `conflicts_with` declarations between
  packages
- **VS Code extension packs**: curated bundles with categorized alternatives
