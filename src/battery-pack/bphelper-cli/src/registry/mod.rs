//! Crates.io / local-source registry, API types, and shared data types.
//!
//! This module handles looking up, downloading, and inspecting battery packs
//! from crates.io or a local workspace. It also defines the shared data types
//! used by both the TUI and text output paths.

use anyhow::{Context, Result, bail};
use flate2::read::GzDecoder;
use serde::Deserialize;
use std::collections::{BTreeMap, BTreeSet};
use std::path::{Path, PathBuf};
use tar::Archive;

use crate::manifest::resolve_battery_pack_manifest;

const CRATES_IO_API: &str = "https://crates.io/api/v1/crates";
const CRATES_IO_CDN: &str = "https://static.crates.io/crates";

fn http_client() -> &'static reqwest::blocking::Client {
    static CLIENT: std::sync::OnceLock<reqwest::blocking::Client> = std::sync::OnceLock::new();
    CLIENT.get_or_init(|| {
        reqwest::blocking::Client::builder()
            .user_agent("cargo-bp (https://github.com/battery-pack-rs/battery-pack)")
            .build()
            .expect("failed to build HTTP client")
    })
}

// [impl cli.source.flag]
// [impl cli.source.replace]
#[derive(Debug, Clone)]
pub(crate) enum CrateSource {
    Registry,
    Local(PathBuf),
}

// ============================================================================
// crates.io API types
// ============================================================================

#[derive(Deserialize)]
struct CratesIoResponse {
    versions: Vec<VersionInfo>,
}

#[derive(Deserialize)]
struct VersionInfo {
    num: String,
    yanked: bool,
}

#[derive(Deserialize)]
struct SearchResponse {
    crates: Vec<SearchCrate>,
}

#[derive(Deserialize)]
struct SearchCrate {
    name: String,
    max_version: String,
    description: Option<String>,
}

/// Backward-compatible alias for `bphelper_manifest::TemplateSpec`.
pub(crate) type TemplateConfig = bphelper_manifest::TemplateSpec;

// ============================================================================
// crates.io owner types
// ============================================================================

#[derive(Deserialize)]
struct OwnersResponse {
    users: Vec<Owner>,
}

#[derive(Deserialize, Clone)]
pub(crate) struct Owner {
    login: String,
    name: Option<String>,
}

// ============================================================================
// GitHub API types
// ============================================================================

#[derive(Deserialize)]
struct GitHubTreeResponse {
    tree: Vec<GitHubTreeEntry>,
    #[serde(default)]
    #[allow(dead_code)]
    truncated: bool,
}

#[derive(Deserialize)]
struct GitHubTreeEntry {
    path: String,
}

// ============================================================================
// Shared data types (used by both TUI and text output)
// ============================================================================

/// Summary info for displaying in a list
#[derive(Clone)]
pub(crate) struct BatteryPackSummary {
    pub name: String,
    pub short_name: String,
    pub version: String,
    pub description: String,
}

/// Detailed battery pack info
#[derive(Clone)]
pub(crate) struct BatteryPackDetail {
    pub name: String,
    pub short_name: String,
    pub version: String,
    pub description: String,
    pub repository: Option<String>,
    pub owners: Vec<OwnerInfo>,
    pub crates: Vec<String>,
    pub extends: Vec<String>,
    pub features: BTreeMap<String, Vec<String>>,
    pub templates: Vec<TemplateInfo>,
    pub examples: Vec<ExampleInfo>,
}

#[derive(Clone)]
pub(crate) struct OwnerInfo {
    pub login: String,
    pub name: Option<String>,
}

impl From<Owner> for OwnerInfo {
    fn from(o: Owner) -> Self {
        Self {
            login: o.login,
            name: o.name,
        }
    }
}

#[derive(Clone)]
pub(crate) struct TemplateInfo {
    pub name: String,
    pub path: String,
    pub description: Option<String>,
    /// Full path in the repository (e.g., "src/cli-battery-pack/templates/simple")
    /// Resolved by searching the GitHub tree API
    pub repo_path: Option<String>,
}

#[derive(Clone)]
pub(crate) struct ExampleInfo {
    pub name: String,
    pub description: Option<String>,
    /// Full path in the repository (e.g., "src/cli-battery-pack/examples/mini-grep.rs")
    /// Resolved by searching the GitHub tree API
    pub repo_path: Option<String>,
}

