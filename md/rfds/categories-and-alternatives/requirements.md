# Requirements: Categories and Exclusive Picks

Testable requirements for the [Categories and Exclusive Picks RFD](./README.md).
Each requirement has a unique ID (`r[...]`) and maps to one or more tests.

---

## Parsing

r[parse.category-definition]
A `[package.metadata.battery-pack.categories.<name>]` table with `title`,
`description`, and `pick` fields is parsed into a `CategorySpec`.

- **Test**: Parse a manifest with `categories.hal` containing all three fields.
  Assert `spec.categories["hal"].pick == AtMostOne`, title and description match.

r[parse.category-pick-default]
A category with no `pick` field defaults to `PickMode::Any`.

- **Test**: Parse a manifest with `categories.portable` containing only `title`.
  Assert `spec.categories["portable"].pick == Any`.

r[parse.feature-metadata]
`[package.metadata.battery-pack.features.<name>]` with `description` and
`categories` fields is parsed into an `ItemMeta` stored in `feature_meta`.

- **Test**: Parse manifest with `features.stm32f4` containing
  `categories = ["hal"]` and `description = "STM32F4xx"`.
  Assert correct values in `spec.feature_meta["stm32f4"]`.

r[parse.dependency-metadata]
`[package.metadata.battery-pack.dependencies.<name>]` with `description` and
`categories` fields is parsed into an `ItemMeta` stored in `dep_meta`.

- **Test**: Parse manifest with `dependencies.embedded-hal` containing
  `categories = ["portable"]`. Assert correct values in `spec.dep_meta`.

r[parse.template-categories]
A template entry in `[package.metadata.battery.templates]` with a `categories`
field stores the category list on `TemplateSpec`.

- **Test**: Parse manifest with template `fuzzing` having
  `categories = ["quality"]`. Assert `spec.templates["fuzzing"].categories`.

r[parse.multiple-categories]
An item's `categories` field is a list; an item can belong to multiple
categories.

- **Test**: Parse `categories = ["quality", "ci"]`. Assert both stored.

r[parse.no-metadata-backward-compat]
Items with no metadata entry parse identically to today — no new fields
affect existing behavior.

- **Test**: Parse a manifest with zero `[package.metadata.battery-pack.features.*]`
  entries. Assert `feature_meta` is empty and all other spec fields unchanged.

r[parse.options-category-in-template]
A template placeholder with `options.category = "allocator"` is parsed as a
category-derived options source (untagged enum: literal array vs category ref).

- **Test**: Parse a `bp-template.toml` with `options.category = "allocator"`.
  Assert the placeholder's options source is `Category("allocator")`.

r[parse.options-literal-array]
A template placeholder with `options = ["a", "b", "c"]` continues to work.

- **Test**: Parse `options = ["jemalloc", "mimalloc", "system"]`.
  Assert the placeholder's options source is `Literal(["jemalloc", ...])`.

---

## Validation

r[validate.categories-defined]
Every category name referenced in an item's `categories` list must have a
corresponding `[package.metadata.battery-pack.categories.<name>]` entry.

- **Test (feature)**: Feature with `categories = ["nonexistent"]` →
  error `format.categories.defined`.
- **Test (dependency)**: Dep with `categories = ["bogus"]` →
  error `format.categories.defined`.
- **Test (template)**: Template with `categories = ["bogus"]` →
  error `format.categories.defined`.
- **Test (partial)**: `categories = ["hal", "bogus"]` where `hal` exists →
  error only for `"bogus"`.

r[validate.exclusive-conflict-in-default]
If two or more features belonging to the same `at-most-one` category are both
listed in the `[features] default` array, emit an error.

- **Test (error)**: Two features in `at-most-one` category, both in default →
  error `format.features.exclusive-conflict`.
- **Test (ok, any)**: Two features in `any` category, both in default → no error.
- **Test (ok, one in default)**: Two exclusive features, only one in default →
  no error.

r[validate.empty-category-warns]
A declared category that no item references produces a warning.

- **Test**: Category `foo` declared, no feature/dep/template lists it →
  warning `format.categories.empty`.

r[validate.at-most-one-missing-title]
A category with `pick = "at-most-one"` and no `title` produces a warning.

