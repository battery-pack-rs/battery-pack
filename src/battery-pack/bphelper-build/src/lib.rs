//! Build script utilities for generating facade re-exports.
//!
//! Battery pack authors use this in their build.rs:
//!
//! ```rust,ignore
//! fn main() {
//!     battery_pack::build::generate_facade().unwrap();
//! }
//! ```

use std::collections::{BTreeMap, HashSet};
use std::env;
use std::fs;
use std::path::Path;
use std::process::Command;

use serde::Deserialize;

/// Errors that can occur during facade generation.
#[derive(Debug)]
pub enum Error {
    Io(std::io::Error),
    Toml(toml::de::Error),
    Json(serde_json::Error),
    MissingManifest,
    CargoMetadataFailed(String),
}

impl From<std::io::Error> for Error {
    fn from(e: std::io::Error) -> Self {
        Error::Io(e)
    }
}

impl From<toml::de::Error> for Error {
    fn from(e: toml::de::Error) -> Self {
        Error::Toml(e)
    }
}

impl From<serde_json::Error> for Error {
    fn from(e: serde_json::Error) -> Self {
        Error::Json(e)
    }
}

impl std::fmt::Display for Error {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        match self {
            Error::Io(e) => write!(f, "IO error: {}", e),
            Error::Toml(e) => write!(f, "TOML parse error: {}", e),
            Error::Json(e) => write!(f, "JSON parse error: {}", e),
            Error::MissingManifest => write!(f, "Could not find Cargo.toml"),
            Error::CargoMetadataFailed(e) => write!(f, "cargo metadata failed: {}", e),
        }
    }
}

impl std::error::Error for Error {}

// ============================================================================
// Manifest types for deserialization
// ============================================================================

/// Parsed Cargo.toml manifest (only the fields we care about)
#[derive(Debug, Deserialize, Default)]
#[serde(default)]
pub struct Manifest {
    pub package: PackageSection,
    pub dependencies: BTreeMap<String, toml::Value>,
}

#[derive(Debug, Deserialize, Default)]
#[serde(default)]
pub struct PackageSection {
    pub name: Option<String>,
    pub metadata: Option<PackageMetadata>,
}

#[derive(Debug, Deserialize, Default)]
#[serde(default)]
pub struct PackageMetadata {
    pub battery: Option<BatteryConfig>,
}

/// The [package.metadata.battery] configuration
#[derive(Debug, Deserialize, Default)]
#[serde(default)]
pub struct BatteryConfig {
    #[allow(dead_code)]
    pub schema_version: Option<u32>,
    #[serde(default)]
    pub exclude: Vec<String>,
    pub root: Option<ExportConfig>,
    #[serde(default)]
    pub modules: BTreeMap<String, ExportConfig>,
    #[serde(default)]
    pub templates: BTreeMap<String, TemplateConfig>,
}

/// Configuration for a project template
#[derive(Debug, Deserialize, Clone)]
pub struct TemplateConfig {
    #[allow(dead_code)]
    pub path: String,
    pub description: String,
}

/// Configuration for what to export - can be a list of crates or detailed per-crate config
#[derive(Debug, Deserialize, Clone)]
#[serde(untagged)]
pub enum ExportConfig {
    /// Simple list: ["tokio", "serde"]
    CrateList(Vec<String>),
    /// Detailed config: { tokio = "*", serde = ["Serialize", "Deserialize"] }
    Detailed(BTreeMap<String, CrateExportConfig>),
}

/// How to export a specific crate.
///
/// Can be:
/// - A single item: `"spawn"` or `"*"`
/// - Multiple items: `["spawn", "select"]` or `["*"]`
#[derive(Debug, Deserialize, Clone)]
#[serde(untagged)]
pub enum CrateExportConfig {
    /// Single item: "spawn" or "*"
    Single(String),
    /// Multiple items: ["Serialize", "Deserialize"] or ["*"]
    Items(Vec<String>),
}

impl CrateExportConfig {
    /// Get all items to export, normalizing single to a one-element list.
    fn items(&self) -> Vec<&str> {
        match self {
            CrateExportConfig::Single(s) => vec![s.as_str()],
            CrateExportConfig::Items(items) => items.iter().map(|s| s.as_str()).collect(),
        }
    }

    /// Check if this is a glob export (contains "*").
    fn is_glob(&self) -> bool {
        self.items().contains(&"*")
    }
}

