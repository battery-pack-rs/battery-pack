# RFD: Categories and Exclusive Picks

## Summary

Extend battery packs with metadata to express **categories** (groupings of
related alternatives) and **exclusive picks** (features where you choose one,
not many). This enables battery packs like `embedded-battery-pack` that curate
alternatives — "here are 5 ways to do X, pick the one that fits your chip" —
with first-class UI support in `cargo bp add`.

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
custom metadata in favor of reading Cargo's native declarations. But we don't want to let perfect be enemy of good and block on completion of the 
pre-RFC.

Our design should be forward-compatible: if Cargo globals land, a battery pack
author could migrate from `[package.metadata.battery-pack.categories]` to
native `[globals]` declarations, and `cargo bp` would read the Cargo-native
format instead.

[awesome-embedded-rust]: https://github.com/rust-embedded/awesome-embedded-rust
[pre-rfc]: https://internals.rust-lang.org/t/pre-rfc-mutually-excusive-global-features/19618

## Design

### New metadata: `[package.metadata.battery-pack.features.<name>]`

Each feature in `[features]` can be annotated with metadata that controls how
it's presented in the UI:

```toml
[package.metadata.battery-pack.features.stm32f4]
category = "hal"            # group this feature under the "hal" category

[package.metadata.battery-pack.features.nrf52840]
category = "hal"

[package.metadata.battery-pack.features.esp32]
category = "hal"

[package.metadata.battery-pack.features.rtic]
category = "rtos"

[package.metadata.battery-pack.features.embassy]
category = "rtos"
```

### Category metadata: `[package.metadata.battery-pack.categories.<name>]`

Categories themselves can be described:

```toml
[package.metadata.battery-pack.categories.hal]
title = "Hardware Abstraction Layer"
description = "Pick the HAL for your target chip family"
pick = "at-most-one"        # "at-most-one" | "any"

[package.metadata.battery-pack.categories.rtos]
title = "Concurrency Framework"
description = "Pick your scheduling / concurrency approach"
pick = "at-most-one"
```

The `pick` field controls validation:

| Value | Meaning |
|-------|---------|
| `at-most-one` | User may choose one, or skip the category entirely |
| `any` | No constraint; category is purely organizational |

Default is `any` (category is purely organizational, no exclusivity enforced).

We may add `exactly-one` and `at-least-one` later if needed, but these two
cover the immediate use cases without overcomplicating validation.

### Template categories

Categories apply to templates too, not just features. A battery pack's
templates (declared in `[package.metadata.battery.templates]`) can be assigned
to categories with an additional `category` field:

```toml
[package.metadata.battery-pack.categories.release]
title = "Release Strategy"
description = "How your crate or binary gets published"
pick = "at-most-one"

[package.metadata.battery-pack.categories.quality]
title = "Code Quality"
description = "Static analysis and testing tools"

[package.metadata.battery.templates]
trusted-publishing = { path = "templates/trusted-publishing", description = "release-plz with OIDC trusted publishing", category = "release" }
binary-release = { path = "templates/binary-release", description = "Cross-platform binary builds for GitHub Releases", category = "release" }
fuzzing = { path = "templates/fuzzing", description = "cargo-fuzz scaffold + CI workflows", category = "quality" }
mutation-testing = { path = "templates/mutation-testing", description = "Mutation testing with cargo-mutants", category = "quality" }
spellcheck = { path = "templates/spellcheck", description = "crate-ci/typos config + CI workflow", category = "quality" }
```

This reuses the same category infrastructure — the `release` category is
`at-most-one` (you pick one publishing strategy), while `quality` is `any`
(stack as many checks as you want). Templates and features can share category
namespaces when it makes sense, or use separate ones.

In the picker, categorized templates appear under their category heading just
like features do. `cargo bp add ci -t trusted-publishing` and
`cargo bp add ci -t binary-release` would conflict if both are in an
`at-most-one` category.

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
features. A new placeholder type `"category"` derives its options directly
from a category definition:

```toml
[placeholders.allocator]
type = "category"
category = "allocator"
prompt = "Global allocator"
default = "jemalloc"
```

Behavior:

- The options list is automatically the set of features in the named category.
- The template variable receives the chosen feature name as its value
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

