//! Battery pack manifest parsing and resolution.
//!
//! Parses battery pack Cargo.toml files to extract curated crates,
//! features, hidden dependencies, and templates. Provides resolution
//! logic to determine which crates to install based on active features.

use serde::Deserialize;
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

    #[error("reading {path}: {source}")]
    Io {
        path: String,
        #[source]
        source: std::io::Error,
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
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash, PartialOrd, Ord)]
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
#[derive(Debug, Clone)]
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

/// Template metadata for project scaffolding.
#[derive(Debug, Clone)]
pub struct TemplateSpec {
    pub path: String,
    pub description: Option<String>,
}

/// Parsed battery pack specification.
///
/// This is the core data model extracted from a battery pack's Cargo.toml.
/// All curated crates, features, hidden deps, and templates are represented here.
#[derive(Debug, Clone)]
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
    /// Named features from `[features]`, mapping feature name to crate names.
    // [impl format.features.grouping]
    pub features: BTreeMap<String, BTreeSet<String>>,
    /// Hidden dependency patterns (may include globs).
    // [impl format.hidden.metadata]
    pub hidden: BTreeSet<String>,
    /// Templates registered in metadata.
    pub templates: BTreeMap<String, TemplateSpec>,
}

impl BatteryPackSpec {
    /// Validate that this looks like a valid battery pack.
    // [impl format.crate.name]
    pub fn validate(&self) -> Result<(), Error> {
        if !self.name.ends_with("-battery-pack") {
            return Err(Error::InvalidName {
                name: self.name.clone(),
            });
        }
        self.validate_features()?;
        Ok(())
    }

    /// Check that all feature entries reference crates that actually exist.
    fn validate_features(&self) -> Result<(), Error> {
        for (feature_name, crate_names) in &self.features {
            for crate_name in crate_names {
                if !self.crates.contains_key(crate_name) {
                    return Err(Error::UnknownCrateInFeature {
                        feature: feature_name.clone(),
                        crate_name: crate_name.clone(),
                    });
                }
            }
        }
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
        for (feature_name, crate_names) in &self.features {
            for crate_name in crate_names {
                if !self.crates.contains_key(crate_name) {
                    report.error(
                        "format.features.grouping",
                        format!(
                            "feature '{}' references unknown crate '{}'",
                            feature_name, crate_name
                        ),
                    );
                }
            }
        }

        report
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

        if active_features.is_empty() {
            // Default resolution
            self.add_default_crates(&mut result);
        } else {
            for feature_name in active_features {
                if *feature_name == "default" {
                    self.add_default_crates(&mut result);
                } else if let Some(crate_names) = self.features.get(*feature_name) {
                    self.add_feature_crates(crate_names, &mut result);
                }
            }
        }

        result
    }

    /// Add the default set of crates to the result map.
    // [impl format.features.default]
    fn add_default_crates(&self, result: &mut BTreeMap<String, CrateSpec>) {
        if let Some(default_crate_names) = self.features.get("default") {
            // Explicit default feature exists — use it
            self.add_feature_crates(default_crate_names, result);
        } else {
            // No default feature — all non-optional crates
            for (name, spec) in &self.crates {
                if !spec.optional {
                    result.insert(name.clone(), spec.clone());
                }
            }
        }
    }

    /// Add crates from a feature's crate list to the result map.
    ///
    /// If a crate is already present, its Cargo features are merged additively.
    // [impl format.features.augment]
    fn add_feature_crates(
        &self,
        crate_names: &BTreeSet<String>,
        result: &mut BTreeMap<String, CrateSpec>,
    ) {
        for crate_name in crate_names {
            if let Some(spec) = self.crates.get(crate_name) {
                if let Some(existing) = result.get_mut(crate_name) {
                    // Already present — merge features additively
                    existing.features.extend(spec.features.iter().cloned());
                } else {
                    result.insert(crate_name.clone(), spec.clone());
                }
            }
        }
    }

    /// Resolve all crates regardless of features or optional status.
    pub fn resolve_all(&self) -> BTreeMap<String, CrateSpec> {
        self.crates.clone()
    }

