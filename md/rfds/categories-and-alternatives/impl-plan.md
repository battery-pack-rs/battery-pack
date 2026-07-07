# Implementation Plan: Categories and Exclusive Picks

TDD-driven implementation plan for the [Categories and Exclusive Picks RFD](./README.md).

## Status

Phases 1–8 are implemented. Two items are deliberately deferred:

- **Live shared state for multi-category items** (`r[cli.picker-item-in-multiple-categories]`):
  an item in several categories is rendered as an independent row per section.
  The confirmed *result* is correct (an item checked in any section is
  selected), but the running picker does not live-propagate a toggle across an
  item's other section copies, and edit-mode pre-checks can desync. Full
  propagation needs a shared-item-identity mechanism in `sectioned-picker`
  (its rows are currently independent) — tracked as follow-up work.
- **`backend-service` service template `options.category`**: the pack now
  declares the `allocator` category, but the service template still uses a
  literal `options` list. Switching it to `options.category = "allocator"`
  changes the option values from `[jemalloc, mimalloc, system]` to the feature
  names `[jemalloc, mimalloc-alloc]`, which requires rewriting the template's
  allocator conditionals and regenerating `file!` snapshots — deferred to a
  change that can regenerate those snapshots.

## Phase dependency graph

```
Phase 1 (parsing) ──→ Phase 2 (validation) ──→ Phase 7 (validate/show)
     │                       │
     │                       ├──→ Phase 5 (CLI -F validation)
     │                       │
Phase 3 (picker) ────────────├──→ Phase 4 (CLI picker wiring)
                             │
                             └──→ Phase 6 (template category placeholders)
                                       │
                                       └──→ Phase 8 (migration)
```

Phase 1 and Phase 3 are **fully independent** — Phase 1 adds types to
`bphelper-manifest`, Phase 3 adds `SelectionMode` to the `sectioned-picker`
crate. They share no types. Phase 4 is the integration point where manifest
types meet picker types.

---

## Phase 1: Parse Item and Category Metadata

**Goal**: Extend `bphelper-manifest` to read category definitions and per-item
metadata (features, dependencies, templates) from Cargo.toml.

### 1.1 Write tests

**File**: `src/battery-pack/bphelper-manifest/src/lib.rs` (append to `mod tests`)

| Test | Input | Expected |
|------|-------|----------|
| `parse_category_definition` | `categories.hal` with title, description, `pick = "at-most-one"` | `spec.categories["hal"].pick == PickMode::AtMostOne` |
| `parse_category_default_pick_is_any` | Category with no `pick` field | `pick == PickMode::Any` |
| `parse_feature_metadata` | `features.stm32f4` with `categories = ["hal"]`, `description = "..."` | Correct `FeatureMeta` entry |
| `parse_dependency_metadata` | `dependencies.embedded-hal` with `categories = ["portable"]` | Correct `DepMeta` entry |
| `parse_template_categories` | Template entry with `categories = ["quality"]` | `spec.templates["name"].categories == vec!["quality"]` |
| `parse_item_with_multiple_categories` | `categories = ["quality", "ci"]` | Both categories stored |
| `parse_item_without_metadata` | Feature with no metadata entry | Parses identically to today |

### 1.2 Implement types

**File**: `src/battery-pack/bphelper-manifest/src/lib.rs`

```rust
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize, Default)]
#[serde(rename_all = "kebab-case")]
pub enum PickMode {
    #[default]
    Any,
    AtMostOne,
}

#[derive(Debug, Clone, Serialize, Deserialize, Default)]
pub struct CategorySpec {
    pub title: Option<String>,
    pub description: Option<String>,
    #[serde(default)]
    pub pick: PickMode,
}

#[derive(Debug, Clone, Serialize, Deserialize, Default)]
pub struct ItemMeta {
    #[serde(default)]
    pub categories: Vec<String>,
    pub description: Option<String>,
}
```

Add to `BatteryPackSpec`:
```rust
pub categories: BTreeMap<String, CategorySpec>,
pub feature_meta: BTreeMap<String, ItemMeta>,
pub dep_meta: BTreeMap<String, ItemMeta>,
```