// ============================================================================
// Cargo metadata types
// ============================================================================

/// Subset of cargo metadata we care about
#[derive(Deserialize)]
struct CargoMetadata {
    packages: Vec<CargoPackage>,
}

#[derive(Deserialize)]
struct CargoPackage {
    name: String,
    description: Option<String>,
    manifest_path: String,
    metadata: Option<CargoPackageMetadata>,
}

#[derive(Deserialize)]
struct CargoPackageMetadata {
    battery: Option<toml::Value>,
}

// ============================================================================
// Public API for build.rs
// ============================================================================

/// Generate the facade.rs file based on Cargo.toml metadata.
///
/// Reads `[package.metadata.battery]` configuration and generates
/// appropriate `pub use` statements for the curated crates.
///
/// If a dependency is itself a battery pack (has `[package.metadata.battery]`),
/// its contents are re-exported instead of the battery pack crate itself.
pub fn generate_facade() -> Result<(), Error> {
    let manifest_dir = env::var("CARGO_MANIFEST_DIR").map_err(|_| Error::MissingManifest)?;
    let manifest_dir_path = Path::new(&manifest_dir);
    let manifest_path = manifest_dir_path.join("Cargo.toml");
    let out_dir = env::var("OUT_DIR").map_err(|_| Error::MissingManifest)?;
    let out_dir_path = Path::new(&out_dir);

    let manifest_content = fs::read_to_string(&manifest_path)?;
    let manifest: Manifest = toml::from_str(&manifest_content)?;

    // Get cargo metadata to find battery pack dependencies and descriptions
    let cargo_metadata = get_cargo_metadata(&manifest_dir)?;
    let battery_pack_manifests = find_battery_pack_manifests(&manifest, &cargo_metadata);
    let descriptions = extract_crate_descriptions(&cargo_metadata);

    let generator = FacadeGenerator::new(&manifest, &battery_pack_manifests, &descriptions);

    // Generate facade.rs
    let code = generator.generate();
    fs::write(out_dir_path.join("facade.rs"), code)?;

    // Generate docs.md by combining README.md with auto-generated sections
    let readme_path = find_readme(manifest_dir_path);
    let readme_content = readme_path
        .as_ref()
        .and_then(|p| fs::read_to_string(p).ok())
        .unwrap_or_default();
    let crates_list = generator.generate_crates_list();
    let examples_section = generate_examples_section(manifest_dir_path);

    // Get templates from battery config
    let battery_config = manifest
        .package
        .metadata
        .as_ref()
        .and_then(|m| m.battery.as_ref());
    let crate_name = manifest.package.name.as_deref().unwrap_or("");
    let templates_section = battery_config
        .map(|b| &b.templates)
        .and_then(|t| generate_templates_section(crate_name, t));

    let mut docs = String::new();
    if !readme_content.is_empty() {
        docs.push_str(readme_content.trim());
        docs.push_str("\n\n");
    }
    docs.push_str("## Included crates\n\n");
    docs.push_str(&crates_list);
    if let Some(examples) = examples_section {
        docs.push_str("\n");
        docs.push_str(&examples);
    }
    if let Some(templates) = templates_section {
        docs.push_str("\n");
        docs.push_str(&templates);
    }
    fs::write(out_dir_path.join("docs.md"), docs)?;

    // Tell Cargo to rerun if Cargo.toml changes
    println!("cargo:rerun-if-changed={}", manifest_path.display());

    // Rerun if README.md changes
    if let Some(readme) = &readme_path {
        println!("cargo:rerun-if-changed={}", readme.display());
    }

    // Rerun if examples directory changes
    let examples_dir = manifest_dir_path.join("examples");
    if examples_dir.exists() {
        println!("cargo:rerun-if-changed={}", examples_dir.display());
    }

    // Also rerun if any battery pack dependency's Cargo.toml changes
    for (_, bp_manifest_path) in &battery_pack_manifests {
        println!("cargo:rerun-if-changed={}", bp_manifest_path);
    }

    Ok(())
}

/// Find README.md, checking src/ first, then crate root.
fn find_readme(manifest_dir: &Path) -> Option<std::path::PathBuf> {
    let src_readme = manifest_dir.join("src").join("README.md");
    if src_readme.exists() {
        return Some(src_readme);
    }
    let root_readme = manifest_dir.join("README.md");
    if root_readme.exists() {
        return Some(root_readme);
    }
    None
}