    /// Resolve all visible (non-hidden) crates regardless of features or optional status.
    // [impl format.hidden.effect]
    pub fn resolve_all_visible(&self) -> BTreeMap<String, CrateSpec> {
        self.crates
            .iter()
            .filter(|(name, _)| !self.is_hidden(name))
            .map(|(name, spec)| (name.clone(), spec.clone()))
            .collect()
    }

    /// Resolve crates for a set of active features, handling the "all" sentinel.
    ///
    /// If `active_features` contains `"all"`, returns all visible crates.
    /// Otherwise delegates to `resolve_crates`.
    // [impl format.hidden.effect]
    pub fn resolve_for_features(
        &self,
        active_features: &BTreeSet<String>,
    ) -> BTreeMap<String, CrateSpec> {
        if active_features.iter().any(|s| s == "all") {
            self.resolve_all_visible()
        } else {
            let str_features: Vec<&str> = active_features.iter().map(|s| s.as_str()).collect();
            self.resolve_crates(&str_features)
        }
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
        for (feature_name, crate_names) in &self.features {
            for crate_name in crate_names {
                if self.is_hidden(crate_name) {
                    continue;
                }
                if let Some(spec) = self.crates.get(crate_name)
                    && seen.insert(crate_name.clone())
                {
                    let is_default = default_crates.contains_key(crate_name);
                    result.push((feature_name.clone(), crate_name.clone(), spec, is_default));
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
struct RawManifest {
    package: Option<RawPackage>,
    #[serde(default)]
    features: BTreeMap<String, Vec<String>>,
    #[serde(default)]
    dependencies: BTreeMap<String, toml::Value>,
    #[serde(default, rename = "dev-dependencies")]
    dev_dependencies: BTreeMap<String, toml::Value>,
    #[serde(default, rename = "build-dependencies")]
    build_dependencies: BTreeMap<String, toml::Value>,
}

#[derive(Deserialize)]
struct RawPackage {
    name: Option<String>,
    version: Option<String>,
    #[serde(default)]
    description: Option<String>,
    #[serde(default)]
    repository: Option<String>,
    #[serde(default)]
    keywords: Vec<String>,
    #[serde(default)]
    metadata: Option<RawMetadata>,
}

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
}

/// Parsed fields from a single dependency entry.
struct RawDep {
    version: String,
    features: Vec<String>,
    optional: bool,
}

// ============================================================================
// Parsing
// ============================================================================

/// Parse a battery pack's Cargo.toml into a `BatteryPackSpec`.
pub fn parse_battery_pack(manifest_str: &str) -> Result<BatteryPackSpec, Error> {
    let raw: RawManifest = toml::from_str(manifest_str)?;

    let package = raw
        .package
        .ok_or(Error::MissingField("[package] section"))?;
    let name = package.name.ok_or(Error::MissingField("package.name"))?;
    let version = package
        .version
        .ok_or(Error::MissingField("package.version"))?;
    let description = package.description.unwrap_or_default();
    let repository = package.repository;
    let keywords = package.keywords;

    // Parse crates from all three dependency sections
    let mut crates = BTreeMap::new();
    parse_dep_section(&raw.dependencies, DepKind::Normal, &mut crates);
    parse_dep_section(&raw.dev_dependencies, DepKind::Dev, &mut crates);
    parse_dep_section(&raw.build_dependencies, DepKind::Build, &mut crates);

    // Parse features (standard Cargo features)
    let features: BTreeMap<String, BTreeSet<String>> = raw
        .features
        .into_iter()
        .map(|(k, v)| (k, v.into_iter().collect()))
        .collect();

    // Parse hidden deps from package.metadata.battery-pack
    let hidden: BTreeSet<String> = package
        .metadata
        .as_ref()
        .and_then(|m| m.battery_pack.as_ref())
        .map(|bp| bp.hidden.iter().cloned().collect())
        .unwrap_or_default();

    // [impl format.templates.metadata]
    // Parse templates from package.metadata.battery.templates
    let templates = package
        .metadata
        .as_ref()
        .and_then(|m| m.battery.as_ref())
        .map(|b| {
            b.templates
                .iter()
                .map(|(name, raw)| {
                    (
                        name.clone(),
                        TemplateSpec {
                            path: raw.path.clone(),
                            description: raw.description.clone(),
                        },
                    )
                })
                .collect()
        })
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
    })
}

/// Parse a single dependency section into the crates map.
fn parse_dep_section(
    raw: &BTreeMap<String, toml::Value>,
    kind: DepKind,
    crates: &mut BTreeMap<String, CrateSpec>,
) {
    for (name, value) in raw {
        let dep = parse_single_dep(value);
        crates.insert(
            name.clone(),
            CrateSpec {
                version: dep.version,
                features: dep.features.into_iter().collect(),
                dep_kind: kind,
                optional: dep.optional,
            },
        );
    }
}

/// Extract version, features, and optional flag from a dependency value.
fn parse_single_dep(value: &toml::Value) -> RawDep {
    match value {
        toml::Value::String(version) => RawDep {
            version: version.clone(),
            features: Vec::new(),
            optional: false,
        },
        toml::Value::Table(table) => {
            let version = table
                .get("version")
                .and_then(|v| v.as_str())
                .unwrap_or("")
                .to_string();
            let features = table
                .get("features")
                .and_then(|v| v.as_array())
                .map(|arr| {
                    arr.iter()
                        .filter_map(|v| v.as_str().map(String::from))
                        .collect()
                })
                .unwrap_or_default();
            let optional = table
                .get("optional")
                .and_then(|v| v.as_bool())
                .unwrap_or(false);
            RawDep {
                version,
                features,
                optional,
            }
        }
        _ => RawDep {
            version: String::new(),
            features: Vec::new(),
            optional: false,
        },
    }
}

// ============================================================================
// Source discovery
// ============================================================================

/// Discover battery packs in a workspace by scanning members for
/// crates whose names end in `-battery-pack`.
// [impl cli.source.discover]
pub fn discover_battery_packs(workspace_path: &Path) -> Result<Vec<BatteryPackSpec>, Error> {
    let workspace_toml = workspace_path.join("Cargo.toml");
    let content = std::fs::read_to_string(&workspace_toml).map_err(|e| Error::Io {
        path: workspace_toml.display().to_string(),
        source: e,
    })?;

    let raw: RawWorkspace = toml::from_str(&content)?;

    let members = raw
        .workspace
        .ok_or(Error::MissingField("[workspace] section"))?
        .members;

    let mut packs = Vec::new();

    for member_path in &members {
        let member_dir = workspace_path.join(member_path);
        let member_toml = member_dir.join("Cargo.toml");

        if !member_toml.exists() {
            continue;
        }

        let member_content = std::fs::read_to_string(&member_toml).map_err(|e| Error::Io {
            path: member_toml.display().to_string(),
            source: e,
        })?;

        // Parse once, check name, keep if it's a battery pack
        // [impl format.crate.name]
        let spec = parse_battery_pack(&member_content)?;
        if spec.name.ends_with("-battery-pack") {
            packs.push(spec);
        }
    }

    Ok(packs)
}

/// Minimal workspace-level deserialization for member discovery.
#[derive(Deserialize)]
struct RawWorkspace {
    workspace: Option<RawWorkspaceInner>,
}

#[derive(Deserialize)]
struct RawWorkspaceInner {
    #[serde(default)]
    members: Vec<String>,
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
        }
    }
}