- **Test**: Category with `pick = "at-most-one"`, no title → warning
  `format.categories.pick-missing-title`.

r[validate.feature-metadata-unknown]
`[package.metadata.battery-pack.features.X]` where `X` does not appear as a
key in `[features]` is an error.

- **Test**: Metadata for `features.foo` where `foo` not in `[features]` →
  error `format.features.unknown-feature`.

r[validate.dep-metadata-unknown]
`[package.metadata.battery-pack.dependencies.X]` where `X` does not appear
in any dependency section is an error.

- **Test**: Metadata for `dependencies.foo` where `foo` is not a dependency →
  error `format.dependencies.unknown-dep`.

r[validate.template-category-placeholder]
A template placeholder using `options.category = "X"` where `X` is not a
declared category is an error.

- **Test**: `options.category = "nonexistent"` →
  error `format.template.category-placeholder-mismatch`.

---

## Picker: selection behavior

r[picker.radio-toggle-deselects-others]
In a Radio section, toggling an unchecked item checks it and unchecks all
other items in that section.

- **Unit test**: Section with items A(checked), B, C. Toggle B →
  A unchecked, B checked, C unchanged.

r[picker.radio-toggle-allows-deselect]
In a Radio section, toggling an already-checked item unchecks it (allows zero
selections).

- **Unit test**: Section with A(checked). Toggle A → nothing checked.

r[picker.radio-backspace-clears]
In a Radio section, pressing Backspace clears all selections in the current
category.

- **Unit test**: Section with A(checked). Backspace → nothing checked.

r[picker.checkbox-toggle-independent]
In a Checkbox section, toggling an item does not affect other items.

- **Unit test**: Section with A(checked), B. Toggle B →
  A still checked, B checked.

r[picker.radio-section-toggle-noop]
`toggle_current_section()` (`a` key) in a Radio section is a no-op.

- **Unit test**: Radio section with A(checked). Press `a` →
  A still checked, no change.

r[picker.checkbox-section-toggle-selects-all]
`toggle_current_section()` (`a` key) in a Checkbox section checks all items
(existing behavior preserved).

- **Unit test**: Checkbox section with A(checked), B, C. Section toggle →
  all checked.

r[picker.radio-pre-existing-multiple]
If a Radio section is initialized with multiple items checked, the state is
preserved honestly (no auto-deselection on load).

- **Unit test**: Construct Radio section with A(checked), B(checked).
  Assert `into_results()` shows both checked.

r[picker.radio-pre-existing-toggle-clears-all-others]
In a Radio section with multiple items pre-checked, toggling a new item
clears all others.

- **Unit test**: Radio section with A(checked), B(checked). Toggle C →
  only C checked.

r[picker.confirm-blocked-on-radio-conflict]
`try_confirm()` returns an error when a Radio section has more than one
item checked.

- **Unit test**: Radio section with A(checked), B(checked). Call
  `try_confirm()` → `Err("...")` containing the section title.

r[picker.confirm-succeeds-on-valid-state]
`try_confirm()` succeeds when all Radio sections have 0 or 1 selection.

- **Unit test**: Radio section with A(checked). Call `try_confirm()` →
  `Ok(results)`.

---

## Picker: navigation and collapsing

r[picker.collapse-hides-from-navigation]
Collapsing a section causes `move_down`/`move_up` to skip its items.

- **Unit test**: Two sections. Collapse first. Cursor at top, move down →
  lands in second section.

r[picker.expand-restores-navigation]
Expanding a collapsed section restores normal traversal through its items.

- **Unit test**: Collapse then expand first section. move_down from header →
  lands on first item.

r[picker.collapsed-results-preserved]
Collapsed sections' checked state is included in `into_results()`.

- **Unit test**: Check item, collapse section, call `into_results()` →
  item shows as checked.

r[picker.left-arrow-collapses]
Pressing Left on a section header collapses that section.

- **Integration test** (key event simulation): Send Left on header →
  section becomes collapsed.

r[picker.right-arrow-expands]
Pressing Right on a section header expands that section.

- **Integration test**: Collapsed section, send Right → section expands.

---

## Picker: rendering

