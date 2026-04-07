//! Build-time documentation generation for battery packs.
//!
//! Renders Handlebars templates with battery pack metadata to produce
//! documentation for docs.rs. The rendering pipeline is split into
//! pure functions (`build_context`, `render_docs`) for testability,
//! with `generate_docs` as the I/O entry point for build.rs.

use bphelper_manifest::BatteryPackSpec;
use serde::Serialize;
use std::collections::BTreeMap;

// ============================================================================
// Error type
// ============================================================================

/// Errors from documentation generation.
#[derive(Debug, thiserror::Error)]
pub enum Error {
    #[error("template rendering failed: {0}")]
    Render(#[from] handlebars::RenderError),

    #[error("template parse error: {0}")]
    Template(#[from] Box<handlebars::TemplateError>),

    #[error("reading {path}: {source}")]
    Io {
        path: String,
        #[source]
        source: std::io::Error,
    },

    #[error("cargo metadata failed: {0}")]
    Metadata(String),
}

// ============================================================================
// Template context types
// ============================================================================

/// Context for Handlebars template rendering.
// [impl docgen.vars.crates]
// [impl docgen.vars.features]
// [impl docgen.vars.readme]
// [impl docgen.vars.package]
// [impl docgen.template.custom]
#[derive(Debug, Serialize)]
pub struct DocsContext {
    /// Non-hidden curated crates.
    pub crates: Vec<CrateEntry>,
    /// Named feature groups.
    pub features: Vec<FeatureEntry>,
    /// Contents of README.md.
    pub readme: String,
    /// Package metadata.
    pub package: PackageInfo,
}

/// A single crate in the template context.
#[derive(Debug, Serialize)]
pub struct CrateEntry {
    pub name: String,
    pub version: String,
    pub description: String,
    pub features: Vec<String>,
    pub dep_kind: String,
}

/// A feature group in the template context.
#[derive(Debug, Serialize)]
pub struct FeatureEntry {
    pub name: String,
    pub crates: Vec<String>,
}

/// Package-level metadata in the template context.
#[derive(Debug, Serialize)]
pub struct PackageInfo {
    pub name: String,
    pub version: String,
    pub description: String,
    pub repository: String,
}

// ============================================================================
// Core rendering pipeline (pure functions)
// ============================================================================

/// Build the template context from a parsed spec, crate descriptions, and readme.
///
/// This is a pure function with no I/O — all inputs are passed in.
/// Hidden crates are excluded from the context.
// [impl docgen.hidden.excluded]
pub fn build_context(
    spec: &BatteryPackSpec,
    descriptions: &BTreeMap<String, String>,
    readme: &str,
) -> DocsContext {
    let visible = spec.visible_crates();

    let crates = visible
        .iter()
        .map(|(&name, &crate_spec)| CrateEntry {
            name: name.to_string(),
            version: crate_spec.version.clone(),
            description: descriptions.get(name).cloned().unwrap_or_default(),
            features: crate_spec.features.iter().cloned().collect(),
            dep_kind: crate_spec.dep_kind.to_string(),
        })
        .collect();

    let features = spec
        .features
        .iter()
        .map(|(name, crate_names)| FeatureEntry {
            name: name.clone(),
            crates: crate_names.iter().cloned().collect(),
        })
        .collect();

    DocsContext {
        crates,
        features,
        readme: readme.to_string(),
        package: PackageInfo {
            name: spec.name.clone(),
            version: spec.version.clone(),
            description: spec.description.clone(),
            repository: spec.repository.clone().unwrap_or_default(),
        },
    }
}

/// Render a Handlebars template string with the given context.
///
/// Registers the `{{readme}}` and `{{crate-table}}` helpers.
/// HTML escaping is disabled since we generate markdown.
// [impl docgen.template.handlebars]
// [impl docgen.helper.readme]
// [impl docgen.helper.crate-table]
pub fn render_docs(template: &str, context: &DocsContext) -> Result<String, Error> {
    let mut hbs = handlebars::Handlebars::new();
    hbs.set_strict_mode(false);
    // We generate markdown, not HTML — disable escaping.
    hbs.register_escape_fn(handlebars::no_escape);

    hbs.register_helper("readme", Box::new(ReadmeHelper));
    hbs.register_helper("crate-table", Box::new(CrateTableHelper));

    hbs.register_template_string("docs", template)
        .map_err(|e| Error::Template(Box::new(e)))?;

    Ok(hbs.render("docs", context)?)
}

// ============================================================================
// Handlebars helpers
// ============================================================================

/// Helper that expands `{{readme}}` to the readme contents from context.
struct ReadmeHelper;

impl handlebars::HelperDef for ReadmeHelper {
    fn call<'reg: 'rc, 'rc>(
        &self,
        _: &handlebars::Helper<'rc>,
        _: &'reg handlebars::Handlebars<'reg>,
        ctx: &'rc handlebars::Context,
        _: &mut handlebars::RenderContext<'reg, 'rc>,
        out: &mut dyn handlebars::Output,
    ) -> handlebars::HelperResult {
        if let Some(readme) = ctx.data().get("readme").and_then(|v| v.as_str()) {
            out.write(readme)?;
        }
        Ok(())
    }
}

/// Helper that expands `{{crate-table}}` to a markdown table of curated crates.
// [impl docgen.helper.crate-table]
struct CrateTableHelper;

impl handlebars::HelperDef for CrateTableHelper {
    fn call<'reg: 'rc, 'rc>(
        &self,
        _: &handlebars::Helper<'rc>,
        _: &'reg handlebars::Handlebars<'reg>,
        ctx: &'rc handlebars::Context,
        _: &mut handlebars::RenderContext<'reg, 'rc>,
        out: &mut dyn handlebars::Output,
    ) -> handlebars::HelperResult {
        let crates = ctx
            .data()
            .get("crates")
            .and_then(|v| v.as_array())
            .cloned()
            .unwrap_or_default();

        if crates.is_empty() {
            return Ok(());
        }

        out.write("| Crate | Version | Description |\n")?;
        out.write("|-------|---------|-------------|\n")?;

        for entry in &crates {
            let name = entry.get("name").and_then(|v| v.as_str()).unwrap_or("");
            let version = entry.get("version").and_then(|v| v.as_str()).unwrap_or("");
            let description = entry
                .get("description")
                .and_then(|v| v.as_str())
                .unwrap_or("");

            out.write(&format!(
                "| [{}](https://crates.io/crates/{}) | {} | {} |\n",
                name, name, version, description
            ))?;
        }

        Ok(())
    }
}