But the source of truth is the category definition, not a duplicated options
list. Adding a new feature to the category automatically makes it available as
a template option — no `bp-template.toml` edit required.

### Features without a category

Features (and templates) that have no `category` annotation continue to work
exactly as today — they appear as additive checkboxes in the picker.

### Nesting: sub-categories

For deeply structured domains like embedded (vendor → chip family → specific
chip), categories can be nested with dot notation:

```toml
[package.metadata.battery-pack.categories.hal]
title = "Hardware Abstraction Layer"
pick = "at-most-one"

[package.metadata.battery-pack.categories."hal.stm32"]
title = "STMicroelectronics"

[package.metadata.battery-pack.categories."hal.nordic"]
title = "Nordic Semiconductor"

[package.metadata.battery-pack.features.stm32f4]
category = "hal.stm32"

[package.metadata.battery-pack.features.nrf52840]
category = "hal.nordic"
```

The picker displays this as a tree. The `pick` constraint applies at the level
it's declared — here, `at-most-one` on `hal` means you pick at most one item
from any sub-category.

## UI: How `cargo bp add embedded` looks

### Interactive (TUI)

The picker renders categories as expandable sections with radio-button (●/○)
or checkbox ([x]/[ ]) semantics based on the `pick` constraint:

```
  embedded-battery-pack v0.1.0
  ─────────────────────────────────────────────────

  Hardware Abstraction Layer (pick at most one):
  ▼ STMicroelectronics
      ○ stm32f0     STM32F0xx HAL
      ○ stm32f1     STM32F1xx HAL
      ○ stm32f3     STM32F3xx HAL
      ● stm32f4     STM32F4xx HAL          ← selected
      ○ stm32f7     STM32F7xx HAL
      ○ stm32h7     STM32H7xx HAL
      ○ stm32l4     STM32L4xx HAL
  ▼ Nordic Semiconductor
      ○ nrf52832    nRF52832 HAL
      ○ nrf52840    nRF52840 HAL
      ○ nrf9160     nRF9160 HAL
  ▼ Espressif
      ○ esp32       ESP32 HAL (no_std)
      ○ esp32-s3    ESP32-S3 HAL (no_std)
      ○ esp32-c3    ESP32-C3 HAL (no_std)
  ▼ Raspberry Pi
      ○ rp2040      RP2040 HAL
      ○ rp2350      RP2350 HAL

  Concurrency Framework (pick at most one):
      ○ rtic        RTIC — interrupt-driven concurrency
      ○ embassy     Embassy — async/await for embedded
      ○ none        bare-metal (no framework)

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
  deselects any other in the same category. However, it's possible to arrive
  at a state with multiple selections (e.g., the user previously ran
  `cargo add` manually). In that case the picker shows the current state
  honestly — multiple items selected. Selecting a new radio item clears the
  others; the deselected crates are removed from `Cargo.toml` on confirm.
- **`any` categories** use checkboxes as today.
- **Collapsing**: pressing left-arrow on a category header collapses it;
  right-arrow expands it. Sub-categories within a category are independently
  collapsible.
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
    STMicroelectronics: stm32f0, stm32f1, stm32f3, stm32f4, stm32f7, ...
    Nordic: nrf52832, nrf52840, nrf9160
    Espressif: esp32, esp32-s3, esp32-c3
    Raspberry Pi: rp2040, rp2350
  rtos — Concurrency Framework (pick at most one)
    rtic, embassy

Portable utilities:
  embedded-hal, defmt, embedded-io, heapless, ...

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

[package.metadata.battery-pack.categories."hal.stm32"]
title = "STMicroelectronics"

[package.metadata.battery-pack.categories."hal.nordic"]
title = "Nordic Semiconductor"

[package.metadata.battery-pack.categories."hal.espressif"]
title = "Espressif"

[package.metadata.battery-pack.categories."hal.rpi"]
title = "Raspberry Pi"

[package.metadata.battery-pack.categories.rtos]
title = "Concurrency Framework"
description = "Pick your scheduling / concurrency approach"
pick = "at-most-one"

# --- Feature → category mapping ---

[package.metadata.battery-pack.features.stm32f0]
category = "hal.stm32"
description = "STM32F0xx family"

[package.metadata.battery-pack.features.stm32f1]
category = "hal.stm32"
description = "STM32F1xx family"

[package.metadata.battery-pack.features.stm32f4]
category = "hal.stm32"
description = "STM32F4xx family"

[package.metadata.battery-pack.features.nrf52840]
category = "hal.nordic"
description = "nRF52840 SoC"

[package.metadata.battery-pack.features.esp32]
category = "hal.espressif"
description = "ESP32 (no_std via esp-hal)"

[package.metadata.battery-pack.features.rp2040]
category = "hal.rpi"
description = "RP2040 (Raspberry Pi Pico)"

[package.metadata.battery-pack.features.rtic]
category = "rtos"
description = "RTIC — interrupt-driven concurrency"

[package.metadata.battery-pack.features.embassy]
category = "rtos"
description = "Embassy — async/await for embedded"

# --- Hidden deps ---

[package.metadata.battery-pack]
hidden = ["battery-pack"]

# --- Dependencies ---

[dependencies]
# Portable ecosystem (always available)
embedded-hal = { version = "1", optional = true }
defmt = { version = "1", optional = true }
embedded-io = { version = "0.6", optional = true }
heapless = { version = "0.8", optional = true }
critical-section = { version = "1", optional = true }

# STM32 HALs
stm32f0xx-hal = { version = "0.18", optional = true }
stm32f1xx-hal = { version = "0.10", optional = true }
stm32f4xx-hal = { version = "0.22", features = ["rt"], optional = true }

# Nordic HALs
nrf52840-hal = { version = "0.18", optional = true }

# Espressif
esp-hal = { version = "1", optional = true }

# Raspberry Pi
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

## Inheritance model: chipset-specific sub-packs

For maximum flexibility, we anticipate a layered approach:

```
embedded-battery-pack          (portable: embedded-hal, defmt, heapless)
├── stm32-battery-pack         (STM32 ecosystem: stm32f4xx-hal, stm32-device-signature, ...)
├── nrf-battery-pack           (Nordic ecosystem: nrf52840-hal, nrf-softdevice, ...)
├── esp-battery-pack           (Espressif: esp-hal, esp-wifi, esp-alloc, ...)
└── rp-battery-pack            (RP: rp2040-hal, rp-pico board support, ...)
```

Users who want a *simple* experience:

```bash
cargo bp add embedded -F stm32f4 -F embassy
```

Users who want deep chipset curation install the specific sub-pack:

```bash
cargo bp add stm32 -F stm32f4 -F embassy -F defmt-rtt
```

Both approaches compose: `stm32-battery-pack` could internally declare
`embedded-battery-pack` features as dependencies (forwarding them), so the
user gets the portable layer automatically.

The categories/exclusive metadata works at *any* level — both the high-level
`embedded-battery-pack` choosing between vendors, and within
`stm32-battery-pack` choosing between chip families.

## Error conditions

### Authoring errors (`cargo bp validate`)

These are errors in the battery pack's Cargo.toml, caught when the pack
author runs `cargo bp validate`:

| Rule | Condition | Message |
|------|-----------|---------|
| `format.categories.defined` | A feature's `category` references a name with no corresponding `categories.<name>` entry (or ancestor) | `feature 'stm32f4' references undefined category 'hal'` |
| `format.categories.template-defined` | A template's `category` references an undefined category | `template 'binary-release' references undefined category 'release'` |
| `format.features.exclusive-conflict` | Two or more features in the same `at-most-one` category are both listed in `default` | `features 'jemalloc' and 'mimalloc-alloc' are both in default but belong to at-most-one category 'allocator'` |
| `format.categories.empty` | A category is declared but no feature or template references it | warning: `category 'foo' is declared but has no members` |
| `format.categories.pick-missing-title` | A category has `pick = "at-most-one"` but no `title` | warning: `at-most-one category 'hal' should have a title for the picker UI` |
| `format.features.unknown-feature` | `[package.metadata.battery-pack.features.X]` where `X` is not a key in `[features]` | `feature metadata 'X' does not match any entry in [features]` |
| `format.template.category-placeholder-mismatch` | A template uses `type = "category"` referencing a category that doesn't exist | `template placeholder 'allocator' references undefined category 'allocator'` |

### Usage errors (`cargo bp add`)

These are errors at install time, caused by the user's selection:

| Context | Condition | Message |
|---------|-----------|---------|
| Non-interactive `-F` | Two features from the same `at-most-one` category passed | `error: features 'stm32f4' and 'nrf52840' are exclusive (category: hal)` |
| Non-interactive `-t` | Two templates from the same `at-most-one` category passed | `error: templates 'trusted-publishing' and 'binary-release' are exclusive (category: release)` |
| Interactive (picker) | User presses Enter with >1 selection in an `at-most-one` category | Inline error in the picker: `"category 'hal' allows at most one selection"` — picker refuses to confirm |
| `--all-features` | Multiple exclusive selections | No error — `--all-features` bypasses exclusive checks |

### Edge case: pre-existing multi-selection

If the user previously installed features via `cargo add` that conflict with
an `at-most-one` constraint, the picker shows them honestly (multiple radio
buttons filled). The user must deselect down to one (or zero) before the
picker will confirm. If they select a new item, the others are automatically
deselected. On confirm, deselected crates are removed from `Cargo.toml`.

## Interaction with existing features

- Features without category annotations work exactly as before.
- A single battery pack can mix categorized (exclusive) features and plain
  additive features freely.
- `cargo bp add --all-features` skips validation for exclusive categories
  (since the user explicitly asked for everything — useful for CI builds
  that test all combinations).
- `battery-pack.toml` records which features were chosen, unchanged from
  today's format.

## Impact on existing battery packs

### `backend-service-battery-pack`

Today the allocator choice (`jemalloc` vs
`mimalloc-alloc`) is two independent features with no expressed relationship —
a user could enable both and get linker errors. The tower-http middleware
layers are also a flat list that would benefit from grouping.

```toml
[package.metadata.battery-pack.categories.allocator]
title = "Global Allocator"
description = "Pick a high-performance allocator (or use system default)"
pick = "at-most-one"