/// Generate the examples section by scanning the examples directory.
/// Returns None if no examples directory exists or it's empty.
fn generate_examples_section(manifest_dir: &Path) -> Option<String> {
    let examples_dir = manifest_dir.join("examples");
    if !examples_dir.exists() {
        return None;
    }

    let mut examples: Vec<(String, String)> = Vec::new();

    if let Ok(entries) = fs::read_dir(&examples_dir) {
        for entry in entries.flatten() {
            let path = entry.path();
            if path.extension().is_some_and(|ext| ext == "rs") {
                if let Some(name) = path.file_stem().and_then(|s| s.to_str()) {
                    let desc = extract_example_description(&path).unwrap_or_default();
                    examples.push((name.to_string(), desc));
                }
            }
        }
    }

    if examples.is_empty() {
        return None;
    }

    // Sort by name
    examples.sort_by(|a, b| a.0.cmp(&b.0));

    // Build the markdown
    let mut md = String::new();
    md.push_str("## Examples\n\n");

    for (name, desc) in &examples {
        if desc.is_empty() {
            md.push_str(&format!("- **{}**\n", name));
        } else {
            md.push_str(&format!("- **{}** — {}\n", name, desc));
        }
    }

    Some(md)
}

/// Extract the first line of the module doc comment from an example file.
fn extract_example_description(path: &Path) -> Option<String> {
    let content = fs::read_to_string(path).ok()?;

    for line in content.lines() {
        let trimmed = line.trim();
        if trimmed.starts_with("//!") {
            // Extract the doc comment content
            let doc = trimmed.strip_prefix("//!").unwrap_or("").trim();
            if !doc.is_empty() {
                return Some(doc.to_string());
            }
        } else if !trimmed.is_empty() && !trimmed.starts_with("//") {
            // Hit non-comment code, stop looking
            break;
        }
    }

    None
}

/// Generate the templates section from battery config.
/// Returns None if no templates are defined.
fn generate_templates_section(
    crate_name: &str,
    templates: &BTreeMap<String, TemplateConfig>,
) -> Option<String> {
    if templates.is_empty() {
        return None;
    }

    // Convert crate name to short form (e.g., "cli-battery-pack" -> "cli")
    let short_name = crate_name
        .strip_suffix("-battery-pack")
        .unwrap_or(crate_name);

    let mut md = String::new();
    md.push_str("## Templates\n\n");

    // Sort templates by name
    let mut template_names: Vec<_> = templates.keys().collect();
    template_names.sort();

    for name in template_names {
        let template = &templates[name];
        md.push_str(&format!(
            "- `cargo bp new {} --template {}` — {}\n",
            short_name, name, template.description
        ));
    }

    Some(md)
}

fn get_cargo_metadata(manifest_dir: &str) -> Result<CargoMetadata, Error> {
    let output = Command::new("cargo")
        .args(["metadata", "--format-version=1"])
        .current_dir(manifest_dir)
        .output()?;

    if !output.status.success() {
        return Err(Error::CargoMetadataFailed(
            String::from_utf8_lossy(&output.stderr).to_string(),
        ));
    }

    let metadata: CargoMetadata = serde_json::from_slice(&output.stdout)?;
    Ok(metadata)
}

/// Find dependencies that are battery packs.
/// Returns a map of crate name -> manifest path for battery pack deps.
fn find_battery_pack_manifests(
    manifest: &Manifest,
    metadata: &CargoMetadata,
) -> BTreeMap<String, String> {
    let mut battery_packs = BTreeMap::new();

    let deps: HashSet<&String> = manifest.dependencies.keys().collect();

    for package in &metadata.packages {
        if deps.contains(&package.name) {
            if let Some(ref pkg_metadata) = package.metadata {
                if pkg_metadata.battery.is_some() {
                    battery_packs.insert(package.name.clone(), package.manifest_path.clone());
                }
            }
        }
    }

    battery_packs
}

/// Extract crate descriptions from cargo metadata.
/// Returns a map of crate name -> description (first line only).
fn extract_crate_descriptions(metadata: &CargoMetadata) -> BTreeMap<String, String> {
    let mut descriptions = BTreeMap::new();

    for package in &metadata.packages {
        if let Some(ref desc) = package.description {
            // Take only the first line, trimmed
            let first_line = desc.lines().next().unwrap_or("").trim();
            if !first_line.is_empty() {
                descriptions.insert(package.name.clone(), first_line.to_string());
            }
        }
    }

    descriptions
}

