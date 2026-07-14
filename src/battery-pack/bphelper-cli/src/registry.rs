//! Crates.io / local-source registry, API types, and shared data types.
//!
//! This module handles looking up, downloading, and inspecting battery packs
//! from crates.io or a local workspace. It also defines the shared data types
//! used by both the TUI and text output paths.

use anyhow::{Context, Result, bail};
use bphelper_manifest::{
    BatteryPackSpec, CrateSpec, discover_battery_packs, parse_battery_pack_from_path,
};
use flate2::read::GzDecoder;
use serde::Deserialize;
use std::collections::BTreeMap;
use std::fs;
use std::path::{Path, PathBuf};
use tar::Archive;
use toml_edit::{Array, DocumentMut, InlineTable, Item, TableLike, Value};

use crate::completions::get_cache_dir;
use crate::manifest::{self, resolve_battery_pack_manifest};

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
    pub categories: Vec<CategoryDetail>,
    pub templates: Vec<TemplateInfo>,
    pub examples: Vec<ExampleInfo>,
}

/// A category and its member items, resolved from the spec for display.
#[derive(Clone)]
pub(crate) struct CategoryDetail {
    pub name: String,
    pub title: Option<String>,
    pub description: Option<String>,
    pub pick: bphelper_manifest::PickMode,
    pub members: Vec<String>,
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
    let spec = parse_battery_pack_from_path(&manifest_path)
        .with_context(|| format!("Failed to parse battery pack '{}'", crate_name))?;

    // Cache the parsed spec as JSON for autocomplete (best-effort).
    cache_spec_for_completion(crate_name, &spec);

    Ok((crate_info.version, spec))
}

/// Write a parsed spec to the autocomplete cache as JSON.
fn cache_spec_for_completion(crate_name: &str, spec: &BatteryPackSpec) {
    let cache_dir = get_cache_dir();
    if let Err(err) = fs::create_dir_all(&cache_dir) {
        eprintln!("warning: failed to create completion cache directory: {err}");
        return;
    }

    let json = match serde_json::to_string(&spec) {
        Ok(json) => json,
        Err(err) => {
            eprintln!("warning: failed to serialize '{crate_name}' for completion cache: {err}");
            return;
        }
    };

    let cache_file = cache_dir.join(format!("{crate_name}_spec.json"));

    if let Err(err) = fs::write(&cache_file, json) {
        eprintln!("warning: failed to write completion cache for '{crate_name}': {err}");
    }
}
// ============================================================================
// bp-managed dependency resolution
// ============================================================================

/// Resolve `bp-managed = true` dependencies in a Cargo.toml string,
/// returning the rewritten content with concrete versions.
/// Parse `(pack short name, features)` from a rendered `battery-pack.toml` state file. Unparsable
/// content yields no packs; an entry without features defaults to `default`.
fn active_packs_from_state(state: &str) -> Vec<(String, std::collections::BTreeSet<String>)> {
    let Ok(raw) = toml::from_str::<toml::Value>(state) else {
        return Vec::new();
    };
    raw.get("battery-pack")
        .and_then(|b| b.as_array())
        .map(|entries| {
            entries
                .iter()
                .filter_map(|entry| {
                    let name = entry.get("name")?.as_str()?.to_string();
                    // A present features array is used as-is (empty means base deps only, matching
                    // the metadata path); a missing key means the implicit default feature.
                    let features = match entry.get("features").and_then(|f| f.as_array()) {
                        Some(values) => values
                            .iter()
                            .filter_map(|v| v.as_str().map(str::to_string))
                            .collect::<std::collections::BTreeSet<_>>(),
                        None => std::collections::BTreeSet::from(["default".to_string()]),
                    };
                    Some((name, features))
                })
                .collect()
        })
        .unwrap_or_default()
}