r[picker.render-radio-bullets]
Radio items render with `●` (checked) and `○` (unchecked) instead of
`[x]`/`[ ]`.

- **Unit test** (render snapshot): Radio section with one checked item →
  output contains `●` and `○`.

r[picker.render-checkbox-squares]
Checkbox items continue to render with `[x]`/`[ ]`.

- **Unit test** (render snapshot): Checkbox section → output contains
  `[x]` and `[ ]`.

r[picker.render-collapsed-chevron]
Collapsed section headers render with `▶`; expanded with `▼`.

- **Unit test** (render snapshot): Collapsed section → `▶` in output.

r[picker.render-at-most-one-hint]
Radio section headers include the text "(pick at most one)".

- **Unit test** (render snapshot): Radio section header →
  output contains `(pick at most one)`.

r[picker.render-warning-banner]
When a Radio section has >1 item checked on initial render, a warning line
is displayed.

- **Unit test** (render snapshot): Radio section with 2 checked →
  output contains `⚠` warning text.

r[picker.render-descriptions]
When items have descriptions, they are shown alongside the item name.

- **Unit test** (render snapshot): Item with description →
  output contains the description text.

---

## CLI: interactive picker wiring

r[cli.picker-categories-become-sections]
When a battery pack has category definitions, the picker groups items by
category — one section per category, with the category title as section header.

- **Integration test**: Pack with `hal` (at-most-one) and `utils` (any) →
  picker has sections titled "Hardware Abstraction Layer" and "Utilities".

r[cli.picker-radio-for-at-most-one]
Sections for `at-most-one` categories use `SelectionMode::Radio`.

- **Integration test**: Pack with at-most-one category → section has Radio mode.

r[cli.picker-checkbox-for-any]
Sections for `any` categories use `SelectionMode::Checkbox`.

- **Integration test**: Pack with `any` category → section has Checkbox mode.

r[cli.picker-uncategorized-in-generic]
Items not in any category appear in generic "Features" / "Dependencies"
sections, exactly as today.

- **Integration test**: Pack with some categorized and some uncategorized
  features → uncategorized appear in "Features:" section.

r[cli.picker-item-in-multiple-categories]
An item belonging to multiple categories appears in each category's section.
Selection state is shared: toggling the item in one section updates its state
in all sections where it appears.

- **Integration test**: Feature with `categories = ["quality", "ci"]` →
  appears in both sections.
- **Integration test**: Toggle item in "quality" section → also shown as
  checked in "ci" section.

r[cli.picker-category-item-order]
Items within a category section appear in declaration order (the order their
metadata entries appear in `Cargo.toml`).

- **Integration test**: Features `stm32f4`, `nrf52840`, `esp32` declared in
  that order, all in category `hal` → picker shows them in that order.

r[cli.picker-deselection-removes-dep]
When a user deselects an item in an at-most-one category (by selecting
another), the deselected crate is removed from the project's `Cargo.toml`
on confirm.

- **Integration test**: Project has both `jemalloc` and `mimalloc` in
  Cargo.toml. User selects `jemalloc` in picker. After confirm, `mimalloc`
  is removed from Cargo.toml.

r[cli.picker-pre-existing-conflict-shown]
If the project already has multiple items from an `at-most-one` category
installed, the picker opens with all of them checked and shows a warning.

- **Integration test**: Project has `jemalloc` + `mimalloc`. Open picker →
  both radio items shown as checked, warning banner visible.

---

## CLI: non-interactive validation

r[cli.noninteractive-exclusive-conflict-error]
When `-F` passes two features from the same `at-most-one` category,
`cargo bp add` exits with an error naming both features and the category.

- **Integration test**: `cargo bp add pack -F stm32f4 -F nrf52840` →
  exit code 1, stderr contains "exclusive" and "hal".

r[cli.noninteractive-any-category-ok]
Two features from the same `any` category can be passed together without error.

- **Integration test**: `cargo bp add pack -F http-trace -F http-timeout` →
  success.

r[cli.noninteractive-different-categories-ok]
Features from different categories can always be combined.

- **Integration test**: `cargo bp add pack -F stm32f4 -F embassy` → success.

r[cli.noninteractive-all-features-bypasses]
`--all-features` bypasses exclusive constraint checking.