// ============================================================================
// Testable facade generation
// ============================================================================

/// Trait for looking up battery pack manifests during generation.
/// This abstraction allows testing without filesystem access.
pub trait BatteryPackResolver {
    /// If the crate is a battery pack, return its parsed manifest.
    fn resolve(&self, crate_name: &str) -> Option<Manifest>;
}

/// Resolver that reads manifests from the filesystem (used in real builds).
pub struct FileSystemResolver<'a> {
    pub(crate) battery_pack_paths: &'a BTreeMap<String, String>,
}

impl BatteryPackResolver for FileSystemResolver<'_> {
    fn resolve(&self, crate_name: &str) -> Option<Manifest> {
        let path = self.battery_pack_paths.get(crate_name)?;
        let content = fs::read_to_string(path).ok()?;
        toml::from_str(&content).ok()
    }
}

/// Resolver backed by in-memory manifests (used in tests).
pub struct InMemoryResolver {
    manifests: BTreeMap<String, Manifest>,
}

impl InMemoryResolver {
    pub fn new() -> Self {
        Self {
            manifests: BTreeMap::new(),
        }
    }

    pub fn add(&mut self, crate_name: &str, manifest_toml: &str) {
        let manifest: Manifest = toml::from_str(manifest_toml).expect("invalid test manifest");
        self.manifests.insert(crate_name.to_string(), manifest);
    }
}

impl Default for InMemoryResolver {
    fn default() -> Self {
        Self::new()
    }
}

impl BatteryPackResolver for InMemoryResolver {
    fn resolve(&self, crate_name: &str) -> Option<Manifest> {
        self.manifests.get(crate_name).cloned()
    }
}

// Need Clone for InMemoryResolver
impl Clone for Manifest {
    fn clone(&self) -> Self {
        Self {
            package: PackageSection {
                name: self.package.name.clone(),
                metadata: self.package.metadata.as_ref().map(|m| PackageMetadata {
                    battery: m.battery.clone(),
                }),
            },
            dependencies: self.dependencies.clone(),
        }
    }
}

impl Clone for BatteryConfig {
    fn clone(&self) -> Self {
        Self {
            schema_version: self.schema_version,
            exclude: self.exclude.clone(),
            root: self.root.clone(),
            modules: self.modules.clone(),
            templates: self.templates.clone(),
        }
    }
}

/// Facade code generator. Separates generation logic from I/O.
pub struct FacadeGenerator<'a, R: BatteryPackResolver = FileSystemResolver<'a>> {
    manifest: &'a Manifest,
    resolver: R,
    descriptions: &'a BTreeMap<String, String>,
}

impl<'a> FacadeGenerator<'a, FileSystemResolver<'a>> {
    /// Create a generator using filesystem-based battery pack resolution.
    pub fn new(
        manifest: &'a Manifest,
        battery_pack_paths: &'a BTreeMap<String, String>,
        descriptions: &'a BTreeMap<String, String>,
    ) -> Self {
        Self {
            manifest,
            resolver: FileSystemResolver { battery_pack_paths },
            descriptions,
        }
    }
}

impl<'a, R: BatteryPackResolver> FacadeGenerator<'a, R> {
    /// Create a generator with a custom resolver (for testing).
    pub fn with_resolver(
        manifest: &'a Manifest,
        resolver: R,
        descriptions: &'a BTreeMap<String, String>,
    ) -> Self {
        Self {
            manifest,
            resolver,
            descriptions,
        }
    }

    /// Generate the facade code as a string.
    pub fn generate(&self) -> String {
        let mut code = String::new();
        code.push_str("// Auto-generated by battery-pack. Do not edit.\n\n");

        let battery = self
            .manifest
            .package
            .metadata
            .as_ref()
            .and_then(|m| m.battery.as_ref());

        let exclude = self.get_exclude_set(battery);
        let deps = self.get_dependencies();

        let root_config = battery.and_then(|b| b.root.as_ref());
        let modules_config = battery.map(|b| &b.modules);

        // Handle explicit root exports
        if let Some(root) = root_config {
            self.generate_exports(&mut code, root, &exclude, "");
        }

        // Handle module exports
        if let Some(modules) = modules_config {
            if !modules.is_empty() {
                self.generate_module_exports(&mut code, modules, &exclude);
            }
        }

        // If no explicit configuration, export all deps at root
        let has_explicit_config =
            root_config.is_some() || modules_config.is_some_and(|m| !m.is_empty());
        if !has_explicit_config {
            for dep in &deps {
                if !exclude.contains(dep) {
                    code.push_str(&self.generate_dep_export(dep, ""));
                }
            }
        }

        code
    }

