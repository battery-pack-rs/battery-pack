//! Cargo.toml manipulation helpers and metadata location abstraction.
//!
//! This module handles reading and writing battery pack registrations,
//! feature storage, and dependency management in user Cargo.toml files.
//! No dependencies on other internal modules.

use anyhow::{Context, Result, bail};
use std::collections::{BTreeMap, BTreeSet};
use std::path::{Path, PathBuf};

// ============================================================================
// Cargo.toml location helpers
// ============================================================================

/// Find the user's Cargo.toml in the given directory.
pub(crate) fn find_user_manifest(project_dir: &Path) -> Result<PathBuf> {
    let path = project_dir.join("Cargo.toml");
    if path.exists() {
        Ok(path)
    } else {
        bail!("No Cargo.toml found in {}", project_dir.display());
    }
}

/// Extract battery pack crate names from a parsed Cargo.toml.
///
/// Filters `[build-dependencies]` for entries ending in `-battery-pack` or equal to `"battery-pack"`.
// [impl manifest.register.location]
pub(crate) fn find_installed_bp_names(manifest_content: &str) -> Result<Vec<String>> {
    let raw: toml::Value =
        toml::from_str(manifest_content).context("Failed to parse Cargo.toml")?;

    let build_deps = raw
        .get("build-dependencies")
        .and_then(|bd| bd.as_table())
        .cloned()
        .unwrap_or_default();

    Ok(build_deps
        .keys()
        .filter(|k| k.ends_with("-battery-pack") || *k == "battery-pack")
        .cloned()
        .collect())
}

/// Find the workspace root Cargo.toml, if any.
/// Returns None if the crate is not in a workspace.
// [impl manifest.register.workspace-default]
// [impl manifest.register.both-levels]
pub(crate) fn find_workspace_manifest(crate_manifest: &Path) -> Result<Option<PathBuf>> {
    let parent = crate_manifest.parent().unwrap_or(Path::new("."));
    let parent = if parent.as_os_str().is_empty() {
        Path::new(".")
    } else {
        parent
    };
    let crate_dir = parent
        .canonicalize()
        .context("Failed to resolve crate directory")?;

    // Walk up from the crate directory looking for a workspace root
    let mut dir = crate_dir.clone();
    loop {
        let candidate = dir.join("Cargo.toml");
        if candidate.exists() && candidate != crate_dir.join("Cargo.toml") {
            let content = std::fs::read_to_string(&candidate)?;
            if content.contains("[workspace]") {
                return Ok(Some(candidate));
            }
        }
        if !dir.pop() {
            break;
        }
    }

    // Also check if the crate's own Cargo.toml has a [workspace] section
    // (single-crate workspace) — in that case we don't use workspace deps
    Ok(None)
}

// ============================================================================
// Dependency section helpers
// ============================================================================

/// Return the TOML section name for a dependency kind.
pub(crate) fn dep_kind_section(kind: bphelper_manifest::DepKind) -> &'static str {
    match kind {
        bphelper_manifest::DepKind::Normal => "dependencies",
        bphelper_manifest::DepKind::Dev => "dev-dependencies",
        bphelper_manifest::DepKind::Build => "build-dependencies",
    }
}

/// Write dependencies (with full version+features) to the correct sections by `dep_kind`.
///
/// When `if_missing` is true, only inserts crates that don't already exist in
/// the target section. Returns the number of crates actually written.
// [impl cli.add.dep-kind]
pub(crate) fn write_deps_by_kind(
    doc: &mut toml_edit::DocumentMut,
    crates: &BTreeMap<String, bphelper_manifest::CrateSpec>,
    if_missing: bool,
) -> usize {
    let mut written = 0;
    for (dep_name, dep_spec) in crates {
        let section = dep_kind_section(dep_spec.dep_kind);
        let table = doc[section].or_insert(toml_edit::Item::Table(toml_edit::Table::new()));
        if let Some(table) = table.as_table_mut()
            && (!if_missing || !table.contains_key(dep_name))
        {
            add_dep_to_table(table, dep_name, dep_spec);
            written += 1;
        }
    }
    written
}