[package.metadata.battery-pack.categories.http-layers]
title = "HTTP Middleware Layers"
description = "Tower-HTTP middleware for your service"

[package.metadata.battery-pack.features.jemalloc]
category = "allocator"
description = "jemalloc (not available on MSVC)"

[package.metadata.battery-pack.features.mimalloc-alloc]
category = "allocator"
description = "mimalloc (works everywhere including MSVC)"

[package.metadata.battery-pack.features.http-trace]
category = "http-layers"
description = "Request/response tracing spans"

[package.metadata.battery-pack.features.http-request-id]
category = "http-layers"
description = "X-Request-Id propagation"

[package.metadata.battery-pack.features.http-timeout]
category = "http-layers"
description = "Request timeout enforcement"

[package.metadata.battery-pack.features.http-catch-panic]
category = "http-layers"
description = "Convert panics to 500 responses"
```

The picker would render allocators as radio buttons (picking one deselects the
other), and middleware layers as a checkbox group under a clear heading. Today
these are all in a flat list where the structure is invisible.

### `ci-battery-pack`

This pack is template-heavy. Its 13 templates are currently a flat list, but
several are alternatives to each other or belong to natural groupings:

```toml
[package.metadata.battery-pack.categories.release]
title = "Release Strategy"
description = "How your crate or binary gets published"
pick = "at-most-one"