    /// Generate a markdown list of included crates with descriptions.
    ///
    /// Flattens battery pack contents into a single sorted list, with attribution
    /// showing which battery pack each crate comes from.
    pub fn generate_crates_list(&self) -> String {
        let battery = self
            .manifest
            .package
            .metadata
            .as_ref()
            .and_then(|m| m.battery.as_ref());

        let exclude = self.get_exclude_set(battery);
        let deps = self.get_dependencies();

        // Collect all crates with their source (None = direct, Some = from battery pack)
        let mut all_crates: Vec<(String, Option<String>)> = Vec::new();

        for dep in &deps {
            if exclude.contains(dep) {
                continue;
            }

            if let Some(bp_manifest) = self.resolver.resolve(dep) {
                // This is a battery pack - collect its contents
                let bp_battery = bp_manifest
                    .package
                    .metadata
                    .as_ref()
                    .and_then(|m| m.battery.as_ref());

                let mut bp_exclude: HashSet<String> = bp_battery
                    .map(|b| b.exclude.iter().cloned().collect())
                    .unwrap_or_default();
                bp_exclude.insert("battery-pack".to_string());

                for bp_dep in bp_manifest.dependencies.keys() {
                    if !bp_exclude.contains(bp_dep) {
                        all_crates.push((bp_dep.clone(), Some(dep.clone())));
                    }
                }
            } else {
                // Regular crate
                all_crates.push((dep.clone(), None));
            }
        }

        // Sort by crate name
        all_crates.sort_by(|a, b| a.0.cmp(&b.0));

        // Generate the markdown list
        let mut md = String::new();
        for (crate_name, source) in all_crates {
            let ident = crate_name.replace('-', "_");
            let desc = self
                .descriptions
                .get(&crate_name)
                .map(|s| s.as_str())
                .unwrap_or("");

            let attribution = match source {
                Some(bp_name) => {
                    let bp_ident = bp_name.replace('-', "_");
                    format!(" *(via [`{}`])*", bp_ident)
                }
                None => String::new(),
            };

            md.push_str(&format!("- [`{}`] — {}{}\n", ident, desc, attribution));
        }

        md
    }

    fn get_exclude_set(&self, battery: Option<&BatteryConfig>) -> HashSet<String> {
        let mut exclude: HashSet<String> = battery
            .map(|b| b.exclude.iter().cloned().collect())
            .unwrap_or_default();

        // Always exclude battery-pack itself
        exclude.insert("battery-pack".to_string());
        exclude
    }

    fn get_dependencies(&self) -> Vec<String> {
        let mut deps: Vec<String> = self.manifest.dependencies.keys().cloned().collect();
        deps.sort();
        deps
    }

    fn generate_exports(
        &self,
        code: &mut String,
        config: &ExportConfig,
        exclude: &HashSet<String>,
        indent: &str,
    ) {
        match config {
            ExportConfig::CrateList(crates) => {
                for crate_name in crates {
                    if !exclude.contains(crate_name) {
                        code.push_str(&self.generate_dep_export(crate_name, indent));
                    }
                }
            }
            ExportConfig::Detailed(detailed) => {
                for (crate_name, crate_config) in detailed {
                    if !exclude.contains(crate_name) {
                        let ident = crate_name.replace('-', "_");
                        if crate_config.is_glob() {
                            code.push_str(&format!("{}pub use {}::*;\n", indent, ident));
                        } else {
                            let items = crate_config.items();
                            if !items.is_empty() {
                                code.push_str(&format!(
                                    "{}pub use {}::{{{}}};\n",
                                    indent,
                                    ident,
                                    items.join(", ")
                                ));
                            }
                        }
                    }
                }
            }
        }
    }

    fn generate_module_exports(
        &self,
        code: &mut String,
        modules: &BTreeMap<String, ExportConfig>,
        exclude: &HashSet<String>,
    ) {
        for (module_name, module_config) in modules {
            let mod_ident = if is_rust_keyword(module_name) {
                format!("r#{}", module_name)
            } else {
                module_name.clone()
            };

            code.push_str(&format!("\npub mod {} {{\n", mod_ident));
            self.generate_exports(code, module_config, exclude, "    ");
            code.push_str("}\n");
        }
    }

