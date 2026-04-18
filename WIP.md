# `cargo bp` Interface Rework

Each item below is a self-contained change, ordered by execution sequence.
Implement and commit each one individually with a descriptive message.

---

## 1. Expanded Cargo.toml metadata — track bp-managed dependencies

When `cargo bp add` writes a dependency, record it in metadata so we know it was tool-managed.

### Current format

```toml
[package.metadata.battery-pack.cli-battery-pack]
features = ["default"]
```

### New format

```toml
[package.metadata.battery-pack.cli-battery-pack]
features = ["default"]
managed-deps = ["clap", "dialoguer", "console"]
```

`features` continues to track which battery pack features are currently active (as before).
`managed-deps` is new — it tracks which crate dependencies the tool added to Cargo.toml, so that `cargo bp rm` and `cargo bp show` can distinguish tool-managed deps from user-manual additions.

Note: the metadata format switches from an inline table to a regular table to accommodate the longer `managed-deps` array. This will break existing snapshot tests (acceptable).

### Implementation

Split into two commits:

**Commit 1a — format change:** Refactor `write_bp_features_to_doc()` to write a regular TOML table instead of an inline table. Fix all broken snapshot tests. No functional change.

**Commit 1b — add `managed-deps`:**
- In `add_battery_pack()` and the TUI add flow, after resolving which crates to install, write `managed-deps` with the list of crate names added.
- In `sync_battery_packs()`, update `managed-deps` if new crates are added during sync.
- `managed-deps` is append-only unless `rm` removes entries.
- Refactor `write_bp_features_to_doc()` to write a regular TOML table instead of an inline table, and accept an optional `managed_deps` parameter.
- **Migration:** when `sync` or `add` runs on a project with old-format metadata (no `managed-deps`), populate `managed-deps` from the currently-resolved crates. This provides graceful migration for existing projects.
- **Metadata location:** `managed-deps` must respect the existing `MetadataLocation` logic (package vs workspace) — written to the same place as `features`.

### Testing

- **Unit-testable:** After calling the add logic on a fixture manifest, parse the output and assert `managed-deps` contains the expected crate names.
- **Integration-testable:** Round-trip: add a pack, read metadata, assert managed-deps; add a feature, assert managed-deps grew.
- **Edge case:** Ensure `sync` doesn't duplicate entries in `managed-deps`.

---

## 2. Remove `cargo bp enable` command

The `enable` subcommand is removed. Its functionality is absorbed into `cargo bp add` (item 4).

### Implementation

- Remove the `Enable` variant from `BpCommands`.
- Remove the `enable_feature()` function and its match arm in `main()`.
- Update help text / README if it references `enable`.

### Testing

- **Unit-testable:** `Cli::parse()` rejects `cargo bp enable ...` (assert it errors).
- **Integration test:** Run `cargo-bp bp enable foo` via `assert_cmd::Command`, assert non-zero exit and stderr contains an error about unknown subcommand.
- Note: there are no existing tests for `enable_feature()`, so this is a clean deletion.

---

## 3. `cargo bp add` with no argument — helpful message instead of TUI

When `cargo bp add` is run without a battery pack name, instead of launching the TUI:
- List the battery packs currently installed in the project.
- Tell the user to rerun with a specific pack name.
- Suggest `cargo bp ls` to discover and install new packs.

### Implementation

- In the `None` branch of `BpCommands::Add { battery_pack: None, .. }` (replaces both the interactive TUI launch and the existing non-interactive error message):
  - Call `find_installed_bp_names()`.
  - If packs are found, print them and suggest `cargo bp add <name>`.
  - Always suggest `cargo bp ls` for discovery.
  - Remove the `run_add(source)` TUI launch from this path.

### Testing

- **Integration test (packs installed):** `make_temp_project()`, add a pack via `add_battery_pack()`, then call the no-arg `add` path. Capture stdout, snapshot-assert it lists the installed pack name and includes `cargo bp add <name>` and `cargo bp ls` suggestions.
- **Integration test (no packs):** `make_temp_project()` with no packs installed, call the no-arg `add` path. Snapshot-assert it says no packs are installed and suggests `cargo bp ls`.
- **Integration test (not in project):** Run from a directory with no Cargo.toml. Assert appropriate error message.

---

## 4. Reworked `cargo bp add` interactive picker

`cargo bp add` is an edit operation — re-running it on an already-installed pack lets you change the selection. `edit` should be a visible alias for `add`.