// ============================================================================
// Tests
// ============================================================================

#[cfg(test)]
mod tests {
    use super::*;

    // -- Parsing tests --

    #[test]
    // [verify format.deps.source-of-truth]
    // [verify format.deps.kind-mapping]
    fn parse_deps_from_all_sections() {
        let manifest = r#"
            [package]
            name = "test-battery-pack"
            version = "0.1.0"

            [dependencies]
            serde = { version = "1", features = ["derive"] }

            [dev-dependencies]
            insta = "1.34"

            [build-dependencies]
            cc = "1.0"
        "#;

        let spec = parse_battery_pack(manifest).unwrap();
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
        let manifest = r#"
            [package]
            name = "test-battery-pack"
            version = "0.1.0"

            [dependencies]
            tokio = { version = "1", features = ["macros", "rt-multi-thread"] }
            anyhow = "1"
        "#;

        let spec = parse_battery_pack(manifest).unwrap();
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
        let manifest = r#"
            [package]
            name = "test-battery-pack"
            version = "0.1.0"

            [dependencies]
            clap = { version = "4", features = ["derive"] }
            indicatif = { version = "0.17", optional = true }
        "#;

        let spec = parse_battery_pack(manifest).unwrap();
        assert!(!spec.crates["clap"].optional);
        assert!(spec.crates["indicatif"].optional);
    }

