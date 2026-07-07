//! Battery pack manifest parsing and resolution.
//!
//! Parses battery pack Cargo.toml files to extract curated crates,
//! features, hidden dependencies, and templates. Provides resolution
//! logic to determine which crates to install based on active features.
#[cfg(test)]
mod test_support;

mod feature_ref;
pub use feature_ref::{FeatureParseError, FeatureRef};

use cargo_metadata::{DependencyKind, Metadata, MetadataCommand, Package};
use serde::{Deserialize, Serialize};
use std::collections::btree_map::Entry;
use std::collections::{BTreeMap, BTreeSet};
use std::path::Path;

// ============================================================================
// Error type
// ============================================================================

/// Errors that can occur when parsing or discovering battery packs.
#[derive(Debug, thiserror::Error)]
pub enum Error {
    #[error("TOML parse error: {0}")]
    Toml(#[from] toml::de::Error),

    #[error("missing {0}")]
    MissingField(&'static str),

    #[error("invalid battery pack name '{name}': must end in '-battery-pack'")]
    InvalidName { name: String },

    #[error("feature '{feature}' references unknown crate '{crate_name}'")]
    UnknownCrateInFeature { feature: String, crate_name: String },

    #[error("feature `{feature}` has invalid reference `{raw}`: {source}")]
    FeatureRefParse {
        feature: String,
        raw: String,
        #[source]
        source: FeatureParseError,
    },

    #[error("cycle in local feature references: {path}")]
    FeatureCycle { path: String },

    #[error("reading {path}: {source}")]
    Io {
        path: String,
        #[source]
        source: std::io::Error,
    },

    #[error("cargo metadata failed: {0}")]
    Metadata(#[from] cargo_metadata::Error),

    #[error("decoding [package.metadata] for `{package}`: {source}")]
    MetadataDecode {
        package: String,
        #[source]
        source: serde_json::Error,
    },
}

// ============================================================================
// Validation diagnostics
// ============================================================================

/// Severity level for a validation diagnostic.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Severity {
    /// Violation of a MUST rule in the spec.
    Error,
    /// Violation of a SHOULD rule in the spec.
    Warning,
}

/// A single validation finding, tied to a spec rule.
#[derive(Debug, Clone)]
pub struct Diagnostic {
    pub severity: Severity,
    /// Spec rule ID (e.g., `"format.crate.keyword"`).
    pub rule: &'static str,
    pub message: String,
}

/// Collected validation results from checking a battery pack.
#[derive(Debug, Default)]
pub struct ValidationReport {
    pub diagnostics: Vec<Diagnostic>,
}

impl ValidationReport {
    /// True if any diagnostic is an error.
    pub fn has_errors(&self) -> bool {
        self.diagnostics
            .iter()
            .any(|d| d.severity == Severity::Error)
    }

    /// True if there are no diagnostics at all.
    pub fn is_clean(&self) -> bool {
        self.diagnostics.is_empty()
    }

    /// Merge another report into this one.
    pub fn merge(&mut self, other: ValidationReport) {
        self.diagnostics.extend(other.diagnostics);
    }

    fn error(&mut self, rule: &'static str, message: impl Into<String>) {
        self.diagnostics.push(Diagnostic {
            severity: Severity::Error,
            rule,
            message: message.into(),
        });
    }

    fn warning(&mut self, rule: &'static str, message: impl Into<String>) {
        self.diagnostics.push(Diagnostic {
            severity: Severity::Warning,
            rule,
            message: message.into(),
        });
    }
}

// ============================================================================
// Battery pack types
// ============================================================================

/// The dependency kind, determined by which section of the battery pack's
/// Cargo.toml the crate appears in.
// [impl format.deps.kind-mapping]
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash, PartialOrd, Ord, Serialize, Deserialize)]
pub enum DepKind {
    /// `[dependencies]` — becomes a regular dependency for the user.
    Normal,
    /// `[dev-dependencies]` — becomes a dev-dependency for the user.
    Dev,
    /// `[build-dependencies]` — becomes a build-dependency for the user.
    Build,
}

impl std::fmt::Display for DepKind {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        match self {
            DepKind::Normal => write!(f, "dependencies"),
            DepKind::Dev => write!(f, "dev-dependencies"),
            DepKind::Build => write!(f, "build-dependencies"),
        }
    }
}

/// A curated crate within a battery pack.
// [impl format.deps.version-features]
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct CrateSpec {
    /// Recommended version.
    pub version: String,
    /// Recommended Cargo features.
    pub features: BTreeSet<String>,
    /// Which dependency section this crate comes from.
    pub dep_kind: DepKind,
    /// Whether this crate is marked `optional = true`.
    // [impl format.features.optional]
    pub optional: bool,
}

/// How many items a user may pick from a category.
// [impl format.categories.pick]
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize, Default)]
#[serde(rename_all = "kebab-case")]
pub enum PickMode {
    /// Any number of items may be selected (checkboxes).
    #[default]
    Any,
    /// At most one item may be selected (radio buttons).
    AtMostOne,
}

/// A category grouping related selectable items in the picker.
// [impl format.categories.definition]
#[derive(Debug, Clone, Serialize, Deserialize, Default)]
pub struct CategorySpec {
    /// Display name in the picker header.
    pub title: Option<String>,
    /// Explanatory text shown under the header.
    pub description: Option<String>,
    /// Selection constraint for this category.
    #[serde(default)]
    pub pick: PickMode,
}

/// Per-item metadata (features and dependencies share the same shape).
// [impl format.features.metadata]
// [impl format.deps.metadata]
#[derive(Debug, Clone, Serialize, Deserialize, Default)]
pub struct ItemMeta {
    /// Category names this item belongs to.
    #[serde(default)]
    pub categories: Vec<String>,
    /// Description shown next to the item in the picker.
    pub description: Option<String>,
}

/// Template metadata for project scaffolding.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct TemplateSpec {
    pub path: String,
    pub description: Option<String>,
    /// Category names this template belongs to.
    #[serde(default)]
    pub categories: Vec<String>,
}

/// Active feature selection at the resolver boundary.
///
/// Note: Cargo permits `all` as a feature name (one exists in the `mixed-kind-battery-pack` oracle fixture), so a persisted [`BTreeSet<String>`] containing that literal is ambiguous.
/// The [`From`] conversion always favors [`ActiveFeatures::All`]; a future tagged on-disk format would remove the ambiguity.
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum ActiveFeatures {
    /// Every declared feature is active
    All,
    /// An explicit subset (may include `"default"` to expand the default set).
    Subset(BTreeSet<String>),
}

impl From<&BTreeSet<String>> for ActiveFeatures {
    fn from(value: &BTreeSet<String>) -> Self {
        if value.iter().any(|feat| feat == "all") {
            Self::All
        } else {
            Self::Subset(value.clone())
        }
    }
}

/// Parsed battery pack specification.
///
/// This is the core data model extracted from a battery pack's Cargo.toml.
/// All curated crates, features, hidden deps, and templates are represented here.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct BatteryPackSpec {
    /// Crate name (e.g., `cli-battery-pack`).
    pub name: String,
    /// Version string.
    pub version: String,
    /// Package description.
    pub description: String,
    /// Repository URL.
    pub repository: Option<String>,
    /// Package keywords.
    pub keywords: Vec<String>,
    /// All curated crates, keyed by crate name.
    // [impl format.deps.source-of-truth]
    pub crates: BTreeMap<String, CrateSpec>,
    /// Named features from `[features]`, mapping feature name to parsed refs.
    // [impl format.features.grouping]
    pub features: BTreeMap<String, BTreeSet<FeatureRef>>,
    /// Hidden dependency patterns (may include globs).
    // [impl format.hidden.metadata]
    pub hidden: BTreeSet<String>,
    /// Templates registered in metadata.
    pub templates: BTreeMap<String, TemplateSpec>,
    /// Category definitions, keyed by category name.
    // [impl format.categories.definition]
    #[serde(default)]
    pub categories: BTreeMap<String, CategorySpec>,
    /// Per-feature metadata, keyed by feature name.
    // [impl format.features.metadata]
    #[serde(default)]
    pub feature_meta: BTreeMap<String, ItemMeta>,
    /// Per-dependency metadata, keyed by dependency name.
    // [impl format.deps.metadata]
    #[serde(default)]
    pub dep_meta: BTreeMap<String, ItemMeta>,
}

impl BatteryPackSpec {
    /// Validate that this looks like a valid battery pack.
    ///
    /// Accepts both `battery-pack` (the meta package) and any crate suffixed `-battery-pack
    /// -- matching the filter applied by [`discover-battery-packs`].
    ///
    /// [impl format.crate.name]
    pub fn validate(&self) -> Result<(), Error> {
        if self.name != "battery-pack" && !self.name.ends_with("-battery-pack") {
            return Err(Error::InvalidName {
                name: self.name.clone(),
            });
        }
        self.validate_features()?;
        Ok(())
    }

    /// Check that all feature entries reference crates that actually exist, and that
    /// local-feature references do not form cycles.
    fn validate_features(&self) -> Result<(), Error> {
        for (feature_name, refs) in &self.features {
            for fref in refs {
                if !self.reference_resolves(fref) {
                    return Err(Error::UnknownCrateInFeature {
                        feature: feature_name.clone(),
                        crate_name: fref.dep_name().to_string(),
                    });
                }
            }
        }
        // Cycle detection over local-feature edges.
        let mut visited = BTreeSet::new();
        let mut stack = Vec::new();

        for start in self.features.keys() {
            self.dfs_feature(start, &mut stack, &mut visited)?;
        }
        Ok(())
    }

    /// Does this reference's target exist?
    /// `Dep`/`DepFeature` must point at a declared crate; bare `Feature` may be either
    /// a local feature key or a dep.
    fn reference_resolves(&self, fref: &FeatureRef) -> bool {
        let target = fref.dep_name();

        match fref {
            FeatureRef::Dep(_) | FeatureRef::DepFeature { .. } => self.crates.contains_key(target),
            FeatureRef::Feature(_) => {
                self.crates.contains_key(target) || self.features.contains_key(target)
            }
        }
    }

    fn dfs_feature(
        &self,
        node: &str,
        stack: &mut Vec<String>,
        visited: &mut BTreeSet<String>,
    ) -> Result<(), Error> {
        if stack.iter().any(|stacked| stacked == node) {
            let mut cycle = stack.clone();
            cycle.push(node.to_string());

            return Err(Error::FeatureCycle {
                path: cycle.join("->"),
            });
        }

        if !visited.insert(node.to_string()) {
            return Ok(());
        }

        stack.push(node.to_string());

        if let Some(refs) = self.features.get(node) {
            for fref in refs {
                if let FeatureRef::Feature(name) = fref
                    && self.features.contains_key(name)
                {
                    self.dfs_feature(name, stack, visited)?;
                }
            }
        }

        stack.pop();
        Ok(())
    }
    /// Comprehensive spec validation — collects all issues rather than
    /// failing on the first one. Checks data-only rules from the spec.
    pub fn validate_spec(&self) -> ValidationReport {
        let mut report = ValidationReport::default();

        // [impl format.crate.name]
        if self.name != "battery-pack" && !self.name.ends_with("-battery-pack") {
            report.error(
                "format.crate.name",
                format!("name '{}' must end in '-battery-pack'", self.name),
            );
        }

        // [impl format.crate.keyword]
        if !self.keywords.iter().any(|k| k == "battery-pack") {
            report.error(
                "format.crate.keyword",
                "keywords must include 'battery-pack'",
            );
        }

        // [impl format.crate.repository]
        if self.repository.is_none() {
            report.warning(
                "format.crate.repository",
                "battery pack should set the `repository` field for linking to examples and templates",
            );
        }

        // [impl format.features.grouping]
        for (feature_name, refs) in &self.features {
            for fref in refs {
                if !self.reference_resolves(fref) {
                    report.error(
                        "format.features.grouping",
                        format!(
                            "feature '{feature_name}' references unknown crate '{}'",
                            fref.dep_name()
                        ),
                    );
                }
            }
        }

        self.validate_categories(&mut report);

        report
    }

    /// Category and item-metadata validation rules.
    ///
    /// Appended to [`BatteryPackSpec::validate_spec`]: verifies category
    /// references resolve, exclusive picks are not both defaulted, item metadata
    /// matches real items, and warns on unused or under-specified categories.
    fn validate_categories(&self, report: &mut ValidationReport) {
        // Every referenced category must be declared. Report once per missing
        // (item, category) pair, tagging the item kind for a clear message.
        // [impl format.categories.defined]
        for (feature, meta) in &self.feature_meta {
            for category in &meta.categories {
                if !self.categories.contains_key(category) {
                    report.error(
                        "format.categories.defined",
                        format!("feature '{feature}' references undefined category '{category}'"),
                    );
                }
            }
        }
        for (dep, meta) in &self.dep_meta {
            for category in &meta.categories {
                if !self.categories.contains_key(category) {
                    report.error(
                        "format.categories.defined",
                        format!("dependency '{dep}' references undefined category '{category}'"),
                    );
                }
            }
        }
        for (template, spec) in &self.templates {
            for category in &spec.categories {
                if !self.categories.contains_key(category) {
                    report.error(
                        "format.categories.defined",
                        format!("template '{template}' references undefined category '{category}'"),
                    );
                }
            }
        }

        // Two or more features in the same at-most-one category cannot both be in
        // `default` — that would enable conflicting picks out of the box.
        // [impl format.features.exclusive-conflict]
        let default_features: BTreeSet<&str> = self
            .features
            .get("default")
            .map(|refs| {
                refs.iter()
                    .filter_map(|fref| match fref {
                        FeatureRef::Feature(name) => Some(name.as_str()),
                        _ => None,
                    })
                    .collect()
            })
            .unwrap_or_default();
        for (category, spec) in &self.categories {
            if spec.pick != PickMode::AtMostOne {
                continue;
            }
            let mut in_default: Vec<&str> = self
                .feature_meta
                .iter()
                .filter(|(feature, meta)| {
                    meta.categories.iter().any(|c| c == category)
                        && default_features.contains(feature.as_str())
                })
                .map(|(feature, _)| feature.as_str())
                .collect();
            in_default.sort_unstable();
            if in_default.len() > 1 {
                let names = in_default
                    .iter()
                    .map(|f| format!("'{f}'"))
                    .collect::<Vec<_>>()
                    .join(" and ");
                report.error(
                    "format.features.exclusive-conflict",
                    format!(
                        "features {names} are both in default but belong to at-most-one category '{category}'"
                    ),
                );
            }
        }

        // Feature metadata must name a real feature. An optional dependency
        // implicitly defines a same-named feature (Cargo's `dep:`/implicit
        // feature), so those count as valid even though the parser filters the
        // auto-generated `foo = ["dep:foo"]` mirror out of `self.features`.
        // [impl format.features.unknown-feature]
        for feature in self.feature_meta.keys() {
            let is_named_feature = self.features.contains_key(feature);
            let is_optional_dep = self.crates.get(feature).is_some_and(|spec| spec.optional);
            if !is_named_feature && !is_optional_dep {
                report.error(
                    "format.features.unknown-feature",
                    format!("feature metadata '{feature}' does not match any entry in [features]"),
                );
            }
        }

        // Dependency metadata must name a real dependency.
        // [impl format.dependencies.unknown-dep]
        for dep in self.dep_meta.keys() {
            if !self.crates.contains_key(dep) {
                report.error(
                    "format.dependencies.unknown-dep",
                    format!("dependency metadata '{dep}' does not match any dependency"),
                );
            }
        }

        // Warn about categories nothing references.
        // [impl format.categories.empty]
        for category in self.categories.keys() {
            let referenced = self
                .feature_meta
                .values()
                .chain(self.dep_meta.values())
                .any(|meta| meta.categories.iter().any(|c| c == category))
                || self
                    .templates
                    .values()
                    .any(|spec| spec.categories.iter().any(|c| c == category));
            if !referenced {
                report.warning(
                    "format.categories.empty",
                    format!("category '{category}' is declared but has no members"),
                );
            }
        }

        // Warn when an at-most-one category lacks a title for the picker header.
        // [impl format.categories.pick-missing-title]
        for (category, spec) in &self.categories {
            if spec.pick == PickMode::AtMostOne && spec.title.is_none() {
                report.warning(
                    "format.categories.pick-missing-title",
                    format!(
                        "at-most-one category '{category}' should have a title for the picker UI"
                    ),
                );
            }
        }
    }