    /// Generate export statement for a dependency.
    /// If the dep is a battery pack, re-export its contents instead.
    fn generate_dep_export(&self, crate_name: &str, indent: &str) -> String {
        let ident = crate_name.replace('-', "_");

        if let Some(bp_manifest) = self.resolver.resolve(crate_name) {
            // This is a battery pack - re-export its contents
            self.generate_battery_pack_reexport(&ident, crate_name, &bp_manifest, indent)
        } else {
            // Regular crate - simple re-export with doc comment
            let mut code = String::new();
            if let Some(desc) = self.descriptions.get(crate_name) {
                code.push_str(&format!("{}/// {}\n", indent, desc));
            }
            code.push_str(&format!("{}pub use {};\n", indent, ident));
            code
        }
    }

    /// Generate re-exports for a battery pack's contents.
    fn generate_battery_pack_reexport(
        &self,
        bp_ident: &str,
        bp_name: &str,
        bp_manifest: &Manifest,
        indent: &str,
    ) -> String {
        let mut code = String::new();

        // Add a doc comment for the battery pack itself
        if let Some(desc) = self.descriptions.get(bp_name) {
            code.push_str(&format!("{}// From {}: {}\n", indent, bp_name, desc));
        }

        let mut bp_deps: Vec<String> = bp_manifest.dependencies.keys().cloned().collect();
        bp_deps.sort();

        let bp_battery = bp_manifest
            .package
            .metadata
            .as_ref()
            .and_then(|m| m.battery.as_ref());

        let mut bp_exclude: HashSet<String> = bp_battery
            .map(|b| b.exclude.iter().cloned().collect())
            .unwrap_or_default();
        bp_exclude.insert("battery-pack".to_string());

        for dep in bp_deps {
            if !bp_exclude.contains(&dep) {
                let dep_ident = dep.replace('-', "_");
                if let Some(desc) = self.descriptions.get(&dep) {
                    code.push_str(&format!("{}/// {}\n", indent, desc));
                }
                code.push_str(&format!("{}pub use {}::{};\n", indent, bp_ident, dep_ident));
            }
        }

        code
    }
}

fn is_rust_keyword(s: &str) -> bool {
    matches!(
        s,
        "as" | "async"
            | "await"
            | "break"
            | "const"
            | "continue"
            | "crate"
            | "dyn"
            | "else"
            | "enum"
            | "extern"
            | "false"
            | "fn"
            | "for"
            | "if"
            | "impl"
            | "in"
            | "let"
            | "loop"
            | "match"
            | "mod"
            | "move"
            | "mut"
            | "pub"
            | "ref"
            | "return"
            | "self"
            | "Self"
            | "static"
            | "struct"
            | "super"
            | "trait"
            | "true"
            | "type"
            | "unsafe"
            | "use"
            | "where"
            | "while"
    )
}

#[cfg(test)]
mod tests {
    use super::*;
    use expect_test::{Expect, expect};

    fn check(manifest_toml: &str, resolver: InMemoryResolver, expect: Expect) {
        check_with_descriptions(manifest_toml, resolver, &BTreeMap::new(), expect);
    }

    fn check_with_descriptions(
        manifest_toml: &str,
        resolver: InMemoryResolver,
        descriptions: &BTreeMap<String, String>,
        expect: Expect,
    ) {
        let manifest: Manifest = toml::from_str(manifest_toml).unwrap();
        let generator = FacadeGenerator::with_resolver(&manifest, resolver, descriptions);
        let actual = generator.generate();
        expect.assert_eq(&actual);
    }

