# ci-battery-pack

A [battery pack](https://crates.io/crates/battery-pack) for CI/CD workflows in Rust projects. This is the kind of thing you'd copy from tokio or hyper, but generated fresh with pinned SHAs and your project's MSRV.

Currently supports GitHub Actions. Someday there will be more supported platforms.

## Quick Start

The `full` template generates a complete project with CI. Use it to bootstrap a new project:

```sh
cargo bp new ci --name my-project
```

Everything enabled:

```sh
cargo bp new ci --name my-project -d all
```

Pick individual features:

```sh
cargo bp new ci --name my-project -d ci_platform=github -d benchmarks -d fuzzing -d spellcheck
```

Config files only (no CI workflows, no stub project):

```sh
cargo bp new ci --name my-project -d ci_platform=none
```

Or run `cargo bp new ci` interactively and answer the prompts.

Each optional feature is also available as a standalone template for adding to an existing project:

```sh
cargo bp new ci --template fuzzing --name my-project
cargo bp new ci --template trusted-publishing --name my-project
```

## What the `full` template generates

The `full` template generates a stub Rust project (Cargo.toml, src/lib.rs, README with badges) along with CI configuration. If you're adding CI to an existing project, use the standalone templates instead.

### Core CI (GitHub Actions)

- CI workflow: fmt, clippy, warnings check, docsrs check, build matrix (stable × nightly), feature powerset, MSRV, semver-checks, gate job
- Security audit workflow (cargo-deny, daily + on Cargo.toml changes)
- Dependabot config for Cargo and GitHub Actions updates
- cargo-deny config (`deny.toml`)

### Optional features (`-d flag`)

Use `-d all` to enable every optional feature. Otherwise, each defaults to off
(except `trusted_publishing` which defaults to on). In interactive mode, you'll
be prompted for each.

| Flag | Default | What it adds | Curated deps |
|------|---------|-------------|-------------|
| `trusted_publishing` | true | [release-plz](https://release-plz.dev/) with OIDC trusted publishing | |
| `binary_release` | false | Cross-platform binary builds for GitHub Releases + [cargo-binstall](https://github.com/cargo-bins/cargo-binstall) | |
| `benchmarks` | false | [Criterion](https://crates.io/crates/criterion) bench scaffold + [Bencher](https://bencher.dev/) regression detection | `criterion` |
| `fuzzing` | false | [cargo-fuzz](https://github.com/rust-fuzz/cargo-fuzz) scaffold + PR smoke test + nightly extended run | `libfuzzer-sys`, `arbitrary` |
| `stress_tests` | false | [nextest](https://nexte.st/) stress test workflow | |
| `mdbook` | false | [mdBook](https://rust-lang.github.io/mdBook/) scaffold + GitHub Pages deployment | |
| `spellcheck` | false | [typos](https://github.com/crate-ci/typos) config + workflow | |
| `xtask` | false | [cargo-xtask](https://github.com/matklad/cargo-xtask) scaffold with codegen `--check` | `xshell`, `xflags` |
| `mutation_testing` | false | [cargo-mutants](https://mutants.rs/) mutation testing | |
| `clippy_sarif` | false | Clippy with GitHub PR annotations via [SARIF](https://github.com/psastras/sarif-rs) | |

### SHA pinning

All GitHub Actions are pinned to commit SHAs at generation time per
[GitHub's security guidance](https://docs.github.com/en/actions/security-for-github-actions/security-guides/security-hardening-for-github-actions#using-third-party-actions).
Use [Dependabot](https://docs.github.com/en/code-security/dependabot/working-with-dependabot/keeping-your-actions-up-to-date-with-dependabot)
to keep them up to date.

## Setup

After generating your project, set `ci-pass` as the required status check in branch protection.

### release-plz

1. [Configure trusted publishing](https://doc.rust-lang.org/cargo/reference/registry-authentication.html#trusted-publishing) on crates.io
2. In repo settings → Actions → General, enable "Allow GitHub Actions to create and approve pull requests"

If you enabled `binary_release`, you also need a PAT so the release event triggers the binary build:

3. Create a [fine-grained PAT](https://github.com/settings/personal-access-tokens/new) with `contents: write` and `pull-requests: write` for your repo
4. Add it as a `RELEASE_PLZ_TOKEN` repo secret

Without `binary_release`, `GITHUB_TOKEN` works fine and no PAT is needed.

Alternatively, you can avoid the PAT by moving the binary build steps into the release workflow itself.

See [release-plz docs](https://release-plz.dev/docs) for more.

### Bencher (if benchmarks enabled)

1. [Create a project](https://bencher.dev/docs) on Bencher
2. Add `BENCHER_API_TOKEN` as a repo secret
3. Add your project slug as a `BENCHER_PROJECT` repo variable

See [Bencher docs](https://bencher.dev/docs) for more.

### Clippy SARIF (if clippy_sarif enabled)

Uploads clippy results to GitHub [Code Scanning](https://docs.github.com/en/code-security/code-scanning), showing warnings as inline PR annotations. Replaces the regular clippy job when enabled.

Works automatically on public repos. For private repos, enable Code Scanning at Settings → Security → Code security.

### mdBook (if mdbook enabled)

Enable GitHub Pages in repo settings (Settings → Pages → Source: GitHub Actions).

## License

Licensed under either of:

- Apache License, Version 2.0 ([LICENSE-APACHE](LICENSE-APACHE) or <http://www.apache.org/licenses/LICENSE-2.0>)
- MIT license ([LICENSE-MIT](LICENSE-MIT) or <http://opensource.org/licenses/MIT>)

at your option.