pub(crate) struct CrateMetadata {
    pub(crate) version: String,
}

/// Look up a crate on crates.io and return its metadata
pub(crate) fn lookup_crate(crate_name: &str) -> Result<CrateMetadata> {
    let client = http_client();

    let url = format!("{}/{}", CRATES_IO_API, crate_name);
    let response = client
        .get(&url)
        .send()
        .with_context(|| format!("Failed to query crates.io for '{}'", crate_name))?;

    if !response.status().is_success() {
        bail!(
            "Crate '{}' not found on crates.io (status: {})",
            crate_name,
            response.status()
        );
    }

    let parsed: CratesIoResponse = response
        .json()
        .with_context(|| format!("Failed to parse crates.io response for '{}'", crate_name))?;

    // Find the latest non-yanked version
    let version = parsed
        .versions
        .iter()
        .find(|v| !v.yanked)
        .map(|v| v.num.clone())
        .ok_or_else(|| anyhow::anyhow!("No non-yanked versions found for '{}'", crate_name))?;

    Ok(CrateMetadata { version })
}

/// Download a crate tarball and extract it to a temp directory
pub(crate) fn download_and_extract_crate(
    crate_name: &str,
    version: &str,
) -> Result<tempfile::TempDir> {
    let client = http_client();

    // Download from CDN: https://static.crates.io/crates/{name}/{name}-{version}.crate
    let url = format!(
        "{}/{}/{}-{}.crate",
        CRATES_IO_CDN, crate_name, crate_name, version
    );

    let response = client
        .get(&url)
        .send()
        .with_context(|| format!("Failed to download crate from {}", url))?;

    if !response.status().is_success() {
        bail!(
            "Failed to download '{}' version {} (status: {})",
            crate_name,
            version,
            response.status()
        );
    }

    let bytes = response
        .bytes()
        .with_context(|| "Failed to read crate tarball")?;

    // Create temp directory and extract
    let temp_dir = tempfile::tempdir().with_context(|| "Failed to create temp directory")?;

    let decoder = GzDecoder::new(&bytes[..]);
    let mut archive = Archive::new(decoder);
    archive
        .unpack(temp_dir.path())
        .with_context(|| "Failed to extract crate tarball")?;

    Ok(temp_dir)
}

pub(crate) fn fetch_bp_spec_from_registry(
    crate_name: &str,
) -> Result<(String, bphelper_manifest::BatteryPackSpec)> {
    let crate_info = lookup_crate(crate_name)?;
    let temp_dir = download_and_extract_crate(crate_name, &crate_info.version)?;
    let crate_dir = temp_dir
        .path()
        .join(format!("{}-{}", crate_name, crate_info.version));

    let manifest_path = crate_dir.join("Cargo.toml");
    let manifest_content = std::fs::read_to_string(&manifest_path)
        .with_context(|| format!("Failed to read {}", manifest_path.display()))?;

    let spec = bphelper_manifest::parse_battery_pack(&manifest_content)
        .map_err(|e| anyhow::anyhow!("Failed to parse battery pack '{}': {}", crate_name, e))?;

    Ok((crate_info.version, spec))
}

// ============================================================================
// bp-managed dependency resolution
// ============================================================================

