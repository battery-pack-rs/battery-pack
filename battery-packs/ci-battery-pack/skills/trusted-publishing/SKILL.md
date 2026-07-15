---
name: trusted-publishing
description: Setting up automated crate releases with release-plz and OIDC trusted publishing
---

# Trusted Publishing

Automated crate releases using release-plz with OIDC trusted publishing on crates.io.

> **Add to an existing project:**
> ```sh
> cargo bp add ci -t trusted-publishing
> ```

## How It Works

The generated `release.yml` workflow has two jobs that run sequentially on pushes to main:

1. **`release-plz-release`**: Checks if any crate versions were bumped. If so, publishes to crates.io using OIDC (no API tokens stored as secrets) and creates a GitHub Release.
2. **`release-plz-pr`**: Opens/updates a PR that bumps versions and updates changelogs based on conventional commits since the last release.

The PR job runs after the release job (`needs: release-plz-release`) with `if: ${{ !failure() }}`, so it runs even if the release job was skipped (nothing to publish) but not if it failed.

## What the agent can do

- Run `cargo bp add ci -t trusted-publishing` to generate the workflow and config
- Edit `release-plz.toml` to customize release behavior
- Add the repository owner guard value to the workflow

## What requires human action (outside the repo)

These steps involve web UIs that the agent cannot access. Instruct the user to complete them after generating files.

### 1. Configure trusted publishing on crates.io

Go to your crate's settings on crates.io and add a trusted publisher:

- **Repository owner:** your GitHub org or username
- **Repository name:** the repo name
- **Workflow name:** `release.yml`
- **Environment:** (leave blank)

For new crates that don't exist on crates.io yet, use "Add a new crate" under your account settings.

Docs: https://crates.io/docs/trusted-publishing

### 2. Enable PR creation

In your repo's Settings, go to Actions, then General, and enable "Allow GitHub Actions to create and approve pull requests." Without this, the release-pr job silently fails to create PRs.

### 3. Token choice

**Without binary releases:** `GITHUB_TOKEN` (automatic, no setup needed). The workflow uses it for both creating releases and opening PRs.

**With binary releases:** You need a `RELEASE_PLZ_TOKEN` PAT. This is because GitHub's `GITHUB_TOKEN` does not trigger other workflows. Since the binary build workflow triggers on `release: [published]`, the release event must come from a PAT to propagate.

Create a fine-grained PAT at https://github.com/settings/personal-access-tokens/new with:
- **Repository access:** Only the target repo
- **Permissions:** `contents: write`, `pull-requests: write`

Add it as a `RELEASE_PLZ_TOKEN` repository secret.

## release-plz.toml

The generated config is minimal:

```toml
[workspace]
git_release_enable = true
changelog_update = true
```

Common customizations:
- `publish_timeout = "10m"` for large crates or slow registry propagation
- `semver_check = true` to run cargo-semver-checks before publishing
- `changelog_config = "cliff.toml"` for custom changelog formatting

Full reference: https://release-plz.dev/docs/config

## Concurrency

The release-pr job uses a non-cancelling concurrency group:

```yaml
concurrency:
  group: release-plz-${{ github.ref }}
  cancel-in-progress: false
```

This prevents concurrent release-pr runs from racing on the same branch, which could cause force-push conflicts on the release PR.

## Repository Owner Guard

Both jobs include an `if` condition checking the repository owner:

```yaml
if: ${{ github.repository_owner == 'your-org' }}
```

This prevents the workflow from running on forks (where it would fail due to missing secrets and permissions).

## Common Issues

**"Resource not accessible by integration"**: The PR creation permission isn't enabled. See step 2 above.

**Release created but binary build didn't trigger**: You're using `GITHUB_TOKEN` instead of a PAT. See step 3 above.

**"crate not found" on first publish**: You need to pre-register the crate's trusted publisher on crates.io before the first publish. For new crates, use the "Add a new crate" flow under your crates.io account settings.

**Release PR not updating**: Check that the concurrency group isn't stuck. A failed run with `cancel-in-progress: false` can block subsequent runs until it times out.