/// Write workspace references (`{ workspace = true }`) to the correct
/// dependency sections based on each crate's `dep_kind`.
///
/// When `if_missing` is true, only inserts references for crates that don't
/// already exist in the target section. Returns the number of refs written.
// [impl cli.add.dep-kind]
pub(crate) fn write_workspace_refs_by_kind(
    doc: &mut toml_edit::DocumentMut,
    crates: &BTreeMap<String, bphelper_manifest::CrateSpec>,
    if_missing: bool,
) -> usize {
    let mut written = 0;
    for (dep_name, dep_spec) in crates {
        let section = dep_kind_section(dep_spec.dep_kind);
        let table = doc[section].or_insert(toml_edit::Item::Table(toml_edit::Table::new()));
        if let Some(table) = table.as_table_mut()
            && (!if_missing || !table.contains_key(dep_name))
        {
            let mut dep = toml_edit::InlineTable::new();
            dep.insert("workspace", toml_edit::Value::from(true));
            table.insert(
                dep_name,
                toml_edit::Item::Value(toml_edit::Value::InlineTable(dep)),
            );
            written += 1;
        }
    }
    written
}

/// Add a dependency to a toml_edit table (non-workspace mode).
// [impl manifest.deps.add]
// [impl manifest.deps.version-features]
// [impl manifest.toml.style]
// [impl cli.add.idempotent]
pub(crate) fn add_dep_to_table(
    table: &mut toml_edit::Table,
    name: &str,
    spec: &bphelper_manifest::CrateSpec,
) {
    if spec.features.is_empty() {
        table.insert(name, toml_edit::value(&spec.version));
    } else {
        let mut dep = toml_edit::InlineTable::new();
        dep.insert("version", toml_edit::Value::from(spec.version.as_str()));
        let mut features = toml_edit::Array::new();
        for feat in &spec.features {
            features.push(feat.as_str());
        }
        dep.insert("features", toml_edit::Value::Array(features));
        table.insert(
            name,
            toml_edit::Item::Value(toml_edit::Value::InlineTable(dep)),
        );
    }
}

/// Remove dependencies from the correct sections by `dep_kind`.
///
/// Returns the number of crates actually removed.
pub(crate) fn remove_deps_by_kind(
    doc: &mut toml_edit::DocumentMut,
    crates: &BTreeMap<String, bphelper_manifest::CrateSpec>,
) -> usize {
    let mut removed = 0;
    for (dep_name, dep_spec) in crates {
        let section = dep_kind_section(dep_spec.dep_kind);
        if let Some(table) = doc.get_mut(section).and_then(|t| t.as_table_mut())
            && table.remove(dep_name).is_some()
        {
            removed += 1;
        }
    }
    removed
}

/// Return true when `recommended` is strictly newer than `current` (semver).
///
/// Falls back to string equality when either side is not a valid semver
/// version, so non-standard version strings still get updated when they
/// differ.
pub(crate) fn should_upgrade_version(current: &str, recommended: &str) -> bool {
    match (
        semver::Version::parse(current)
            .or_else(|_| semver::Version::parse(&format!("{}.0", current)))
            .or_else(|_| semver::Version::parse(&format!("{}.0.0", current))),
        semver::Version::parse(recommended)
            .or_else(|_| semver::Version::parse(&format!("{}.0", recommended)))
            .or_else(|_| semver::Version::parse(&format!("{}.0.0", recommended))),
    ) {
        // [impl manifest.sync.version-bump]
        (Ok(cur), Ok(rec)) => rec > cur,
        // Non-parsable: fall back to "update if different"
        _ => current != recommended,
    }
}