pub fn resolve_bp_managed_content(
    content: &str,
    bp_crate_root: &Path,
    bp_state: Option<&str>,
) -> Result<String> {
    let mut doc: DocumentMut = content.parse().context("failed to parse Cargo.toml")?;

    // Detect bp-managed deps and reject any with conflicting keys, before doing
    // the expensive battery-pack discovery below.
    let mut has_managed = false;
    for_each_dep_table(&mut doc, |label, table| {
        has_managed |= scan_section(table, label)?;
        Ok(())
    })?;

    if !has_managed {
        return Ok(content.to_string());
    }

    // Read active features for each battery pack from the generated manifest's metadata.
    let raw: toml::Value = toml::from_str(content).context("failed to parse Cargo.toml")?;

    // Discover battery specs reachable from bp_crate_root.
    let all_specs = discover_battery_packs(bp_crate_root)?;

    // Determine the active battery packs and their features. Prefer the rendered battery-pack.toml
    // state (short names); fall back to [package.metadata.battery-pack] (full names) for templates
    // that still record their packs there.
    let active: Vec<(String, std::collections::BTreeSet<String>)> = match bp_state {
        Some(state) => active_packs_from_state(state),
        None => raw
            .get("package")
            .and_then(|p| p.get("metadata"))
            .and_then(|m| m.get("battery-pack"))
            .and_then(|bp| bp.as_table())
            .map(|table| {
                table
                    .iter()
                    // Pack entries are tables (`pack = { features = [...] }`); skip other keys
                    // such as `hidden` (an array of dependency names).
                    .filter(|(_, value)| value.is_table())
                    .map(|(name, _)| {
                        let features =
                            manifest::read_features_at(&raw, &["package", "metadata"], name);
                        (name.clone(), features)
                    })
                    .collect()
            })
            .unwrap_or_default(),
    };

    // Build a merged map of crate_name -> spec from all active battery packs.
    let mut resolved: std::collections::BTreeMap<String, bphelper_manifest::CrateSpec> =
        std::collections::BTreeMap::new();
    // Also track battery pack versions for resolving bp-managed build-deps.
    let mut bp_versions: std::collections::BTreeMap<String, String> =
        std::collections::BTreeMap::new();

    for (bp_name, active_features) in &active {
        // State entries use the short pack name; metadata uses the full name. Match either.
        let spec = if let Some(bspec) = all_specs
            .iter()
            .find(|pack_spec| pack_spec.name == *bp_name || short_name(&pack_spec.name) == bp_name)
        {
            bspec.clone()
        } else {
            // Not local; fetch from crates.io. State entries use the short name, so restore the
            // full `*-battery-pack` crate name for the registry lookup.
            let full_name = if bp_name == "battery-pack" || bp_name.ends_with("-battery-pack") {
                bp_name.clone()
            } else {
                format!("{bp_name}-battery-pack")
            };
            let (_version, bpspec) =
                fetch_bp_spec_from_registry(&full_name).with_context(|| {
                    format!("battery pack '{full_name}' not found locally or on crates.io")
                })?;
            bpspec
        };

        bp_versions.insert(spec.name.clone(), spec.version.clone());
        for (crate_name, crate_spec) in spec.resolve_for_features(&active_features.into()) {
            resolved.insert(crate_name, crate_spec);
        }
    }

    // Allow battery packs to reference their own version as bp-managed
    // (e.g. battery-pack.bp-managed = true in battery-pack's own templates).
    for spec in &all_specs {
        bp_versions
            .entry(spec.name.clone())
            .or_insert_with(|| spec.version.clone());
    }

    // Rewrite each bp-managed entry to a concrete version pin, across the same
    // set of tables the scan visited.
    for_each_dep_table(&mut doc, |label, table| {
        rewrite_section(table, label, &resolved, &bp_versions)
    })?;

    Ok(doc.to_string())
}

/// The dependency table Cargo recognizes. Each may also appear under a `[target.<cfg>]`
/// gate, where the sub-table mirrors this top-level structure.
const DEP_SECTION: [&str; 3] = ["dependencies", "dev-dependencies", "build-dependencies"];

