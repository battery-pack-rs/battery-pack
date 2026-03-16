# Contributing

> [!tip]
> Use the `init.py` script to initialise git pre-push hooks that check the formatting of your code before CI, catching common mistakes. Note that you need `typos-cli` for it to work.

## Doing releases

There is a `.github/workflows/release-plz.yml` workflow that automates releases via [release-plz]. On every push to `main`, it:

1. Opens (or updates) a release PR with version bumps and changelog entries
2. When the release PR is merged, publishes the new versions to crates.io and creates GitHub releases

The release PR is generated from [conventional commits], so please use that format for your commit messages.

Before merging a release PR, review the generated changelog entries and edit them to be human-readable — release-plz generates entries from raw commit messages, which often need cleanup.

[release-plz]: https://github.com/release-plz/release-plz/
[conventional commits]: https://www.conventionalcommits.org/en/v1.0.0/

### Publishing a new crate

Trusted publishing cannot publish a crate that doesn't exist on crates.io yet. To add a new crate:

1. Get a temporary crates.io token and run `cargo publish -p <crate-name>` manually.
2. Set up [trusted publishing](https://doc.rust-lang.org/cargo/reference/registry-authentication.html#trusted-publishing) for the new crate on crates.io, pointing to the `release-plz.yml` workflow.
3. Revoke the temporary token.
4. If the new crate should appear in the `battery-pack` release notes, add it to `changelog_include` in `release-plz.toml`.
