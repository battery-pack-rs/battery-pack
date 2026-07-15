# Getting Started

## Install the CLI

```bash
cargo install cargo-bp
```

This gives you the `cargo bp` command.

## Browse available battery packs

```bash
cargo bp ls
```

This searches crates.io for published battery packs and lists what's available:

```
                                              Battery Packs

┌──────────────────────────────────────────────────────────────────────────────────────────┐
│> backend-service    0.1.1    Opinionated battery pack for resilient async backend services│
│  ci                 0.1.5    Battery pack for CI/CD workflows in Rust projects            │
│  cli                0.6.2    Battery pack for building CLI applications in Rust           │
│  error              0.6.4    Error handling done well — anyhow for apps, thiserror for …  │
│  logging            0.5.2    Battery pack for logging and tracing in Rust                 │
└──────────────────────────────────────────────────────────────────────────────────────────┘
↑↓/jk Navigate | Enter Select | q Quit
```

## Inspect a battery pack

```bash
cargo bp show ci
```

This shows you what's inside — curated crates, features, and templates. Use `p` to preview a template's rendered output before committing to it.

```
ci-battery-pack 0.1.5

┌──────────────────────────────────────────────────────────────────────────────────────────┐
│Features:                                                                                 │
│  benchmarks → criterion                                                                  │
│  fuzzing → arbitrary, libfuzzer-sys                                                      │
│  xtask → xflags, xshell                                                                 │
│                                                                                          │
│Templates:                                                                                │
│  benchmarks - Criterion bench scaffold + Bencher CI                                      │
│  clippy-sarif - Clippy with GitHub PR annotations via SARIF                              │
│  full - Full CI setup with optional benchmarks, fuzzing, mdbook, spellcheck, and xtask   │
│  fuzzing - cargo-fuzz scaffold + CI workflows                                            │
│  mdbook - mdBook scaffold + GitHub Pages deployment                                      │
│  mutation-testing - Mutation testing with cargo-mutants                                   │
│  spellcheck - crate-ci/typos config + CI workflow                                        │
│  stress-test - nextest stress test workflow                                               │
│  trusted-publishing - release-plz with OIDC trusted publishing                           │
│  xtask - cargo-xtask scaffold with codegen --check                                       │
│                                                                                          │
│Actions:                                                                                  │
│> Open on crates.io                                                                       │
└──────────────────────────────────────────────────────────────────────────────────────────┘
↑↓/jk Navigate | Enter Open/Select | Esc/q Quit
```

## Add crates from a battery pack

```bash
cargo bp add cli
```

This opens an interactive picker where you toggle the crates and features you want:

```
────────────────────────────────────────────────────────────────────────────────────────────
 ▼ Features: (pick any number)
 > [ ] ✦ config [etcetera]
   [ ] ✦ indicators [console, indicatif]
   [ ] ✦ search [ignore, regex]

 ▼ Dependencies: (11 items selected)
   [x] anstream (1.0.0)
   [x] anstyle (1.0.14)
   [x] anyhow (1)
   [x] clap (4, features: derive)
   [x] dialoguer (0.11)
   [x] human-panic (2.0.8)
   [ ] console (0.15)
   [ ] indicatif (0.17)
   …

 ▼ Actions: (pick any number)
   [ ] Add `simple` template — Minimal CLI with argument parsing
   [ ] Add `subcmds` template — CLI with subcommands
────────────────────────────────────────────────────────────────────────────────────────────
 cli-battery-pack v0.6.2  ↑↓/jk Navigate | Space Toggle | p Preview
```

When you confirm, the selected crates are added to your `Cargo.toml`:

```toml
[package.metadata.battery-pack]
cli-battery-pack = "0.6.2"

[dependencies]
anyhow = "1"
clap = { version = "4", features = ["derive"] }
dialoguer = "0.11"
human-panic = "2.0.8"
# ... and so on
```

The `[package.metadata.battery-pack]` section records which battery packs you've installed. The actual crates are real entries in `[dependencies]` that you use directly.

### Features and categories

Some battery packs group crates into features you can opt into:

```bash
cargo bp add cli -F indicators
```

Others use categories to present alternatives — "pick one HAL for your chip family" or "pick an allocator". The interactive picker shows these as radio buttons (pick one) or checkboxes (pick any). See [Our Battery Packs](./battery-packs/index.md) for examples.

## Start a new project from a template

```bash
cargo bp new cli
```

You'll be prompted for a project name and directory. The result is a ready-to-go Rust project with the battery pack's recommended crates and structure already in place.

See [Templates](./templates.md) for template options, merge behavior, placeholders, and non-interactive mode.
