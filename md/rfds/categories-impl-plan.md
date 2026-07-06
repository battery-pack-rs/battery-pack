# Implementation Plan: Categories and Exclusive Picks

TDD-driven implementation plan for the [Categories and Exclusive Picks RFD](./categories-and-alternatives.md).

## Phase dependency graph

```
Phase 1 (parsing) ──→ Phase 2 (validation) ──→ Phase 7 (validate/show)
     │                       │
     ├───────────────────────├──→ Phase 5 (CLI -F validation)
     │                       │
     └──→ Phase 3 (picker) ─├──→ Phase 4 (CLI picker wiring)
                             │
                             └──→ Phase 6 (template category placeholders)
                                       │
                                       └──→ Phase 8 (migration)
```

Phases 1–2 are foundational. Phase 3 is independent from 2 but depends on 1's
types. Phases 4–6 can proceed in parallel once their prerequisites land. Phase
7 depends on 2. Phase 8 is last (depends on everything).

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
    pub title: String,
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

### 1.3 Implement parsing

**File**: `src/battery-pack/bphelper-manifest/src/lib.rs`

Extend `RawBatteryPackMetadata` deserialization to include `categories` and
`features` maps. Extend `RawTemplateSpec` with `category`. Populate new fields
in `package_to_spec()`.

---

## Phase 2: Validation Rules

**Goal**: Enforce `format.features.exclusive-conflict` and
`format.categories.defined`.

### 2.1 Write tests

**File**: `src/battery-pack/bphelper-manifest/src/lib.rs` (append to `mod tests`)

| Test | Input | Expected |
|------|-------|----------|
| `validate_exclusive_conflict_in_default` | Two features in an `at-most-one` category both in `default` | Error `format.features.exclusive-conflict` |
| `validate_exclusive_conflict_not_triggered_for_any` | Two features in an `any` category both in default | No error |
| `validate_category_reference_exists` | `category = "nonexistent"` | Error `format.categories.defined` |
| `validate_nested_category_resolves_to_ancestor` | `category = "hal.stm32"` when `categories.hal` exists | No error |
| `validate_clean_when_exclusive_not_in_default` | Two exclusive features, only one in default | No error |
| `validate_template_category_reference_exists` | Template `category = "bogus"` | Error `format.categories.defined` |
| `validate_empty_category_warns` | Category declared but no feature or template references it | Warning `format.categories.empty` |
| `validate_at_most_one_missing_title_warns` | `pick = "at-most-one"` with no `title` field | Warning `format.categories.pick-missing-title` |
| `validate_feature_metadata_for_unknown_feature` | `[features.X]` metadata where `X` not in `[features]` | Error `format.features.unknown-feature` |

### 2.2 Implement

**File**: `src/battery-pack/bphelper-manifest/src/lib.rs`

Add `validate_categories()` to `validate_spec()`:

- **r[format.categories.defined]**: For each feature's `category` and each
  template's `category`, check that a matching `categories.<name>` entry
  exists (or an ancestor via dot notation).
- **r[format.features.exclusive-conflict]**: Collect features in each
  `at-most-one` category that appear in `default`. If any category has >1,
  emit error.

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
behavior.

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
| `radio_confirm_blocked_when_multiple_selected` | Enter pressed with >1 radio selection → returns error/blocked state, does not confirm |

**File**: `src/sectioned-picker/src/tests/render.rs`

| Test | Scenario |
|------|----------|
| `render_radio_items_use_bullet_symbols` | `●`/`○` instead of `[x]`/`[ ]` |
| `render_collapsed_section_shows_chevron` | `▶` prefix on collapsed header |
| `render_radio_multiple_selected_shows_all_filled` | When 2 items are pre-checked in radio mode, both render as `●` |

### 3.2 Implement

**File**: `src/sectioned-picker/src/lib.rs`

Add to `Section`:
```rust
pub selection_mode: SelectionMode,  // default: Checkbox
pub collapsed: bool,                // default: false
```

```rust
#[derive(Debug, Clone, Copy, PartialEq, Eq, Default)]
pub enum SelectionMode {
    #[default]
    Checkbox,
    Radio,
}
```

**File**: `src/sectioned-picker/src/state.rs`

- `toggle()`: check `SelectionMode` — radio deselects siblings first.
- `toggle_current_section()`: in radio mode, deselects all.
- `move_up()`/`move_down()`: skip items in collapsed sections.
- New: `toggle_collapse()` — flips `collapsed` on current section header.

