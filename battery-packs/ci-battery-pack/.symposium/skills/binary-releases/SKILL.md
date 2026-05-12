---
name: binary-releases
description: Cross-platform binary builds for GitHub Releases with cargo-binstall support
---

# Binary Releases

Cross-platform binary distribution via GitHub Releases, discoverable by cargo-binstall.

> **Add to an existing project:**
> ```sh
> cargo bp add ci -t binary-release
> ```

## How It Works

The `build-binaries.yml` workflow triggers on `release: [published]` events. When release-plz publishes a new version and creates a GitHub Release, this workflow builds binaries for each target platform and uploads them as release assets.

Users can then install your binary without compiling:

```sh
cargo binstall your-crate
```

## Build Matrix

The default matrix covers the most common targets:

| Target | Runner | Archive |
|--------|--------|---------|
| `x86_64-unknown-linux-gnu` | `ubuntu-latest` | `.tar.gz` |
| `aarch64-unknown-linux-gnu` | `ubuntu-24.04-arm` | `.tar.gz` |
| `x86_64-apple-darwin` | `macos-latest` | `.tar.gz` |
| `aarch64-apple-darwin` | `macos-latest` | `.tar.gz` |
| `x86_64-pc-windows-msvc` | `windows-latest` | `.zip` |

To add or remove targets, edit the `matrix.include` array in `build-binaries.yml`.

## cargo-binstall Discovery

The template adds a `[package.metadata.binstall]` section to your `Cargo.toml`:

```toml
[package.metadata.binstall]
pkg-url = "{ repo }/releases/download/{ name }-v{ version }/{ name }-{ target }-v{ version }{ archive-suffix }"
```

This tells cargo-binstall where to find pre-built binaries. The URL pattern must match the archive naming convention used in the workflow's packaging steps.

## What the agent can do

- Run `cargo bp add ci -t binary-release` to generate the workflow and Cargo.toml metadata
- Edit the build matrix (add/remove targets, change runners)
- Add or update `[[bin]]` and `[package.metadata.binstall]` in Cargo.toml
- Adjust the version extraction logic or archive naming

## What requires human action (outside the repo)

The PAT and secret setup requires GitHub web UI access. Instruct the user to complete:

1. Create a fine-grained PAT at https://github.com/settings/personal-access-tokens/new
2. Grant `contents: write` and `pull-requests: write` for the target repo
3. Add it as a `RELEASE_PLZ_TOKEN` repository secret
4. The trusted-publishing template automatically uses this token when `binary_release` is enabled

See the **trusted-publishing** skill for the full release pipeline setup.

## PAT Requirement (why it's needed)

This workflow requires a `RELEASE_PLZ_TOKEN` PAT (not just `GITHUB_TOKEN`). Events created by `GITHUB_TOKEN` do not trigger other workflows. Since release-plz creates the GitHub Release, and this workflow triggers on that release event, the release must be created with a PAT for the event to propagate.

## Version Extraction

The workflow extracts the version from the release tag name. Release-plz uses tags like `my-crate-v0.1.0` for workspace crates or `v0.1.0` for single-crate repos. The extraction step strips the package name prefix:

```yaml
TAG="${{ github.event.release.tag_name }}"
VERSION="${TAG##*-v}"
```

This produces archive names like `my-crate-x86_64-unknown-linux-gnu-v0.1.0.tar.gz`.

## Cargo.toml Requirements

Your crate needs a `[[bin]]` section:

```toml
[[bin]]
name = "my-tool"
path = "src/main.rs"
```

The template generates a placeholder `src/main.rs` if one doesn't exist. The `-p` flag in the build command (`cargo build --release --target ${{ matrix.target }} -p my-crate`) targets the specific package in a workspace.

## Relationship to Trusted Publishing

Binary releases sit downstream of the release pipeline:

1. Code merges to main
2. `release.yml` (release-plz) publishes to crates.io and creates a GitHub Release
3. The release event triggers `build-binaries.yml`
4. Binaries are uploaded to the GitHub Release as assets

If you only need crate publishing (no pre-built binaries), use `cargo bp add ci -t trusted-publishing` alone. The binary-release template is an add-on that requires trusted-publishing to be configured first.