Add to `TemplateSpec`:
```rust
pub categories: Vec<String>,
```

Note: `ItemMeta` is shared for features and dependencies — same fields.

### 1.3 Implement parsing

**File**: `src/battery-pack/bphelper-manifest/src/lib.rs`

Two deserialization structs need changes:

1. **`RawBatteryPackMetadata`** (currently only has `hidden`) — add:
   ```rust
   #[serde(default)]
   categories: BTreeMap<String, CategorySpec>,
   #[serde(default)]
   features: BTreeMap<String, ItemMeta>,
   #[serde(default)]
   dependencies: BTreeMap<String, ItemMeta>,
   ```

2. **`RawTemplateSpec`** (under `RawBatteryMetadata`, which lives under
   `[package.metadata.battery]`) — add:
   ```rust
   #[serde(default)]
   categories: Vec<String>,
   ```

   Note: templates live under `[package.metadata.battery.templates]`, NOT
   `[package.metadata.battery-pack]`. This is a two-struct split that already
   exists in the code.

Populate new `BatteryPackSpec` fields in `package_to_spec()`.

---

## Phase 2: Validation Rules

**Goal**: Enforce category reference validity and exclusive-conflict checks.

### 2.1 Write tests

**File**: `src/battery-pack/bphelper-manifest/src/lib.rs` (append to `mod tests`)

| Test | Input | Expected |
|------|-------|----------|
| `validate_exclusive_conflict_in_default` | Two features in an `at-most-one` category both in `default` | Error `format.features.exclusive-conflict` |
| `validate_exclusive_conflict_not_triggered_for_any` | Two features in an `any` category both in default | No error |
| `validate_category_reference_exists` | Feature with `categories = ["nonexistent"]` | Error `format.categories.defined` |
| `validate_clean_when_exclusive_not_in_default` | Two exclusive features, only one in default | No error |
| `validate_template_category_reference_exists` | Template with `categories = ["bogus"]` | Error `format.categories.defined` |
| `validate_dep_category_reference_exists` | Dep metadata with `categories = ["bogus"]` | Error `format.categories.defined` |
| `validate_empty_category_warns` | Category declared but nothing references it | Warning `format.categories.empty` |
| `validate_at_most_one_missing_title_warns` | `pick = "at-most-one"` with no `title` | Warning `format.categories.pick-missing-title` |
| `validate_feature_metadata_for_unknown_feature` | `features.X` metadata where `X` not in `[features]` | Error `format.features.unknown-feature` |
| `validate_dep_metadata_for_unknown_dep` | `dependencies.X` metadata where `X` not a dependency | Error `format.dependencies.unknown-dep` |
| `validate_multiple_categories_all_checked` | Feature in `categories = ["hal", "bogus"]` | Error for "bogus", not "hal" |

### 2.2 Implement

**File**: `src/battery-pack/bphelper-manifest/src/lib.rs`

Add category validation checks inside the existing `validate_spec()` function.
Not a separate function — additional checks within the existing validation
flow.

Checks:

- **r[format.categories.defined]**: For each item's `categories` entries,
  verify a matching `categories.<name>` entry exists.
- **r[format.features.exclusive-conflict]**: Collect features in each
  `at-most-one` category that appear in `default`. If any category has >1,
  emit error.
- **r[format.features.unknown-feature]**: Metadata for non-existent feature.
- **r[format.dependencies.unknown-dep]**: Metadata for non-existent dep.
- **r[format.categories.empty]**: Warning for declared but unreferenced categories.
- **r[format.categories.pick-missing-title]**: Warning for `at-most-one` without title.

---

## Phase 3: Picker UI Changes (Radio Buttons, Collapsing)

**Goal**: Extend `sectioned-picker` with `SelectionMode` and collapse
behavior. This phase is **fully independent of Phase 1** — it only touches
the `sectioned-picker` crate.

### 3.1 Write tests

**File**: `src/sectioned-picker/src/tests/state.rs`