/// Sync a dependency in-place: update version if behind, add missing features.
/// Returns true if changes were made.
// [impl manifest.deps.existing]
// [impl manifest.toml.style]
pub(crate) fn sync_dep_in_table(
    table: &mut toml_edit::Table,
    name: &str,
    spec: &bphelper_manifest::CrateSpec,
) -> bool {
    let Some(existing) = table.get_mut(name) else {
        // Not present — add it
        add_dep_to_table(table, name, spec);
        return true;
    };

    let mut changed = false;

    match existing {
        toml_edit::Item::Value(toml_edit::Value::String(version_str)) => {
            let current = version_str.value().to_string();
            // [impl manifest.sync.version-bump]
            if !spec.version.is_empty() && should_upgrade_version(&current, &spec.version) {
                *version_str = toml_edit::Formatted::new(spec.version.clone());
                changed = true;
            }
            // [impl manifest.sync.feature-add]
            if !spec.features.is_empty() {
                let keep_version = if !spec.version.is_empty()
                    && should_upgrade_version(&current, &spec.version)
                {
                    spec.version.clone()
                } else {
                    current.clone()
                };
                let patched = bphelper_manifest::CrateSpec {
                    version: keep_version,
                    features: spec.features.clone(),
                    dep_kind: spec.dep_kind,
                    optional: spec.optional,
                };
                add_dep_to_table(table, name, &patched);
                changed = true;
            }
        }
        toml_edit::Item::Value(toml_edit::Value::InlineTable(inline)) => {
            // [impl manifest.sync.version-bump]
            if let Some(toml_edit::Value::String(v)) = inline.get_mut("version")
                && !spec.version.is_empty()
                && should_upgrade_version(v.value(), &spec.version)
            {
                *v = toml_edit::Formatted::new(spec.version.clone());
                changed = true;
            }
            // [impl manifest.sync.feature-add]
            if !spec.features.is_empty() {
                let existing_features: Vec<String> = inline
                    .get("features")
                    .and_then(|f| f.as_array())
                    .map(|arr| {
                        arr.iter()
                            .filter_map(|v| v.as_str().map(String::from))
                            .collect()
                    })
                    .unwrap_or_default();

                let mut needs_update = false;
                let existing_set: BTreeSet<&str> =
                    existing_features.iter().map(|s| s.as_str()).collect();
                let mut all_features = existing_features.clone();
                for feat in &spec.features {
                    if !existing_set.contains(feat.as_str()) {
                        all_features.push(feat.clone());
                        needs_update = true;
                    }
                }

                if needs_update {
                    let mut arr = toml_edit::Array::new();
                    for f in &all_features {
                        arr.push(f.as_str());
                    }
                    inline.insert("features", toml_edit::Value::Array(arr));
                    changed = true;
                }
            }
        }
        toml_edit::Item::Table(tbl) => {
            // [impl manifest.sync.version-bump]
            if let Some(toml_edit::Item::Value(toml_edit::Value::String(v))) =
                tbl.get_mut("version")
                && !spec.version.is_empty()
                && should_upgrade_version(v.value(), &spec.version)
            {
                *v = toml_edit::Formatted::new(spec.version.clone());
                changed = true;
            }
            // [impl manifest.sync.feature-add]
            if !spec.features.is_empty() {
                let existing_features: Vec<String> = tbl
                    .get("features")
                    .and_then(|f| f.as_value())
                    .and_then(|v| v.as_array())
                    .map(|arr| {
                        arr.iter()
                            .filter_map(|v| v.as_str().map(String::from))
                            .collect()
                    })
                    .unwrap_or_default();

                let existing_set: BTreeSet<&str> =
                    existing_features.iter().map(|s| s.as_str()).collect();
                let mut all_features = existing_features.clone();
                let mut needs_update = false;
                for feat in &spec.features {
                    if !existing_set.contains(feat.as_str()) {
                        all_features.push(feat.clone());
                        needs_update = true;
                    }
                }

                if needs_update {
                    let mut arr = toml_edit::Array::new();
                    for f in &all_features {
                        arr.push(f.as_str());
                    }
                    tbl.insert(
                        "features",
                        toml_edit::Item::Value(toml_edit::Value::Array(arr)),
                    );
                    changed = true;
                }
            }
        }
        _ => {}
    }

    changed
}

// ============================================================================
// Feature reading / writing
// ============================================================================

/// Read active features from a parsed TOML value at a given path prefix.
///
/// `prefix` is `&["package", "metadata"]` for package metadata or
/// `&["workspace", "metadata"]` for workspace metadata.
// [impl manifest.features.storage]
pub(crate) fn read_features_at(
    raw: &toml::Value,
    prefix: &[&str],
    bp_name: &str,
) -> BTreeSet<String> {
    let mut node = Some(raw);
    for key in prefix {
        node = node.and_then(|n| n.get(key));
    }
    node.and_then(|m| m.get("battery-pack"))
        .and_then(|bp| bp.get(bp_name))
        .and_then(|entry| entry.get("features"))
        .and_then(|sets| sets.as_array())
        .map(|arr| {
            arr.iter()
                .filter_map(|v| v.as_str().map(String::from))
                .collect()
        })
        .unwrap_or_else(|| BTreeSet::from(["default".to_string()]))
}