/// Visit every dependency table in the manifest -- the top-level sections and every
/// `[target.<cfg>.*]` mirror -- handing each a label that names it in diagnostics
/// (`dependencies`, `target. 'cfg(unix)'.dependencies`, ...).
///
/// Both the scan and rewrite passes route through here, so the document layout is encoded
/// in exactly one place. `visit` takes `&mut` so the rewrite pass can edit in place; the
/// read-only scan pass simply ignores the mutability.
///
/// Tables are matched as `TableLike`, so block (`[deps]`) and inline (`dep = {...}`) syntax
/// are handled the same.
fn for_each_dep_table<V>(doc: &mut DocumentMut, mut visit: V) -> Result<()>
where
    V: FnMut(&str, &mut dyn TableLike) -> Result<()>,
{
    for section in DEP_SECTION {
        if let Some(table) = doc
            .get_mut(section)
            .and_then(|item| item.as_table_like_mut())
        {
            visit(section, table)?;
        }
    }

    let Some(target) = doc
        .get_mut("target")
        .and_then(|item| item.as_table_like_mut())
    else {
        return Ok(());
    };

    for (cfg, value) in target.iter_mut() {
        let Some(cfg_table) = value.as_table_like_mut() else {
            continue;
        };

        for section in DEP_SECTION {
            if let Some(table) = cfg_table
                .get_mut(section)
                .and_then(|item| item.as_table_like_mut())
            {
                // Single-quote the cfg key so it round-trips verbatim -- `cfg(target_os = "linux")` keeps its inner quotes.
                visit(&format!("target.'{}'.{section}", cfg.get()), table)?;
            }
        }
    }

    Ok(())
}

/// Scan one dependency table, returning whether it holds any `bp-managed = true` entry. Rejects entries that pair `bp-managed` with a conflicting key, and entries whose `bp-managed` value is not `true`. `label` names the section in those errors (`dependencies`, `target.'cfg(unix)'.dependencies`, …).
fn scan_section(table: &dyn TableLike, label: &str) -> Result<bool> {
    let mut found = false;
    for (name, value) in table.iter() {
        match bp_managed_state(value) {
            BpManaged::Absent => {}
            BpManaged::Enabled(entry) => {
                found = true;
                let extra = extra_keys_on_bp_managed(entry);
                if !extra.is_empty() {
                    bail!(
                        "dependency '{}' in [{}] has `bp-managed = true` with conflicting keys: {}",
                        name,
                        label,
                        extra.join(", ")
                    );
                }
            }
            // `bp-managed = false` / non-bool: a mistake that would otherwise ship verbatim.
            BpManaged::Malformed => {
                bail!(
                    "dependency '{}' in [{}] has `bp-managed` set to a non-`true` value; use `bp-managed = true` or remove the key",
                    name,
                    label
                );
            }
        }
    }
    Ok(found)
}