| Test | Scenario |
|------|----------|
| `radio_mode_toggle_deselects_others` | Toggle item B when A is checked → only B checked |
| `radio_mode_allows_deselect_all` | Toggle already-selected item → nothing checked |
| `radio_mode_backspace_clears` | Backspace in radio section → all unchecked |
| `checkbox_mode_toggle_is_independent` | Existing behavior preserved |
| `radio_mode_section_toggle_is_noop` | `toggle_current_section()` in radio mode → no change |
| `checkbox_mode_section_toggle_selects_all` | `toggle_current_section()` in checkbox mode → all checked |
| `collapse_section_hides_items_from_navigation` | `move_down` skips collapsed items |
| `expand_section_restores_navigation` | Re-expand → normal traversal |
| `into_results_includes_collapsed_sections` | Collapsed sections still in results |
| `radio_pre_selected_multiple_renders_honestly` | Radio section starts with 2 checked → both shown |
| `radio_pre_selected_multiple_toggle_clears_others` | Radio section with 2 checked, toggle a third → only third checked |
| `radio_confirm_blocked_when_multiple_selected` | `try_confirm()` with >1 radio selection → returns error |
| `radio_confirm_succeeds_valid_state` | `try_confirm()` with 0 or 1 radio selection → Ok |

**File**: `src/sectioned-picker/src/tests/render.rs`

| Test | Scenario |
|------|----------|
| `render_radio_items_use_bullet_symbols` | `●`/`○` instead of `[x]`/`[ ]` |
| `render_checkbox_items_use_squares` | `[x]`/`[ ]` unchanged (regression) |
| `render_collapsed_section_shows_chevron` | `▶` prefix on collapsed header |
| `render_radio_multiple_selected_shows_all_filled` | Both render as `●` when pre-checked |
| `render_radio_section_header_shows_constraint` | Header includes `(pick at most one)` |
| `render_warning_banner_for_pre_existing_conflict` | Warning line when radio section has >1 on open |
| `render_item_with_description` | Item description shown alongside label |

### 3.2 Implement

**File**: `src/sectioned-picker/src/lib.rs`

To avoid breaking existing callers (struct literal syntax), add a constructor:

```rust
pub struct SectionItem {
    pub label: String,
    pub checked: bool,
    pub description: Option<String>,  // NEW: shown alongside label in picker
}

pub struct Section {
    pub title: String,
    pub items: Vec<SectionItem>,
    pub selection_mode: SelectionMode,
    pub collapsed: bool,
}

impl Section {
    pub fn new(title: impl Into<String>, items: Vec<SectionItem>) -> Self {
        Self {
            title: title.into(),
            items,
            selection_mode: SelectionMode::Checkbox,
            collapsed: false,
        }
    }
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Default)]
pub enum SelectionMode {
    #[default]
    Checkbox,
    Radio,
}
```

**File**: `src/sectioned-picker/src/state.rs`

The existing `coordinates: Vec<(usize, usize)>` maps each selectable index to
`(section_idx, item_idx)`. For radio behavior, `toggle()` can look up the
current section and deselect siblings.

Add to `PickerState`:
```rust
section_modes: Vec<SelectionMode>,
visible: Vec<bool>,  // per-entry; toggled by collapse
```

Changes:
- `new()`: store `section_modes` from input. Initialize `visible` all-true.
- `toggle()`: for Radio, deselect all siblings before checking toggled item.
  If already checked, just uncheck (allows zero).
- `toggle_current_section()`: in Radio mode, **no-op**. In Checkbox mode,
  existing behavior (toggle all).
- `clear_current_section()`: unconditionally uncheck all items in current
  section. Used by Backspace in Radio mode.
- `move_up()`/`move_down()`: skip entries where `visible[idx] == false`.
- New `toggle_collapse(section_idx)`: flips visibility for section's items.
- New `try_confirm() -> Result<Vec<Vec<bool>>, String>`: checks radio sections
  for >1 selection. Returns section title in error.

**File**: `src/sectioned-picker/src/lib.rs` (event loop)

- `KeyCode::Left` on section header → collapse
- `KeyCode::Right` on section header → expand
- `KeyCode::Backspace` → if current section is Radio, call
  `clear_current_section()`
- `KeyCode::Enter`: call `try_confirm()`. On `Err(msg)`, render inline error,
  stay in loop.

**File**: `src/sectioned-picker/src/render.rs`