/// Resolve `bp-managed = true` dependencies in a Cargo.toml string,
/// returning the rewritten content with concrete versions.
pub fn resolve_bp_managed_content(content: &str, bp_crate_root: &Path) -> Result<String> {
    let mut doc: toml_edit::DocumentMut = content.parse().context("failed to parse Cargo.toml")?;

    // Collect all bp-managed dep names across all sections.
    let sections = ["dependencies", "dev-dependencies", "build-dependencies"];
    let mut has_managed = false;
    for section in &sections {
        if let Some(table) = doc.get(section).and_then(|v| v.as_table()) {
            for (name, value) in table.iter() {
                if is_bp_managed(value) {
                    has_managed = true;
                    let extra = extra_keys_on_bp_managed(value);
                    if !extra.is_empty() {
                        bail!(
                            "dependency '{}' in [{}] has `bp-managed = true` with conflicting keys: {}",
                            name,
                            section,
                            extra.join(", ")
                        );
                    }
                }
            }
        }
    }

    if !has_managed {
        return Ok(content.to_string());
    }

    // Read active features for each battery pack from the generated manifest's metadata.
    let raw: toml::Value = toml::from_str(content).context("failed to parse Cargo.toml")?;

    // Discover battery pack specs reachable from bp_crate_root.
    let all_specs = bphelper_manifest::discover_from_crate_root(bp_crate_root)?;

    // Build a merged map of crate_name -> (version, features, dep_kind) from all
    // battery packs referenced in the generated project's metadata.
    let mut resolved: std::collections::BTreeMap<String, bphelper_manifest::CrateSpec> =
        std::collections::BTreeMap::new();
    // Also track battery pack versions for resolving bp-managed build-deps.
    let mut bp_versions: std::collections::BTreeMap<String, String> =
        std::collections::BTreeMap::new();

    let bp_metadata = raw
        .get("package")
        .and_then(|p| p.get("metadata"))
        .and_then(|m| m.get("battery-pack"))
        .and_then(|bp| bp.as_table());

    if let Some(bp_table) = bp_metadata {
        for (bp_name, _entry) in bp_table {
            let active_features =
                crate::manifest::read_features_at(&raw, &["package", "metadata"], bp_name);

            let spec = if let Some(s) = all_specs.iter().find(|s| s.name == *bp_name) {
                s.clone()
            } else {
                // Battery pack not in local workspace; fetch from crates.io.
                let (_version, s) = fetch_bp_spec_from_registry(bp_name).with_context(|| {
                    format!("battery pack '{bp_name}' not found locally or on crates.io")
                })?;
                s
            };

            bp_versions.insert(bp_name.clone(), spec.version.clone());
            let crates = spec.resolve_for_features(&active_features);
            for (crate_name, crate_spec) in crates {
                resolved.insert(crate_name, crate_spec);
            }
        }
    }

    // Rewrite bp-managed entries with resolved versions.
    for section in &sections {
        let Some(table) = doc.get_mut(section).and_then(|v| v.as_table_mut()) else {
            continue;
        };

        let managed_names: Vec<String> = table
            .iter()
            .filter(|(_, v)| is_bp_managed_item(v))
            .map(|(k, _)| k.to_string())
            .collect();

        for name in managed_names {
            if let Some(crate_spec) = resolved.get(&name) {
                // Regular dependency managed by a battery pack.
                crate::manifest::add_dep_to_table(table, &name, crate_spec);
            } else if let Some(bp_version) = bp_versions.get(&name) {
                // Battery pack itself in [build-dependencies].
                table.insert(&name, toml_edit::value(bp_version));
            } else {
                bail!(
                    "dependency '{}' in [{}] has `bp-managed = true` but no battery pack provides it",
                    name,
                    section
                );
            }
        }
    }

    Ok(doc.to_string())
}

/// Check if a toml_edit Item has `bp-managed = true`.
fn is_bp_managed_item(item: &toml_edit::Item) -> bool {
    match item {
        toml_edit::Item::Value(toml_edit::Value::InlineTable(t)) => t
            .get("bp-managed")
            .and_then(|v| v.as_bool())
            .unwrap_or(false),
        toml_edit::Item::Table(t) => t
            .get("bp-managed")
            .and_then(|v| v.as_value())
            .and_then(|v| v.as_bool())
            .unwrap_or(false),
        _ => false,
    }
}

/// Check if a toml::Value has `bp-managed = true`.
fn is_bp_managed(value: &toml_edit::Item) -> bool {
    is_bp_managed_item(value)
}

/// Return any keys besides `bp-managed` on a bp-managed dep entry.
fn extra_keys_on_bp_managed(value: &toml_edit::Item) -> Vec<String> {
    let keys: Box<dyn Iterator<Item = &str>> = match value {
        toml_edit::Item::Value(toml_edit::Value::InlineTable(t)) => {
            Box::new(t.iter().map(|(k, _)| k))
        }
        toml_edit::Item::Table(t) => Box::new(t.iter().map(|(k, _)| k)),
        _ => return vec![],
    };
    keys.filter(|k| *k != "bp-managed")
        .map(String::from)
        .collect()
}