/// Read active features for a battery pack from user's package metadata.
pub(crate) fn read_active_features(manifest_content: &str, bp_name: &str) -> BTreeSet<String> {
    let raw: toml::Value = match toml::from_str(manifest_content) {
        Ok(v) => v,
        Err(_) => return BTreeSet::from(["default".to_string()]),
    };
    read_features_at(&raw, &["package", "metadata"], bp_name)
}

/// Read active features from `workspace.metadata.battery-pack[bp_name].features`.
pub(crate) fn read_active_features_ws(ws_content: &str, bp_name: &str) -> BTreeSet<String> {
    let raw: toml::Value = match toml::from_str(ws_content) {
        Ok(v) => v,
        Err(_) => return BTreeSet::from(["default".to_string()]),
    };
    read_features_at(&raw, &["workspace", "metadata"], bp_name)
}

// ============================================================================
// Metadata location abstraction
// ============================================================================

/// Where battery-pack metadata (registrations, active features) is stored.
///
/// `add_battery_pack` writes to either `package.metadata` or `workspace.metadata`
/// depending on the `--target` flag. All other commands (sync, enable, load) must
/// read from the same location, so they use `resolve_metadata_location` to detect
/// where metadata currently lives.
#[derive(Debug, Clone)]
pub(crate) enum MetadataLocation {
    /// `package.metadata.battery-pack` in the user manifest.
    Package,
    /// `workspace.metadata.battery-pack` in the workspace manifest.
    Workspace { ws_manifest_path: PathBuf },
}

/// Determine where battery-pack metadata lives for this project.
///
/// If a workspace manifest exists AND already contains
/// `workspace.metadata.battery-pack`, returns `Workspace`.
/// Otherwise returns `Package`.
pub(crate) fn resolve_metadata_location(user_manifest_path: &Path) -> Result<MetadataLocation> {
    if let Some(ws_path) = find_workspace_manifest(user_manifest_path)? {
        let ws_content =
            std::fs::read_to_string(&ws_path).context("Failed to read workspace Cargo.toml")?;
        let raw: toml::Value =
            toml::from_str(&ws_content).context("Failed to parse workspace Cargo.toml")?;
        if raw
            .get("workspace")
            .and_then(|w| w.get("metadata"))
            .and_then(|m| m.get("battery-pack"))
            .is_some()
        {
            return Ok(MetadataLocation::Workspace {
                ws_manifest_path: ws_path,
            });
        }
    }
    Ok(MetadataLocation::Package)
}

/// Read active features for a battery pack, respecting metadata location.
pub(crate) fn read_active_features_from(
    location: &MetadataLocation,
    user_manifest_content: &str,
    bp_name: &str,
) -> BTreeSet<String> {
    match location {
        MetadataLocation::Package => read_active_features(user_manifest_content, bp_name),
        MetadataLocation::Workspace { ws_manifest_path } => {
            let ws_content = match std::fs::read_to_string(ws_manifest_path) {
                Ok(c) => c,
                Err(_) => return BTreeSet::from(["default".to_string()]),
            };
            read_active_features_ws(&ws_content, bp_name)
        }
    }
}

/// Read managed-deps from a parsed TOML value at a given path prefix.
///
/// Returns `None` when the key is absent (old-format metadata).
fn read_managed_deps_at(
    raw: &toml::Value,
    prefix: &[&str],
    bp_name: &str,
) -> Option<BTreeSet<String>> {
    let mut node = Some(raw);
    for key in prefix {
        node = node.and_then(|n| n.get(key));
    }
    let arr = node
        .and_then(|m| m.get("battery-pack"))
        .and_then(|bp| bp.get(bp_name))
        .and_then(|entry| entry.get("managed-deps"))
        .and_then(|v| v.as_array())?;
    Some(
        arr.iter()
            .filter_map(|v| v.as_str().map(String::from))
            .collect(),
    )
}

