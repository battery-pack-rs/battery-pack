# Implementation Plan: Categories and Exclusive Picks

TDD-driven implementation plan for the [Categories and Exclusive Picks RFD](./categories-and-alternatives.md).

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

## Phase 1: Parse Category and Feature Metadata

**Goal**: Extend `bphelper-manifest` to read
`[package.metadata.battery-pack.categories.*]` and
`[package.metadata.battery-pack.features.*]` from Cargo.toml.

### 1.1 Write tests

**File**: `src/battery-pack/bphelper-manifest/src/lib.rs` (append to `mod tests`)

| Test | Input | Expected |
|------|-------|----------|
| `parse_categories_from_metadata` | `categories.hal` with title, description, `pick = "at-most-one"` | `spec.categories["hal"].pick == PickMode::AtMostOne` |
| `parse_feature_category_annotation` | `features.stm32f4` with `category = "hal"`, `description = "STM32F4xx"` | `spec.feature_meta["stm32f4"].category == Some("hal")` |
| `parse_nested_categories` | `categories."hal.stm32"` with title | Parsed with key `"hal.stm32"` |
| `parse_categories_default_pick_is_any` | Category with no `pick` field | `pick == PickMode::Any` |
| `parse_template_category` | Template entry with `category = "release"` | `spec.templates["name"].category == Some("release")` |
| `feature_without_category_still_works` | Features with no metadata annotations | Parses identically to today |

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

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct CategorySpec {
    pub title: Option<String>,
    pub description: Option<String>,
    #[serde(default)]
    pub pick: PickMode,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct FeatureMeta {
    pub category: Option<String>,
    pub description: Option<String>,
}
```

Add to `BatteryPackSpec`:
```rust
pub categories: BTreeMap<String, CategorySpec>,
pub feature_meta: BTreeMap<String, FeatureMeta>,
```

Add to `TemplateSpec`:
```rust
pub category: Option<String>,
```

Note: `CategorySpec.title` is `Option<String>` (not required) — validation
warns if missing for `at-most-one` categories, but it's not a parse error.

### 1.3 Implement parsing

**File**: `src/battery-pack/bphelper-manifest/src/lib.rs`

Two deserialization structs need changes:

1. **`RawBatteryPackMetadata`** (currently only has `hidden`) — add:
   ```rust
   #[serde(default)]
   categories: BTreeMap<String, CategorySpec>,
   #[serde(default)]
   features: BTreeMap<String, FeatureMeta>,
   ```

2. **`RawTemplateSpec`** (under `RawBatteryMetadata`, which lives under
   `[package.metadata.battery]`) — add:
   ```rust
   #[serde(default)]
   category: Option<String>,
   ```

   Note: templates live under `[package.metadata.battery.templates]`, NOT
   `[package.metadata.battery-pack]`. The category and features maps live
   under `battery-pack`. This is a two-struct split that already exists in the
   code; we just extend each side.

Populate new `BatteryPackSpec` fields in `package_to_spec()` (~line 936).

---

## Phase 2: Validation Rules

**Goal**: Enforce `format.features.exclusive-conflict`,
`format.categories.defined`, and related checks.

### 2.1 Write tests

**File**: `src/battery-pack/bphelper-manifest/src/lib.rs` (append to `mod tests`)

| Test | Input | Expected |
|------|-------|----------|
| `validate_exclusive_conflict_in_default` | Two features in an `at-most-one` category both in `default` | Error `format.features.exclusive-conflict` |
| `validate_exclusive_conflict_not_triggered_for_any` | Two features in an `any` category both in default | No error |
| `validate_category_reference_exists` | `category = "nonexistent"` | Error `format.categories.defined` |
| `validate_nested_category_resolves_to_ancestor` | `category = "hal.stm32"` when `categories.hal` exists | No error |
| `validate_orphaned_subcategory` | `category = "hal.stm32"` declared, but no `categories.hal` parent | Error `format.categories.defined` |
| `validate_clean_when_exclusive_not_in_default` | Two exclusive features, only one in default | No error |
| `validate_template_category_reference_exists` | Template `category = "bogus"` | Error `format.categories.defined` |
| `validate_empty_category_warns` | Category declared but no feature or template references it | Warning `format.categories.empty` |
| `validate_at_most_one_missing_title_warns` | `pick = "at-most-one"` with no `title` field | Warning `format.categories.pick-missing-title` |
| `validate_feature_metadata_for_unknown_feature` | `[package.metadata.battery-pack.features.X]` where `X` not in `[features]` | Error `format.features.unknown-feature` |

### 2.2 Implement

**File**: `src/battery-pack/bphelper-manifest/src/lib.rs`

Add category validation checks inside the existing `validate_spec()` function
(~line 319). This is not a separate function to wire — it's additional checks
within the existing validation flow.

Checks:

- **r[format.categories.defined]**: For each feature's `category` and each
  template's `category`, verify a matching `categories.<name>` entry exists
  (or an ancestor via dot notation).
- **r[format.features.exclusive-conflict]**: Collect features in each
  `at-most-one` category that appear in `default`. If any category has >1,
  emit error.
- **r[format.categories.empty]**: Warning for declared but unreferenced categories.
- **r[format.categories.pick-missing-title]**: Warning for `at-most-one` without title.
- **r[format.features.unknown-feature]**: Error for metadata on non-existent feature.

Helper:
```rust
fn category_exists(&self, name: &str) -> bool {
    self.categories.contains_key(name)
        || name.rsplit_once('.')
            .map(|(parent, _)| self.category_exists(parent))
            .unwrap_or(false)
}
```

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
| `checkbox_mode_toggle_is_independent` | Existing behavior preserved |
| `radio_mode_section_toggle_clears_all` | `toggle_current_section()` in radio mode → all unchecked |
| `collapse_section_hides_items_from_navigation` | `move_down` skips collapsed items |
| `expand_section_restores_navigation` | Re-expand → normal traversal |
| `into_results_includes_collapsed_sections` | Collapsed sections still in results |
| `radio_pre_selected_multiple_renders_honestly` | Radio section starts with 2 checked → both shown (no auto-deselection on load) |
| `radio_pre_selected_multiple_toggle_clears_others` | Radio section with 2 checked, user toggles a third → only the third is checked |
| `radio_confirm_blocked_when_multiple_selected` | `try_confirm()` with >1 radio selection → returns `Err` with category name |

**File**: `src/sectioned-picker/src/tests/render.rs`

| Test | Scenario |
|------|----------|
| `render_radio_items_use_bullet_symbols` | `●`/`○` instead of `[x]`/`[ ]` |
| `render_collapsed_section_shows_chevron` | `▶` prefix on collapsed header |
| `render_radio_multiple_selected_shows_all_filled` | When 2 items are pre-checked in radio mode, both render as `●` |
| `render_radio_section_header_shows_constraint` | Header includes `(pick at most one)` text |

### 3.2 Implement

**File**: `src/sectioned-picker/src/lib.rs`

The `Section` struct currently has no derives and callers construct it with
struct literal syntax. To avoid breaking all existing call sites, add defaults:

```rust
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

Existing callers migrate from `Section { title, items }` to
`Section::new(title, items)`. New callers can set `selection_mode` and
`collapsed` after construction.

**File**: `src/sectioned-picker/src/state.rs`

Key insight: `PickerState` already stores a `coordinates: Vec<(usize, usize)>`
mapping each selectable index to `(section_idx, item_idx)`. For radio
behavior, `toggle()` can look up the current section via
`self.coordinates[self.cursor].0` and iterate `coordinates` for all entries
with matching `section_idx` to deselect siblings.

Add to `PickerState`:
```rust
section_modes: Vec<SelectionMode>,  // one per section, set during new()
visible: Vec<bool>,                 // per-entry visibility; toggled by collapse
```

Changes:
- `new()`: store `section_modes` from input sections. Initialize `visible`
  all-true (collapsed sections start with items hidden).
- `toggle()`: check `section_modes[section_idx]` — for Radio, deselect all
  siblings in same section before checking the toggled item.
- `toggle_current_section()`: in Radio mode, deselects all (can't "check all").
- `move_up()`/`move_down()`: skip entries where `visible[idx] == false`.
- New `toggle_collapse(section_idx)`: flips visibility for all items in that section.
- New `try_confirm() -> Result<Vec<Vec<bool>>, String>`: checks radio sections
  for >1 selection before returning results. On violation, returns the section
  title in the error.

**File**: `src/sectioned-picker/src/lib.rs` (event loop)

- `KeyCode::Left` on a section header → collapse
- `KeyCode::Right` on a section header → expand
- `KeyCode::Enter`: call `try_confirm()`. If `Err(msg)`, render inline error
  and stay in the loop (no new `PickerOutcome` variant needed).

**File**: `src/sectioned-picker/src/render.rs`

- Radio items: `●`/`○` instead of `[x]`/`[ ]`
- Collapsed header: `▶` prefix; expanded: `▼` prefix
- Radio section header appends `(pick at most one)`
- Inline error line rendered below the section when confirm is blocked

---

## Phase 4: Wire Categories into CLI Picker

**Goal**: `cargo bp add` builds picker sections from category metadata and
handles deselection (removal of previously-installed exclusive crates).

### 4.1 Write tests

**File**: `src/battery-pack/bphelper-cli/src/commands/tests.rs`

| Test | Scenario |
|------|----------|
| `picker_sections_from_categories` | Pack with `hal` (at-most-one) and `utils` (any) → radio and checkbox sections |
| `picker_nested_categories_produce_subsections` | `hal.stm32` features under "STMicroelectronics" subsection |
| `picker_templates_grouped_by_category` | Templates with `category` appear in category section |
| `picker_uncategorized_features_at_end` | Features without category in trailing "Features" section |
| `picker_result_maps_radio_to_active_features` | Single radio selection → correct `active_features` |
| `picker_pre_existing_exclusive_crates_shown` | Project Cargo.toml already has both `jemalloc` and `mimalloc` deps; picker opens with both radio items checked |
| `picker_deselection_removes_from_cargo_toml` | Project has `jemalloc` + `mimalloc`; user selects `jemalloc` (deselecting `mimalloc`); after confirm, `mimalloc` is removed from Cargo.toml |
| `picker_all_features_bypasses_exclusive` | `--all-features` adds all exclusive features without error |

### 4.2 Implement

**File**: `src/battery-pack/bphelper-cli/src/commands.rs`

The existing `all_crates_with_grouping()` (lib.rs:586) already groups crates
by feature. Extend it (or wrap it) to also return category info from
`spec.feature_meta`. This avoids rebuilding grouping from scratch.

Refactor `pick_crates_interactive()` (~line 1414):

1. Group features by `feature_meta[name].category`.
2. For each category (in definition order):
   - Create `Section::new(title, items)` with `selection_mode` from `pick`.
   - Nested categories become collapsed sub-sections.
3. Uncategorized features go into a "Features:" section (unchanged).
4. Templates with categories go into their category section.
5. Dependencies section remains at bottom.

**Deselection / removal path:**

The picker result currently only records positive selections via
`PickerResult` (~line 1354). To support removal:

1. Before showing the picker, record which crates from `at-most-one`
   categories are already in the project's `Cargo.toml` (the "before" set).
2. After picker confirm, compute the "after" set (selected items).
3. Diff: `removed = before - after`.
4. For removed crates, call the existing `remove_deps_by_kind()` from
   manifest.rs (currently only used by `cargo bp rm`). Factor it out if needed
   so both `rm` and `add` can use it.

---

## Phase 5: CLI Validation for Non-Interactive `-F` Flags

**Goal**: Reject conflicting exclusive selections on the command line.

### 5.1 Write tests

**File**: `src/battery-pack/bphelper-cli/src/commands/tests.rs`

| Test | Input | Expected |
|------|-------|----------|
| `non_interactive_rejects_exclusive_conflict` | `-F stm32f4 -F nrf52840` (both in `hal`, at-most-one) | Error naming both features and the category |
| `non_interactive_allows_same_any_category` | `-F http-trace -F http-timeout` (both in `http-layers`, any) | OK |
| `non_interactive_allows_different_categories` | `-F stm32f4 -F embassy` | OK |
| `non_interactive_all_features_skips_validation` | `--all-features` | Bypasses checks |
| `non_interactive_template_exclusive_conflict` | `-t trusted-publishing -t binary-release` (at-most-one) | Error |

### 5.2 Implement

**File**: `src/battery-pack/bphelper-cli/src/commands.rs`

```rust
fn validate_exclusive_constraints(
    spec: &BatteryPackSpec,
    features: &[String],
    templates: &[String],
) -> Result<()> {
    // Group requested features by at-most-one category.
    // If any category has >1, bail with descriptive error.
    // Same for templates.
}
```

Call from the non-interactive path in `resolve_add_crates()` (~line 512) —
NOT `resolve_add_args()` which does not exist.

---

## Phase 6: Category-Linked Template Placeholders

**Goal**: `type = "category"` placeholder derives options from a category
definition — single source of truth.

### 6.1 Write tests

**File**: `src/battery-pack/bphelper-cli/src/template_engine/tests.rs`

| Test | Scenario |
|------|----------|
| `category_placeholder_derives_options` | `type = "category"`, `category = "allocator"` → options list equals features in that category |
| `category_placeholder_uses_default` | Non-interactive: uses `default` value |
| `category_placeholder_prefilled_from_picker` | User selected `jemalloc` in picker → placeholder auto-filled, not prompted |
| `category_placeholder_error_on_unknown_category` | `category = "nonexistent"` → clear error |
| `category_placeholder_renders_as_select` | Interactive UX identical to `select` type |
| `category_placeholder_no_options_field_needed` | `options` key absent in bp-template.toml → options derived from category |
| `category_placeholder_options_update_when_feature_added` | Add a feature to category → placeholder options include it without template edit |

### 6.2 Implement

**File**: `src/battery-pack/bphelper-cli/src/template_engine.rs`

Extend `PlaceholderType`:
```rust
enum PlaceholderType {
    String,
    Bool,
    Select,
    Category,  // NEW
}
```

Extend `PlaceholderDef`:
```rust
category: Option<String>,  // for type = "category"
```

Implementation approach — **pre-compute options before calling
`resolve_placeholders()`**:

Rather than threading `&BatteryPackSpec` into the template engine (which would
couple it to `bphelper-manifest`), the CLI layer pre-computes the options list
from the spec and injects it into the `PlaceholderDef` before resolution:

```rust
// In commands.rs, before calling template generation:
for placeholder in &mut template_defs {
    if placeholder.type_ == PlaceholderType::Category {
        let cat_name = placeholder.category.as_ref().expect("validated");
        placeholder.options = spec.features_in_category(cat_name);
        // Pre-fill from picker selection if available:
        if let Some(selected) = active_features.iter().find(|f| {
            spec.feature_meta.get(*f).and_then(|m| m.category.as_deref()) == Some(cat_name)
        }) {
            placeholder.resolved_value = Some(selected.clone());
        }
    }
}
```

Then `resolve_placeholders()` treats `Category` identically to `Select`
(options are already populated). No changes to `resolve_placeholders()`
signature needed.

Thread `active_features: BTreeSet<String>` from picker result through
`GenerateOpts` to the pre-computation site.

---

## Phase 7: `cargo bp validate` and `cargo bp show` Updates

**Goal**: Surface categories in validate diagnostics and show output.

### 7.1 Write tests

**File**: tests using fixtures

| Test | Scenario |
|------|----------|
| `validate_category_fixture_clean` | Fixture `category-battery-pack` validates without errors |
| `validate_exclusive_default_fails` | Fixture with two exclusive features in default → error |
| `show_renders_categories` | Show output includes "Categories:" with hierarchy |
| `show_renders_template_categories` | Template grouping in show |
| `show_json_includes_categories` | JSON output has category data |

### 7.2 Implement

Validation is already wired from Phase 2 (checks are in `validate_spec()`).
Just ensure the error messages surface clearly through `cargo bp validate`.

**File**: `src/battery-pack/bphelper-cli/src/commands.rs` (`build_show_report()`)

Add category rendering to show output — categories listed with their features,
grouped hierarchically.

**File**: `src/cargo-bp-script/src/show.rs`

Add `CategoryInfo` to the JSON schema. This is a translation type — it
mirrors `CategorySpec`/`PickMode` from `bphelper-manifest` in
`cargo-bp-script`'s own serializable format. The conversion happens in
`build_show_report()`.

---

## Phase 8: Migration of Existing Packs

**Goal**: Add category metadata to existing battery packs.

### 8.1 Test fixture

Create `tests/fixtures/category-battery-pack/` with a minimal pack exercising
categories, nested categories, exclusive features, and template categories.
Add workspace membership in `tests/fixtures/Cargo.toml`.

### 8.2 Migrate `backend-service-battery-pack`

- Add `allocator` category (`at-most-one`) with `jemalloc` and `mimalloc-alloc`
- Add `http-layers` category (`any`) with the tower-http features
- Update `templates/service/bp-template.toml`: change allocator placeholder
  from `type = "select"` with hardcoded options to `type = "category"` with
  `category = "allocator"`

### 8.3 Migrate `ci-battery-pack`

- Add `release` category (`at-most-one`) for `trusted-publishing` / `binary-release` templates
- Add `quality` category (`any`) for fuzzing, mutation-testing, spellcheck, clippy-sarif, security-scanning

### 8.4 Migrate `cli-battery-pack`

- Add `output` category (`any`) for indicators
- Add `search` category (`any`) for search feature
- Add `input` category (`any`) for config feature

---

## Known implementation challenges

These are non-obvious issues identified during code review. Each is addressed
in the relevant phase above, but collected here for visibility:

1. **Two-struct metadata split** (Phase 1.3): Categories and feature metadata
   live under `[package.metadata.battery-pack]` (`RawBatteryPackMetadata`),
   but template `category` lives under `[package.metadata.battery.templates]`
   (`RawTemplateSpec`). Both structs need changes.

2. **`Section` struct breaking change** (Phase 3.2): Adding fields breaks
   struct-literal callers. Mitigated with `Section::new()` constructor +
   field setters.

3. **`PickerState` discards section metadata** (Phase 3.2): After
   construction, the state has no reference to `SelectionMode`. Fixed by
   storing `section_modes: Vec<SelectionMode>` during `new()`.

4. **`selectable` vec is static** (Phase 3.2): Collapse/expand needs dynamic
   visibility. Fixed with a `visible: Vec<bool>` that `move_up/move_down`
   consults, avoiding index invalidation.

5. **No removal path in the add flow** (Phase 4.2): `PickerResult` only
   records positive selections. Deselection requires diffing before/after
   state and calling `remove_deps_by_kind()`.

6. **Template engine doesn't have access to spec** (Phase 6.2): Solved by
   pre-computing category options in the CLI layer and injecting them into
   `PlaceholderDef.options` before calling `resolve_placeholders()`.

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