[package.metadata.battery-pack.categories.quality]
title = "Code Quality"
description = "Static analysis and testing tools"

[package.metadata.battery-pack.categories.docs]
title = "Documentation"

[package.metadata.battery.templates]
# Release strategies — pick one
trusted-publishing = { path = "...", description = "release-plz with OIDC trusted publishing", category = "release" }
binary-release = { path = "...", description = "Cross-platform binary builds for GitHub Releases", category = "release" }

# Quality tools — mix and match
fuzzing = { path = "...", description = "cargo-fuzz scaffold + CI workflows", category = "quality" }
mutation-testing = { path = "...", description = "Mutation testing with cargo-mutants", category = "quality" }
spellcheck = { path = "...", description = "crate-ci/typos config + CI workflow", category = "quality" }
clippy-sarif = { path = "...", description = "Clippy with GitHub PR annotations via SARIF", category = "quality" }
security-scanning = { path = "...", description = "RustSec audit workflow", category = "quality" }

# Docs
mdbook = { path = "...", description = "mdBook scaffold + GitHub Pages deployment", category = "docs" }
```

`trusted-publishing` and `binary-release` are alternative release strategies
(you'd pick one), while the quality tools are mix-and-match. Today a user sees
13 templates in a flat list with no signal about which ones are related or
mutually exclusive.

### `cli-battery-pack`

No exclusive choices here — its features are genuinely additive. But
categories still help organize the picker visually:

```toml
[package.metadata.battery-pack.categories.output]
title = "Terminal Output"
description = "Color, hyperlinks, and progress display"

