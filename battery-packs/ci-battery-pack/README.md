# ci-battery-pack

A [battery pack](https://crates.io/crates/battery-pack) for CI/CD workflows in Rust projects.

Currently supports GitHub Actions. Someday there will be more supported platforms.

## Quick Start

Minimalist (core CI only):

```sh
cargo bp new ci --name my-project
```

Maximalist (everything enabled):

```sh
cargo bp new ci --name my-project -d all
```

Pick individual features:

```sh
cargo bp new ci --name my-project -d ci_platform=github -d benchmarks -d fuzzing -d spellcheck
```

Config files only (no CI workflows):

```sh
cargo bp new ci --name my-project -d ci_platform=none
```

Generates config files only (deny.toml, release-plz.toml) without CI workflows. Useful if you use a different CI system.

Or run `cargo bp new ci` interactively and answer the prompts.

Each optional feature is also available as a standalone template:

```sh
cargo bp new ci --template fuzzing --name my-project
```

## What You Get

### Always included

- [cargo-deny](https://github.com/EmbarkStudios/cargo-deny) config (`deny.toml`)
- README with CI, crates.io, and docs.rs badges
- Stub Cargo.toml, src/lib.rs with test

### GitHub Actions (`ci_platform=github`, the default)

- CI workflow: fmt, clippy, build matrix (MSRV x stable x nightly), feature powerset, semver-checks, gate job
- Security audit workflow (cargo-deny, daily + on Cargo.toml changes)
- Dependabot config for Cargo and GitHub Actions updates

### Optional features (`-d flag`)

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

See [release-plz docs](https://release-plz.dev/docs) for more.

### Bencher (if benchmarks enabled)

1. [Create a project](https://bencher.dev/docs) on Bencher
2. Add `BENCHER_API_TOKEN` as a repo secret
3. Add your project slug as a `BENCHER_PROJECT` repo variable

See [Bencher docs](https://bencher.dev/docs) for more.

## License

Licensed under either of:

- Apache License, Version 2.0 ([LICENSE-APACHE](LICENSE-APACHE) or <http://www.apache.org/licenses/LICENSE-2.0>)
- MIT license ([LICENSE-MIT](LICENSE-MIT) or <http://opensource.org/licenses/MIT>)

at your option.