    #[test]
    // [verify format.features.grouping]
    fn parse_cargo_features() {
        let manifest = r#"
            [package]
            name = "test-battery-pack"
            version = "0.1.0"

            [dependencies]
            clap = { version = "4", features = ["derive"] }
            dialoguer = "0.11"
            indicatif = { version = "0.17", optional = true }
            console = { version = "0.15", optional = true }

            [features]
            default = ["clap", "dialoguer"]
            indicators = ["indicatif", "console"]
        "#;

        let spec = parse_battery_pack(manifest).unwrap();
        assert_eq!(spec.features.len(), 2);
        assert_eq!(
            spec.features["default"],
            BTreeSet::from(["clap".to_string(), "dialoguer".to_string()])
        );
        assert_eq!(
            spec.features["indicators"],
            BTreeSet::from(["indicatif".to_string(), "console".to_string()])
        );
    }

    #[test]
    // [verify format.hidden.metadata]
    fn parse_hidden_deps() {
        let manifest = r#"
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
        "#;

        let spec = parse_battery_pack(manifest).unwrap();
        assert_eq!(spec.hidden, BTreeSet::from(["serde*".to_string()]));
    }

    #[test]
    fn parse_templates() {
        let manifest = r#"
            [package]
            name = "test-battery-pack"
            version = "0.1.0"

            [package.metadata.battery.templates]
            default = { path = "templates/default", description = "A basic starting point" }
            advanced = { path = "templates/advanced", description = "Full-featured setup" }
        "#;

        let spec = parse_battery_pack(manifest).unwrap();
        assert_eq!(spec.templates.len(), 2);
        assert_eq!(spec.templates["default"].path, "templates/default");
        assert_eq!(
            spec.templates["advanced"].description.as_deref(),
            Some("Full-featured setup")
        );
    }

    #[test]
    fn parse_description_and_repository() {
        let manifest = r#"
            [package]
            name = "test-battery-pack"
            version = "0.1.0"
            description = "Error handling crates"
            repository = "https://github.com/example/repo"
        "#;

        let spec = parse_battery_pack(manifest).unwrap();
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
        let manifest = r#"
            [package]
            name = "test-battery-pack"
            version = "0.1.0"
        "#;
        let spec = parse_battery_pack(manifest).unwrap();
        assert!(spec.validate().is_ok());

        let manifest_bad = r#"
            [package]
            name = "not-a-battery-pack-crate"
            version = "0.1.0"
        "#;
        let spec_bad = parse_battery_pack(manifest_bad).unwrap();
        let err = spec_bad.validate().unwrap_err();
        assert!(matches!(err, Error::InvalidName { .. }));
    }

    #[test]
    fn validate_features_reference_real_crates() {
        let manifest = r#"
            [package]
            name = "test-battery-pack"
            version = "0.1.0"

            [dependencies]
            clap = "4"

            [features]
            default = ["clap", "nonexistent"]
        "#;
        let spec = parse_battery_pack(manifest).unwrap();
        let err = spec.validate().unwrap_err();
        assert!(matches!(err, Error::UnknownCrateInFeature { .. }));

        // Valid case
        let manifest_ok = r#"
            [package]
            name = "test-battery-pack"
            version = "0.1.0"

            [dependencies]
            clap = "4"
            dialoguer = "0.11"

            [features]
            default = ["clap", "dialoguer"]
        "#;
        let spec_ok = parse_battery_pack(manifest_ok).unwrap();
        assert!(spec_ok.validate().is_ok());
    }