    /// Resolve which crates should be installed for the given active features.
    ///
    /// With no features specified (empty slice), returns the default set:
    /// crates from the `default` feature, or all non-optional crates if
    /// no `default` feature exists.
    ///
    /// Features are additive — each named feature adds its crates on top.
    // [impl format.features.additive]
    pub fn resolve_crates(&self, active_features: &[&str]) -> BTreeMap<String, CrateSpec> {
        let mut result: BTreeMap<String, CrateSpec> = BTreeMap::new();
        let mut visiting: BTreeSet<String> = BTreeSet::new();

        // Weak `foo?/bar` refs are deferred -- they only add features to deps activated by something else in this combo.
        //  [impl feature-refs.resolution.weak]
        let mut pending_weak: Vec<(String, String)> = Vec::new();

        if active_features.is_empty() {
            self.add_default_crates(&mut result, &mut visiting, &mut pending_weak);
        } else {
            for feature_name in active_features {
                if *feature_name == "default" {
                    self.add_default_crates(&mut result, &mut visiting, &mut pending_weak);
                } else if let Some(refs) = self.features.get(*feature_name) {
                    self.add_feature_crates(refs, &mut result, &mut visiting, &mut pending_weak);
                }
            }
        }

        // [impl format.features.dev-build-always]
        // Crates listed in a feature are gated by feature selection. Non-optional normal deps
        // that belong to no feature are unconditional base deps, so always include them.
        // Dev/build deps are never gated by Cargo features either.
        let featured: std::collections::BTreeSet<&str> = self
            .features
            .values()
            .flat_map(|refs| refs.iter().map(FeatureRef::dep_name))
            .collect();
        for (name, spec) in &self.crates {
            let unconditional = spec.dep_kind != DepKind::Normal
                || (!spec.optional && !featured.contains(name.as_str()));
            if unconditional && !self.is_hidden(name) {
                result.entry(name.clone()).or_insert_with(|| spec.clone());
            }
        }

        // Apply deferred weak refs to deps that were activated by other means.
        for (dep, feature) in pending_weak {
            if let Some(entry) = result.get_mut(&dep) {
                entry.features.insert(feature);
            }
        }

        result
    }

    /// Add the default set of crates to the result map.
    // [impl format.features.default]
    fn add_default_crates(
        &self,
        result: &mut BTreeMap<String, CrateSpec>,
        visiting: &mut BTreeSet<String>,
        pending_weak: &mut Vec<(String, String)>,
    ) {
        if let Some(default_refs) = self.features.get("default") {
            // Explicit default feature exists -- expand it.
            self.add_feature_crates(default_refs, result, visiting, pending_weak);
        } else {
            // No default feature -- include all non-optional crates.
            for (name, spec) in &self.crates {
                if !spec.optional {
                    result.insert(name.clone(), spec.clone());
                }
            }
        }
    }

    /// Resolve a list of `FeatureRef`s into the result map.
    ///
    /// Strong `foo/bar` activates `foo` and adds feature `bar`.
    /// Weak is deferred -- pushed into `pending_weak` for the final-pass apply in
    /// `resolve_crates`.
    /// Bare `Feature(name)` expands a local feature recursively (with cycle guard)
    /// or falls back to dep lookup.
    ///
    // [impl format.features.augment]
    fn add_feature_crates(
        &self,
        refs: &BTreeSet<FeatureRef>,
        result: &mut BTreeMap<String, CrateSpec>,
        visiting: &mut BTreeSet<String>,
        pending_weak: &mut Vec<(String, String)>,
    ) {
        for fref in refs {
            match fref {
                // Bare `foo`: recurse into a local feature if one exists, else fall back to
                // dep lookup (covers the implicit-feature case where `foo` is an optional dep).
                FeatureRef::Feature(name) => {
                    if let Some(inner) = self.features.get(name) {
                        if visiting.insert(name.clone()) {
                            self.add_feature_crates(inner, result, visiting, pending_weak);
                            visiting.remove(name);
                        }
                    } else {
                        self.add_dep(name, None, result);
                    }
                }
                // `dep:foo`: activate the dep directly, never as a feature.
                FeatureRef::Dep(name) => self.add_dep(name, None, result),
                FeatureRef::DepFeature { dep, feature, weak } => {
                    if *weak {
                        pending_weak.push((dep.clone(), feature.clone()));
                    } else {
                        self.add_dep(dep, Some(feature), result);
                    }
                }
            }
        }
    }

    /// Insert a dep into the result map, merging its row-declared features and
    /// optionally adding one extra per-dep feature on top.
    fn add_dep(
        &self,
        dep_name: &str,
        extra_feature: Option<&str>,
        result: &mut BTreeMap<String, CrateSpec>,
    ) {
        let Some(spec) = self.crates.get(dep_name) else {
            return;
        };

        // Fresh insert: clone the spec (its declared features come along).
        // Repeat insert: merge declared features into the existing entry. Either way, layer
        // `extra_feature` on top.
        let entry = match result.entry(dep_name.to_string()) {
            Entry::Vacant(vacant_entry) => vacant_entry.insert(spec.clone()),
            Entry::Occupied(occupied_entry) => {
                let existing = occupied_entry.into_mut();
                existing.features.extend(spec.features.iter().cloned());

                existing
            }
        };

        if let Some(extra) = extra_feature {
            entry.features.insert(extra.to_string());
        }
    }

    /// Resolve all crates regardless of features or optional status.
    /// Only used in tests; prefer [`resolve_for_features`] for user-facing paths.
    #[cfg(test)]
    pub(crate) fn resolve_all(&self) -> BTreeMap<String, CrateSpec> {
        self.crates.clone()
    }

    /// Resolve crates for a typed [`ActiveFeatures`] selection, filtered for visibility.
    ///
    /// `ActiveFeatures::All` expands to every declared feature so per-dep activations
    /// (`serde/derive`, `foo?/bar`, nested refs) reach `resolve_crates` instead of being dropped.
    /// Hidden crates are always excluded from the result -- this is the surface every
    /// user-facing caller (sync, status, `--all-features`, bp-managed rewrite) reads from.
    ///
    /// When `All` is active, optional deps whose implicit feature was stripped during parsing
    /// (i.e., they have no explicit feature entry referencing them) are also included. This
    /// mirrors Cargo's behavior where `--all-features` activates every optional dep.
    ///
    /// [impl format.hidden.effect]
    pub fn resolve_for_features(&self, active: &ActiveFeatures) -> BTreeMap<String, CrateSpec> {
        let expanded: Vec<&str> = match active {
            ActiveFeatures::All => self.features.keys().map(String::as_str).collect(),
            ActiveFeatures::Subset(features) => features.iter().map(String::as_str).collect(),
        };

        let mut resolved = self.resolve_crates(&expanded);

        // When all features are active, also activate optional deps whose implicit
        // feature (`feat = ["dep:feat"]`) was stripped during parsing. Cargo's
        // `--all-features` activates these, so we must too.
        if matches!(active, ActiveFeatures::All) {
            let featured: BTreeSet<&str> = self
                .features
                .values()
                .flat_map(|refs| refs.iter().map(FeatureRef::dep_name))
                .collect();
            for (name, spec) in &self.crates {
                if spec.optional && !featured.contains(name.as_str()) {
                    resolved.entry(name.clone()).or_insert_with(|| spec.clone());
                }
            }
        }

        resolved.retain(|name, _| !self.is_hidden(name));

        resolved
    }

    /// Check whether a crate name matches the hidden patterns.
    // [impl format.hidden.effect]
    pub fn is_hidden(&self, crate_name: &str) -> bool {
        self.hidden
            .iter()
            .any(|pattern| glob_match(pattern, crate_name))
    }

    /// Return all non-hidden crates.
    pub fn visible_crates(&self) -> BTreeMap<&str, &CrateSpec> {
        self.crates
            .iter()
            .filter(|(name, _)| !self.is_hidden(name))
            .map(|(name, spec)| (name.as_str(), spec))
            .collect()
    }

    /// Return all visible (non-hidden) crates grouped by feature, with a flag
    /// indicating whether each crate is in the default set.
    ///
    /// Returns `Vec<(group_name, crate_name, &CrateSpec, is_default)>`.
    /// Crates not in any feature are grouped under `"default"`.
    // [impl format.hidden.effect]
    // [impl tui.installed.hidden]
    // [impl tui.browse.hidden]
    pub fn all_crates_with_grouping(&self) -> Vec<(String, String, &CrateSpec, bool)> {
        let default_crates = self.resolve_crates(&[]);
        let mut result = Vec::new();
        let mut seen = std::collections::BTreeSet::new();

        // First, emit crates grouped by features
        for (feature_name, refs) in &self.features {
            for fref in refs {
                let key = fref.dep_name();
                if self.is_hidden(key) {
                    continue;
                }
                if let Some(spec) = self.crates.get(key)
                    && seen.insert(key.to_string())
                {
                    let is_default = default_crates.contains_key(key);
                    result.push((feature_name.clone(), key.to_string(), spec, is_default))
                }
            }
        }

        // Then, emit any crates not covered by a feature (grouped as "default")
        for (crate_name, spec) in &self.crates {
            if self.is_hidden(crate_name) {
                continue;
            }
            if seen.insert(crate_name.clone()) {
                let is_default = default_crates.contains_key(crate_name);
                result.push(("default".to_string(), crate_name.clone(), spec, is_default));
            }
        }

        result
    }

    /// Returns true if this battery pack has meaningful choices for the user
    /// (more than 3 crates or has named features beyond default).
    pub fn has_meaningful_choices(&self) -> bool {
        let non_default_features = self
            .features
            .keys()
            .filter(|k| k.as_str() != "default")
            .count();
        non_default_features > 0 || self.crates.len() > 3
    }

    /// Item names (features + deps) that belong to the named category, sorted.
    ///
    /// Scans both feature and dependency metadata for entries whose `categories`
    /// list contains `category`, then returns their sorted, deduped names.
    // [impl format.categories.definition]
    pub fn items_in_category(&self, category: &str) -> Vec<String> {
        let mut items: BTreeSet<String> = BTreeSet::new();
        for (name, meta) in self.feature_meta.iter().chain(self.dep_meta.iter()) {
            if meta.categories.iter().any(|c| c == category) {
                items.insert(name.clone());
            }
        }
        items.into_iter().collect()
    }
}

// ============================================================================
// Glob matching (minimal, for hidden dep patterns)
// ============================================================================

/// Simple glob matching for crate name patterns.
///
/// Supports:
/// - `*` matches any sequence of characters
/// - `?` matches any single character
/// - Literal characters match exactly
// [impl format.hidden.glob]
// [impl format.hidden.wildcard]
fn glob_match(pattern: &str, name: &str) -> bool {
    let pat: Vec<char> = pattern.chars().collect();
    let txt: Vec<char> = name.chars().collect();
    glob_match_inner(&pat, &txt)
}

fn glob_match_inner(pat: &[char], txt: &[char]) -> bool {
    match (pat.first(), txt.first()) {
        (None, None) => true,
        (Some('*'), _) => {
            // * matches zero chars (skip the *) or one char (consume from txt)
            glob_match_inner(&pat[1..], txt)
                || (!txt.is_empty() && glob_match_inner(pat, &txt[1..]))
        }
        (Some('?'), Some(_)) => glob_match_inner(&pat[1..], &txt[1..]),
        (Some(a), Some(b)) if a == b => glob_match_inner(&pat[1..], &txt[1..]),
        _ => false,
    }
}

// ============================================================================
// Cross-pack merging
// ============================================================================

/// A crate spec produced by merging the same crate across multiple battery packs.
///
/// Unlike `CrateSpec` which has a single `dep_kind`, a merged spec may need to
/// appear in multiple dependency sections (e.g., both `[dev-dependencies]` and
/// `[build-dependencies]`).
#[derive(Debug, Clone)]
pub struct MergedCrateSpec {
    /// Recommended version (highest wins across all packs).
    pub version: String,
    /// Union of all recommended Cargo features.
    pub features: BTreeSet<String>,
    /// Which dependency sections this crate should be added to.
    /// Usually contains a single element. Contains two elements
    /// when one pack lists it as dev and another as build.
    pub dep_kinds: Vec<DepKind>,
    /// Whether this crate is optional.
    pub optional: bool,
}

/// Merge crate specs from multiple battery packs.
///
/// When the same crate appears in multiple packs, applies merging rules:
/// - Version: highest wins, even across major versions
///   (`manifest.merge.version`)
/// - Features: union all (`manifest.merge.features`)
/// - Dep kind: Normal wins (widest scope); if dev vs build conflict,
///   adds to both sections (`manifest.merge.dep-kind`)
// [impl manifest.merge.version]
// [impl manifest.merge.features]
// [impl manifest.merge.dep-kind]
pub fn merge_crate_specs(
    specs: &[BTreeMap<String, CrateSpec>],
) -> BTreeMap<String, MergedCrateSpec> {
    let mut merged: BTreeMap<String, MergedCrateSpec> = BTreeMap::new();

    for pack in specs {
        for (name, spec) in pack {
            match merged.get_mut(name) {
                Some(existing) => {
                    // Version: highest wins
                    if compare_versions(&spec.version, &existing.version)
                        == std::cmp::Ordering::Greater
                    {
                        existing.version = spec.version.clone();
                    }

                    // Features: union
                    existing.features.extend(spec.features.iter().cloned());

                    // Dep kind: merge
                    existing.dep_kinds = merge_dep_kinds(&existing.dep_kinds, spec.dep_kind);

                    // Optional: if any pack makes it non-optional, it's non-optional
                    if !spec.optional {
                        existing.optional = false;
                    }
                }
                None => {
                    merged.insert(
                        name.clone(),
                        MergedCrateSpec {
                            version: spec.version.clone(),
                            features: spec.features.clone(),
                            dep_kinds: vec![spec.dep_kind],
                            optional: spec.optional,
                        },
                    );
                }
            }
        }
    }

    merged
}

