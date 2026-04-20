# Snapbox assertions

When adding new screens or components, you must add snapbox snapshot tests using `snapbox::assert_data_eq!` with `file![_]` or `str![[...]]`. To generate or update snapshot files:

```
SNAPSHOTS=overwrite cargo nextest run --all-features
```

# When authoring Rust code...

* Use `cargo add` to add dependencies or edit Cargo.toml to be sure and get the latest version
* Use Rust 2024 edition
* Separate code into logical "paragraphs" and include a short comment indicating the intent of each "paragraph", what is it trying to achieve?

# When writing comments and documentation...

* Document the destination, not the journey:
  * Don't discuss intermediate states that arose during the editing. Just document how things work now!
  * Ask yourself what a new reader with no context would think.

# After editing Rust code...

After you have complete changes to Rust code, remember to

* run `cargo fmt` to ensure it is formatted
* run `cargo clippy` to address any lint warnings
* run `cargo check` to see if it builds

# How to run tests...

Run `cargo test --all --workspace` to check tests.

# Before creating a commit...

The following checks are done in CI. Before creating a git commit always do the following:

* check that the mdbook docs, particularly user-facing docs and architecture docs, are up-to-date!
* run `cargo check` and address warnings like dead code, unused imports, etc
* run `cargo fmt` to ensure the code is formatted
* run `cargo clippy` to ensure the code has no lint warnings
* run `cargo test --all --workspace` to be sure the tests are still passing after doing the above

# Use git responsibly, track your history

Prefer to create new commits unless the user explicitly asks you to amend.

Do not ever run git push without confirming it with the user.