/// Rewrite every `bp-managed = true` entry in one dependency table to a concrete version pin, drawn from the resolved crate set (`resolved`) or, for a battery pack referencing itself, from `bp_version`. `label` names the section in errors.
fn rewrite_section(
    table: &mut dyn TableLike,
    label: &str,
    resolved: &BTreeMap<String, CrateSpec>,
    bp_version: &BTreeMap<String, String>,
) -> Result<()> {
    // Snapshot each managed entry as an owned inline table so we can mutate `table` below.
    let managed: Vec<(String, InlineTable)> = table
        .iter()
        .filter_map(|(name, item)| match bp_managed_state(item) {
            BpManaged::Enabled(entry) => Some((name.to_string(), to_owned_inline(entry))),
            _ => None,
        })
        .collect();

    for (name, entry) in managed {
        // A dependency renamed via `package = "..."` resolves against the real crate name.
        let crate_name = entry
            .get("package")
            .and_then(Value::as_str)
            .unwrap_or(name.as_str());
        let crate_spec = resolved.get(crate_name);
        let version = if let Some(spec) = crate_spec {
            spec.version.clone()
        } else if let Some(bp_version) = bp_version.get(crate_name) {
            bp_version.clone()
        } else {
            bail!(
                "dependency '{}' in [{}] has `bp-managed = true` but no battery pack provides it",
                name,
                label
            );
        };

        let spec_features: Vec<String> = crate_spec
            .map(|spec| spec.features.iter().cloned().collect())
            .unwrap_or_default();

        let has_explicit_features = entry.contains_key("features");

        // A bare `dep.bp-managed = true` (only the marker, no spec features) becomes a plain
        // version string; anything richer becomes an inline table.
        if entry.len() == 1 && !has_explicit_features && spec_features.is_empty() {
            table.insert(&name, toml_edit::value(&version));
            continue;
        }

        let mut dep = InlineTable::new();
        dep.insert("version", Value::from(version.as_str()));
        for (key, value) in entry.iter() {
            if key != "bp-managed" {
                dep.insert(key, value.clone());
            }
        }

        if !has_explicit_features && !spec_features.is_empty() {
            let mut features = Array::new();
            for feature in &spec_features {
                features.push(feature.as_str());
            }

            dep.insert("features", Value::Array(features));
        }

        // Canonicalize spacing: cloned keys carry decor from their original position
        // (e.g. no space before a following `,` or `}`), so normalize the rebuilt table.
        dep.fmt();
        table.insert(&name, Item::Value(Value::InlineTable(dep)));
    }

    Ok(())
}

/// The `bp-managed` marker on a dependency entry. The `Enabled` variant carries the
/// entry's table view, so a caller acting on a managed dep never re-derives it.
enum BpManaged<'a> {
    /// No `bp-managed` key: an ordinary dependency, left untouched.
    Absent,
    /// `bp-managed = true`: resolve this entry to a concrete version pin.
    Enabled(&'a dyn TableLike),
    /// `bp-managed` present but not `true` (e.g. `false` or a string). The only meaningful
    /// value is `true`, so this is a mistake -- rejected rather than shipped verbatim.
    Malformed,
}

/// Classify a dependency entry by its `bp-managed` marker.
fn bp_managed_state(item: &Item) -> BpManaged<'_> {
    // The marker only lives on table-like entries (`dep = { ... }` or `[deps.dep]`).
    let Some(table) = item.as_table_like() else {
        return BpManaged::Absent;
    };
    match table.get("bp-managed").map(Item::as_bool) {
        None => BpManaged::Absent,
        Some(Some(true)) => BpManaged::Enabled(table),
        Some(_) => BpManaged::Malformed,
    }
}

/// Clone a table-like dependency entry (`dep = {...}` or `[deps.dep]`) into an owned inline
/// table, so it can outlive a mutable borrow of the section it came from. Non-value members
/// (which a dependency spec never has) are dropped.
fn to_owned_inline(table: &dyn TableLike) -> InlineTable {
    let mut inline = InlineTable::new();
    for (key, item) in table.iter() {
        if let Some(value) = item.as_value() {
            inline.insert(key, value.clone());
        }
    }
    inline
}

/// Return any keys on a bp-managed dep entry that contradict the version pin.
fn extra_keys_on_bp_managed(table: &dyn TableLike) -> Vec<String> {
    // `version` and `workspace` each supply what bp-managed supplies, so either alongside
    // `bp-managed` is a contradiction Cargo would reject.
    table
        .iter()
        .map(|(key, _)| key)
        .filter(|key| matches!(*key, "version" | "workspace"))
        .map(String::from)
        .collect()
}

pub(crate) fn fetch_battery_pack_spec(bp_name: &str) -> Result<bphelper_manifest::BatteryPackSpec> {
    let manifest_path = resolve_battery_pack_manifest(bp_name)?;

    parse_battery_pack_from_path(&manifest_path)
        .with_context(|| format!("Failed to parse battery pack '{}'", bp_name))
}