/// Compare two version strings using semver-like ordering.
///
/// Parses dot-separated numeric components (e.g., "1.2.3") and compares
/// them left-to-right. Non-numeric or missing components are compared
/// as strings as a fallback. The highest version wins, even across
/// major versions.
fn compare_versions(a: &str, b: &str) -> std::cmp::Ordering {
    let a_parts: Vec<&str> = a.split('.').collect();
    let b_parts: Vec<&str> = b.split('.').collect();

    let max_len = a_parts.len().max(b_parts.len());

    for i in 0..max_len {
        let a_part = a_parts.get(i).copied().unwrap_or("0");
        let b_part = b_parts.get(i).copied().unwrap_or("0");

        // Try numeric comparison first
        match (a_part.parse::<u64>(), b_part.parse::<u64>()) {
            (Ok(a_num), Ok(b_num)) => {
                let ord = a_num.cmp(&b_num);
                if ord != std::cmp::Ordering::Equal {
                    return ord;
                }
            }
            // Fallback to string comparison for non-numeric parts
            _ => {
                let ord = a_part.cmp(b_part);
                if ord != std::cmp::Ordering::Equal {
                    return ord;
                }
            }
        }
    }

    std::cmp::Ordering::Equal
}

/// Merge dependency kinds according to the spec rules.
///
/// - If any side includes `Normal`, the result is `[Normal]` (widest scope).
/// - If one side is `Dev` and the other is `Build`, the result is `[Dev, Build]`.
/// - Otherwise, the existing set is returned unchanged.
fn merge_dep_kinds(existing: &[DepKind], incoming: DepKind) -> Vec<DepKind> {
    // If Normal is already present or incoming, Normal wins
    if existing.contains(&DepKind::Normal) || incoming == DepKind::Normal {
        return vec![DepKind::Normal];
    }

    // Build the combined set
    let mut kinds: Vec<DepKind> = existing.to_vec();
    if !kinds.contains(&incoming) {
        kinds.push(incoming);
    }
    kinds.sort();
    kinds
}

// ============================================================================
// Raw deserialization types (internal)
// ============================================================================

#[derive(Deserialize)]
struct RawMetadata {
    #[serde(default, rename = "battery-pack")]
    battery_pack: Option<RawBatteryPackMetadata>,
    #[serde(default)]
    battery: Option<RawBatteryMetadata>,
}

#[derive(Deserialize)]
struct RawBatteryPackMetadata {
    #[serde(default)]
    hidden: Vec<String>,
    #[serde(default)]
    categories: BTreeMap<String, CategorySpec>,
    #[serde(default)]
    features: BTreeMap<String, ItemMeta>,
    #[serde(default)]
    dependencies: BTreeMap<String, ItemMeta>,
}

#[derive(Deserialize)]
struct RawBatteryMetadata {
    #[serde(default)]
    templates: BTreeMap<String, RawTemplateSpec>,
}

#[derive(Deserialize)]
struct RawTemplateSpec {
    path: String,
    #[serde(default)]
    description: Option<String>,
    #[serde(default)]
    categories: Vec<String>,
}

// ============================================================================
// Parsing
// ============================================================================

fn package_to_spec(pkg: &Package) -> Result<BatteryPackSpec, Error> {
    // -- direct field copies --
    let name = pkg.name.to_string();
    let version = pkg.version.to_string();
    let description = pkg.description.clone().unwrap_or_default();
    let repository = pkg.repository.clone();
    let keywords = pkg.keywords.clone();

    // -- dep mapping: cargo_metadata::Dependency -> CrateSpec --
    let mut crates = BTreeMap::new();
    for dep in &pkg.dependencies {
        let kind = match dep.kind {
            DependencyKind::Normal => DepKind::Normal,
            DependencyKind::Development => DepKind::Dev,
            DependencyKind::Build => DepKind::Build,
            _ => {
                eprintln!(
                    "warning: skipping dependency '{}' with unrecognized kind {:?}",
                    dep.name, dep.kind
                );
                continue;
            }
        };

        // Strip the implicit caret cargo_metadata adds so emitted Cargo.toml
        // entries match `cargo add` convention (`"1"` not `"^1"`).
        let version = dep.req.to_string();
        let version = version
            .strip_prefix('^')
            .map(str::to_owned)
            .unwrap_or(version);

        crates.insert(
            dep.name.clone(),
            CrateSpec {
                version,
                features: dep.features.iter().cloned().collect(),
                dep_kind: kind,
                optional: dep.optional,
            },
        );
    }

    // -- features: filter out auto-gen optional-dep features --
    let optional_dep_names = pkg
        .dependencies
        .iter()
        .filter(|dep| dep.optional)
        .map(|dep| dep.name.as_str())
        .collect::<BTreeSet<_>>();

    // Skip cargo's auto-generated `feat = ["dep:feat"]` entries that mirror an
    // optional dep one-to-one — they're noise, not author intent.
    let is_auto_optional = |key: &str, value: &[String]| {
        optional_dep_names.contains(key)
            && value.len() == 1
            && value[0].strip_prefix("dep:") == Some(key)
    };
    let mut features: BTreeMap<String, BTreeSet<FeatureRef>> = BTreeMap::new();
    for (key, value) in &pkg.features {
        if is_auto_optional(key.as_str(), value) {
            continue;
        }
        let parsed = value
            .iter()
            .map(|raw| {
                FeatureRef::parse(raw).map_err(|source| Error::FeatureRefParse {
                    feature: key.to_string(),
                    raw: raw.clone(),
                    source,
                })
            })
            .collect::<Result<BTreeSet<_>, _>>()?;
        features.insert(key.to_string(), parsed);
    }

    // -- read [package.metadata.battery-pack].hidden + battery.templates --
    let raw_meta: Option<RawMetadata> = if pkg.metadata.is_null() {
        None
    } else {
        Some(
            serde_json::from_value(pkg.metadata.clone()).map_err(|source| {
                Error::MetadataDecode {
                    package: pkg.name.to_string(),
                    source,
                }
            })?,
        )
    };

    let hidden = raw_meta
        .as_ref()
        .and_then(|meta| meta.battery_pack.as_ref())
        .map(|raw| raw.hidden.iter().cloned().collect::<BTreeSet<_>>())
        .unwrap_or_default();

    let templates = raw_meta
        .as_ref()
        .and_then(|meta| meta.battery.as_ref())
        .map(|bp| {
            bp.templates
                .iter()
                .map(|(name, raw)| {
                    (
                        name.clone(),
                        TemplateSpec {
                            path: raw.path.clone(),
                            description: raw.description.clone(),
                            categories: raw.categories.clone(),
                        },
                    )
                })
                .collect()
        })
        .unwrap_or_default();

    // -- read category definitions and per-item metadata --
    let categories = raw_meta
        .as_ref()
        .and_then(|meta| meta.battery_pack.as_ref())
        .map(|raw| raw.categories.clone())
        .unwrap_or_default();

    let feature_meta = raw_meta
        .as_ref()
        .and_then(|meta| meta.battery_pack.as_ref())
        .map(|raw| raw.features.clone())
        .unwrap_or_default();

    let dep_meta = raw_meta
        .as_ref()
        .and_then(|meta| meta.battery_pack.as_ref())
        .map(|raw| raw.dependencies.clone())
        .unwrap_or_default();

    Ok(BatteryPackSpec {
        name,
        version,
        description,
        repository,
        keywords,
        crates,
        features,
        hidden,
        templates,
        categories,
        feature_meta,
        dep_meta,
    })
}

// ============================================================================
// Source discovery
// ============================================================================

/// Run `cargo metadata --manifest-path PATH --no-deps`.
fn load_metadata(manifest_path: &Path) -> Result<Metadata, Error> {
    MetadataCommand::new()
        .manifest_path(manifest_path)
        .no_deps()
        .exec()
        .map_err(Error::Metadata)
}

/// Discover battery packs reachable from a path.
///
/// `path` may be a workspace root or any crate within a workspace;
/// `cargo metadata` walks up to find the workspace root either way.
/// A crate without a `[workspace]` section is treated as a 1-member workspace, so
/// standalone packs are also covered.
pub fn discover_battery_packs(path: &Path) -> Result<Vec<BatteryPackSpec>, Error> {
    let manifest_path = path.join("Cargo.toml");
    let metadata = load_metadata(&manifest_path)?;

    metadata
        .workspace_packages()
        .into_iter()
        .filter(|pkg| pkg.name == "battery-pack" || pkg.name.ends_with("-battery-pack"))
        .map(package_to_spec)
        .collect()
}

/// Parse a single battery pack from its `Cargo.toml` path.
///
/// Runs `cargo metadata` against the given manifest and returns the spec for the matching package.
///
/// Note: Does NOT call [`BatteryPackSpec::validate`] -- `bphelper-build` consumes this from
/// the build script of scaffolded user crates whose package name isn't a `*-battery-pack`.
/// Callers that install or sync (CLI commands, registry fetches) must validate explicitly.
pub fn parse_battery_pack_from_path(manifest_path: &Path) -> Result<BatteryPackSpec, Error> {
    let metadata = load_metadata(manifest_path)?;

    let target = manifest_path.canonicalize().map_err(|err| Error::Io {
        path: manifest_path.display().to_string(),
        source: err,
    })?;

    let pkg = metadata
        .packages
        .iter()
        .find(|pkg| {
            pkg.manifest_path
                .as_std_path()
                .canonicalize()
                .map(|path| path == target)
                .unwrap_or(false)
        })
        .ok_or(Error::MissingField("package for manifest"))?;

    package_to_spec(pkg)
}

// ============================================================================
// On-disk validation
// ============================================================================

/// Validate a battery pack's on-disk structure against the spec.
///
/// `crate_root` is the directory containing the battery pack's `Cargo.toml`.
/// This checks filesystem-level rules that can't be verified from the parsed
/// manifest alone.
pub fn validate_on_disk(spec: &BatteryPackSpec, crate_root: &Path) -> ValidationReport {
    let mut report = ValidationReport::default();
    validate_lib_rs(crate_root, &mut report);
    validate_no_extra_code(crate_root, &mut report);
    validate_templates_on_disk(spec, crate_root, &mut report);
    report
}

/// Check that `src/lib.rs` contains only doc-comments, whitespace, and
/// include directives — no functional code.
// [impl format.crate.lib]
fn validate_lib_rs(crate_root: &Path, report: &mut ValidationReport) {
    let lib_rs = crate_root.join("src/lib.rs");
    let content = match std::fs::read_to_string(&lib_rs) {
        Ok(c) => c,
        Err(_) => return, // Missing lib.rs is a different problem
    };

    for line in content.lines() {
        let trimmed = line.trim();
        if trimmed.is_empty()
            || trimmed.starts_with("//")
            || trimmed.starts_with("#!")
            || trimmed.starts_with("include!")
            || trimmed.starts_with("include_str!")
        {
            continue;
        }
        report.warning(
            "format.crate.lib",
            format!(
                "src/lib.rs contains code beyond doc-comments and includes: {}",
                trimmed
            ),
        );
        return; // One warning is enough
    }
}

/// Check that `src/` contains no `.rs` files beyond `lib.rs`.
// [impl format.crate.no-code]
fn validate_no_extra_code(crate_root: &Path, report: &mut ValidationReport) {
    let src_dir = crate_root.join("src");
    let entries = match std::fs::read_dir(&src_dir) {
        Ok(e) => e,
        Err(_) => return,
    };

    for entry in entries.flatten() {
        let path = entry.path();
        if path.is_file()
            && let Some(ext) = path.extension()
            && ext == "rs"
            && path.file_name().is_some_and(|n| n != "lib.rs")
        {
            report.error(
                "format.crate.no-code",
                format!(
                    "src/ contains '{}' — battery packs must not contain functional code",
                    path.file_name().unwrap().to_string_lossy()
                ),
            );
        }
    }
}

/// Check that each template declared in metadata exists on disk.
// [impl format.templates.directory]
fn validate_templates_on_disk(
    spec: &BatteryPackSpec,
    crate_root: &Path,
    report: &mut ValidationReport,
) {
    for (name, template) in &spec.templates {
        let template_dir = crate_root.join(&template.path);
        if !template_dir.is_dir() {
            report.error(
                "format.templates.directory",
                format!(
                    "template '{}' path '{}' does not exist",
                    name, template.path
                ),
            );
            continue;
        }

        // Cargo excludes any subdirectory containing a Cargo.toml from the
        // published tarball (it treats them as separate crate boundaries).
        // Template Cargo.toml files must be named _Cargo.toml instead.
        for entry in walkdir::WalkDir::new(&template_dir) {
            let entry = match entry {
                Ok(e) => e,
                Err(e) => {
                    report.error(
                        "format.templates.walk",
                        format!("failed to walk template '{}': {}", name, e),
                    );
                    continue;
                }
            };
            if entry.file_type().is_file() && entry.file_name() == "Cargo.toml" {
                let rel = entry
                    .path()
                    .strip_prefix(crate_root)
                    .unwrap_or(entry.path());
                report.error(
                    "format.templates.cargo-toml",
                    format!(
                        "{} will be excluded from the published crate. \
                         Rename to _Cargo.toml (the template engine maps it back automatically).",
                        rel.display()
                    ),
                );
            }
        }
    }
}

// ============================================================================
// Tests
// ============================================================================

#[cfg(test)]
mod tests {
    use std::fs;

    use crate::{
        ActiveFeatures,
        feature_ref::FeatureRef,
        test_support::{WorkspaceFixture, parse_test},
    };

    use super::*;
    use indoc::indoc;

    // -- Helper unit tests --