/// Read managed-deps for a battery pack, respecting metadata location.
///
/// Returns `None` when the key is absent (old-format / pre-migration metadata).
pub(crate) fn read_managed_deps_from(
    location: &MetadataLocation,
    user_manifest_content: &str,
    bp_name: &str,
) -> Option<BTreeSet<String>> {
    let (content, prefix): (std::borrow::Cow<'_, str>, &[&str]) = match location {
        MetadataLocation::Package => (
            std::borrow::Cow::Borrowed(user_manifest_content),
            &["package", "metadata"],
        ),
        MetadataLocation::Workspace { ws_manifest_path } => {
            let ws = std::fs::read_to_string(ws_manifest_path).ok()?;
            (std::borrow::Cow::Owned(ws), &["workspace", "metadata"])
        }
    };
    let raw: toml::Value = toml::from_str(&content).ok()?;
    read_managed_deps_at(&raw, prefix, bp_name)
}

/// Write features and optional managed-deps into a `toml_edit::DocumentMut`.
///
/// `path_prefix` is `["package", "metadata"]` for package metadata or
/// `["workspace", "metadata"]` for workspace metadata.
///
/// Writes as a regular TOML table:
/// ```toml
/// [package.metadata.battery-pack.cli-battery-pack]
/// features = ["default", "indicators"]
/// managed-deps = ["clap", "dialoguer", "console"]
/// ```
pub(crate) fn write_bp_features_to_doc(
    doc: &mut toml_edit::DocumentMut,
    path_prefix: &[&str],
    bp_name: &str,
    active_features: &BTreeSet<String>,
    managed_deps: Option<&BTreeSet<String>>,
) {
    let mut features_array = toml_edit::Array::new();
    for feature in active_features {
        features_array.push(feature.as_str());
    }

    doc[path_prefix[0]].or_insert(toml_edit::Item::Table(toml_edit::Table::new()));
    doc[path_prefix[0]][path_prefix[1]].or_insert(toml_edit::Item::Table(toml_edit::Table::new()));
    doc[path_prefix[0]][path_prefix[1]]["battery-pack"]
        .or_insert(toml_edit::Item::Table(toml_edit::Table::new()));

    let bp_table = doc[path_prefix[0]][path_prefix[1]]["battery-pack"]
        .as_table_mut()
        .expect("battery-pack table must exist");

    // Preserve existing managed-deps when caller passes None.
    let existing_managed_deps: Option<toml_edit::Array> = bp_table
        .get(bp_name)
        .and_then(|item| {
            let val = if let Some(t) = item.as_table() {
                t.get("managed-deps").and_then(|i| i.as_value())
            } else {
                item.as_inline_table().and_then(|t| t.get("managed-deps"))
            };
            val.and_then(|v| v.as_array())
        })
        .cloned();

    let mut entry_table = toml_edit::Table::new();
    entry_table.insert(
        "features",
        toml_edit::Item::Value(toml_edit::Value::Array(features_array)),
    );
    if let Some(deps) = managed_deps {
        let mut deps_array = toml_edit::Array::new();
        for dep in deps {
            deps_array.push(dep.as_str());
        }
        entry_table.insert(
            "managed-deps",
            toml_edit::Item::Value(toml_edit::Value::Array(deps_array)),
        );
    } else if let Some(arr) = existing_managed_deps {
        entry_table.insert(
            "managed-deps",
            toml_edit::Item::Value(toml_edit::Value::Array(arr)),
        );
    }
    entry_table.set_implicit(true);
    bp_table.insert(bp_name, toml_edit::Item::Table(entry_table));
}

/// Resolve the manifest path for a battery pack using `cargo metadata`.
///
/// Works for any dependency source: path deps, registry deps, git deps.
/// The battery pack must already be in [build-dependencies].
pub(crate) fn resolve_battery_pack_manifest(bp_name: &str) -> Result<PathBuf> {
    let metadata = cargo_metadata::MetadataCommand::new()
        .exec()
        .context("Failed to run `cargo metadata`")?;

    let package = metadata
        .packages
        .iter()
        .find(|p| p.name == bp_name)
        .ok_or_else(|| {
            anyhow::anyhow!(
                "Battery pack '{}' not found in dependency graph. Is it in [build-dependencies]?",
                bp_name
            )
        })?;

    Ok(package.manifest_path.clone().into())
}

// ============================================================================
// Version collection for status
// ============================================================================

#[cfg(test)]
mod tests;