The picker shown by `cargo bp add <pack>` is redesigned:
- **Features are listed first**, then individual crates.
- Selecting/deselecting features and crates is independent (no bidirectional auto-sync — that's a nice-to-have for later).

### Implementation

- Add `#[command(visible_alias = "edit")]` to the `Add` variant.
- Update `pick_crates_interactive()` to show features as selectable items before the individual crates. Keep using `dialoguer::MultiSelect`.
- **Edit semantics:** when re-running on an installed pack, deselected crates are removed from `[dependencies]` and from `managed-deps`. Newly selected crates are added. This makes `add` a full edit operation.
- **Removal path:** `add_battery_pack()` currently only adds. It needs a new code path (or a companion function) to remove deselected crates from the manifest and from `managed-deps`. The TUI's `apply_add_changes()` also has a `// TODO` stub for fine-grained removal that needs to be resolved.

### Testing

- **Unit-testable:** The edit logic — given a previous set of crates and a new set, assert the correct adds and removes are computed.
- **Integration test:** `make_temp_project()`, add a pack, then re-run add with a different selection. Snapshot the resulting Cargo.toml to verify crates were added/removed and `managed-deps` updated correctly.
- **Hard to test:** The interactive picker behavior in a real terminal.

---

## 5. `cargo bp add` picker — pre-select based on current Cargo.toml

When running `cargo bp add <pack>` on a pack that's already installed, the picker's initial selection should reflect what's actually in the project's Cargo.toml, not the pack's defaults.

### Implementation

- Before showing the picker, read the project's dependencies (all sections + workspace deps).
- For each crate in the battery pack spec, check if it's already present in the project.
- Use that as the initial checked state instead of the pack's default feature set.
- **Fresh-install fallback:** if the pack is not yet installed (no deps in Cargo.toml match), fall back to the pack's default feature set for initial selection.
- Also derive initial feature checkbox state: a feature is checked iff all its member crates are present.

### Testing

- **Unit-testable:** Given a manifest with some deps already present and a battery pack spec, assert the computed initial selection matches.
- **Integration test:** `make_temp_project()`, add a pack with a subset of crates, then call the pre-selection resolution logic. Assert the initial selection matches what's in the manifest (not the pack defaults). Also test the fresh-install case (no deps present) to confirm it falls back to pack defaults.
- **Hard to test:** Visual correctness of the pre-selected state in the TUI.

---

## 6. New `cargo bp rm` command

Remove a battery pack from the current crate. Offers to also remove dependencies that were added by the tool.

### Implementation

- Add `Rm` variant to `BpCommands` with a required `battery_pack: String` argument and optional `--remove-deps` / `--keep-deps` flags (for non-interactive/CI use; if neither is passed and stdout is a TTY, prompt interactively; if not a TTY, default to `--keep-deps`).
- Steps:
  1. Read the project manifest and find the battery pack in `[build-dependencies]` and metadata.
  2. Read `managed-deps` from metadata (see item 1) to know which deps the tool added.
  3. Check which of those deps are still needed by other installed packs (cross-reference).
  4. Prompt: "Also remove these dependencies? [list]" (skip deps shared with other packs).
  5. Remove the battery pack from `[build-dependencies]`, metadata, and (if confirmed) the managed deps from `[dependencies]`/`[dev-dependencies]`/workspace deps.
  6. Clean up `build.rs` validate call for the removed pack. If removing the call leaves an empty `main() {}`, delete `build.rs` entirely. If the user modified the line, warn and skip.

**Edge cases:**
- **Pre-migration projects** (no `managed-deps` in metadata): assume all deps are user-managed — remove the battery pack registration and build-dep but don't touch any crate dependencies.
- **Workspace deps:** when deps live in `[workspace.dependencies]`, only remove them if no other workspace member references them. If unsure, warn and skip.
- **`managed-deps` must respect `MetadataLocation`** (package vs workspace) — same logic as the existing `resolve_metadata_location()` in `manifest/mod.rs`.

### Testing

- **Unit-testable:** The logic that determines which deps are safe to remove (given two packs' managed-deps lists and the user's choice). Pure function.
- **Integration test:** Write a Cargo.toml with a registered pack + managed deps, run the removal logic, assert the output manifest is correct. Second test: add two packs that share a dep, remove one, assert the shared dep survives.
- **Hard to test:** The interactive prompt and `build.rs` cleanup edge cases.

---

## 7. `cargo bp show` — display features

The `show` / `info` command (both TUI and non-interactive) should display the battery pack's features alongside its crates.

### Implementation

- In `print_battery_pack_detail()`: after the "Crates" section, add a "Features" section listing each feature name and its member crates.
- In the TUI `DetailScreen` rendering: add a features section.
- The `BatteryPackDetail` struct needs a `features: BTreeMap<String, Vec<String>>` field. It must be populated in both `fetch_battery_pack_detail()` and `fetch_battery_pack_detail_from_source()` in `registry/mod.rs` (both paths construct this struct).

### Testing

Both paths need snapshot tests proving features appear:

- **Non-interactive (`print_battery_pack_detail`):** Use `assert_cmd` to run `cargo-bp bp show <pack> --non-interactive --path <fixture>`, capture stdout, snapshot-assert that feature names and their member crates appear in the output.
- **TUI (`render_detail`):** Use `render_to_string()` with a `BatteryPackDetail` that has features; snapshot-assert the features section appears in the rendered buffer.
- Duplication of rendering logic between the two paths is fine — the snapshot tests keep them honest.

---

## 8. `cargo bp show` — display installed state for current project

When running `cargo bp show <pack>` inside a project that has the pack installed, the output should annotate which crates and features are currently installed and managed by the tool.

### Implementation

- Read the project's metadata for the pack (`managed-deps` and `features` from item 1).
- `print_battery_pack_detail()` currently takes `(name, path, source)` — it needs an additional optional project-dir parameter to read installed state. Same for the TUI path (pass installed state into `DetailScreen`).
- In `print_battery_pack_detail()` (non-interactive): next to each crate, show a marker (e.g. `✓` or `[installed]`) if it's in `managed-deps`. Same for features — mark which are active via `features`.
- In the TUI `render_detail()`: same markers in the crates and features sections.
- If not inside a project (or pack not installed), show the plain view with no markers.

### Testing

- **Integration test (non-interactive):** `make_temp_project()`, add a pack, then call `print_battery_pack_detail()` from that project dir. Snapshot stdout and assert installed markers appear on the right crates/features.
- **Integration test (not installed):** Call `print_battery_pack_detail()` for a pack that isn't installed. Assert no markers appear.
- **TUI snapshot:** `render_to_string()` with a `DetailScreen` that has installed-state info. Assert markers render.

---

## 9. `cargo bp ls` — disable "Add to project" when not in a cargo workspace

In the TUI detail view (reached from `cargo bp ls`), the "Add to project" action should be disabled (grayed out, non-selectable) when the user is not inside a Cargo project/workspace.

### Implementation

- In `tui.rs`, detect whether a Cargo.toml is reachable from the current directory at app startup (e.g. `find_user_manifest` returns `Err`).
- Store an `in_project: bool` flag on `App`.
- In `DetailScreen` rendering: if `!in_project`, render "Add to project" with a dim/disabled style and skip it during navigation / Enter handling.

### Testing

- **Unit-testable:** The project-detection logic itself (already exists as `find_user_manifest`).
- **Snapshot-testable:** Render the detail screen with `in_project = false` and assert the "Add to project" line is styled as disabled.
- **Hard to test:** The actual TUI interaction (skipping the item on keypress) — best verified manually.

---

## 10. `cargo bp ls` — context-aware action when pack is already installed

In the TUI detail view, if the battery pack is already installed in the current project:
- Replace "Add to project" with **"Add crates or features"** (which routes to the `cargo bp add` picker for that pack).

### Implementation

- At app startup (or when entering the detail view), check if the battery pack name appears in the project's `[build-dependencies]` or metadata.
- Use `find_installed_bp_names()` to get the list of installed packs.
- Swap the action label and route the action to shell out to `cargo bp add <name>` (consistent with current `PendingAction::AddToProject` pattern — no need to embed the picker in the TUI).

### Testing

- **Unit-testable:** Given a mock manifest with an installed pack, assert the detail view produces the "Add crates or features" action variant.
- **Snapshot-testable:** Render the detail screen for an installed pack and assert the label text.
- **Hard to test:** The routing from the relabeled action into the picker — manual verification.

---

## 11. One-shot TUI actions — exit on success, return to TUI on cancel

When the user takes an action from the TUI (add to project, create new project from template):
- **Success** → exit the entire `cargo bp` process.
- **Cancel** → return to the TUI screen the user was on.

### Implementation

- The TUI shells out to `cargo bp add` and `cargo bp new` as subprocesses via `std::process::Command` (see `PendingAction::AddToProject` and `PendingAction::NewProject` in `tui.rs`).
- After the subprocess completes, check `status.success()`:
  - If true → set `should_quit = true` (exit the TUI process entirely).
  - If false (error or cancel) → restore the previous screen and return to the TUI.
- Remove the `wait_for_enter()` call on success. Keep it only for error/cancel.

### Testing

- **Unit-testable:** The action-dispatch logic can be tested by asserting `should_quit` is set after a successful `PendingAction` completes.
- **Hard to test:** The full flow of exit-vs-return requires manual TUI interaction or an integration harness that simulates keypresses.

---

## Execution order rationale

The items above are already in execution order. The dependencies are:

- **1 first** (metadata expansion) — foundational for `rm`, `show` installed state, and the reworked `add`.
- **2 next** (remove `enable`) — clears the deck before reworking `add`.
- **3** (no-arg `add` message) — simple, decouples `add` from the TUI launcher.
- **4 + 5** (picker rework + pre-selection) — the big interactive change, best done together.
- **6** (`rm`) — depends on 1.
- **7 + 8** (`show` features + installed state) — 7 adds the features section, 8 annotates it with installed markers. Both depend on 1 for `managed-deps`.
- **9 + 10** (list/detail context-awareness) — depends on knowing installed state.
- **11 last** (one-shot exit) — touches all TUI action paths, best done last.