    #[test]
    fn parse_bp_from_path_normalizes_non_canonical_input() {
        let mut fx = WorkspaceFixture::new();
        fx.add_pack(
            "test-pack",
            indoc! {r#"
                [package]
                name = "test-battery-pack"
                version = "0.1.0"
                keywords = ["battery-pack"]
            "#},
        );

        let root = fx.finalize();
        let non_canonical = root
            .join("test-pack")
            .join("..")
            .join("test-pack")
            .join("Cargo.toml");

        let spec = parse_battery_pack_from_path(&non_canonical).unwrap();

        // Snapshot: identity survives path normalization.
        snapbox::assert_data_eq!(
            format!("{} {}", spec.name, spec.version),
            snapbox::str![[r#"test-battery-pack 0.1.0"#]]
        );

        // Point assertion retained for direct failure messages.
        assert_eq!(spec.name, "test-battery-pack");
    }

    #[test]
    fn feature_with_dep_prefix_resolves() {
        let manifest = indoc! {r#"
            [package]
            name = "test-battery-pack"
            version = "0.1.0"
            keywords = ["battery-pack"]

            [dependencies]
            indicatif = { version = "0.17", optional = true }

            [features]
            indicators = ["dep:indicatif"]
        "#};

        let spec = parse_test(manifest).unwrap();
        spec.validate().unwrap();
        let resolved = spec.resolve_for_features(&ActiveFeatures::Subset(BTreeSet::from([
            "indicators".to_string(),
        ])));

        // Snapshot: `dep:indicatif` pulls indicatif into the resolved set.
        snapbox::assert_data_eq!(
            render_resolved(&resolved),
            snapbox::str![[r#"indicatif 0.17"#]]
        );

        // Point assertion retained.
        assert!(resolved.contains_key("indicatif"));
    }

    #[test]
    fn feature_with_slash_feature_resolves() {
        let manifest = indoc! {r#"
            [package]
            name = "test-battery-pack"
            version = "0.1.0"
            keywords = ["battery-pack"]

            [dependencies]
            serde = { version = "1", optional = true }

            [features]
            fancy = ["serde/derive"]
        "#};

        let spec = parse_test(manifest).unwrap();
        spec.validate().unwrap();
        let resolved = spec.resolve_for_features(&ActiveFeatures::Subset(BTreeSet::from([
            "fancy".to_string(),
        ])));

        // Snapshot: `serde/derive` pulls serde into the resolved set.
        snapbox::assert_data_eq!(render_resolved(&resolved), snapbox::str![[r#"serde 1"#]]);

        // Point assertion retained.
        assert!(resolved.contains_key("serde"));
    }

    #[test]
    fn feature_with_weak_slash_feature_resolve() {
        let manifest = indoc! {r#"
            [package]
            name = "test-battery-pack"
            version = "0.1.0"
            keywords = ["battery-pack"]

            [dependencies]
            serde = { version = "1", optional = true }

            [features]
            maybe-derive = ["serde?/derive"]
        "#};

        let spec = parse_test(manifest).unwrap();
        spec.validate().unwrap();
        let resolved = spec.resolve_for_features(&ActiveFeatures::Subset(BTreeSet::from([
            "maybe-derive".to_string(),
        ])));

        // Weak-only refs do not activate the dep on their own — matches
        // Cargo's weak-dep semantics. See `feature-refs.resolution.weak`.
        snapbox::assert_data_eq!(render_resolved(&resolved), snapbox::str![""]);
        assert!(!resolved.contains_key("serde"));
    }

    // Hidden crate referenced from a feature; ActiveFeatures::All must filter it out so
    // callers (--all-features, sync verify, status verify, bp-managed rewrite) never
    // see it in their resolved set.
    #[test]
    fn resolve_for_features_excludes_hidden_crates() {
        let manifest = indoc! {r#"
        [package]
        name = "test-battery-pack"
        version = "0.1.0"
        keywords = ["battery-pack"]

        [package.metadata.battery-pack]
        hidden = ["serde_derive"]

        [dependencies]
        serde = { version = "1.0.0", optional = true }
        serde_derive = { version = "1.0.0", optional = true }

        [features]
        fancy = ["dep:serde", "dep:serde_derive"]
        "#};

        let spec = parse_test(manifest).unwrap();
        let resolved = spec.resolve_for_features(&ActiveFeatures::All);

        assert!(
            !resolved.contains_key("serde_derive"),
            "hidden crate must not be returned by ActiveFeatures::All"
        );
        assert!(resolved.contains_key("serde"));
    }

    // Optional dep whose implicit feature (`feat = ["dep:feat"]`) was stripped during
    // parsing must still appear when ActiveFeatures::All is active. Cargo's
    // `--all-features` activates every optional dep regardless of whether it appears
    // in an explicit feature entry.
    #[test]
    fn resolve_for_features_all_includes_implicit_optional_deps() {
        let manifest = indoc! {r#"
        [package]
        name = "test-battery-pack"
        version = "0.1.0"
        keywords = ["battery-pack"]

        [dependencies]
        clap = { version = "4", optional = true }
        indicatif = { version = "0.17", optional = true }

        [features]
        default = ["dep:clap"]
        "#};

        // `indicatif` has no explicit feature entry; its implicit `indicatif = ["dep:indicatif"]`
        // is stripped during parsing. ActiveFeatures::All must still include it.
        let spec = parse_test(manifest).unwrap();
        let resolved = spec.resolve_for_features(&ActiveFeatures::All);

        assert!(
            resolved.contains_key("clap"),
            "clap is in an explicit feature, should be resolved"
        );
        assert!(
            resolved.contains_key("indicatif"),
            "indicatif is optional with only an implicit feature; All must still include it"
        );
    }

    // Two local features pointing at each other -- dfs_feature must report a cycle.
    #[test]
    fn validate_rejects_feature_cycle() {
        let manifest = indoc! {r#"
        [package]
        name = "test-battery-pack"
        version = "0.1.0"
        keywords = ["battery-pack"]

        [features]
        a = ["b"]
        b = ["a"]
        "#};

        let spec = parse_test(manifest).unwrap();

        match spec.validate() {
            Err(Error::FeatureCycle { path }) => {
                assert!(path.contains("a") && path.contains("b"), "path = {path}");
            }
            other => panic!("expected FeatureCycle, got {other:?}"),
        }
    }

    // Documented Limitation: a persisted set containing "all" always collapses to ActiveFeatures::All,
    // even when the spec declares a literal feature named `all`.
    #[test]
    fn active_features_from_btreeset_treats_all_literal_as_sentinel() {
        let set = BTreeSet::from(["all".to_string()]);
        let active: ActiveFeatures = (&set).into();
        assert_eq!(active, ActiveFeatures::All);

        let explicit = BTreeSet::from(["default".to_string(), "fancy".to_string()]);
        let active: ActiveFeatures = (&explicit).into();
        assert_eq!(active, ActiveFeatures::Subset(explicit));
    }

    // -- Parsing tests --

    #[test]
    // [verify format.deps.source-of-truth]
    // [verify format.deps.kind-mapping]
    fn parse_deps_from_all_sections() {
        let manifest = indoc! {r#"
            [package]
            name = "test-battery-pack"
            version = "0.1.0"

            [dependencies]
            serde = { version = "1", features = ["derive"] }

            [dev-dependencies]
            insta = "1.34"

            [build-dependencies]
            cc = "1.0"
        "#};

        let spec = parse_test(manifest).unwrap();
        assert_eq!(spec.crates.len(), 3);

        let serde = &spec.crates["serde"];
        assert_eq!(serde.dep_kind, DepKind::Normal);
        assert_eq!(serde.version, "1");
        assert_eq!(serde.features, BTreeSet::from(["derive".to_string()]));

        let insta = &spec.crates["insta"];
        assert_eq!(insta.dep_kind, DepKind::Dev);
        assert_eq!(insta.version, "1.34");

        let cc = &spec.crates["cc"];
        assert_eq!(cc.dep_kind, DepKind::Build);
        assert_eq!(cc.version, "1.0");
    }

    #[test]
    // [verify format.deps.version-features]
    fn parse_version_and_features() {
        let manifest = indoc! {r#"
            [package]
            name = "test-battery-pack"
            version = "0.1.0"

            [dependencies]
            tokio = { version = "1", features = ["macros", "rt-multi-thread"] }
            anyhow = "1"
        "#};

        let spec = parse_test(manifest).unwrap();
        let tokio = &spec.crates["tokio"];
        assert_eq!(tokio.version, "1");
        assert_eq!(
            tokio.features,
            BTreeSet::from(["macros".to_string(), "rt-multi-thread".to_string()])
        );
        assert!(!tokio.optional);

        let anyhow = &spec.crates["anyhow"];
        assert_eq!(anyhow.version, "1");
        assert!(anyhow.features.is_empty());
    }

    #[test]
    // [verify format.features.optional]
    fn parse_optional_deps() {
        let manifest = indoc! {r#"
            [package]
            name = "test-battery-pack"
            version = "0.1.0"

            [dependencies]
            clap = { version = "4", features = ["derive"] }
            indicatif = { version = "0.17", optional = true }
        "#};

        let spec = parse_test(manifest).unwrap();
        assert!(!spec.crates["clap"].optional);
        assert!(spec.crates["indicatif"].optional);
    }

    #[test]
    // [verify format.features.grouping]
    fn parse_cargo_features() {
        let manifest = indoc! {r#"
            [package]
            name = "test-battery-pack"
            version = "0.1.0"

            [dependencies]
            clap = { version = "4", features = ["derive"], optional = true }
            dialoguer = { version = "0.11", optional = true }
            indicatif = { version = "0.17", optional = true }
            console = { version = "0.15", optional = true }

            [features]
            default = ["clap", "dialoguer"]
            indicators = ["indicatif", "console"]
        "#};

        let spec = parse_test(manifest).unwrap();
        assert_eq!(spec.features.len(), 2);
        assert_eq!(
            spec.features["default"],
            BTreeSet::from([
                FeatureRef::Feature("clap".into()),
                FeatureRef::Feature("dialoguer".into()),
            ])
        );
        assert_eq!(
            spec.features["indicators"],
            BTreeSet::from([
                FeatureRef::Feature("indicatif".into()),
                FeatureRef::Feature("console".into()),
            ])
        );
    }

    #[test]
    // [verify format.hidden.metadata]
    fn parse_hidden_deps() {
        let manifest = indoc! {r#"
            [package]
            name = "test-battery-pack"
            version = "0.1.0"

            [dependencies]
            serde = "1"
            serde_json = "1"
            serde_derive = "1"
            clap = "4"

            [package.metadata.battery-pack]
            hidden = ["serde*"]
        "#};

        let spec = parse_test(manifest).unwrap();
        assert_eq!(spec.hidden, BTreeSet::from(["serde*".to_string()]));
    }

    #[test]
    fn parse_templates() {
        let manifest = indoc! {r#"
            [package]
            name = "test-battery-pack"
            version = "0.1.0"

            [package.metadata.battery.templates]
            default = { path = "templates/default", description = "A basic starting point" }
            advanced = { path = "templates/advanced", description = "Full-featured setup" }
        "#};

        let spec = parse_test(manifest).unwrap();
        assert_eq!(spec.templates.len(), 2);
        assert_eq!(spec.templates["default"].path, "templates/default");
        assert_eq!(
            spec.templates["advanced"].description.as_deref(),
            Some("Full-featured setup")
        );
    }

    #[test]
    fn parse_description_and_repository() {
        let manifest = indoc! {r#"
            [package]
            name = "test-battery-pack"
            version = "0.1.0"
            description = "Error handling crates"
            repository = "https://github.com/example/repo"
        "#};

        let spec = parse_test(manifest).unwrap();
        assert_eq!(spec.description, "Error handling crates");
        assert_eq!(
            spec.repository.as_deref(),
            Some("https://github.com/example/repo")
        );
    }

    // -- Validation tests --

    #[test]
    // [verify format.crate.name]
    fn validate_name() {
        let manifest = indoc! {r#"
            [package]
            name = "test-battery-pack"
            version = "0.1.0"
        "#};
        let spec = parse_test(manifest).unwrap();
        assert!(spec.validate().is_ok());

        let manifest_bad = indoc! {r#"
            [package]
            name = "not-a-battery-pack-crate"
            version = "0.1.0"
        "#};
        let spec_bad = parse_test(manifest_bad).unwrap();
        let err = spec_bad.validate().unwrap_err();
        assert!(matches!(err, Error::InvalidName { .. }));
    }

    #[test]
    fn validate_features_reference_real_crates() {
        // Constructed manually: cargo metadata also rejects feature refs to
        // unknown crates, so this case can't reach `validate()` via parse_test.
        // The check still guards manually-constructed specs (e.g. from JSON state).
        let bad = BatteryPackSpec {
            name: "test-battery-pack".into(),
            version: "0.1.0".into(),
            description: String::new(),
            repository: None,
            keywords: vec![],
            crates: BTreeMap::from([(
                "clap".into(),
                CrateSpec {
                    version: "4".into(),
                    features: BTreeSet::new(),
                    dep_kind: DepKind::Normal,
                    optional: true,
                },
            )]),
            features: BTreeMap::from([(
                "default".into(),
                BTreeSet::from([
                    FeatureRef::Feature("clap".into()),
                    FeatureRef::Feature("nonexistent".into()),
                ]),
            )]),
            hidden: BTreeSet::new(),
            templates: BTreeMap::new(),
            categories: BTreeMap::new(),
            feature_meta: BTreeMap::new(),
            dep_meta: BTreeMap::new(),
        };
        let err = bad.validate().unwrap_err();
        assert!(matches!(err, Error::UnknownCrateInFeature { .. }));

        // Valid case (round-tripped through parse_test)
        let manifest_ok = indoc! {r#"
            [package]
            name = "test-battery-pack"
            version = "0.1.0"

            [dependencies]
            clap = { version = "4", optional = true }
            dialoguer = { version = "0.11", optional = true }

            [features]
            default = ["clap", "dialoguer"]
        "#};
        let spec_ok = parse_test(manifest_ok).unwrap();
        assert!(spec_ok.validate().is_ok());
    }

    // -- Resolution tests --

    #[test]
    // [verify format.features.default]
    fn resolve_default_feature() {
        let manifest = indoc! {r#"
            [package]
            name = "test-battery-pack"
            version = "0.1.0"

            [dependencies]
            clap = { version = "4", features = ["derive"], optional = true }
            dialoguer = { version = "0.11", optional = true }
            indicatif = { version = "0.17", optional = true }

            [features]
            default = ["clap", "dialoguer"]
            indicators = ["indicatif"]
        "#};

        let spec = parse_test(manifest).unwrap();
        let resolved = spec.resolve_crates(&[]);

        assert_eq!(resolved.len(), 2);
        assert!(resolved.contains_key("clap"));
        assert!(resolved.contains_key("dialoguer"));
        assert!(!resolved.contains_key("indicatif"));
    }

    #[test]
    // [verify format.features.default]
    fn resolve_no_default_feature() {
        let manifest = indoc! {r#"
            [package]
            name = "test-battery-pack"
            version = "0.1.0"

            [dependencies]
            clap = "4"
            dialoguer = "0.11"
            indicatif = { version = "0.17", optional = true }
        "#};

        let spec = parse_test(manifest).unwrap();
        // No features section at all
        let resolved = spec.resolve_crates(&[]);

        // All non-optional crates
        assert_eq!(resolved.len(), 2);
        assert!(resolved.contains_key("clap"));
        assert!(resolved.contains_key("dialoguer"));
        assert!(!resolved.contains_key("indicatif"));
    }

    #[test]
    // [verify format.features.additive]
    fn resolve_additive_features() {
        let manifest = indoc! {r#"
            [package]
            name = "test-battery-pack"
            version = "0.1.0"

            [dependencies]
            clap = { version = "4", optional = true }
            dialoguer = { version = "0.11", optional = true }
            indicatif = { version = "0.17", optional = true }
            console = { version = "0.15", optional = true }

            [features]
            default = ["clap", "dialoguer"]
            indicators = ["indicatif", "console"]
        "#};

        let spec = parse_test(manifest).unwrap();
        let resolved = spec.resolve_crates(&["default", "indicators"]);

        assert_eq!(resolved.len(), 4);
        assert!(resolved.contains_key("clap"));
        assert!(resolved.contains_key("dialoguer"));
        assert!(resolved.contains_key("indicatif"));
        assert!(resolved.contains_key("console"));
    }

    #[test]
    fn resolve_feature_without_default() {
        let manifest = indoc! {r#"
            [package]
            name = "test-battery-pack"
            version = "0.1.0"

            [dependencies]
            clap = { version = "4", optional = true }
            dialoguer = { version = "0.11", optional = true }
            indicatif = { version = "0.17", optional = true }

            [features]
            default = ["clap", "dialoguer"]
            indicators = ["indicatif"]
        "#};

        let spec = parse_test(manifest).unwrap();
        // Only indicators, no default
        let resolved = spec.resolve_crates(&["indicators"]);

        assert_eq!(resolved.len(), 1);
        assert!(resolved.contains_key("indicatif"));
        assert!(!resolved.contains_key("clap"));
    }

    #[test]
    // [verify format.features.augment]
    fn resolve_feature_augmentation() {
        let manifest = indoc! {r#"
            [package]
            name = "test-battery-pack"
            version = "0.1.0"

            [dependencies]
            tokio = { version = "1", features = ["macros", "rt"], optional = true }

            [features]
            default = ["tokio"]
            full = ["tokio"]
        "#};

        let spec = parse_test(manifest).unwrap();
        // Both default and full reference tokio — features should be merged
        let resolved = spec.resolve_crates(&["default", "full"]);

        assert_eq!(resolved.len(), 1);
        let tokio = &resolved["tokio"];
        assert!(tokio.features.contains("macros"));
        assert!(tokio.features.contains("rt"));
    }

    #[test]
    // [verify feature-refs.resolution.dep-feature]
    fn resolve_dep_feature_adds_per_dep_fx() {
        // Regression: `fancy = ["serde/derive"]` previously dropped the `derive`
        // feature and resolved bare `serde`

        let manifest = indoc! {r#"
          [package]
          name = "test-battery-pack"
          version = "0.1.0"

          [dependencies]
          serde = { version = "1", optional = true }

          [features]
          fancy = ["serde/derive"]
          "#};

        let spec = parse_test(manifest).unwrap();
        let resolved = spec.resolve_crates(&["fancy"]);

        assert_eq!(resolved.len(), 1);
        assert!(resolved["serde"].features.contains("derive"));
    }

    #[test]
    // [verify feature-refs.resolution.weak]
    fn resolve_weak_alone_does_not_activate_dep() {
        let manifest = indoc! {r#"
        [package]
        name = "test-battery-pack"
        version = "0.1.0"

        [dependencies]
        serde = { version = "1", optional = true }

        [features]
        weak = ["serde?/derive"]
        "#};

        let spec = parse_test(manifest).unwrap();
        let resolved = spec.resolve_crates(&["weak"]);

        // Weak-only must not pull `serde` in.
        assert!(!resolved.contains_key("serde"));
    }

    #[test]
    // [verify feature-refs.resolution.weak]
    fn resolve_weak_adds_feature_when_dep_otherwise_activated() {
        let manifest = indoc! {r#"
        [package]
        name = "test-battery-pack"
        version = "0.1.0"

        [dependencies]
        serde = { version = "1", optional = true }

        [features]
        weak = ["serde?/derive"]
        strong = ["serde"]
        "#};

        let spec = parse_test(manifest).unwrap();
        let resolved = spec.resolve_crates(&["weak", "strong"]);

        // `strong` activates serde; `weak` then adds the `derive` feature.
        assert!(resolved["serde"].features.contains("derive"));
    }

    #[test]
    // [verify feature-refs.resolution.recursion]
    fn resolve_recurses_through_local_features() {
        let manifest = indoc! {r#"
        [package]
        name = "test-battery-pack"
        version = "0.1.0"

        [dependencies]
        serde = { version = "1", optional = true }

        [features]
        fancy = ["super-fancy"]
        super-fancy = ["serde/derive"]
        "#};

        let spec = parse_test(manifest).unwrap();
        let resolved = spec.resolve_crates(&["fancy"]);

        assert!(resolved["serde"].features.contains("derive"));
    }

    #[test]
    // [verify feature-refs.validation.cycles]
    fn validate_rejects_feature_cyc() {
        let bad = BatteryPackSpec {
            name: "test-battery-pack".into(),
            version: "0.1.0".into(),
            description: String::new(),
            repository: None,
            keywords: vec![],
            crates: BTreeMap::new(),
            features: BTreeMap::from([
                (
                    "a".to_string(),
                    BTreeSet::from([FeatureRef::Feature("b".into())]),
                ),
                (
                    "b".to_string(),
                    BTreeSet::from([FeatureRef::Feature("a".into())]),
                ),
            ]),
            hidden: BTreeSet::new(),
            templates: BTreeMap::new(),
            categories: BTreeMap::new(),
            feature_meta: BTreeMap::new(),
            dep_meta: BTreeMap::new(),
        };

        let err = bad.validate().unwrap_err();
        assert!(matches!(err, Error::FeatureCycle { .. }))
    }

    #[test]
    fn resolve_all() {
        let manifest = indoc! {r#"
            [package]
            name = "test-battery-pack"
            version = "0.1.0"

            [dependencies]
            clap = { version = "4", optional = true }
            indicatif = { version = "0.17", optional = true }

            [dev-dependencies]
            insta = "1.34"

            [features]
            default = ["clap"]
        "#};

        let spec = parse_test(manifest).unwrap();
        let all = spec.resolve_all();

        // Everything including optional and dev-deps
        assert_eq!(all.len(), 3);
        assert!(all.contains_key("clap"));
        assert!(all.contains_key("indicatif"));
        assert!(all.contains_key("insta"));
    }

    // -- Hidden dep tests --

    #[test]
    // [verify format.hidden.effect]
    fn hidden_exact_match() {
        let manifest = indoc! {r#"
            [package]
            name = "test-battery-pack"
            version = "0.1.0"

            [dependencies]
            serde = "1"
            clap = "4"

            [package.metadata.battery-pack]
            hidden = ["serde"]
        "#};

        let spec = parse_test(manifest).unwrap();
        assert!(spec.is_hidden("serde"));
        assert!(!spec.is_hidden("clap"));
    }

    #[test]
    // [verify format.hidden.glob]
    fn hidden_glob_pattern() {
        let manifest = indoc! {r#"
            [package]
            name = "test-battery-pack"
            version = "0.1.0"

            [dependencies]
            serde = "1"
            serde_json = "1"
            serde_derive = "1"
            clap = "4"

            [package.metadata.battery-pack]
            hidden = ["serde*"]
        "#};

        let spec = parse_test(manifest).unwrap();
        assert!(spec.is_hidden("serde"));
        assert!(spec.is_hidden("serde_json"));
        assert!(spec.is_hidden("serde_derive"));
        assert!(!spec.is_hidden("clap"));
    }

    #[test]
    // [verify format.hidden.wildcard]
    fn hidden_wildcard_all() {
        let manifest = indoc! {r#"
            [package]
            name = "test-battery-pack"
            version = "0.1.0"

            [dependencies]
            serde = "1"
            clap = "4"

            [package.metadata.battery-pack]
            hidden = ["*"]
        "#};

        let spec = parse_test(manifest).unwrap();
        assert!(spec.is_hidden("serde"));
        assert!(spec.is_hidden("clap"));
        assert!(spec.is_hidden("anything"));
    }

    #[test]
    fn visible_crates_filters_hidden() {
        let manifest = indoc! {r#"
            [package]
            name = "test-battery-pack"
            version = "0.1.0"

            [dependencies]
            serde = "1"
            serde_json = "1"
            clap = "4"
            anyhow = "1"

            [package.metadata.battery-pack]
            hidden = ["serde*"]
        "#};

        let spec = parse_test(manifest).unwrap();
        let visible = spec.visible_crates();

        assert_eq!(visible.len(), 2);
        assert!(visible.contains_key("clap"));
        assert!(visible.contains_key("anyhow"));
        assert!(!visible.contains_key("serde"));
        assert!(!visible.contains_key("serde_json"));
    }

    // [verify tui.installed.hidden]
    // [verify tui.browse.hidden]
    #[test]
    fn all_crates_with_grouping_filters_hidden() {
        let manifest = indoc! {r#"
            [package]
            name = "test-battery-pack"
            version = "0.1.0"

            [dependencies]
            serde = "1"
            serde_json = "1"
            clap = "4"
            anyhow = "1"

            [package.metadata.battery-pack]
            hidden = ["serde*"]
        "#};

        let spec = parse_test(manifest).unwrap();
        let grouped = spec.all_crates_with_grouping();
        let names: Vec<&str> = grouped.iter().map(|(_, n, _, _)| n.as_str()).collect();
        assert!(names.contains(&"clap"));
        assert!(names.contains(&"anyhow"));
        assert!(!names.contains(&"serde"), "hidden crate must be excluded");
        assert!(
            !names.contains(&"serde_json"),
            "hidden crate must be excluded"
        );
    }

    // -- Glob matching unit tests --

    #[test]
    fn glob_match_basics() {
        assert!(glob_match("*", "anything"));
        assert!(glob_match("serde*", "serde"));
        assert!(glob_match("serde*", "serde_json"));
        assert!(glob_match("serde*", "serde_derive"));
        assert!(!glob_match("serde*", "clap"));

        assert!(glob_match("*-sys", "openssl-sys"));
        assert!(!glob_match("*-sys", "openssl"));

        assert!(glob_match("?lap", "clap"));
        assert!(!glob_match("?lap", "claps"));

        assert!(glob_match("exact", "exact"));
        assert!(!glob_match("exact", "exacto"));
    }

    // -- Error type tests --

    #[test]
    fn error_on_invalid_toml() {
        // cargo metadata rejects unparsable manifests before our parser runs.
        let result = parse_test("not valid toml [[[");
        assert!(matches!(result, Err(Error::Metadata(_))));
    }

    #[test]
    fn error_on_missing_package() {
        // cargo metadata rejects manifests without [package].
        let result = parse_test("[dependencies]\nfoo = \"1\"");
        assert!(matches!(result, Err(Error::Metadata(_))));
    }

    // -- Comprehensive battery pack test --

    #[test]
    fn full_battery_pack_parse() {
        let manifest = indoc! {r#"
            [package]
            name = "cli-battery-pack"
            version = "0.3.0"
            description = "CLI essentials for Rust applications"
            repository = "https://github.com/battery-pack-rs/battery-pack"
            keywords = ["battery-pack"]

            [dependencies]
            clap = { version = "4", features = ["derive"], optional = true }
            dialoguer = { version = "0.11", optional = true }
            indicatif = { version = "0.17", optional = true }
            console = { version = "0.15", optional = true }

            [dev-dependencies]
            assert_cmd = "2.0"

            [build-dependencies]
            cc = "1.0"

            [features]
            default = ["clap", "dialoguer"]
            indicators = ["indicatif", "console"]
            fancy = ["clap", "indicatif", "console"]

            [package.metadata.battery-pack]
            hidden = ["cc"]

            [package.metadata.battery.templates]
            default = { path = "templates/default", description = "Basic CLI app" }
        "#};

        let spec = parse_test(manifest).unwrap();
        assert!(spec.validate().is_ok());

        // Basic fields
        assert_eq!(spec.name, "cli-battery-pack");
        assert_eq!(spec.version, "0.3.0");
        assert_eq!(spec.description, "CLI essentials for Rust applications");

        // Crates from all sections
        assert_eq!(spec.crates.len(), 6);
        assert_eq!(spec.crates["clap"].dep_kind, DepKind::Normal);
        assert_eq!(spec.crates["assert_cmd"].dep_kind, DepKind::Dev);
        assert_eq!(spec.crates["cc"].dep_kind, DepKind::Build);

        // Optional (clap is now optional so default-feature gating is meaningful)
        assert!(spec.crates["indicatif"].optional);
        assert!(spec.crates["clap"].optional);

        // Features
        assert_eq!(spec.features.len(), 3);

        // Hidden
        assert!(spec.is_hidden("cc"));
        assert!(!spec.is_hidden("clap"));

        // Visible
        let visible = spec.visible_crates();
        assert_eq!(visible.len(), 5); // 6 total - 1 hidden (cc)

        // Templates
        assert_eq!(spec.templates.len(), 1);

        // Resolution: default (+ non-optional, non-hidden dev/build deps)
        let default = spec.resolve_crates(&[]);
        assert_eq!(default.len(), 3);
        assert!(default.contains_key("clap"));
        assert!(default.contains_key("dialoguer"));
        assert!(default.contains_key("assert_cmd"));

        // Resolution: default + indicators
        let with_indicators = spec.resolve_crates(&["default", "indicators"]);
        assert_eq!(with_indicators.len(), 5);

        // Resolution: only indicators (no default)
        let only_indicators = spec.resolve_crates(&["indicators"]);
        assert_eq!(only_indicators.len(), 3);
        assert!(only_indicators.contains_key("indicatif"));
        assert!(only_indicators.contains_key("console"));
        assert!(only_indicators.contains_key("assert_cmd"));

        // Resolution: all
        let all = spec.resolve_all();
        assert_eq!(all.len(), 6);
    }

    // -- Discovery tests --

    #[test]
    // [verify cli.source.discover]
    fn discover_battery_packs_in_fixture_workspace() {
        // Find the fixtures directory relative to the workspace root
        let manifest_dir = Path::new(env!("CARGO_MANIFEST_DIR"));
        let workspace_root = manifest_dir
            .parent()
            .unwrap()
            .parent()
            .unwrap()
            .parent()
            .unwrap();
        let fixtures_dir = workspace_root.join("tests/fixtures");

        let packs = discover_battery_packs(&fixtures_dir).unwrap();

        assert_eq!(packs.len(), 9);

        let names: Vec<&str> = packs.iter().map(|p| p.name.as_str()).collect();
        assert!(names.contains(&"basic-battery-pack"));
        assert!(names.contains(&"fancy-battery-pack"));
        assert!(names.contains(&"broken-battery-pack"));
        assert!(names.contains(&"managed-battery-pack"));
        assert!(names.contains(&"feature-syntax-battery-pack"));
        assert!(names.contains(&"optional-feature-battery-pack"));
        assert!(names.contains(&"mixed-kinds-battery-pack"));
        assert!(names.contains(&"implicit-feature-battery-pack"));
        assert!(names.contains(&"category-battery-pack"));

        // Verify basic-battery-pack
        let basic = packs
            .iter()
            .find(|p| p.name == "basic-battery-pack")
            .unwrap();
        assert_eq!(basic.version, "0.1.0");
        assert_eq!(basic.crates.len(), 3); // anyhow, thiserror, eyre
        assert!(basic.crates["eyre"].optional);
        assert!(basic.crates["anyhow"].optional);

        // Verify fancy-battery-pack
        let fancy = packs
            .iter()
            .find(|p| p.name == "fancy-battery-pack")
            .unwrap();
        assert_eq!(fancy.version, "0.2.0");
        assert!(fancy.is_hidden("serde"));
        assert!(fancy.is_hidden("serde_json"));
        assert!(fancy.is_hidden("cc"));
        assert!(!fancy.is_hidden("clap"));
        assert_eq!(fancy.templates.len(), 2);

        // fancy default resolution (+ non-hidden dev/build deps)
        let default = fancy.resolve_crates(&[]);
        assert_eq!(default.len(), 4);
        assert!(default.contains_key("clap"));
        assert!(default.contains_key("dialoguer"));
        assert!(default.contains_key("assert_cmd"));
        assert!(default.contains_key("predicates"));

        // fancy visible crates (hidden: serde, serde_json, cc)
        let visible = fancy.visible_crates();
        assert!(!visible.contains_key("serde"));
        assert!(!visible.contains_key("serde_json"));
        assert!(!visible.contains_key("cc"));
        assert!(visible.contains_key("clap"));

        // Verify managed-battery-pack
        let managed = packs
            .iter()
            .find(|p| p.name == "managed-battery-pack")
            .unwrap();
        assert_eq!(managed.version, "0.2.0");
        assert_eq!(managed.crates.len(), 4); // anyhow, clap, insta, cc
        assert!(managed.crates["anyhow"].optional);
        assert!(managed.crates["clap"].optional);
        assert_eq!(managed.templates.len(), 1);
        let default = managed.resolve_crates(&[]);
        assert_eq!(default.len(), 4);
        assert!(default.contains_key("anyhow"));
        assert!(default.contains_key("clap"));
        assert!(default.contains_key("insta"));
        assert!(default.contains_key("cc"));
    }

    #[test]
    // [verify cli.source.discover] workspace case — member crate discovers siblings
    fn discover_battery_packs_finds_workspace() {
        let manifest_dir = Path::new(env!("CARGO_MANIFEST_DIR"));
        let workspace_root = manifest_dir
            .parent()
            .unwrap()
            .parent()
            .unwrap()
            .parent()
            .unwrap();
        let member = workspace_root.join("tests/fixtures/basic-battery-pack");

        let packs = discover_battery_packs(&member).unwrap();
        assert_eq!(packs.len(), 9);
        let names: Vec<&str> = packs.iter().map(|p| p.name.as_str()).collect();
        assert!(names.contains(&"basic-battery-pack"));
        assert!(names.contains(&"fancy-battery-pack"));
    }

    /// [verify cli.source.discover] negative case -- non-battery-pack members are excluded.
    #[test]
    fn discover_battery_packs_excludes_non_battery_pack_members() {
        let mut fx = WorkspaceFixture::new();
        fx.add_pack(
            "cli-pack",
            indoc! {r#"
        [package]
        name = "cli-battery-pack"
        version = "0.1.0"
        "#},
        );
        fx.add_pack(
            "helper",
            indoc! {r#"
        [package]
        name = "regular-helper"
        version = "0.1.0"
        "#},
        );

        let root = fx.finalize();
        let packs = discover_battery_packs(root).unwrap();

        // Snapshot: only the BP member survives the filter; the helper is dropped.
        let summary = render_discovered(&packs);
        snapbox::assert_data_eq!(summary, snapbox::str![[r#"cli-battery-pack 0.1.0"#]]);

        // Point assertions retained for direct failure messages.
        let names = packs.iter().map(|pk| pk.name.as_str()).collect::<Vec<_>>();
        assert!(names.contains(&"cli-battery-pack"));
        assert!(!names.contains(&"regular-helper"));
    }

    /// [verify cli.source.discover] glob members - `members = ["crates/*"]` is expanded
    #[test]
    fn discover_bp_handles_glob_members() {
        let tmp = tempfile::tempdir().unwrap();
        let root = tmp.path();

        // Pure workspace root using a glob members pattern.
        fs::write(
            root.join("Cargo.toml"),
            indoc! {r#"
                [workspace]
                resolver = "2"
                members = ["crates/*"]
            "#},
        )
        .unwrap();

        // Two battery-pack members under crates/ for the glob to expand to.
        for name in ["foo-battery-pack", "bar-battery-pack"] {
            let pkg_dir = root.join("crates").join(name);
            fs::create_dir_all(pkg_dir.join("src")).unwrap();
            fs::write(pkg_dir.join("src").join("lib.rs"), "").unwrap();
            fs::write(
                pkg_dir.join("Cargo.toml"),
                format!(
                    indoc! {r#"
                        [package]
                        name = "{name}"
                        version = "0.1.0"
                    "#},
                    name = name
                ),
            )
            .unwrap();
        }

        let packs = discover_battery_packs(root).unwrap();

        // Snapshot: glob expanded to both members.
        let summary = render_discovered(&packs);
        snapbox::assert_data_eq!(
            summary,
            snapbox::str![[r#"
bar-battery-pack 0.1.0
foo-battery-pack 0.1.0
"#]]
        );

        // Point assertions retained for direct failure messages.
        let names = packs.iter().map(|pk| pk.name.as_str()).collect::<Vec<_>>();

        assert!(names.contains(&"bar-battery-pack"));
        assert!(names.contains(&"foo-battery-pack"));
    }

    /// Render discovered packs as a stable, sorted `name version` listing for snapshots.
    fn render_discovered(packs: &[BatteryPackSpec]) -> String {
        let mut lines: Vec<String> = packs
            .iter()
            .map(|p| format!("{} {}", p.name, p.version))
            .collect();
        lines.sort();
        lines.join("\n")
    }

    /// Render a resolved crate map as a `name version` listing for snapshots
    fn render_resolved(crates: &BTreeMap<String, CrateSpec>) -> String {
        crates
            .iter()
            .map(|(name, spec)| format!("{} {}", name, spec.version))
            .collect::<Vec<_>>()
            .join("\n")
    }

    #[test]
    // [verify cli.source.discover] standalone case — no workspace, parses crate directly
    fn discover_battery_packs_standalone() {
        let tmp = tempfile::tempdir().unwrap();
        std::fs::write(
            tmp.path().join("Cargo.toml"),
            indoc! {r#"
                [package]
                name = "solo-battery-pack"
                version = "1.0.0"

                [features]
                default = ["dep:tokio"]

                [dependencies]
                tokio = { version = "1", optional = true }
            "#},
        )
        .unwrap();
        std::fs::create_dir(tmp.path().join("src")).unwrap();
        std::fs::write(tmp.path().join("src/lib.rs"), "").unwrap();

        let packs = discover_battery_packs(tmp.path()).unwrap();

        // Snapshot: standalone crate is treated as a 1-member workspace.
        snapbox::assert_data_eq!(
            render_discovered(&packs),
            snapbox::str![[r#"solo-battery-pack 1.0.0"#]]
        );

        // Point assertions retained.
        assert_eq!(packs.len(), 1);
        assert_eq!(packs[0].name, "solo-battery-pack");
        assert_eq!(packs[0].version, "1.0.0");
    }

    #[test]
    fn discover_battery_packs_includes_battery_pack_itself() {
        // battery-pack (the framework crate) should be discoverable from its
        // own directory, so bp-managed self-references resolve correctly.
        let manifest_dir = Path::new(env!("CARGO_MANIFEST_DIR"));
        let bp_crate = manifest_dir.parent().unwrap();

        let packs = discover_battery_packs(bp_crate).unwrap();
        let names: Vec<&str> = packs.iter().map(|p| p.name.as_str()).collect();
        assert!(
            names.contains(&"battery-pack"),
            "battery-pack should be discoverable, found: {:?}",
            names
        );
    }

    // -- validate_spec tests --

    #[test]
    // [verify format.crate.name]
    fn validate_spec_name() {
        let good = parse_test(indoc! {r#"
            [package]
            name = "test-battery-pack"
            version = "0.1.0"
            repository = "https://github.com/example/test"
            keywords = ["battery-pack"]
        "#})
        .unwrap();
        assert!(good.validate_spec().is_clean());

        let exact = parse_test(indoc! {r#"
            [package]
            name = "battery-pack"
            version = "0.1.0"
            repository = "https://github.com/example/test"
            keywords = ["battery-pack"]
        "#})
        .unwrap();
        assert!(exact.validate_spec().is_clean());

        let bad = parse_test(indoc! {r#"
            [package]
            name = "not-a-pack"
            version = "0.1.0"
            keywords = ["battery-pack"]
        "#})
        .unwrap();
        let report = bad.validate_spec();
        assert!(report.has_errors());
        assert!(
            report
                .diagnostics
                .iter()
                .any(|d| d.rule == "format.crate.name")
        );
    }

    #[test]
    // [verify format.crate.keyword]
    fn validate_spec_keyword() {
        let good = parse_test(indoc! {r#"
            [package]
            name = "test-battery-pack"
            version = "0.1.0"
            repository = "https://github.com/example/test"
            keywords = ["battery-pack", "helpers"]
        "#})
        .unwrap();
        assert!(good.validate_spec().is_clean());

        let missing = parse_test(indoc! {r#"
            [package]
            name = "test-battery-pack"
            version = "0.1.0"
        "#})
        .unwrap();
        let report = missing.validate_spec();
        assert!(report.has_errors());
        assert!(
            report
                .diagnostics
                .iter()
                .any(|d| d.rule == "format.crate.keyword")
        );

        let wrong = parse_test(indoc! {r#"
            [package]
            name = "test-battery-pack"
            version = "0.1.0"
            keywords = ["cli", "helpers"]
        "#})
        .unwrap();
        let report = wrong.validate_spec();
        assert!(report.has_errors());
        assert!(
            report
                .diagnostics
                .iter()
                .any(|d| d.rule == "format.crate.keyword")
        );
    }

    #[test]
    // [verify format.features.grouping]
    fn validate_spec_features() {
        let good = parse_test(indoc! {r#"
            [package]
            name = "test-battery-pack"
            version = "0.1.0"
            repository = "https://github.com/example/test"
            keywords = ["battery-pack"]

            [dependencies]
            clap = { version = "4", optional = true }

            [features]
            default = ["clap"]
        "#})
        .unwrap();
        assert!(good.validate_spec().is_clean());

        // Constructed manually: cargo metadata rejects features referencing unknown crates
        // so this case can't be reached through parse_test.
        let bad = BatteryPackSpec {
            name: "test-battery-pack".into(),
            version: "0.1.0".into(),
            description: String::new(),
            repository: None,
            keywords: vec!["battery-pack".into()],
            crates: BTreeMap::from([(
                "clap".into(),
                CrateSpec {
                    version: "4".into(),
                    features: BTreeSet::new(),
                    dep_kind: DepKind::Normal,
                    optional: true,
                },
            )]),
            features: BTreeMap::from([(
                "default".into(),
                BTreeSet::from([
                    FeatureRef::Feature("clap".into()),
                    FeatureRef::Feature("ghost".into()),
                ]),
            )]),
            hidden: BTreeSet::new(),
            templates: BTreeMap::new(),
            categories: BTreeMap::new(),
            feature_meta: BTreeMap::new(),
            dep_meta: BTreeMap::new(),
        };
        let report = bad.validate_spec();
        assert!(report.has_errors());
        assert!(
            report
                .diagnostics
                .iter()
                .any(|d| d.rule == "format.features.grouping" && d.message.contains("ghost"))
        );
    }

    // -- validate_on_disk tests --

    #[test]
    // [verify format.crate.lib]
    fn validate_lib_rs_clean() {
        let dir = tempfile::tempdir().unwrap();
        let src = dir.path().join("src");
        std::fs::create_dir(&src).unwrap();
        std::fs::write(
            src.join("lib.rs"),
            "//! Doc comment\n\n// Regular comment\n",
        )
        .unwrap();

        let spec = parse_test(indoc! {r#"
            [package]
            name = "test-battery-pack"
            version = "0.1.0"
            keywords = ["battery-pack"]
        "#})
        .unwrap();

        let report = validate_on_disk(&spec, dir.path());
        assert!(report.is_clean());
    }

    #[test]
    // [verify format.crate.lib]
    fn validate_lib_rs_with_code() {
        let dir = tempfile::tempdir().unwrap();
        let src = dir.path().join("src");
        std::fs::create_dir(&src).unwrap();
        std::fs::write(src.join("lib.rs"), "//! Doc comment\npub fn hello() {}\n").unwrap();

        let spec = parse_test(indoc! {r#"
            [package]
            name = "test-battery-pack"
            version = "0.1.0"
            keywords = ["battery-pack"]
        "#})
        .unwrap();

        let report = validate_on_disk(&spec, dir.path());
        assert!(!report.is_clean());
        assert!(!report.has_errors()); // It's a warning, not an error
        assert!(
            report
                .diagnostics
                .iter()
                .any(|d| d.rule == "format.crate.lib" && d.severity == Severity::Warning)
        );
    }

    #[test]
    // [verify format.crate.no-code]
    fn validate_no_extra_rs_files() {
        let dir = tempfile::tempdir().unwrap();
        let src = dir.path().join("src");
        std::fs::create_dir(&src).unwrap();
        std::fs::write(src.join("lib.rs"), "//! Doc\n").unwrap();

        let spec = parse_test(indoc! {r#"
            [package]
            name = "test-battery-pack"
            version = "0.1.0"
            keywords = ["battery-pack"]
        "#})
        .unwrap();

        // Clean case — only lib.rs
        let report = validate_on_disk(&spec, dir.path());
        assert!(report.is_clean());

        // Add an extra .rs file
        std::fs::write(src.join("helper.rs"), "pub fn help() {}\n").unwrap();
        let report = validate_on_disk(&spec, dir.path());
        assert!(report.has_errors());
        assert!(
            report
                .diagnostics
                .iter()
                .any(|d| d.rule == "format.crate.no-code" && d.message.contains("helper.rs"))
        );
    }

    #[test]
    // [verify format.templates.directory]
    fn validate_templates_exist() {
        let dir = tempfile::tempdir().unwrap();
        let src = dir.path().join("src");
        std::fs::create_dir(&src).unwrap();
        std::fs::write(src.join("lib.rs"), "//! Doc\n").unwrap();

        let spec = parse_test(indoc! {r#"
            [package]
            name = "test-battery-pack"
            version = "0.1.0"
            keywords = ["battery-pack"]

            [package.metadata.battery.templates]
            default = { path = "templates/default", description = "Basic" }
        "#})
        .unwrap();

        // Missing template directory
        let report = validate_on_disk(&spec, dir.path());
        assert!(report.has_errors());
        assert!(
            report
                .diagnostics
                .iter()
                .any(|d| d.rule == "format.templates.directory")
        );

        // Create the directory — should now be clean
        let tmpl = dir.path().join("templates/default");
        std::fs::create_dir_all(&tmpl).unwrap();
        let report = validate_on_disk(&spec, dir.path());
        let template_errors: Vec<_> = report
            .diagnostics
            .iter()
            .filter(|d| d.rule.starts_with("format.templates."))
            .collect();
        assert!(template_errors.is_empty());
    }

    #[test]
    fn validate_templates_cargo_toml_rejected() {
        let dir = tempfile::tempdir().unwrap();
        let src = dir.path().join("src");
        std::fs::create_dir(&src).unwrap();
        std::fs::write(src.join("lib.rs"), "//! Doc\n").unwrap();

        let spec = parse_test(indoc! {r#"
            [package]
            name = "test-battery-pack"
            version = "0.1.0"
            keywords = ["battery-pack"]

            [package.metadata.battery.templates]
            default = { path = "templates/default", description = "Basic" }
        "#})
        .unwrap();

        let tmpl = dir.path().join("templates/default");
        std::fs::create_dir_all(&tmpl).unwrap();
        std::fs::write(tmpl.join("Cargo.toml"), "[package]\nname = \"x\"\n").unwrap();

        let report = validate_on_disk(&spec, dir.path());
        assert!(report.has_errors());
        assert!(
            report
                .diagnostics
                .iter()
                .any(|d| d.rule == "format.templates.cargo-toml"
                    && d.message.contains("_Cargo.toml"))
        );
    }

    #[test]
    fn validate_templates_underscore_cargo_toml_accepted() {
        let dir = tempfile::tempdir().unwrap();
        let src = dir.path().join("src");
        std::fs::create_dir(&src).unwrap();
        std::fs::write(src.join("lib.rs"), "//! Doc\n").unwrap();

        let spec = parse_test(indoc! {r#"
            [package]
            name = "test-battery-pack"
            version = "0.1.0"
            keywords = ["battery-pack"]

            [package.metadata.battery.templates]
            default = { path = "templates/default", description = "Basic" }
        "#})
        .unwrap();

        let tmpl = dir.path().join("templates/default");
        std::fs::create_dir_all(&tmpl).unwrap();
        std::fs::write(tmpl.join("_Cargo.toml"), "[package]\nname = \"x\"\n").unwrap();

        let report = validate_on_disk(&spec, dir.path());
        let cargo_toml_errors: Vec<_> = report
            .diagnostics
            .iter()
            .filter(|d| d.rule == "format.templates.cargo-toml")
            .collect();
        assert!(cargo_toml_errors.is_empty());
    }

    // -- Repository warning tests --

    #[test]
    // [verify format.crate.repository]
    fn validate_warns_on_missing_repository() {
        let spec = parse_test(indoc! {r#"
            [package]
            name = "test-battery-pack"
            version = "0.1.0"
            keywords = ["battery-pack"]
        "#})
        .unwrap();
        let report = spec.validate_spec();
        assert!(
            !report.has_errors(),
            "missing repository should not be an error"
        );
        assert!(
            report
                .diagnostics
                .iter()
                .any(|d| d.rule == "format.crate.repository" && d.severity == Severity::Warning),
            "should warn when repository is missing"
        );
    }

    #[test]
    // [verify format.crate.repository]
    fn validate_no_warning_when_repository_present() {
        let spec = parse_test(indoc! {r#"
            [package]
            name = "test-battery-pack"
            version = "0.1.0"
            repository = "https://github.com/example/repo"
            keywords = ["battery-pack"]
        "#})
        .unwrap();
        let report = spec.validate_spec();
        assert!(
            !report
                .diagnostics
                .iter()
                .any(|d| d.rule == "format.crate.repository"),
            "should not warn when repository is present"
        );
    }

    // -- Fixture integration tests --

    #[test]
    fn validate_fixture_basic_battery_pack() {
        let manifest_dir = Path::new(env!("CARGO_MANIFEST_DIR"));
        let workspace_root = manifest_dir
            .parent()
            .unwrap()
            .parent()
            .unwrap()
            .parent()
            .unwrap();
        let fixture = workspace_root.join("tests/fixtures/basic-battery-pack");

        let content = std::fs::read_to_string(fixture.join("Cargo.toml")).unwrap();
        let spec = parse_test(&content).unwrap();

        let mut report = spec.validate_spec();
        report.merge(validate_on_disk(&spec, &fixture));
        // basic-battery-pack has no repository — expect a warning but no errors
        assert!(
            !report.has_errors(),
            "basic-battery-pack should have no errors: {:?}",
            report.diagnostics
        );
        assert!(
            report
                .diagnostics
                .iter()
                .any(|d| d.rule == "format.crate.repository" && d.severity == Severity::Warning),
            "basic-battery-pack should warn about missing repository"
        );
    }

    #[test]
    fn validate_fixture_fancy_battery_pack() {
        let manifest_dir = Path::new(env!("CARGO_MANIFEST_DIR"));
        let workspace_root = manifest_dir
            .parent()
            .unwrap()
            .parent()
            .unwrap()
            .parent()
            .unwrap();
        let fixture = workspace_root.join("tests/fixtures/fancy-battery-pack");

        let content = std::fs::read_to_string(fixture.join("Cargo.toml")).unwrap();
        let spec = parse_test(&content).unwrap();

        let mut report = spec.validate_spec();
        report.merge(validate_on_disk(&spec, &fixture));
        assert!(
            report.is_clean(),
            "fancy-battery-pack should be clean: {:?}",
            report.diagnostics
        );
    }

    #[test]
    fn validate_fixture_broken_battery_pack() {
        let manifest_dir = Path::new(env!("CARGO_MANIFEST_DIR"));
        let workspace_root = manifest_dir
            .parent()
            .unwrap()
            .parent()
            .unwrap()
            .parent()
            .unwrap();
        let fixture = workspace_root.join("tests/fixtures/broken-battery-pack");

        let content = std::fs::read_to_string(fixture.join("Cargo.toml")).unwrap();
        let spec = parse_test(&content).unwrap();

        let mut report = spec.validate_spec();
        report.merge(validate_on_disk(&spec, &fixture));

        assert!(report.has_errors());

        let rules: Vec<&str> = report.diagnostics.iter().map(|d| d.rule).collect();
        assert!(
            rules.contains(&"format.crate.keyword"),
            "missing keyword error"
        );
        // Note: format.features.grouping can't be triggered from a fixture because
        // cargo itself rejects features that reference nonexistent dependencies.
        assert!(
            rules.contains(&"format.crate.no-code"),
            "missing no-code error"
        );
        assert!(
            rules.contains(&"format.templates.directory"),
            "missing template dir error"
        );

        // lib.rs has code — should be a warning
        assert!(
            report
                .diagnostics
                .iter()
                .any(|d| d.rule == "format.crate.lib" && d.severity == Severity::Warning)
        );
    }

    #[test]
    fn validate_fixture_managed_battery_pack() {
        let manifest_dir = Path::new(env!("CARGO_MANIFEST_DIR"));
        let workspace_root = manifest_dir
            .parent()
            .unwrap()
            .parent()
            .unwrap()
            .parent()
            .unwrap();
        let fixture = workspace_root.join("tests/fixtures/managed-battery-pack");

        let content = std::fs::read_to_string(fixture.join("Cargo.toml")).unwrap();
        let spec = parse_test(&content).unwrap();

        let mut report = spec.validate_spec();
        report.merge(validate_on_disk(&spec, &fixture));
        assert!(
            report.is_clean(),
            "managed-battery-pack should be clean: {:?}",
            report.diagnostics
        );
    }

    // -- Cross-pack merging tests --

    /// Helper to build a CrateSpec quickly in tests.
    fn crate_spec(version: &str, features: &[&str], dep_kind: DepKind) -> CrateSpec {
        CrateSpec {
            version: version.to_string(),
            features: features
                .iter()
                .map(|s| s.to_string())
                .collect::<BTreeSet<_>>(),
            dep_kind,
            optional: false,
        }
    }

    #[test]
    // [verify manifest.merge.version]
    fn merge_version_newest_wins() {
        let pack_a = BTreeMap::from([(
            "serde".to_string(),
            crate_spec("1.0.100", &["derive"], DepKind::Normal),
        )]);
        let pack_b = BTreeMap::from([(
            "serde".to_string(),
            crate_spec("1.0.210", &["derive"], DepKind::Normal),
        )]);

        let merged = merge_crate_specs(&[pack_a, pack_b]);
        assert_eq!(merged["serde"].version, "1.0.210");
    }

    #[test]
    // [verify manifest.merge.version]
    fn merge_version_across_major() {
        let pack_a = BTreeMap::from([(
            "clap".to_string(),
            crate_spec("3.4.0", &[], DepKind::Normal),
        )]);
        let pack_b = BTreeMap::from([(
            "clap".to_string(),
            crate_spec("4.5.0", &[], DepKind::Normal),
        )]);

        let merged = merge_crate_specs(&[pack_a, pack_b]);
        assert_eq!(merged["clap"].version, "4.5.0");
    }

    #[test]
    // [verify manifest.merge.version]
    fn merge_version_same_version_no_conflict() {
        let pack_a = BTreeMap::from([(
            "anyhow".to_string(),
            crate_spec("1.0.80", &[], DepKind::Normal),
        )]);
        let pack_b = BTreeMap::from([(
            "anyhow".to_string(),
            crate_spec("1.0.80", &[], DepKind::Normal),
        )]);

        let merged = merge_crate_specs(&[pack_a, pack_b]);
        assert_eq!(merged["anyhow"].version, "1.0.80");
    }

    #[test]
    // [verify manifest.merge.features]
    fn merge_features_union() {
        let pack_a = BTreeMap::from([(
            "tokio".to_string(),
            crate_spec("1", &["macros", "rt"], DepKind::Normal),
        )]);
        let pack_b = BTreeMap::from([(
            "tokio".to_string(),
            crate_spec("1", &["rt", "net", "io-util"], DepKind::Normal),
        )]);

        let merged = merge_crate_specs(&[pack_a, pack_b]);
        let features = &merged["tokio"].features;
        assert!(features.contains(&"macros".to_string()));
        assert!(features.contains(&"rt".to_string()));
        assert!(features.contains(&"net".to_string()));
        assert!(features.contains(&"io-util".to_string()));
        // "rt" should not be duplicated
        assert_eq!(features.iter().filter(|f| f.as_str() == "rt").count(), 1);
    }

    #[test]
    // [verify manifest.merge.dep-kind]
    fn merge_dep_kind_normal_wins_over_dev() {
        let pack_a = BTreeMap::from([("serde".to_string(), crate_spec("1", &[], DepKind::Normal))]);
        let pack_b = BTreeMap::from([("serde".to_string(), crate_spec("1", &[], DepKind::Dev))]);

        let merged = merge_crate_specs(&[pack_a, pack_b]);
        assert_eq!(merged["serde"].dep_kinds, vec![DepKind::Normal]);
    }

    #[test]
    // [verify manifest.merge.dep-kind]
    fn merge_dep_kind_normal_wins_over_build() {
        let pack_a = BTreeMap::from([("cc".to_string(), crate_spec("1", &[], DepKind::Build))]);
        let pack_b = BTreeMap::from([("cc".to_string(), crate_spec("1", &[], DepKind::Normal))]);

        let merged = merge_crate_specs(&[pack_a, pack_b]);
        assert_eq!(merged["cc"].dep_kinds, vec![DepKind::Normal]);
    }

    #[test]
    // [verify manifest.merge.dep-kind]
    fn merge_dep_kind_dev_and_build_yields_both() {
        let pack_a = BTreeMap::from([("serde".to_string(), crate_spec("1", &[], DepKind::Dev))]);
        let pack_b = BTreeMap::from([("serde".to_string(), crate_spec("1", &[], DepKind::Build))]);

        let merged = merge_crate_specs(&[pack_a, pack_b]);
        let kinds = &merged["serde"].dep_kinds;
        assert_eq!(kinds.len(), 2);
        assert!(kinds.contains(&DepKind::Dev));
        assert!(kinds.contains(&DepKind::Build));
    }

    #[test]
    // [verify manifest.merge.version]
    // [verify manifest.merge.features]
    // [verify manifest.merge.dep-kind]
    fn merge_three_packs_all_rules() {
        let pack_a = BTreeMap::from([
            (
                "tokio".to_string(),
                crate_spec("1.35.0", &["macros"], DepKind::Normal),
            ),
            (
                "serde".to_string(),
                crate_spec("1.0.100", &["derive"], DepKind::Dev),
            ),
        ]);
        let pack_b = BTreeMap::from([
            (
                "tokio".to_string(),
                crate_spec("1.38.0", &["rt"], DepKind::Dev),
            ),
            (
                "serde".to_string(),
                crate_spec("1.0.210", &["alloc"], DepKind::Build),
            ),
        ]);
        let pack_c = BTreeMap::from([
            (
                "tokio".to_string(),
                crate_spec("1.36.0", &["net", "macros"], DepKind::Normal),
            ),
            (
                "anyhow".to_string(),
                crate_spec("1.0.80", &[], DepKind::Normal),
            ),
        ]);

        let merged = merge_crate_specs(&[pack_a, pack_b, pack_c]);

        // tokio: version 1.38.0 (highest), features union, Normal wins
        let tokio = &merged["tokio"];
        assert_eq!(tokio.version, "1.38.0");
        assert!(tokio.features.contains("macros"));
        assert!(tokio.features.contains("rt"));
        assert!(tokio.features.contains("net"));
        assert_eq!(tokio.dep_kinds, vec![DepKind::Normal]);

        // serde: version 1.0.210 (highest), features union, dev+build = both
        let serde = &merged["serde"];
        assert_eq!(serde.version, "1.0.210");
        assert!(serde.features.contains("derive"));
        assert!(serde.features.contains("alloc"));
        assert_eq!(serde.dep_kinds.len(), 2);
        assert!(serde.dep_kinds.contains(&DepKind::Dev));
        assert!(serde.dep_kinds.contains(&DepKind::Build));

        // anyhow: only in pack_c, should appear as-is
        let anyhow = &merged["anyhow"];
        assert_eq!(anyhow.version, "1.0.80");
        assert_eq!(anyhow.dep_kinds, vec![DepKind::Normal]);
    }

    #[test]
    // [verify manifest.merge.version]
    // [verify manifest.merge.features]
    fn merge_non_overlapping_crates() {
        let pack_a = BTreeMap::from([(
            "serde".to_string(),
            crate_spec("1.0.210", &["derive"], DepKind::Normal),
        )]);
        let pack_b = BTreeMap::from([(
            "clap".to_string(),
            crate_spec("4.5.0", &["derive"], DepKind::Normal),
        )]);

        let merged = merge_crate_specs(&[pack_a, pack_b]);
        assert_eq!(merged.len(), 2);
        assert_eq!(merged["serde"].version, "1.0.210");
        assert_eq!(merged["clap"].version, "4.5.0");
    }

    #[test]
    fn merge_empty_input() {
        let merged = merge_crate_specs(&[]);
        assert!(merged.is_empty());
    }

    #[test]
    fn merge_single_pack() {
        let pack = BTreeMap::from([
            (
                "serde".to_string(),
                crate_spec("1", &["derive"], DepKind::Normal),
            ),
            ("clap".to_string(), crate_spec("4", &[], DepKind::Normal)),
        ]);

        let merged = merge_crate_specs(&[pack]);
        assert_eq!(merged.len(), 2);
        assert_eq!(merged["serde"].version, "1");
        assert_eq!(
            merged["serde"].features,
            BTreeSet::from(["derive".to_string()])
        );
        assert_eq!(merged["serde"].dep_kinds, vec![DepKind::Normal]);
    }

    // -- Version comparison unit tests --

    #[test]
    fn compare_versions_basic() {
        use std::cmp::Ordering;
        assert_eq!(compare_versions("1.0.0", "1.0.0"), Ordering::Equal);
        assert_eq!(compare_versions("1.0.1", "1.0.0"), Ordering::Greater);
        assert_eq!(compare_versions("1.0.0", "1.0.1"), Ordering::Less);
        assert_eq!(compare_versions("2.0.0", "1.9.9"), Ordering::Greater);
        assert_eq!(compare_versions("1", "1.0"), Ordering::Equal);
        assert_eq!(compare_versions("1", "2"), Ordering::Less);
        assert_eq!(compare_versions("1.0.210", "1.0.100"), Ordering::Greater);
    }

    #[test]
    fn resolve_crates_keeps_non_optional_deps_with_active_features() {
        let manifest = r#"
            [package]
            name = "test-battery-pack"
            version = "0.1.0"

            [dependencies]
            anyhow = "1"
            clap = { version = "4", optional = true }

            [features]
            cli = ["clap"]
        "#;
        let spec = parse_test(manifest).unwrap();
        let resolved = spec.resolve_crates(&["cli"]);
        assert!(
            resolved.contains_key("clap"),
            "feature-gated dep is present"
        );
        // Non-optional normal deps are unconditional in Cargo, so they must be present
        // even when explicit features are active.
        assert!(
            resolved.contains_key("anyhow"),
            "non-optional dep is present regardless of active features"
        );
    }

    // -- Category and item-metadata parsing tests --

    #[test]
    // [verify parse.category-definition]
    fn parse_category_definition() {
        let manifest = indoc! {r#"
            [package]
            name = "test-battery-pack"
            version = "0.1.0"
            keywords = ["battery-pack"]

            [package.metadata.battery-pack.categories.hal]
            title = "Hardware Abstraction Layer"
            description = "Pick the HAL for your target chip family"
            pick = "at-most-one"
        "#};

        let spec = parse_test(manifest).unwrap();
        let hal = &spec.categories["hal"];
        assert_eq!(hal.pick, PickMode::AtMostOne);
        assert_eq!(hal.title.as_deref(), Some("Hardware Abstraction Layer"));
        assert_eq!(
            hal.description.as_deref(),
            Some("Pick the HAL for your target chip family")
        );
    }

    #[test]
    // [verify parse.category-pick-default]
    fn parse_category_default_pick_is_any() {
        let manifest = indoc! {r#"
            [package]
            name = "test-battery-pack"
            version = "0.1.0"
            keywords = ["battery-pack"]

            [package.metadata.battery-pack.categories.portable]
            title = "Portable Utilities"
        "#};

        let spec = parse_test(manifest).unwrap();
        assert_eq!(spec.categories["portable"].pick, PickMode::Any);
    }

    #[test]
    // [verify parse.feature-metadata]
    fn parse_feature_metadata() {
        let manifest = indoc! {r#"
            [package]
            name = "test-battery-pack"
            version = "0.1.0"
            keywords = ["battery-pack"]

            [package.metadata.battery-pack.features.stm32f4]
            description = "STM32F4xx"
            categories = ["hal"]
        "#};

        let spec = parse_test(manifest).unwrap();
        let meta = &spec.feature_meta["stm32f4"];
        assert_eq!(meta.categories, vec!["hal".to_string()]);
        assert_eq!(meta.description.as_deref(), Some("STM32F4xx"));
    }

    #[test]
    // [verify parse.dependency-metadata]
    fn parse_dependency_metadata() {
        let manifest = indoc! {r#"
            [package]
            name = "test-battery-pack"
            version = "0.1.0"
            keywords = ["battery-pack"]

            [package.metadata.battery-pack.dependencies.embedded-hal]
            description = "Trait abstractions for embedded I/O"
            categories = ["portable"]
        "#};

        let spec = parse_test(manifest).unwrap();
        let meta = &spec.dep_meta["embedded-hal"];
        assert_eq!(meta.categories, vec!["portable".to_string()]);
        assert_eq!(
            meta.description.as_deref(),
            Some("Trait abstractions for embedded I/O")
        );
    }

    #[test]
    // [verify parse.template-categories]
    fn parse_template_categories() {
        let manifest = indoc! {r#"
            [package]
            name = "test-battery-pack"
            version = "0.1.0"
            keywords = ["battery-pack"]

            [package.metadata.battery.templates]
            fuzzing = { path = "templates/fuzzing", categories = ["quality"] }
        "#};

        let spec = parse_test(manifest).unwrap();
        assert_eq!(
            spec.templates["fuzzing"].categories,
            vec!["quality".to_string()]
        );
    }

    #[test]
    // [verify parse.multiple-categories]
    fn parse_item_with_multiple_categories() {
        let manifest = indoc! {r#"
            [package]
            name = "test-battery-pack"
            version = "0.1.0"
            keywords = ["battery-pack"]

            [package.metadata.battery-pack.features.spellcheck]
            categories = ["quality", "ci"]
        "#};

        let spec = parse_test(manifest).unwrap();
        assert_eq!(
            spec.feature_meta["spellcheck"].categories,
            vec!["quality".to_string(), "ci".to_string()]
        );
    }

    #[test]
    // [verify parse.no-metadata-backward-compat]
    fn parse_item_without_metadata() {
        let manifest = indoc! {r#"
            [package]
            name = "test-battery-pack"
            version = "0.1.0"
            keywords = ["battery-pack"]

            [dependencies]
            clap = { version = "4", optional = true }

            [features]
            default = ["clap"]
        "#};

        let spec = parse_test(manifest).unwrap();
        assert!(spec.categories.is_empty());
        assert!(spec.feature_meta.is_empty());
        assert!(spec.dep_meta.is_empty());
        // Existing behavior is unchanged.
        assert_eq!(spec.features.len(), 1);
        assert!(spec.crates.contains_key("clap"));
    }

    // -- Category validation tests --

    #[test]
    // [verify format.features.exclusive-conflict]
    fn validate_exclusive_conflict_in_default() {
        let manifest = indoc! {r#"
            [package]
            name = "test-battery-pack"
            version = "0.1.0"
            keywords = ["battery-pack"]

            [dependencies]
            tikv-jemallocator = { version = "0.5", optional = true }
            mimalloc = { version = "0.1", optional = true }

            [features]
            default = ["jemalloc", "mimalloc-alloc"]
            jemalloc = ["tikv-jemallocator"]
            mimalloc-alloc = ["mimalloc"]

            [package.metadata.battery-pack.categories.allocator]
            title = "Global Allocator"
            pick = "at-most-one"

            [package.metadata.battery-pack.features.jemalloc]
            categories = ["allocator"]

            [package.metadata.battery-pack.features.mimalloc-alloc]
            categories = ["allocator"]
        "#};

        let spec = parse_test(manifest).unwrap();
        let report = spec.validate_spec();
        assert!(
            report
                .diagnostics
                .iter()
                .any(|d| d.rule == "format.features.exclusive-conflict"
                    && d.message.contains("jemalloc")
                    && d.message.contains("mimalloc-alloc")
                    && d.message.contains("allocator"))
        );
    }

    #[test]
    // [verify format.features.exclusive-conflict]
    fn validate_exclusive_conflict_not_triggered_for_any() {
        let manifest = indoc! {r#"
            [package]
            name = "test-battery-pack"
            version = "0.1.0"
            keywords = ["battery-pack"]

            [dependencies]
            tower-http = { version = "0.5", optional = true }

            [features]
            default = ["http-trace", "http-timeout"]
            http-trace = ["tower-http"]
            http-timeout = ["tower-http"]

            [package.metadata.battery-pack.categories.http-layers]
            title = "HTTP Middleware Layers"

            [package.metadata.battery-pack.features.http-trace]
            categories = ["http-layers"]

            [package.metadata.battery-pack.features.http-timeout]
            categories = ["http-layers"]
        "#};

        let spec = parse_test(manifest).unwrap();
        let report = spec.validate_spec();
        assert!(
            !report
                .diagnostics
                .iter()
                .any(|d| d.rule == "format.features.exclusive-conflict")
        );
    }

    #[test]
    // [verify format.categories.defined]
    fn validate_category_reference_exists() {
        let manifest = indoc! {r#"
            [package]
            name = "test-battery-pack"
            version = "0.1.0"
            keywords = ["battery-pack"]

            [dependencies]
            stm32f4xx-hal = { version = "0.22", optional = true }

            [features]
            stm32f4 = ["stm32f4xx-hal"]

            [package.metadata.battery-pack.features.stm32f4]
            categories = ["nonexistent"]
        "#};

        let spec = parse_test(manifest).unwrap();
        let report = spec.validate_spec();
        assert!(
            report
                .diagnostics
                .iter()
                .any(|d| d.rule == "format.categories.defined"
                    && d.message.contains("nonexistent"))
        );
    }

    #[test]
    // [verify format.features.exclusive-conflict]
    fn validate_clean_when_exclusive_not_in_default() {
        let manifest = indoc! {r#"
            [package]
            name = "test-battery-pack"
            version = "0.1.0"
            keywords = ["battery-pack"]

            [dependencies]
            tikv-jemallocator = { version = "0.5", optional = true }
            mimalloc = { version = "0.1", optional = true }

            [features]
            default = ["jemalloc"]
            jemalloc = ["tikv-jemallocator"]
            mimalloc-alloc = ["mimalloc"]

            [package.metadata.battery-pack.categories.allocator]
            title = "Global Allocator"
            pick = "at-most-one"

            [package.metadata.battery-pack.features.jemalloc]
            categories = ["allocator"]

            [package.metadata.battery-pack.features.mimalloc-alloc]
            categories = ["allocator"]
        "#};

        let spec = parse_test(manifest).unwrap();
        let report = spec.validate_spec();
        assert!(
            !report
                .diagnostics
                .iter()
                .any(|d| d.rule == "format.features.exclusive-conflict")
        );
    }

    #[test]
    // [verify format.categories.defined]
    fn validate_template_category_reference_exists() {
        let manifest = indoc! {r#"
            [package]
            name = "test-battery-pack"
            version = "0.1.0"
            keywords = ["battery-pack"]

            [package.metadata.battery.templates]
            fuzzing = { path = "templates/fuzzing", categories = ["bogus"] }
        "#};

        let spec = parse_test(manifest).unwrap();
        let report = spec.validate_spec();
        assert!(
            report
                .diagnostics
                .iter()
                .any(|d| d.rule == "format.categories.defined"
                    && d.message.contains("template")
                    && d.message.contains("bogus"))
        );
    }

    #[test]
    // [verify format.categories.defined]
    fn validate_dep_category_reference_exists() {
        let manifest = indoc! {r#"
            [package]
            name = "test-battery-pack"
            version = "0.1.0"
            keywords = ["battery-pack"]

            [dependencies]
            embedded-hal = { version = "1", optional = true }

            [package.metadata.battery-pack.dependencies.embedded-hal]
            categories = ["bogus"]
        "#};

        let spec = parse_test(manifest).unwrap();
        let report = spec.validate_spec();
        assert!(
            report
                .diagnostics
                .iter()
                .any(|d| d.rule == "format.categories.defined"
                    && d.message.contains("dependency")
                    && d.message.contains("bogus"))
        );
    }

    #[test]
    // [verify format.categories.empty]
    fn validate_empty_category_warns() {
        let manifest = indoc! {r#"
            [package]
            name = "test-battery-pack"
            version = "0.1.0"
            keywords = ["battery-pack"]

            [package.metadata.battery-pack.categories.foo]
            title = "Foo"
        "#};

        let spec = parse_test(manifest).unwrap();
        let report = spec.validate_spec();
        assert!(
            report
                .diagnostics
                .iter()
                .any(|d| d.rule == "format.categories.empty"
                    && d.severity == Severity::Warning
                    && d.message.contains("foo"))
        );
    }

    #[test]
    // [verify format.categories.pick-missing-title]
    fn validate_at_most_one_missing_title_warns() {
        let manifest = indoc! {r#"
            [package]
            name = "test-battery-pack"
            version = "0.1.0"
            keywords = ["battery-pack"]

            [package.metadata.battery-pack.categories.hal]
            pick = "at-most-one"
        "#};

        let spec = parse_test(manifest).unwrap();
        let report = spec.validate_spec();
        assert!(
            report
                .diagnostics
                .iter()
                .any(|d| d.rule == "format.categories.pick-missing-title"
                    && d.severity == Severity::Warning
                    && d.message.contains("hal"))
        );
    }

    #[test]
    // [verify format.features.unknown-feature]
    fn validate_feature_metadata_for_unknown_feature() {
        let manifest = indoc! {r#"
            [package]
            name = "test-battery-pack"
            version = "0.1.0"
            keywords = ["battery-pack"]

            [package.metadata.battery-pack.features.ghost]
            description = "not a real feature"
        "#};

        let spec = parse_test(manifest).unwrap();
        let report = spec.validate_spec();
        assert!(
            report
                .diagnostics
                .iter()
                .any(|d| d.rule == "format.features.unknown-feature"
                    && d.message.contains("ghost"))
        );
    }

    #[test]
    // [verify format.features.unknown-feature] optional-dep-backed features are known
    fn validate_feature_metadata_for_optional_dep_feature() {
        // Cargo strips the auto-generated `serde = ["dep:serde"]` mirror feature
        // from the parsed feature map, but metadata for the same name is still
        // valid because the optional dependency implicitly defines that feature.
        let manifest = indoc! {r#"
            [package]
            name = "test-battery-pack"
            version = "0.1.0"
            keywords = ["battery-pack"]

            [dependencies]
            serde = { version = "1", optional = true }

            [package.metadata.battery-pack.features.serde]
            description = "Serde support"
        "#};

        let spec = parse_test(manifest).unwrap();
        let report = spec.validate_spec();
        assert!(
            !report
                .diagnostics
                .iter()
                .any(|d| d.rule == "format.features.unknown-feature"),
            "optional-dep feature must not be flagged unknown: {:?}",
            report.diagnostics
        );
    }

    #[test]
    // [verify format.dependencies.unknown-dep]
    fn validate_dep_metadata_for_unknown_dep() {
        let manifest = indoc! {r#"
            [package]
            name = "test-battery-pack"
            version = "0.1.0"
            keywords = ["battery-pack"]

            [package.metadata.battery-pack.dependencies.ghost]
            description = "not a real dependency"
        "#};

        let spec = parse_test(manifest).unwrap();
        let report = spec.validate_spec();
        assert!(
            report
                .diagnostics
                .iter()
                .any(|d| d.rule == "format.dependencies.unknown-dep"
                    && d.message.contains("ghost"))
        );
    }

    #[test]
    // [verify format.categories.defined]
    fn validate_multiple_categories_all_checked() {
        let manifest = indoc! {r#"
            [package]
            name = "test-battery-pack"
            version = "0.1.0"
            keywords = ["battery-pack"]

            [dependencies]
            stm32f4xx-hal = { version = "0.22", optional = true }

            [features]
            stm32f4 = ["stm32f4xx-hal"]

            [package.metadata.battery-pack.categories.hal]
            title = "Hardware Abstraction Layer"

            [package.metadata.battery-pack.features.stm32f4]
            categories = ["hal", "bogus"]
        "#};

        let spec = parse_test(manifest).unwrap();
        let report = spec.validate_spec();
        // Error for the undefined category only.
        assert!(
            report
                .diagnostics
                .iter()
                .any(|d| d.rule == "format.categories.defined" && d.message.contains("bogus"))
        );
        assert!(
            !report
                .diagnostics
                .iter()
                .any(|d| d.rule == "format.categories.defined" && d.message.contains("'hal'"))
        );
    }
}