**File**: `src/sectioned-picker/src/lib.rs` (event loop)

- `KeyCode::Left` → collapse current section
- `KeyCode::Right` → expand current section

**File**: `src/sectioned-picker/src/render.rs`

- Radio items: `●`/`○`
- Collapsed header: `▶` prefix, expanded: `▼` prefix
- Radio section header appends `(pick at most one)`

---

## Phase 4: Wire Categories into CLI Picker

**Goal**: `cargo bp add` builds picker sections from category metadata.

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

### 4.2 Implement

**File**: `src/battery-pack/bphelper-cli/src/commands.rs`

Refactor `pick_crates_interactive()`:

1. Group features by `feature_meta[name].category`.
2. For each category (in definition order):
   - Create `Section` with `title` from category, `selection_mode` from `pick`.
   - Nested categories become sub-sections.
3. Uncategorized features go into a "Features:" section (unchanged).
4. Templates with categories go into their category section.
5. Dependencies section remains at bottom.

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

Call from the non-interactive path in `resolve_add_args()`.

---

## Phase 6: Category-Linked Template Placeholders

**Goal**: `type = "category"` placeholder derives options from a category.

### 6.1 Write tests

**File**: `src/battery-pack/bphelper-cli/src/template_engine/tests.rs`

| Test | Scenario |
|------|----------|
| `category_placeholder_derives_options` | `type = "category"`, `category = "allocator"` → options from features in that category |
| `category_placeholder_uses_default` | Non-interactive: uses `default` value |
| `category_placeholder_prefilled_from_picker` | User selected `jemalloc` in picker → placeholder auto-filled |
| `category_placeholder_error_on_unknown_category` | `category = "nonexistent"` → clear error |
| `category_placeholder_renders_as_select` | Interactive UX identical to `select` type |

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

In `resolve_placeholders()` for `Category`:
1. Look up category in spec.
2. Collect feature names assigned to it → use as options.
3. If `active_features` (from picker) already includes one, pre-fill.
4. Otherwise prompt as `Select`.

Thread `active_features: BTreeSet<String>` from picker result through
`GenerateOpts` to template generation.

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

Validation is mostly wired from Phase 2. Ensure error messages are clear
and rule IDs match the spec.

**File**: `src/battery-pack/bphelper-cli/src/commands.rs` (`build_show_report()`)

Add category rendering to show output — categories listed with their features,
grouped hierarchically.

**File**: `src/cargo-bp-script/src/show.rs`

Add `CategoryInfo` to the JSON schema.

---

## Phase 8: Migration of Existing Packs

**Goal**: Add category metadata to existing battery packs.

### 8.1 Test fixture

Create `tests/fixtures/category-battery-pack/` with a minimal pack exercising
categories, nested categories, exclusive features, and template categories.

### 8.2 Migrate `backend-service-battery-pack`

- Add `allocator` category (`at-most-one`) with `jemalloc` and `mimalloc-alloc`
- Add `http-layers` category (`any`) with the tower-http features
- Update `templates/service/bp-template.toml` to use `type = "category"` for allocator

### 8.3 Migrate `ci-battery-pack`

- Add `release` category (`at-most-one`) for `trusted-publishing` / `binary-release` templates
- Add `quality` category (`any`) for fuzzing, mutation-testing, spellcheck, clippy-sarif, security-scanning

### 8.4 Migrate `cli-battery-pack`

- Add `output` category (`any`) for indicators
- Add `search` category (`any`) for search feature
- Add `input` category (`any`) for config feature

---

## Critical source files

| File | Role |
|------|------|
| `src/battery-pack/bphelper-manifest/src/lib.rs` | Metadata parsing and validation |
| `src/sectioned-picker/src/state.rs` | Picker state machine |
| `src/sectioned-picker/src/render.rs` | Picker rendering |
| `src/sectioned-picker/src/lib.rs` | Picker event loop |
| `src/battery-pack/bphelper-cli/src/commands.rs` | CLI command logic |
| `src/battery-pack/bphelper-cli/src/template_engine.rs` | Template placeholder resolution |
| `src/battery-pack/bphelper-cli/src/validate.rs` | Validate subcommand |
| `src/cargo-bp-script/src/show.rs` | Show output schema |