- Radio items: `●`/`○`
- Collapsed header: `▶`; expanded: `▼`
- Radio section header appends `(pick at most one)`
- Warning banner when radio section has >1 checked on initial render

---

## Phase 4: Wire Categories into CLI Picker

**Goal**: `cargo bp add` builds picker sections from category metadata and
handles deselection (removal of previously-installed exclusive items).

### 4.1 Write tests

**File**: `src/battery-pack/bphelper-cli/src/commands/tests.rs`

| Test | Scenario |
|------|----------|
| `picker_sections_from_categories` | Pack with `hal` (at-most-one) and `utils` (any) → radio and checkbox sections |
| `picker_templates_grouped_by_category` | Templates with categories appear in category section |
| `picker_uncategorized_items_in_generic_sections` | Items without categories in "Features" / "Dependencies" sections |
| `picker_item_in_multiple_categories` | Item appears in each section; toggling in one updates the other |
| `picker_category_item_order` | Items appear in declaration order within a category |
| `picker_result_maps_radio_to_active_features` | Single radio selection → correct feature activation |
| `picker_pre_existing_exclusive_shown` | Project has both `jemalloc` and `mimalloc`; picker shows both checked with warning |
| `picker_deselection_removes_from_cargo_toml` | User selects `jemalloc` (deselects `mimalloc`); `mimalloc` removed |
| `picker_all_features_bypasses_exclusive` | `--all-features` adds all without error |

### 4.2 Implement

**File**: `src/battery-pack/bphelper-cli/src/commands.rs`

The existing `all_crates_with_grouping()` (lib.rs:586) already groups by
feature. Extend or wrap it to include category info from `spec.feature_meta`
and `spec.dep_meta`.

Refactor `pick_crates_interactive()` (~line 1414):

1. For each category (in definition order):
   - Collect features, deps, and templates that list this category.
   - Create `Section::new(title, items)` with `selection_mode` from `pick`.
2. Uncategorized items go into generic "Features" / "Dependencies" / "Templates"
   sections (unchanged behavior for existing packs).

**Shared selection state for items in multiple categories:**

When an item belongs to multiple categories, it appears in multiple picker
sections. The picker's flat `entries` vec treats these as separate items.
Strategy: maintain a `HashMap<String, Vec<usize>>` mapping item names to all
their entry indices. After each toggle, propagate the checked state to all
entries sharing the same item name. Use a picker action callback (the existing
`PickerAction` mechanism) to run propagation after each toggle.

**Deselection / removal path:**

1. Before showing picker, record which items from `at-most-one` categories are
   already in the project's `Cargo.toml` (the "before" set).
2. After confirm, compute "after" set.
3. `removed = before - after`.
4. For removed items, call `remove_deps_by_kind()` from manifest.rs (currently
   only used by `cargo bp rm`). Factor it out for shared use.

---

## Phase 5: CLI Validation for Non-Interactive `-F` Flags

**Goal**: Reject conflicting exclusive selections on the command line.

### 5.1 Write tests

**File**: `src/battery-pack/bphelper-cli/src/commands/tests.rs`

| Test | Input | Expected |
|------|-------|----------|
| `non_interactive_rejects_exclusive_conflict` | `-F stm32f4 -F nrf52840` (both in `hal`, at-most-one) | Error naming both and the category |
| `non_interactive_allows_same_any_category` | `-F http-trace -F http-timeout` (any) | OK |
| `non_interactive_allows_different_categories` | `-F stm32f4 -F embassy` | OK |
| `non_interactive_all_features_skips_validation` | `--all-features` | Bypasses checks |
| `non_interactive_template_exclusive_conflict` | `-t X -t Y` (at-most-one) | Error |
| `non_interactive_dep_exclusive_conflict` | Two deps in at-most-one category requested | Error |

### 5.2 Implement

**File**: `src/battery-pack/bphelper-cli/src/commands.rs`

```rust
fn validate_exclusive_constraints(
    spec: &BatteryPackSpec,
    features: &[String],
    templates: &[String],
) -> Result<()> {
    // Group requested items by at-most-one category.
    // If any category has >1, bail with descriptive error.
}
```