pub(crate) fn fetch_battery_pack_spec(bp_name: &str) -> Result<bphelper_manifest::BatteryPackSpec> {
    let manifest_path = resolve_battery_pack_manifest(bp_name)?;
    let manifest_content = std::fs::read_to_string(&manifest_path)
        .with_context(|| format!("Failed to read {}", manifest_path.display()))?;

    bphelper_manifest::parse_battery_pack(&manifest_content)
        .map_err(|e| anyhow::anyhow!("Failed to parse battery pack '{}': {}", bp_name, e))
}

pub(crate) fn load_installed_bp_spec(
    bp_name: &str,
    path: Option<&str>,
    source: &CrateSource,
) -> Result<bphelper_manifest::BatteryPackSpec> {
    if let Some(local_path) = path {
        let manifest_path = Path::new(local_path).join("Cargo.toml");
        let manifest_content = std::fs::read_to_string(&manifest_path)
            .with_context(|| format!("Failed to read {}", manifest_path.display()))?;
        return bphelper_manifest::parse_battery_pack(&manifest_content)
            .map_err(|e| anyhow::anyhow!("Failed to parse battery pack '{}': {}", bp_name, e));
    }
    match source {
        CrateSource::Registry => fetch_battery_pack_spec(bp_name),
        CrateSource::Local(_) => {
            let (_version, spec) = fetch_bp_spec(source, bp_name)?;
            Ok(spec)
        }
    }
}

pub(crate) struct InstalledPack {
    pub short_name: String,
    pub version: String,
    pub spec: bphelper_manifest::BatteryPackSpec,
    pub active_features: BTreeSet<String>,
}

pub(crate) fn fetch_battery_pack_list(
    source: &CrateSource,
    filter: Option<&str>,
) -> Result<Vec<BatteryPackSummary>> {
    match source {
        CrateSource::Registry => fetch_battery_pack_list_from_registry(filter),
        CrateSource::Local(path) => discover_local_battery_packs(path, filter),
    }
}

fn fetch_battery_pack_list_from_registry(filter: Option<&str>) -> Result<Vec<BatteryPackSummary>> {
    let client = http_client();

    // Build the search URL with keyword filter
    let url = match filter {
        Some(q) => format!(
            "{CRATES_IO_API}?q={}&keyword=battery-pack&per_page=50",
            urlencoding::encode(q)
        ),
        None => format!("{CRATES_IO_API}?keyword=battery-pack&per_page=50"),
    };

    let response = client
        .get(&url)
        .send()
        .context("Failed to query crates.io")?;

    if !response.status().is_success() {
        bail!(
            "Failed to list battery packs (status: {})",
            response.status()
        );
    }

    let parsed: SearchResponse = response.json().context("Failed to parse response")?;

    // Filter to only crates whose name ends with "-battery-pack"
    let battery_packs = parsed
        .crates
        .into_iter()
        .filter(|c| c.name.ends_with("-battery-pack"))
        .map(|c| BatteryPackSummary {
            short_name: short_name(&c.name).to_string(),
            name: c.name,
            version: c.max_version,
            description: c.description.unwrap_or_default(),
        })
        .collect();

    Ok(battery_packs)
}

pub(crate) fn discover_local_battery_packs(
    workspace_dir: &Path,
    filter: Option<&str>,
) -> Result<Vec<BatteryPackSummary>> {
    let manifest_path = workspace_dir.join("Cargo.toml");
    let metadata = cargo_metadata::MetadataCommand::new()
        .manifest_path(&manifest_path)
        .no_deps()
        .exec()
        .with_context(|| format!("Failed to read workspace at {}", manifest_path.display()))?;

    let mut battery_packs: Vec<BatteryPackSummary> = metadata
        .packages
        .iter()
        .filter(|pkg| pkg.name.ends_with("-battery-pack"))
        .filter(|pkg| {
            if let Some(q) = filter {
                short_name(&pkg.name).contains(q)
            } else {
                true
            }
        })
        .map(|pkg| BatteryPackSummary {
            short_name: short_name(&pkg.name).to_string(),
            name: pkg.name.to_string(),
            version: pkg.version.to_string(),
            description: pkg.description.clone().unwrap_or_default(),
        })
        .collect();

    battery_packs.sort_by(|a, b| a.name.cmp(&b.name));
    Ok(battery_packs)
}