    // -- Resolution tests --

    #[test]
    // [verify format.features.default]
    fn resolve_default_feature() {
        let manifest = r#"
            [package]
            name = "test-battery-pack"
            version = "0.1.0"

            [dependencies]
            clap = { version = "4", features = ["derive"] }
            dialoguer = "0.11"
            indicatif = { version = "0.17", optional = true }

            [features]
            default = ["clap", "dialoguer"]
            indicators = ["indicatif"]
        "#;

        let spec = parse_battery_pack(manifest).unwrap();
        let resolved = spec.resolve_crates(&[]);

        assert_eq!(resolved.len(), 2);
        assert!(resolved.contains_key("clap"));
        assert!(resolved.contains_key("dialoguer"));
        assert!(!resolved.contains_key("indicatif"));
    }

    #[test]
    // [verify format.features.default]
    fn resolve_no_default_feature() {
        let manifest = r#"
            [package]
            name = "test-battery-pack"
            version = "0.1.0"

            [dependencies]
            clap = "4"
            dialoguer = "0.11"
            indicatif = { version = "0.17", optional = true }
        "#;

        let spec = parse_battery_pack(manifest).unwrap();
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
        let manifest = r#"
            [package]
            name = "test-battery-pack"
            version = "0.1.0"

            [dependencies]
            clap = "4"
            dialoguer = "0.11"
            indicatif = { version = "0.17", optional = true }
            console = { version = "0.15", optional = true }

            [features]
            default = ["clap", "dialoguer"]
            indicators = ["indicatif", "console"]
        "#;

        let spec = parse_battery_pack(manifest).unwrap();
        let resolved = spec.resolve_crates(&["default", "indicators"]);

        assert_eq!(resolved.len(), 4);
        assert!(resolved.contains_key("clap"));
        assert!(resolved.contains_key("dialoguer"));
        assert!(resolved.contains_key("indicatif"));
        assert!(resolved.contains_key("console"));
    }

    #[test]
    fn resolve_feature_without_default() {
        let manifest = r#"
            [package]
            name = "test-battery-pack"
            version = "0.1.0"

            [dependencies]
            clap = "4"
            dialoguer = "0.11"
            indicatif = { version = "0.17", optional = true }

            [features]
            default = ["clap", "dialoguer"]
            indicators = ["indicatif"]
        "#;

        let spec = parse_battery_pack(manifest).unwrap();
        // Only indicators, no default
        let resolved = spec.resolve_crates(&["indicators"]);

        assert_eq!(resolved.len(), 1);
        assert!(resolved.contains_key("indicatif"));
        assert!(!resolved.contains_key("clap"));
    }

    #[test]
    // [verify format.features.augment]
    fn resolve_feature_augmentation() {
        let manifest = r#"
            [package]
            name = "test-battery-pack"
            version = "0.1.0"

            [dependencies]
            tokio = { version = "1", features = ["macros", "rt"] }

            [features]
            default = ["tokio"]
            full = ["tokio"]
        "#;

        let spec = parse_battery_pack(manifest).unwrap();
        // Both default and full reference tokio — features should be merged
        let resolved = spec.resolve_crates(&["default", "full"]);

        assert_eq!(resolved.len(), 1);
        let tokio = &resolved["tokio"];
        assert!(tokio.features.contains("macros"));
        assert!(tokio.features.contains("rt"));
    }