Call from the non-interactive path in `resolve_add_crates()` (~line 512).

---

## Phase 6: Category-Linked Template Placeholders

**Goal**: `options.category = "allocator"` on a `type = "select"` placeholder
derives its options from a category — single source of truth.

Note: The `OptionsSource` untagged enum is **defined and parsed in Phase 1**
(it's part of bp-template.toml deserialization). This phase consumes it during
placeholder resolution.

### 6.1 Write tests

**File**: `src/battery-pack/bphelper-cli/src/template_engine/tests.rs`

| Test | Scenario |
|------|----------|
| `options_category_derives_options` | `options.category = "allocator"` → options from category members |
| `options_category_uses_default` | Non-interactive: uses `default` value |
| `options_category_prefilled_from_picker` | User selected `jemalloc` → auto-filled, no prompt |
| `options_category_error_on_unknown` | `options.category = "nonexistent"` → error |
| `options_category_picks_up_new_members` | Add item to category → appears in options |
| `options_category_dep_uses_dep_name` | Category with deps → options use dependency names |
| `options_literal_still_works` | `options = ["a", "b"]` → unchanged behavior |

### 6.2 Implement

**File**: `src/battery-pack/bphelper-cli/src/template_engine.rs`

The `options` field on `PlaceholderDef` changes from `Vec<String>` to
`Option<OptionsSource>` (the untagged enum introduced in Phase 1):

```rust
#[derive(Debug, Deserialize)]
#[serde(untagged)]
pub(crate) enum OptionsSource {
    Literal(Vec<String>),
    Category { category: String },
}
```

No new `PlaceholderType` variant needed — `type = "select"` handles both.

Implementation — **pre-compute options in the CLI layer** (avoids coupling
template engine to manifest crate):

```rust
// In commands.rs, before template generation:
for placeholder in &mut template_defs {
    if let Some(OptionsSource::Category { category }) = &placeholder.options {
        let members = spec.items_in_category(category);
        placeholder.options = Some(OptionsSource::Literal(members));
        // Pre-fill from picker selection if available:
        if let Some(selected) = active_features_in_category(category) {
            placeholder.resolved_value = Some(selected);
        }
    }
}
```

Then `resolve_placeholders()` only sees `OptionsSource::Literal` — no
awareness of categories needed in the template engine itself.

Thread `active_features: BTreeSet<String>` from picker result through
`GenerateOpts`.

---

## Phase 7: `cargo bp validate` and `cargo bp show` Updates

**Goal**: Surface categories in validate diagnostics and show output.

### 7.1 Write tests

| Test | Scenario |
|------|----------|
| `validate_category_fixture_clean` | Fixture `category-battery-pack` validates cleanly |
| `validate_exclusive_default_fails` | Fixture with exclusive conflict → error |
| `show_renders_categories` | Show output includes "Categories:" with members |
| `show_renders_templates_in_categories` | Templates appear under their category heading |
| `show_json_includes_categories` | JSON output has category data |

### 7.2 Implement

Validation is wired from Phase 2. Ensure error messages surface through
`cargo bp validate`.

**File**: `src/battery-pack/bphelper-cli/src/commands.rs` (`build_show_report()`)

Add category rendering — categories listed with their members.

**File**: `src/cargo-bp-script/src/show.rs`

Add `CategoryInfo` to the JSON schema (translation type mirroring
`CategorySpec`/`PickMode`).

---

## Phase 8: Migration of Existing Packs

**Goal**: Add category metadata to existing battery packs.

### 8.1 Test fixture

Create `tests/fixtures/category-battery-pack/` with categories, exclusive
features, dependency metadata, and template categories.

### 8.2 Invariant tests

| Test | Scenario |
|------|----------|
| `pack_scoped_categories_no_conflict` | Two packs both define `hal` (at-most-one); select one item from each → no error |
| `battery_pack_toml_unchanged` | `cargo bp add pack -F stm32f4` where feature is categorized → `battery-pack.toml` has no category fields |

### 8.3 Migrate `backend-service-battery-pack`

- Add `allocator` category (`at-most-one`) with `jemalloc` and `mimalloc-alloc`
- Add `http-layers` category (`any`) with tower-http features
- Update `templates/service/bp-template.toml`: change allocator from
  `options = [...]` to `options.category = "allocator"`

### 8.4 Migrate `ci-battery-pack`

- Add `quality` category (`any`) for testing/analysis templates
- Add `docs` category (`any`) for documentation templates

### 8.5 Migrate `cli-battery-pack`

- Add `output` category (`any`) for indicators
- Add `input` category (`any`) for config

---

## Spec updates

No existing spec rules need to be **removed**. The following need changes:

### `md/spec/format.md` — new sections to add

- New "## Categories" section with rules:
  - `r[format.categories.definition]` — categories table structure
  - `r[format.categories.defined]` — reference validity
  - `r[format.categories.pick]` — pick mode values
  - `r[format.categories.empty]` — unused category warning
  - `r[format.categories.pick-missing-title]` — title recommendation
  - `r[format.features.exclusive-conflict]` — defaults conflict
- New "## Feature metadata" addition to existing Features section:
  - `r[format.features.metadata]` — feature metadata table
  - `r[format.features.unknown-feature]` — metadata for nonexistent feature
- New "## Dependency metadata" section:
  - `r[format.deps.metadata]` — dep metadata table
  - `r[format.dependencies.unknown-dep]` — metadata for nonexistent dep

### `md/spec/format.md` — existing rules to amend

- `r[format.templates.metadata]` (~line 112): add `categories` as optional field

### `md/spec/cli.md` — new rules

- `r[cli.add.exclusive-validation]` — non-interactive exclusive conflict check
- `r[cli.add.category-picker]` — interactive picker uses categories
- `r[cli.show.categories]` — categories displayed with members
- `r[cli.show.pick-mode]` — at-most-one hint in show output

### `md/spec/tui.md` — new section

- `r[tui.picker.radio]` — radio mode for at-most-one categories
- `r[tui.picker.checkbox]` — checkbox mode for any categories (document existing)
- `r[tui.picker.collapse]` — section collapsing with Left/Right
- `r[tui.picker.confirm-validation]` — confirm blocked on radio conflict

### `md/spec/tui.md` — existing rules to amend

- `r[tui.installed.toggle-crate]` (~line 38): note radio mode deselects siblings

---

## Known implementation challenges

1. **Two-struct metadata split** (Phase 1.3): Categories and item metadata
   live under `[package.metadata.battery-pack]` (`RawBatteryPackMetadata`),
   but template `categories` lives under `[package.metadata.battery.templates]`
   (`RawTemplateSpec`). Both structs need changes.

2. **`Section` struct breaking change** (Phase 3.2): Adding fields breaks
   struct-literal callers. Mitigated with `Section::new()` constructor.

3. **`PickerState` discards section metadata** (Phase 3.2): After
   construction, no reference to `SelectionMode`. Fixed by storing
   `section_modes: Vec<SelectionMode>` during `new()`.

4. **`selectable` vec is static** (Phase 3.2): Collapse/expand needs dynamic
   visibility. Fixed with `visible: Vec<bool>` that navigation consults.

5. **No removal path in the add flow** (Phase 4.2): `PickerResult` only
   records positive selections. Deselection requires diffing before/after
   state and calling `remove_deps_by_kind()`.

6. **Template engine doesn't have access to spec** (Phase 6.2): Solved by
   pre-computing category options in the CLI layer before calling
   `resolve_placeholders()`.

---

## Critical source files

| File | Role |
|------|------|
| `src/battery-pack/bphelper-manifest/src/lib.rs` | Metadata parsing and validation |
| `src/sectioned-picker/src/state.rs` | Picker state machine |
| `src/sectioned-picker/src/render.rs` | Picker rendering |
| `src/sectioned-picker/src/lib.rs` | Picker event loop and public types |
| `src/battery-pack/bphelper-cli/src/commands.rs` | CLI command logic |
| `src/battery-pack/bphelper-cli/src/template_engine.rs` | Template placeholder resolution |
| `src/battery-pack/bphelper-cli/src/validate.rs` | Validate subcommand |
| `src/cargo-bp-script/src/show.rs` | Show output schema |