/// Find a specific battery pack's directory within a local workspace.
pub(crate) fn find_local_battery_pack_dir(
    workspace_dir: &Path,
    crate_name: &str,
) -> Result<PathBuf> {
    let manifest_path = workspace_dir.join("Cargo.toml");
    let metadata = cargo_metadata::MetadataCommand::new()
        .manifest_path(&manifest_path)
        .no_deps()
        .exec()
        .with_context(|| format!("Failed to read workspace at {}", manifest_path.display()))?;

    let package = metadata
        .packages
        .iter()
        .find(|p| p.name == crate_name)
        .ok_or_else(|| {
            anyhow::anyhow!(
                "Battery pack '{}' not found in workspace at {}",
                crate_name,
                workspace_dir.display()
            )
        })?;

    Ok(package
        .manifest_path
        .parent()
        .expect("manifest path should have a parent")
        .into())
}

pub(crate) fn fetch_bp_spec(
    source: &CrateSource,
    name: &str,
) -> Result<(Option<String>, bphelper_manifest::BatteryPackSpec)> {
    let crate_name = resolve_crate_name(name);
    match source {
        CrateSource::Registry => {
            let (version, spec) = fetch_bp_spec_from_registry(&crate_name)?;
            Ok((Some(version), spec))
        }
        CrateSource::Local(workspace_dir) => {
            let crate_dir = find_local_battery_pack_dir(workspace_dir, &crate_name)?;
            let manifest_path = crate_dir.join("Cargo.toml");
            let manifest_content = std::fs::read_to_string(&manifest_path)
                .with_context(|| format!("Failed to read {}", manifest_path.display()))?;
            let spec = bphelper_manifest::parse_battery_pack(&manifest_content).map_err(|e| {
                anyhow::anyhow!("Failed to parse battery pack '{}': {}", crate_name, e)
            })?;
            Ok((None, spec))
        }
    }
}

/// Fetch detailed battery pack info, dispatching based on source.
// [impl cli.source.replace]
pub(crate) fn fetch_battery_pack_detail_from_source(
    source: &CrateSource,
    name: &str,
) -> Result<BatteryPackDetail> {
    match source {
        CrateSource::Registry => fetch_battery_pack_detail(name, None),
        CrateSource::Local(workspace_dir) => {
            let crate_name = resolve_crate_name(name);
            let crate_dir = find_local_battery_pack_dir(workspace_dir, &crate_name)?;
            fetch_battery_pack_detail_from_path(&crate_dir.to_string_lossy())
        }
    }
}

pub(crate) fn short_name(crate_name: &str) -> &str {
    crate_name
        .strip_suffix("-battery-pack")
        .unwrap_or(crate_name)
}

/// Convert "cli" to "cli-battery-pack" (adds suffix if not already present)
/// Special case: "battery-pack" stays as "battery-pack" (not "battery-pack-battery-pack")
// [impl cli.name.resolve]
// [impl cli.name.exact]
pub(crate) fn resolve_crate_name(name: &str) -> String {
    if name == "battery-pack" || name.ends_with("-battery-pack") {
        name.to_string()
    } else {
        format!("{}-battery-pack", name)
    }
}

pub(crate) fn fetch_battery_pack_detail(
    name: &str,
    path: Option<&str>,
) -> Result<BatteryPackDetail> {
    // If path is provided, use local directory
    if let Some(local_path) = path {
        return fetch_battery_pack_detail_from_path(local_path);
    }

    let crate_name = resolve_crate_name(name);

    // Look up crate info and download
    let crate_info = lookup_crate(&crate_name)?;
    let temp_dir = download_and_extract_crate(&crate_name, &crate_info.version)?;
    let crate_dir = temp_dir
        .path()
        .join(format!("{}-{}", crate_name, crate_info.version));

    // Parse the battery pack spec
    let manifest_path = crate_dir.join("Cargo.toml");
    let manifest_content = std::fs::read_to_string(&manifest_path)
        .with_context(|| format!("Failed to read {}", manifest_path.display()))?;
    let spec = bphelper_manifest::parse_battery_pack(&manifest_content)
        .map_err(|e| anyhow::anyhow!("Failed to parse battery pack: {}", e))?;

    // Fetch owners from crates.io
    let owners = fetch_owners(&crate_name)?;

    build_battery_pack_detail(&crate_dir, &spec, owners)
}