// ============================================================================
// I/O entry point for build.rs
// ============================================================================

/// Generate documentation for a battery pack.
///
/// Call this from your battery pack's `build.rs`:
///
/// ```rust,ignore
/// fn main() {
///     bphelper_build::generate_docs().unwrap();
/// }
/// ```
///
/// Reads the battery pack's Cargo.toml, `docs.handlebars.md` template,
/// and `README.md`, then renders the template and writes `docs.md`
/// to `OUT_DIR`.
// [impl docgen.build.trigger]
// [impl docgen.build.template]
pub fn generate_docs() -> Result<(), Error> {
    let manifest_dir = std::env::var("CARGO_MANIFEST_DIR").map_err(|_| {
        Error::Metadata("CARGO_MANIFEST_DIR not set — must be called from build.rs".into())
    })?;
    let out_dir = std::env::var("OUT_DIR")
        .map_err(|_| Error::Metadata("OUT_DIR not set — must be called from build.rs".into()))?;

    // Fetch crate descriptions via cargo metadata.
    // [impl docgen.helper.crate-table-metadata]
    let descriptions = fetch_crate_descriptions()?;

    generate_docs_from_dir(&manifest_dir, &out_dir, &descriptions)?;

    // Set up cargo rebuild triggers.
    println!("cargo:rerun-if-changed={manifest_dir}/Cargo.toml");
    println!("cargo:rerun-if-changed={manifest_dir}/docs.handlebars.md");
    println!("cargo:rerun-if-changed={manifest_dir}/README.md");

    Ok(())
}

/// Generate documentation from a specific directory with pre-fetched descriptions.
///
/// Reads Cargo.toml, `docs.handlebars.md`, and `README.md` from `manifest_dir`,
/// then writes `docs.md` to `out_dir`.
// [impl docgen.build.trigger]
// [impl docgen.build.template]
pub fn generate_docs_from_dir(
    manifest_dir: &str,
    out_dir: &str,
    descriptions: &BTreeMap<String, String>,
) -> Result<(), Error> {
    let manifest_path = format!("{manifest_dir}/Cargo.toml");
    let template_path = format!("{manifest_dir}/docs.handlebars.md");
    let readme_path = format!("{manifest_dir}/README.md");

    // Parse the battery pack manifest.
    let manifest_str = std::fs::read_to_string(&manifest_path).map_err(|e| Error::Io {
        path: manifest_path,
        source: e,
    })?;
    let spec = bphelper_manifest::parse_battery_pack(&manifest_str)
        .map_err(|e| Error::Metadata(e.to_string()))?;

    // Read the template.
    let template = std::fs::read_to_string(&template_path).map_err(|e| Error::Io {
        path: template_path,
        source: e,
    })?;

    // Read README (optional — empty string if missing).
    let readme = std::fs::read_to_string(&readme_path).unwrap_or_default();

    // Build context and render.
    let context = build_context(&spec, descriptions, &readme);
    let output = render_docs(&template, &context)?;

    // Write output.
    let output_path = format!("{out_dir}/docs.md");
    std::fs::write(&output_path, output).map_err(|e| Error::Io {
        path: output_path,
        source: e,
    })?;

    Ok(())
}

/// Fetch crate descriptions from cargo metadata.
// [impl docgen.helper.crate-table-metadata]
fn fetch_crate_descriptions() -> Result<BTreeMap<String, String>, Error> {
    let metadata = cargo_metadata::MetadataCommand::new()
        .exec()
        .map_err(|e| Error::Metadata(e.to_string()))?;

    let mut descriptions = BTreeMap::new();
    for pkg in &metadata.packages {
        if let Some(desc) = &pkg.description {
            descriptions.insert(pkg.name.to_string(), desc.clone());
        }
    }
    Ok(descriptions)
}

// ============================================================================
// Tests
// ============================================================================

#[cfg(test)]
mod tests;