pub(crate) fn load_installed_bp_spec(
    bp_name: &str,
    path: Option<&str>,
    source: &CrateSource,
) -> Result<bphelper_manifest::BatteryPackSpec> {
    if let Some(local_path) = path {
        let manifest_path = Path::new(local_path).join("Cargo.toml");
        return parse_battery_pack_from_path(&manifest_path)
            .with_context(|| format!("Failed to parse battery pack '{}'", bp_name));
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
    pub active_features: bphelper_manifest::ActiveFeatures,
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

pub(crate) fn update_cache() -> Result<()> {
    let packs = fetch_battery_pack_list_from_registry(None)?;
    let pack_names: Vec<String> = packs.into_iter().map(|p| p.name).collect();

    let cache_dir = crate::completions::get_cache_dir();
    fs::create_dir_all(&cache_dir)?;

    let cache_file = cache_dir.join("registry_packs.json");
    let content = serde_json::to_string(&pack_names)?;
    fs::write(&cache_file, content)?;
    Ok(())
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
            let spec = parse_battery_pack_from_path(&manifest_path)
                .with_context(|| format!("Failed to parse battery pack '{}'", crate_name))?;
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
    let spec =
        parse_battery_pack_from_path(&manifest_path).context("Failed to parse battery pack")?;

    // Fetch owners from crates.io
    let owners = fetch_owners(&crate_name)?;

    build_battery_pack_detail(&crate_dir, &spec, owners)
}

/// Fetch detailed battery pack info from a local path
fn fetch_battery_pack_detail_from_path(path: &str) -> Result<BatteryPackDetail> {
    let crate_dir = std::path::Path::new(path);
    let manifest_path = crate_dir.join("Cargo.toml");
    let spec =
        parse_battery_pack_from_path(&manifest_path).context("Failed to parse battery pack")?;

    build_battery_pack_detail(crate_dir, &spec, Vec::new())
}

/// Build `BatteryPackDetail` from a parsed `BatteryPackSpec`.
///
/// Derives extends/crates from the spec's crate keys and scans for examples.
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

    // Fetch the GitHub repository tree to resolve example paths (only if examples exist)
    let has_examples = crate_dir.join("examples").exists();
    let repo_tree = if has_examples {
        spec.repository.as_ref().and_then(|r| fetch_github_tree(r))
    } else {
        None
    };

    let templates = spec
        .templates
        .iter()
        .map(|(name, tmpl)| TemplateInfo {
            name: name.clone(),
            path: tmpl.path.clone(),
            description: tmpl.description.clone(),
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
                .map(|fref| fref.dep_name())
                .filter(|c| !spec.is_hidden(c))
                .map(str::to_string)
                .collect();
            (name.clone(), visible)
        })
        .filter(|(_, members)| !members.is_empty())
        .collect();

    // Resolve each category's members: features, dependencies, and templates
    // whose metadata lists the category. Members are sorted for stable output.
    let categories: Vec<CategoryDetail> = spec
        .categories
        .iter()
        .map(|(name, cat)| {
            let mut members: Vec<String> = Vec::new();
            for (feat_name, meta) in &spec.feature_meta {
                if meta.categories.iter().any(|c| c == name) {
                    members.push(feat_name.clone());
                }
            }
            for (dep_name, meta) in &spec.dep_meta {
                if meta.categories.iter().any(|c| c == name) {
                    members.push(dep_name.clone());
                }
            }
            for (tmpl_name, tmpl) in &spec.templates {
                if tmpl.categories.iter().any(|c| c == name) {
                    members.push(tmpl_name.clone());
                }
            }
            members.sort_unstable();
            members.dedup();
            CategoryDetail {
                name: name.clone(),
                title: cat.title.clone(),
                description: cat.description.clone(),
                pick: cat.pick,
                members,
            }
        })
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
        categories,
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