/// Fetch detailed battery pack info from a local path
fn fetch_battery_pack_detail_from_path(path: &str) -> Result<BatteryPackDetail> {
    let crate_dir = std::path::Path::new(path);
    let manifest_path = crate_dir.join("Cargo.toml");
    let manifest_content = std::fs::read_to_string(&manifest_path)
        .with_context(|| format!("Failed to read {}", manifest_path.display()))?;

    let spec = bphelper_manifest::parse_battery_pack(&manifest_content)
        .map_err(|e| anyhow::anyhow!("Failed to parse battery pack: {}", e))?;

    build_battery_pack_detail(crate_dir, &spec, Vec::new())
}

/// Build `BatteryPackDetail` from a parsed `BatteryPackSpec`.
///
/// Derives extends/crates from the spec's crate keys, fetches repo tree for
/// template path resolution, and scans for examples.
pub(crate) fn build_battery_pack_detail(
    crate_dir: &Path,
    spec: &bphelper_manifest::BatteryPackSpec,
    owners: Vec<Owner>,
) -> Result<BatteryPackDetail> {
    // Split visible (non-hidden) crate keys into battery packs (extends) and regular crates
    // [impl format.hidden.effect]
    let (extends_raw, crates_raw): (Vec<_>, Vec<_>) = spec
        .visible_crates()
        .into_keys()
        .partition(|d| d.ends_with("-battery-pack"));

    let extends: Vec<String> = extends_raw
        .into_iter()
        .map(|d| short_name(d).to_string())
        .collect();
    let crates: Vec<String> = crates_raw.into_iter().map(|s| s.to_string()).collect();

    // Fetch the GitHub repository tree to resolve paths
    let repo_tree = spec.repository.as_ref().and_then(|r| fetch_github_tree(r));

    // Convert templates with resolved repo paths
    let templates = spec
        .templates
        .iter()
        .map(|(name, tmpl)| {
            let repo_path = repo_tree
                .as_ref()
                .and_then(|tree| find_template_path(tree, &tmpl.path));
            TemplateInfo {
                name: name.clone(),
                path: tmpl.path.clone(),
                description: tmpl.description.clone(),
                repo_path,
            }
        })
        .collect();

    // Scan examples directory
    let examples = scan_examples(crate_dir, repo_tree.as_deref());

    // Build features map (sorted, visible crates only)
    let features: BTreeMap<String, Vec<String>> = spec
        .features
        .iter()
        .map(|(name, members)| {
            let visible: Vec<String> = members
                .iter()
                .filter(|c| !spec.is_hidden(c))
                .cloned()
                .collect();
            (name.clone(), visible)
        })
        .filter(|(_, members)| !members.is_empty())
        .collect();

    Ok(BatteryPackDetail {
        short_name: short_name(&spec.name).to_string(),
        name: spec.name.clone(),
        version: spec.version.clone(),
        description: spec.description.clone(),
        repository: spec.repository.clone(),
        owners: owners.into_iter().map(OwnerInfo::from).collect(),
        crates,
        extends,
        features,
        templates,
        examples,
    })
}

fn fetch_owners(crate_name: &str) -> Result<Vec<Owner>> {
    let client = http_client();

    let url = format!("{}/{}/owners", CRATES_IO_API, crate_name);
    let response = client
        .get(&url)
        .send()
        .with_context(|| format!("Failed to fetch owners for '{}'", crate_name))?;

    if !response.status().is_success() {
        // Not fatal - just return empty
        return Ok(Vec::new());
    }

    let parsed: OwnersResponse = response
        .json()
        .with_context(|| "Failed to parse owners response")?;

    Ok(parsed.users)
}