    #[test]
    fn test_default_exports_all_deps() {
        check(
            r#"
            [package]
            name = "my-battery"
            version = "0.1.0"

            [package.metadata.battery]
            schema_version = 1

            [dependencies]
            tokio = "1"
            serde = "1"
            "#,
            InMemoryResolver::new(),
            expect![[r#"
                // Auto-generated by battery-pack. Do not edit.

                pub use serde;
                pub use tokio;
            "#]],
        );
    }

    #[test]
    fn test_excludes_battery_pack() {
        check(
            r#"
            [package]
            name = "my-battery"
            version = "0.1.0"

            [package.metadata.battery]
            schema_version = 1

            [dependencies]
            battery-pack = "0.1"
            tokio = "1"
            "#,
            InMemoryResolver::new(),
            expect![[r#"
                // Auto-generated by battery-pack. Do not edit.

                pub use tokio;
            "#]],
        );
    }

    #[test]
    fn test_explicit_root_array() {
        check(
            r#"
            [package]
            name = "my-battery"
            version = "0.1.0"

            [package.metadata.battery]
            schema_version = 1
            root = ["tokio", "serde"]

            [dependencies]
            tokio = "1"
            serde = "1"
            anyhow = "1"
            "#,
            InMemoryResolver::new(),
            expect![[r#"
                // Auto-generated by battery-pack. Do not edit.

                pub use tokio;
                pub use serde;
            "#]],
        );
    }

    #[test]
    fn test_glob_reexport() {
        check(
            r#"
            [package]
            name = "my-battery"
            version = "0.1.0"

            [package.metadata.battery]
            schema_version = 1

            [package.metadata.battery.root]
            tokio = "*"
            "#,
            InMemoryResolver::new(),
            expect![[r#"
                // Auto-generated by battery-pack. Do not edit.

                pub use tokio::*;
            "#]],
        );
    }

    #[test]
    fn test_specific_items() {
        check(
            r#"
            [package]
            name = "my-battery"
            version = "0.1.0"

            [package.metadata.battery]
            schema_version = 1

            [package.metadata.battery.root]
            tokio = ["spawn", "select"]
            serde = ["Serialize", "Deserialize"]
            "#,
            InMemoryResolver::new(),
            expect![[r#"
                // Auto-generated by battery-pack. Do not edit.

                pub use serde::{Serialize, Deserialize};
                pub use tokio::{spawn, select};
            "#]],
        );
    }

    #[test]
    fn test_modules() {
        check(
            r#"
            [package]
            name = "my-battery"
            version = "0.1.0"

            [package.metadata.battery]
            schema_version = 1

            [package.metadata.battery.modules]
            http = ["reqwest", "tower"]
            async = ["tokio"]

            [dependencies]
            reqwest = "0.11"
            tower = "0.4"
            tokio = "1"
            "#,
            InMemoryResolver::new(),
            expect![[r#"
                // Auto-generated by battery-pack. Do not edit.


                pub mod r#async {
                    pub use tokio;
                }

                pub mod http {
                    pub use reqwest;
                    pub use tower;
                }
            "#]],
        );
    }

    #[test]
    fn test_battery_pack_reexport() {
        let mut resolver = InMemoryResolver::new();
        resolver.add(
            "error-bp",
            r#"
            [package]
            name = "error-bp"
            version = "0.1.0"

            [package.metadata.battery]
            schema_version = 1

            [dependencies]
            anyhow = "1"
            thiserror = "2"
            "#,
        );

        check(
            r#"
            [package]
            name = "cli-bp"
            version = "0.1.0"

            [package.metadata.battery]
            schema_version = 1

            [dependencies]
            error-bp = "0.1"
            clap = "4"
            "#,
            resolver,
            expect![[r#"
                // Auto-generated by battery-pack. Do not edit.

                pub use clap;
                pub use error_bp::anyhow;
                pub use error_bp::thiserror;
            "#]],
        );
    }

    #[test]
    fn test_nested_battery_packs() {
        let mut resolver = InMemoryResolver::new();
        resolver.add(
            "error-bp",
            r#"
            [package]
            name = "error-bp"
            version = "0.1.0"

            [package.metadata.battery]
            schema_version = 1

            [dependencies]
            anyhow = "1"
            thiserror = "2"
            "#,
        );
        resolver.add(
            "logging-bp",
            r#"
            [package]
            name = "logging-bp"
            version = "0.1.0"

            [package.metadata.battery]
            schema_version = 1

            [dependencies]
            tracing = "0.1"
            "#,
        );

        check(
            r#"
            [package]
            name = "cli-bp"
            version = "0.1.0"

            [package.metadata.battery]
            schema_version = 1

            [dependencies]
            error-bp = "0.1"
            logging-bp = "0.1"
            clap = "4"
            "#,
            resolver,
            expect![[r#"
                // Auto-generated by battery-pack. Do not edit.

                pub use clap;
                pub use error_bp::anyhow;
                pub use error_bp::thiserror;
                pub use logging_bp::tracing;
            "#]],
        );
    }

    #[test]
    fn test_hyphenated_crate_names() {
        check(
            r#"
            [package]
            name = "my-battery"
            version = "0.1.0"

            [package.metadata.battery]
            schema_version = 1

            [dependencies]
            tracing-subscriber = "0.3"
            serde-json = "1"
            "#,
            InMemoryResolver::new(),
            expect![[r#"
                // Auto-generated by battery-pack. Do not edit.

                pub use serde_json;
                pub use tracing_subscriber;
            "#]],
        );
    }

    #[test]
    fn test_custom_exclude() {
        check(
            r#"
            [package]
            name = "my-battery"
            version = "0.1.0"

            [package.metadata.battery]
            schema_version = 1
            exclude = ["internal-crate"]

            [dependencies]
            tokio = "1"
            internal-crate = "0.1"
            "#,
            InMemoryResolver::new(),
            expect![[r#"
                // Auto-generated by battery-pack. Do not edit.

                pub use tokio;
            "#]],
        );
    }

    #[test]
    fn test_single_item_string() {
        // tokio = "spawn" should be equivalent to tokio = ["spawn"]
        check(
            r#"
            [package]
            name = "my-battery"
            version = "0.1.0"

            [package.metadata.battery]
            schema_version = 1

            [package.metadata.battery.root]
            tokio = "spawn"
            serde = "Serialize"
            "#,
            InMemoryResolver::new(),
            expect![[r#"
                // Auto-generated by battery-pack. Do not edit.

                pub use serde::{Serialize};
                pub use tokio::{spawn};
            "#]],
        );
    }

    #[test]
    fn test_glob_in_array() {
        // tokio = ["*"] should work the same as tokio = "*"
        check(
            r#"
            [package]
            name = "my-battery"
            version = "0.1.0"

            [package.metadata.battery]
            schema_version = 1

            [package.metadata.battery.root]
            tokio = ["*"]
            "#,
            InMemoryResolver::new(),
            expect![[r#"
                // Auto-generated by battery-pack. Do not edit.

                pub use tokio::*;
            "#]],
        );
    }

    #[test]
    fn test_descriptions() {
        let mut descriptions = BTreeMap::new();
        descriptions.insert("tokio".to_string(), "An async runtime for Rust".to_string());
        descriptions.insert("serde".to_string(), "A serialization framework".to_string());

        check_with_descriptions(
            r#"
            [package]
            name = "my-battery"
            version = "0.1.0"

            [package.metadata.battery]
            schema_version = 1

            [dependencies]
            tokio = "1"
            serde = "1"
            "#,
            InMemoryResolver::new(),
            &descriptions,
            expect![[r#"
                // Auto-generated by battery-pack. Do not edit.

                /// A serialization framework
                pub use serde;
                /// An async runtime for Rust
                pub use tokio;
            "#]],
        );
    }

    #[test]
    fn test_battery_pack_reexport_with_descriptions() {
        let mut resolver = InMemoryResolver::new();
        resolver.add(
            "error-bp",
            r#"
            [package]
            name = "error-bp"
            version = "0.1.0"

            [package.metadata.battery]
            schema_version = 1

            [dependencies]
            anyhow = "1"
            thiserror = "2"
            "#,
        );

        let mut descriptions = BTreeMap::new();
        descriptions.insert(
            "error-bp".to_string(),
            "Error handling battery pack".to_string(),
        );
        descriptions.insert(
            "anyhow".to_string(),
            "Flexible concrete Error type".to_string(),
        );
        descriptions.insert(
            "thiserror".to_string(),
            "derive(Error) for custom error types".to_string(),
        );
        descriptions.insert(
            "clap".to_string(),
            "Command line argument parser".to_string(),
        );

        check_with_descriptions(
            r#"
            [package]
            name = "cli-bp"
            version = "0.1.0"

            [package.metadata.battery]
            schema_version = 1

            [dependencies]
            error-bp = "0.1"
            clap = "4"
            "#,
            resolver,
            &descriptions,
            expect![[r#"
                // Auto-generated by battery-pack. Do not edit.

                /// Command line argument parser
                pub use clap;
                // From error-bp: Error handling battery pack
                /// Flexible concrete Error type
                pub use error_bp::anyhow;
                /// derive(Error) for custom error types
                pub use error_bp::thiserror;
            "#]],
        );
    }
}