[package.metadata.battery-pack.categories.input]
title = "User Input"
description = "Argument parsing and interactive prompts"

[package.metadata.battery-pack.categories.search]
title = "File & Text Search"

[package.metadata.battery-pack.features.indicators]
category = "output"
description = "Progress bars and spinners (indicatif + console)"

[package.metadata.battery-pack.features.search]
category = "search"
description = "Regex search with .gitignore-aware file walking"

[package.metadata.battery-pack.features.config]
category = "input"
description = "XDG/platform config directories (etcetera)"
```

All categories use the default `pick = "any"` — no exclusivity, purely
organizational. The picker shows items grouped under meaningful headings
instead of a flat list, making a 15-crate battery pack scannable.

### `logging-battery-pack`

Too small to benefit — it has two crates and one feature. No change needed.

### `error-battery-pack`

Same — two crates, no features. No change needed.

## Implementation plan

1. **Parse new metadata** in `bphelper-manifest` — extend `BatteryPackMeta`
   to read `features.*`, `categories.*`, and template `category` fields from
   `package.metadata.battery-pack`.

2. **Extend `sectioned-picker`** — add a `SelectionMode` enum
   (`Checkbox | Radio`) to `Section`, and implement radio-button behavior
   (selecting one deselects others in the section). Add collapsible
   sub-sections for nested categories.

3. **Wire into CLI** — `cargo bp add` uses the new metadata to build picker
   sections: one section per category (with sub-sections for nesting), then a
   final section for uncategorized additive features. Templates with categories
   are grouped under their category heading in the template selection UI.

4. **CLI validation** — when `-F` or `-t` flags are used non-interactively,
   check exclusive constraints before proceeding.

5. **`cargo bp validate`** — add the new rules above.

6. **`cargo bp show`** — render categories in the show output, for both
   features and templates.

7. **Migrate existing packs** — add category metadata to
   `backend-service-battery-pack` (allocator exclusivity),
   `ci-battery-pack` (template grouping), and `cli-battery-pack`
   (organizational grouping).

8. **Author an `embedded-battery-pack`** — dogfood the feature with a real
   battery pack.

## Open questions

1. **Interaction with Cargo globals (future).** If/when Cargo gets
   mutually-exclusive globals, should battery packs automatically generate the
   corresponding `[globals]` entries? Probably yes, but this is future work.

2. **Feature descriptions.** Today battery pack features don't have
   descriptions (only crates do, via their crates.io description). The
   `description` field in `features.<name>` is new. Should we also support it
   for non-categorized features? Seems useful for the picker UI generally.

3. **Cross-pack exclusive categories.** If `embedded-battery-pack` and
   `stm32-battery-pack` both declare a `hal` category, should the exclusivity
   constraint span both? Probably not (they're different packs, the user
   wouldn't install both for the same project). But worth considering.

4. **"None" option.** For `at-most-one` categories, should we auto-generate a
   "none" entry or require the pack author to define one? Leaning toward:
   the category being optional is already expressed by `at-most-one`; the UI
   just allows leaving it blank.

## Prior art

- **awesome-embedded-rust**: flat curated list organized by vendor, no
  tooling support
- **Cargo pre-RFC for mutually-exclusive globals**: build-system-level
  enforcement of exclusive choices
- **Gentoo USE flags**: global configuration flags with profile defaults
- **Homebrew formulae with conflicts**: `conflicts_with` declarations between
  packages
- **VS Code extension packs**: curated bundles with categorized alternatives