fn scan_examples(crate_dir: &std::path::Path, repo_tree: Option<&[String]>) -> Vec<ExampleInfo> {
    let examples_dir = crate_dir.join("examples");
    if !examples_dir.exists() {
        return Vec::new();
    }

    let mut examples = Vec::new();

    if let Ok(entries) = std::fs::read_dir(&examples_dir) {
        for entry in entries.flatten() {
            let path = entry.path();
            if path.extension().is_some_and(|ext| ext == "rs")
                && let Some(name) = path.file_stem().and_then(|s| s.to_str())
            {
                let description = extract_example_description(&path);
                let repo_path = repo_tree.and_then(|tree| find_example_path(tree, name));
                examples.push(ExampleInfo {
                    name: name.to_string(),
                    description,
                    repo_path,
                });
            }
        }
    }

    // Sort by name
    examples.sort_by(|a, b| a.name.cmp(&b.name));
    examples
}

/// Extract description from the first doc comment in an example file
fn extract_example_description(path: &std::path::Path) -> Option<String> {
    let content = std::fs::read_to_string(path).ok()?;

    // Look for //! doc comments at the start
    for line in content.lines() {
        let trimmed = line.trim();
        if trimmed.starts_with("//!") {
            let desc = trimmed.strip_prefix("//!").unwrap_or("").trim();
            if !desc.is_empty() {
                return Some(desc.to_string());
            }
        } else if !trimmed.is_empty() && !trimmed.starts_with("//") {
            // Stop at first non-comment, non-empty line
            break;
        }
    }
    None
}

fn fetch_github_tree(repository: &str) -> Option<Vec<String>> {
    // Parse GitHub URL: https://github.com/owner/repo
    let gh_path = repository
        .strip_prefix("https://github.com/")
        .or_else(|| repository.strip_prefix("http://github.com/"))?;
    let gh_path = gh_path.strip_suffix(".git").unwrap_or(gh_path);
    let gh_path = gh_path.trim_end_matches('/');

    let client = http_client();

    // Fetch the tree recursively using the main branch
    let url = format!(
        "https://api.github.com/repos/{}/git/trees/main?recursive=1",
        gh_path
    );

    let response = client.get(&url).send().ok()?;
    if !response.status().is_success() {
        return None;
    }

    let tree_response: GitHubTreeResponse = response.json().ok()?;

    // Extract all paths (both blobs/files and trees/directories)
    Some(tree_response.tree.into_iter().map(|e| e.path).collect())
}

/// Find the full repository path for an example file.
/// Searches the tree for a file matching "examples/{name}.rs".
pub(crate) fn find_example_path(tree: &[String], example_name: &str) -> Option<String> {
    let suffix = format!("examples/{}.rs", example_name);
    tree.iter().find(|path| path.ends_with(&suffix)).cloned()
}

/// Find the full repository path for a template directory.
/// Searches the tree for a path matching "templates/{name}" or "{name}".
pub(crate) fn find_template_path(tree: &[String], template_path: &str) -> Option<String> {
    // The template path from config might be "templates/simple" or just the relative path
    tree.iter()
        .find(|path| path.ends_with(template_path))
        .cloned()
}

/// A resolved battery pack crate directory. Owns the temp dir (if any) to keep it alive.
pub(crate) struct ResolvedCrate {
    pub dir: PathBuf,
    _temp: Option<tempfile::TempDir>,
}

/// Resolve a battery pack name to a local crate directory.
///
/// If `path_override` is set, uses that directly. Otherwise resolves via
/// `source` (registry download or local workspace lookup).
pub(crate) fn resolve_crate_dir(
    battery_pack: &str,
    path_override: Option<&str>,
    source: &CrateSource,
) -> Result<ResolvedCrate> {
    if let Some(path) = path_override {
        return Ok(ResolvedCrate {
            dir: PathBuf::from(path),
            _temp: None,
        });
    }

    let crate_name = resolve_crate_name(battery_pack);
    match source {
        CrateSource::Registry => {
            let info = lookup_crate(&crate_name)?;
            let temp = download_and_extract_crate(&crate_name, &info.version)?;
            let dir = temp.path().join(format!("{}-{}", crate_name, info.version));
            Ok(ResolvedCrate {
                dir,
                _temp: Some(temp),
            })
        }
        CrateSource::Local(workspace_dir) => {
            let dir = find_local_battery_pack_dir(workspace_dir, &crate_name)?;
            Ok(ResolvedCrate { dir, _temp: None })
        }
    }
}

#[cfg(test)]
mod tests;