- **Integration test**: `cargo bp add pack --all-features` with multiple
  exclusive features → success.

r[cli.noninteractive-template-exclusive-error]
When `-t` passes two templates from the same `at-most-one` category,
`cargo bp add` exits with an error.

- **Integration test**: `-t X -t Y` where both in at-most-one category →
  exit code 1.

---

## CLI: `cargo bp show`

r[cli.show-categories]
`cargo bp show` displays categories with their member items grouped under
the category title.

- **Integration test** (snapshot): `cargo bp show` on a pack with categories →
  output contains "Categories:" section with expected structure.

r[cli.show-pick-mode-hint]
At-most-one categories display "(pick at most one)" in show output.

- **Integration test** (snapshot): Show output for at-most-one category →
  contains hint text.

r[cli.show-templates-in-categories]
Templates assigned to categories appear under their category heading in
`cargo bp show` output.

- **Integration test** (snapshot): Pack with template in `quality` category →
  show output lists template under "Code Quality" heading.

---

## CLI: `cargo bp validate`

r[cli.validate-clean-pack]
A battery pack with correct category metadata passes validation.

- **Integration test**: Run `cargo bp validate` on fixture with valid
  categories → exit code 0.

r[cli.validate-reports-errors]
A battery pack with invalid category references fails validation with the
appropriate rule ID in the output.

- **Integration test**: Run `cargo bp validate` on fixture with
  `categories = ["nonexistent"]` → exit code 1, output contains
  `format.categories.defined`.

---

## Template: category-linked placeholders

r[template.options-category-derives-list]
A placeholder with `options.category = "allocator"` has its options list
derived from the set of items in that category.

- **Unit test**: Spec with category `allocator` containing features
  `jemalloc` and `mimalloc`. Resolve placeholder → options are
  `["jemalloc", "mimalloc"]`.

r[template.options-category-prefill-from-picker]
If the user already selected an item from the category in the picker, the
placeholder is pre-filled without prompting.

- **Unit test**: Active features include `jemalloc` (in `allocator` category).
  Resolve placeholder → value is `"jemalloc"`, no prompt.

r[template.options-category-prompts-if-no-selection]
If no item from the category was selected in the picker, the placeholder
prompts the user (interactive) or uses the default (non-interactive).

- **Unit test (non-interactive)**: No active feature in category, default is
  `"jemalloc"` → value resolves to `"jemalloc"`.

r[template.options-category-unknown-error]
A placeholder referencing an undefined category produces a clear error.

- **Unit test**: `options.category = "nonexistent"` → error message.

r[template.options-category-picks-up-new-members]
Adding a new feature to a category automatically includes it in the
placeholder's options without editing `bp-template.toml`.

- **Unit test**: Add feature `system` to `allocator` category → options list
  now includes `"system"`.

r[template.options-category-dep-uses-dep-name]
When a category contains dependencies (not features), the option value is the
dependency name (the key from `[dependencies]`).

- **Unit test**: Category `allocator` contains dep `tikv-jemallocator`.
  Placeholder options include `"tikv-jemallocator"`.

---

## Invariants

r[invariant.pack-scoped-categories]
Categories are scoped to the battery pack that defines them. Two different
installed packs can both define a category with the same name without
conflict — their constraints are enforced independently.

- **Integration test**: Install two packs, both defining category `hal`
  (at-most-one). Select one item from each pack's `hal` category → no error.

r[invariant.battery-pack-toml-unchanged]
The `battery-pack.toml` file format (which records installed packs and active
features in user projects) is unchanged by this feature. Category metadata
does not appear in `battery-pack.toml`.

- **Integration test**: Run `cargo bp add pack -F stm32f4` where `stm32f4`
  is in a category. Inspect `battery-pack.toml` → format matches existing
  schema, no category fields present.

r[invariant.noninteractive-dep-exclusive-conflict]
When a dependency (not wrapped in a feature) belongs to an `at-most-one`
category and the user requests multiple such deps non-interactively, it is
an error.

- **Integration test**: Pack has deps `tikv-jemallocator` and `mimalloc` both
  in `at-most-one` category `allocator`. Non-interactive add of both → error.