    #[test]
    fn resolve_all() {
        let manifest = r#"
            [package]
            name = "test-battery-pack"
            version = "0.1.0"

            [dependencies]
            clap = "4"
            indicatif = { version = "0.17", optional = true }

            [dev-dependencies]
            insta = "1.34"

            [features]
            default = ["clap"]
        "#;

        let spec = parse_battery_pack(manifest).unwrap();
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
        let manifest = r#"
            [package]
            name = "test-battery-pack"
            version = "0.1.0"

            [dependencies]
            serde = "1"
            clap = "4"

            [package.metadata.battery-pack]
            hidden = ["serde"]
        "#;

        let spec = parse_battery_pack(manifest).unwrap();
        assert!(spec.is_hidden("serde"));
        assert!(!spec.is_hidden("clap"));
    }

    #[test]
    // [verify format.hidden.glob]
    fn hidden_glob_pattern() {
        let manifest = r#"
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
        "#;

        let spec = parse_battery_pack(manifest).unwrap();
        assert!(spec.is_hidden("serde"));
        assert!(spec.is_hidden("serde_json"));
        assert!(spec.is_hidden("serde_derive"));
        assert!(!spec.is_hidden("clap"));
    }

    #[test]
    // [verify format.hidden.wildcard]
    fn hidden_wildcard_all() {
        let manifest = r#"
            [package]
            name = "test-battery-pack"
            version = "0.1.0"

            [dependencies]
            serde = "1"
            clap = "4"

            [package.metadata.battery-pack]
            hidden = ["*"]
        "#;

        let spec = parse_battery_pack(manifest).unwrap();
        assert!(spec.is_hidden("serde"));
        assert!(spec.is_hidden("clap"));
        assert!(spec.is_hidden("anything"));
    }

    #[test]
    fn visible_crates_filters_hidden() {
        let manifest = r#"
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
        "#;

        let spec = parse_battery_pack(manifest).unwrap();
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
        let manifest = r#"
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
        "#;

        let spec = parse_battery_pack(manifest).unwrap();
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
        let result = parse_battery_pack("not valid toml [[[");
        assert!(matches!(result, Err(Error::Toml(_))));
    }

    #[test]
    fn error_on_missing_package() {
        let result = parse_battery_pack("[dependencies]\nfoo = \"1\"");
        assert!(matches!(result, Err(Error::MissingField(_))));
    }

    // -- Comprehensive battery pack test --

    #[test]
    fn full_battery_pack_parse() {
        let manifest = r#"
            [package]
            name = "cli-battery-pack"
            version = "0.3.0"
            description = "CLI essentials for Rust applications"
            repository = "https://github.com/battery-pack-rs/battery-pack"
            keywords = ["battery-pack"]

            [dependencies]
            clap = { version = "4", features = ["derive"] }
            dialoguer = "0.11"
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
        "#;

        let spec = parse_battery_pack(manifest).unwrap();
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

        // Optional
        assert!(spec.crates["indicatif"].optional);
        assert!(!spec.crates["clap"].optional);

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

        // Resolution: default
        let default = spec.resolve_crates(&[]);
        assert_eq!(default.len(), 2);
        assert!(default.contains_key("clap"));
        assert!(default.contains_key("dialoguer"));

        // Resolution: default + indicators
        let with_indicators = spec.resolve_crates(&["default", "indicators"]);
        assert_eq!(with_indicators.len(), 4);

        // Resolution: only indicators (no default)
        let only_indicators = spec.resolve_crates(&["indicators"]);
        assert_eq!(only_indicators.len(), 2);
        assert!(only_indicators.contains_key("indicatif"));
        assert!(only_indicators.contains_key("console"));

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

        assert_eq!(packs.len(), 3);

        let names: Vec<&str> = packs.iter().map(|p| p.name.as_str()).collect();
        assert!(names.contains(&"basic-battery-pack"));
        assert!(names.contains(&"fancy-battery-pack"));
        assert!(names.contains(&"broken-battery-pack"));

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

        // fancy default resolution
        let default = fancy.resolve_crates(&[]);
        assert_eq!(default.len(), 2);
        assert!(default.contains_key("clap"));
        assert!(default.contains_key("dialoguer"));

        // fancy visible crates (hidden: serde, serde_json, cc)
        let visible = fancy.visible_crates();
        assert!(!visible.contains_key("serde"));
        assert!(!visible.contains_key("serde_json"));
        assert!(!visible.contains_key("cc"));
        assert!(visible.contains_key("clap"));
    }

    // -- validate_spec tests --

    #[test]
    // [verify format.crate.name]
    fn validate_spec_name() {
        let good = parse_battery_pack(
            r#"
            [package]
            name = "test-battery-pack"
            version = "0.1.0"
            repository = "https://github.com/example/test"
            keywords = ["battery-pack"]
        "#,
        )
        .unwrap();
        assert!(good.validate_spec().is_clean());

        let exact = parse_battery_pack(
            r#"
            [package]
            name = "battery-pack"
            version = "0.1.0"
            repository = "https://github.com/example/test"
            keywords = ["battery-pack"]
        "#,
        )
        .unwrap();
        assert!(exact.validate_spec().is_clean());

        let bad = parse_battery_pack(
            r#"
            [package]
            name = "not-a-pack"
            version = "0.1.0"
            keywords = ["battery-pack"]
        "#,
        )
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
        let good = parse_battery_pack(
            r#"
            [package]
            name = "test-battery-pack"
            version = "0.1.0"
            repository = "https://github.com/example/test"
            keywords = ["battery-pack", "helpers"]
        "#,
        )
        .unwrap();
        assert!(good.validate_spec().is_clean());

        let missing = parse_battery_pack(
            r#"
            [package]
            name = "test-battery-pack"
            version = "0.1.0"
        "#,
        )
        .unwrap();
        let report = missing.validate_spec();
        assert!(report.has_errors());
        assert!(
            report
                .diagnostics
                .iter()
                .any(|d| d.rule == "format.crate.keyword")
        );

        let wrong = parse_battery_pack(
            r#"
            [package]
            name = "test-battery-pack"
            version = "0.1.0"
            keywords = ["cli", "helpers"]
        "#,
        )
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
        let good = parse_battery_pack(
            r#"
            [package]
            name = "test-battery-pack"
            version = "0.1.0"
            repository = "https://github.com/example/test"
            keywords = ["battery-pack"]

            [dependencies]
            clap = "4"

            [features]
            default = ["clap"]
        "#,
        )
        .unwrap();
        assert!(good.validate_spec().is_clean());

        let bad = parse_battery_pack(
            r#"
            [package]
            name = "test-battery-pack"
            version = "0.1.0"
            keywords = ["battery-pack"]

            [dependencies]
            clap = "4"

            [features]
            default = ["clap", "ghost"]
        "#,
        )
        .unwrap();
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

        let spec = parse_battery_pack(
            r#"
            [package]
            name = "test-battery-pack"
            version = "0.1.0"
            keywords = ["battery-pack"]
        "#,
        )
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

        let spec = parse_battery_pack(
            r#"
            [package]
            name = "test-battery-pack"
            version = "0.1.0"
            keywords = ["battery-pack"]
        "#,
        )
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

        let spec = parse_battery_pack(
            r#"
            [package]
            name = "test-battery-pack"
            version = "0.1.0"
            keywords = ["battery-pack"]
        "#,
        )
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

        let spec = parse_battery_pack(
            r#"
            [package]
            name = "test-battery-pack"
            version = "0.1.0"
            keywords = ["battery-pack"]

            [package.metadata.battery.templates]
            default = { path = "templates/default", description = "Basic" }
        "#,
        )
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

    // -- Repository warning tests --

    #[test]
    // [verify format.crate.repository]
    fn validate_warns_on_missing_repository() {
        let spec = parse_battery_pack(
            r#"
            [package]
            name = "test-battery-pack"
            version = "0.1.0"
            keywords = ["battery-pack"]
        "#,
        )
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
        let spec = parse_battery_pack(
            r#"
            [package]
            name = "test-battery-pack"
            version = "0.1.0"
            repository = "https://github.com/example/repo"
            keywords = ["battery-pack"]
        "#,
        )
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
        let spec = parse_battery_pack(&content).unwrap();

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
        let spec = parse_battery_pack(&content).unwrap();

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
        let spec = parse_battery_pack(&content).unwrap();

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
}
