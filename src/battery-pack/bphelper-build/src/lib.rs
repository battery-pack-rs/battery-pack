//! Build-time documentation generation for battery packs.
//!
//! Renders Handlebars templates with battery pack metadata to produce
//! documentation for docs.rs. The rendering pipeline is split into
//! pure functions (`build_context`, `render_docs`) for testability,
//! with `generate_docs` as the I/O entry point for build.rs.

use bphelper_manifest::{BatteryPackSpec, FeatureRef, PickMode, parse_battery_pack_from_path};
use serde::Serialize;
use std::{
    collections::{BTreeMap, BTreeSet},
    path::Path,
};

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
    /// Category groupings with their member items.
    pub categories: Vec<CategoryEntry>,
    /// Declared templates.
    pub templates: Vec<TemplateEntry>,
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

/// A category with its member items, for template rendering.
#[derive(Debug, Serialize)]
pub struct CategoryEntry {
    /// Category slug (e.g., "hal").
    pub name: String,
    /// Display title (e.g., "Hardware Abstraction Layer").
    pub title: String,
    /// Explanatory text.
    pub description: String,
    /// Selection mode: "at-most-one" or "any".
    pub pick: String,
    /// Items belonging to this category.
    pub items: Vec<CategoryItemEntry>,
}

/// An item within a category.
#[derive(Debug, Serialize)]
pub struct CategoryItemEntry {
    /// Item name (feature or dependency name).
    pub name: String,
    /// Human-readable description.
    pub description: String,
    /// Whether this is a "feature" or "dependency".
    pub kind: String,
    /// Crates.io URL(s) for the underlying crate(s).
    /// For a dependency: links to that crate directly.
    /// For a feature: links to the crate(s) it activates.
    pub crates: Vec<String>,
}

/// A template entry for the docs context.
#[derive(Debug, Serialize)]
pub struct TemplateEntry {
    pub name: String,
    pub description: String,
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

/// Resolve a named Cargo feature to the visible crates it activates.
fn feature_crate_names(spec: &BatteryPackSpec, feature_name: &str) -> Vec<String> {
    // Restrict documentation links to crates that are visible to battery-pack users.
    let visible_names: BTreeSet<&str> = spec.visible_crates().keys().copied().collect();
    let mut visited_features = BTreeSet::new();
    let mut crate_names = BTreeSet::new();

    // Expand local feature aliases before returning stable, de-duplicated crate names.
    collect_feature_crates(
        spec,
        feature_name,
        &visible_names,
        &mut visited_features,
        &mut crate_names,
    );
    crate_names.into_iter().collect()
}

/// Recursively collect dependency names from one Cargo feature.
fn collect_feature_crates<'a>(
    spec: &'a BatteryPackSpec,
    feature_name: &str,
    visible_names: &BTreeSet<&'a str>,
    visited_features: &mut BTreeSet<String>,
    crate_names: &mut BTreeSet<String>,
) {
    // Stop repeated aliases defensively even though manifest validation rejects cycles.
    if !visited_features.insert(feature_name.to_string()) {
        return;
    }

    // Resolve bare local features recursively and dependency references directly.
    if let Some(refs) = spec.features.get(feature_name) {
        for feature_ref in refs {
            match feature_ref {
                FeatureRef::Feature(name) if spec.features.contains_key(name) => {
                    collect_feature_crates(
                        spec,
                        name,
                        visible_names,
                        visited_features,
                        crate_names,
                    );
                }
                FeatureRef::Feature(name) | FeatureRef::Dep(name) => {
                    if visible_names.contains(name.as_str()) {
                        crate_names.insert(name.clone());
                    }
                }
                FeatureRef::DepFeature { dep, .. } => {
                    if visible_names.contains(dep.as_str()) {
                        crate_names.insert(dep.clone());
                    }
                }
            }
        }
    }

    // Let sibling branches expand the same alias independently while preventing cycles above.
    visited_features.remove(feature_name);
}

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
        .keys()
        .map(|name| FeatureEntry {
            name: name.clone(),
            crates: feature_crate_names(spec, name),
        })
        .collect();

    // Build category entries with their member items.
    let categories = spec
        .categories
        .iter()
        .map(|(cat_name, cat_spec)| {
            let mut items = Vec::new();

            // Collect features in this category.
            for (feat_name, meta) in &spec.feature_meta {
                if meta.categories.contains(cat_name) {
                    let mut crate_names = feature_crate_names(spec, feat_name);
                    // Put the crate matching the feature name first (it's the "primary" crate).
                    if let Some(pos) = crate_names.iter().position(|c| c == feat_name) {
                        crate_names.swap(0, pos);
                    }
                    items.push(CategoryItemEntry {
                        name: feat_name.clone(),
                        description: meta.description.clone().unwrap_or_default(),
                        kind: "feature".to_string(),
                        crates: crate_names,
                    });
                }
            }

            // Collect dependencies in this category.
            for (dep_name, meta) in &spec.dep_meta {
                if meta.categories.contains(cat_name) {
                    items.push(CategoryItemEntry {
                        name: dep_name.clone(),
                        description: meta.description.clone().unwrap_or_default(),
                        kind: "dependency".to_string(),
                        crates: vec![dep_name.clone()],
                    });
                }
            }

            // Collect templates in this category.
            for (tmpl_name, tmpl_spec) in &spec.templates {
                if tmpl_spec.categories.contains(cat_name) {
                    items.push(CategoryItemEntry {
                        name: tmpl_name.clone(),
                        description: tmpl_spec.description.clone().unwrap_or_default(),
                        kind: "template".to_string(),
                        crates: vec![],
                    });
                }
            }

            CategoryEntry {
                name: cat_name.clone(),
                title: cat_spec.title.clone().unwrap_or_else(|| cat_name.clone()),
                description: cat_spec.description.clone().unwrap_or_default(),
                pick: match cat_spec.pick {
                    PickMode::AtMostOne => "at-most-one".to_string(),
                    PickMode::Any => "any".to_string(),
                },
                items,
            }
        })
        .collect();

    // Build template entries.
    let templates = spec
        .templates
        .iter()
        .map(|(name, tmpl_spec)| TemplateEntry {
            name: name.clone(),
            description: tmpl_spec.description.clone().unwrap_or_default(),
        })
        .collect();

    DocsContext {
        crates,
        features,
        categories,
        templates,
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

/// Helper that expands `{{crate-table}}` to markdown documentation of curated crates.
///
/// When categories are defined, renders a section per category with a table of
/// its items, followed by an "Other crates" table for uncategorized dependencies.
/// When no categories exist, renders a flat table of all crates.
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
        let categories = ctx
            .data()
            .get("categories")
            .and_then(|v| v.as_array())
            .cloned()
            .unwrap_or_default();

        let crates = ctx
            .data()
            .get("crates")
            .and_then(|v| v.as_array())
            .cloned()
            .unwrap_or_default();

        let features = ctx
            .data()
            .get("features")
            .and_then(|v| v.as_array())
            .cloned()
            .unwrap_or_default();

        // If categories are defined, render category-aware docs.
        if !categories.is_empty() {
            out.write("## Battery pack contents\n\n")?;

            // Collect all crate names that appear in a category item
            // (category items are features/deps, so we track which crates
            // are "covered" by a category).
            let mut categorized_crates: std::collections::HashSet<String> =
                std::collections::HashSet::new();

            for cat in &categories {
                let title = cat.get("title").and_then(|t| t.as_str()).unwrap_or("");
                let description = cat
                    .get("description")
                    .and_then(|d| d.as_str())
                    .unwrap_or("");
                let pick = cat.get("pick").and_then(|p| p.as_str()).unwrap_or("any");
                let items = cat
                    .get("items")
                    .and_then(|i| i.as_array())
                    .cloned()
                    .unwrap_or_default();

                if items.is_empty() {
                    continue;
                }

                // Section header with pick mode annotation.
                let pick_note = if pick == "at-most-one" {
                    " *(pick at most one)*"
                } else {
                    ""
                };
                out.write(&format!("### {}{}\n\n", title, pick_note))?;

                if !description.is_empty() {
                    out.write(&format!("{}\n\n", description))?;
                }

                out.write("| Name | Description |\n")?;
                out.write("|------|-------------|\n")?;

                for item in &items {
                    let name = item.get("name").and_then(|n| n.as_str()).unwrap_or("");
                    let desc = item
                        .get("description")
                        .and_then(|d| d.as_str())
                        .unwrap_or("")
                        .replace('\n', " ")
                        .replace('|', "\\|");
                    let kind = item.get("kind").and_then(|k| k.as_str()).unwrap_or("");

                    if kind == "dependency" {
                        out.write(&format!(
                            "| [`{}`](https://crates.io/crates/{}) | {} |\n",
                            name,
                            name,
                            desc.trim()
                        ))?;
                        categorized_crates.insert(name.to_string());
                    } else {
                        // Feature: find its crates.
                        let mut feat_crate_names: Vec<String> = Vec::new();
                        for feat in &features {
                            let feat_name = feat.get("name").and_then(|n| n.as_str()).unwrap_or("");
                            if feat_name == name
                                && let Some(feat_crates) =
                                    feat.get("crates").and_then(|c| c.as_array())
                            {
                                for c in feat_crates {
                                    if let Some(cn) = c.as_str() {
                                        feat_crate_names.push(cn.to_string());
                                        categorized_crates.insert(cn.to_string());
                                    }
                                }
                            }
                        }

                        if feat_crate_names.len() == 1 {
                            // Single crate: link feature name to that crate.
                            out.write(&format!(
                                "| [`{}`](https://crates.io/crates/{}) | {} |\n",
                                name,
                                feat_crate_names[0],
                                desc.trim()
                            ))?;
                        } else {
                            // Multi-crate or zero-crate feature.
                            // Link to same-named crate if one exists, otherwise backtick.
                            if feat_crate_names.contains(&name.to_string()) {
                                out.write(&format!(
                                    "| [`{}`](https://crates.io/crates/{}) | {} |\n",
                                    name,
                                    name,
                                    desc.trim()
                                ))?;
                            } else {
                                out.write(&format!("| `{}` | {} |\n", name, desc.trim()))?;
                            }
                            // Second row: list all crates.
                            if !feat_crate_names.is_empty() {
                                let links: Vec<String> = feat_crate_names
                                    .iter()
                                    .map(|c| format!("[`{}`](https://crates.io/crates/{})", c, c))
                                    .collect();
                                out.write(&format!("| | *Crates:* {} |\n", links.join(", ")))?;
                            }
                        }
                    }
                }

                out.write("\n")?;
            }

            // Render uncategorized crates, split by dependency kind.
            let uncategorized: Vec<_> = crates
                .iter()
                .filter(|entry| {
                    let name = entry.get("name").and_then(|v| v.as_str()).unwrap_or("");
                    !categorized_crates.contains(name)
                })
                .collect();

            write_dep_sections(out, &uncategorized, true)?;

            // Render templates.
            write_templates_section(out, ctx)?;
        } else {
            // No categories: render by dep kind.
            if crates.is_empty() {
                // Even without crates, there may be templates.
                let templates = ctx
                    .data()
                    .get("templates")
                    .and_then(|v| v.as_array())
                    .cloned()
                    .unwrap_or_default();
                if templates.is_empty() {
                    return Ok(());
                }
                out.write("## Battery pack contents\n\n")?;
                write_templates_section(out, ctx)?;
                return Ok(());
            }

            out.write("## Battery pack contents\n\n")?;

            let all: Vec<_> = crates.iter().collect();
            write_dep_sections(out, &all, false)?;

            // Render templates.
            write_templates_section(out, ctx)?;
        }

        Ok(())
    }
}

/// Write dependency sections split by kind (dependencies, dev, build).
/// When `always_heading` is true, always emit "### Dependencies" even if
/// there's only one kind (used when categories precede these sections).
fn write_dep_sections(
    out: &mut dyn handlebars::Output,
    entries: &[&serde_json::Value],
    always_heading: bool,
) -> handlebars::HelperResult {
    let runtime_deps: Vec<_> = entries
        .iter()
        .filter(|e| {
            let kind = e.get("dep_kind").and_then(|k| k.as_str()).unwrap_or("");
            kind == "dependencies"
        })
        .copied()
        .collect();

    let dev_deps: Vec<_> = entries
        .iter()
        .filter(|e| {
            let kind = e.get("dep_kind").and_then(|k| k.as_str()).unwrap_or("");
            kind == "dev-dependencies"
        })
        .copied()
        .collect();

    let build_deps: Vec<_> = entries
        .iter()
        .filter(|e| {
            let kind = e.get("dep_kind").and_then(|k| k.as_str()).unwrap_or("");
            kind == "build-dependencies"
        })
        .copied()
        .collect();

    if !runtime_deps.is_empty() {
        if always_heading || !dev_deps.is_empty() || !build_deps.is_empty() {
            out.write("### Dependencies\n\n")?;
        }
        write_item_table(out, &runtime_deps)?;
    }

    if !dev_deps.is_empty() {
        out.write("### Dev dependencies\n\n")?;
        write_item_table(out, &dev_deps)?;
    }

    if !build_deps.is_empty() {
        out.write("### Build dependencies\n\n")?;
        write_item_table(out, &build_deps)?;
    }

    Ok(())
}

/// Write a `| Name | Description |` table for a list of crate entries.
/// Each crate is linked to crates.io with backtick formatting.
fn write_item_table(
    out: &mut dyn handlebars::Output,
    entries: &[&serde_json::Value],
) -> handlebars::HelperResult {
    out.write("| Name | Description |\n")?;
    out.write("|------|-------------|\n")?;

    for entry in entries {
        let name = entry.get("name").and_then(|v| v.as_str()).unwrap_or("");
        let description = entry
            .get("description")
            .and_then(|v| v.as_str())
            .unwrap_or("")
            .replace('\n', " ")
            .replace('|', "\\|");
        out.write(&format!(
            "| [`{}`](https://crates.io/crates/{}) | {} |\n",
            name,
            name,
            description.trim()
        ))?;
    }
    out.write("\n")?;
    Ok(())
}

/// Write a templates section if templates are defined.
fn write_templates_section(
    out: &mut dyn handlebars::Output,
    ctx: &handlebars::Context,
) -> handlebars::HelperResult {
    let templates = ctx
        .data()
        .get("templates")
        .and_then(|v| v.as_array())
        .cloned()
        .unwrap_or_default();

    if templates.is_empty() {
        return Ok(());
    }

    out.write("### Templates\n\n")?;
    out.write("| Name | Description |\n")?;
    out.write("|------|-------------|\n")?;

    for tmpl in &templates {
        let name = tmpl.get("name").and_then(|n| n.as_str()).unwrap_or("");
        let description = tmpl
            .get("description")
            .and_then(|d| d.as_str())
            .unwrap_or("")
            .replace('\n', " ")
            .replace('|', "\\|");
        out.write(&format!("| `{}` | {} |\n", name, description.trim()))?;
    }
    out.write("\n")?;
    Ok(())
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
    let spec = parse_battery_pack_from_path(Path::new(&manifest_path))
        .map_err(|err| Error::Metadata(err.to_string()))?;

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
mod tests {
    use snapbox::{assert_data_eq, str};

    use super::*;

    fn fixtures_dir() -> std::path::PathBuf {
        let manifest_dir = std::path::Path::new(env!("CARGO_MANIFEST_DIR"));
        manifest_dir
            .parent()
            .unwrap()
            .parent()
            .unwrap()
            .parent()
            .unwrap()
            .join("tests/fixtures")
    }

    fn parse_fixture(name: &str) -> BatteryPackSpec {
        let path = fixtures_dir().join(name).join("Cargo.toml");

        parse_battery_pack_from_path(&path)
            .unwrap_or_else(|err| panic!("parsing {}: {}", path.display(), err))
    }

    fn mock_descriptions() -> BTreeMap<String, String> {
        BTreeMap::from([
            (
                "anyhow".into(),
                "Flexible concrete Error type built on std::error::Error".into(),
            ),
            ("thiserror".into(), "derive(Error)".into()),
            (
                "eyre".into(),
                "Flexible concrete Error type for easy idiomatic error handling".into(),
            ),
            (
                "clap".into(),
                "A simple to use, efficient, and full-featured Command Line Argument Parser".into(),
            ),
            (
                "dialoguer".into(),
                "A command line prompting library".into(),
            ),
            (
                "indicatif".into(),
                "A progress bar and CLI reporting library".into(),
            ),
            (
                "console".into(),
                "A terminal and console abstraction for Rust".into(),
            ),
            ("serde".into(), "A serialization framework".into()),
            (
                "serde_json".into(),
                "A JSON serialization file format".into(),
            ),
            ("cc".into(), "A build-time C compiler detection".into()),
            ("assert_cmd".into(), "Easy command testing".into()),
            ("predicates".into(), "Boolean predicate combinators".into()),
        ])
    }

    // ================================================================
    // build_context() tests
    // ================================================================

    #[test]
    // [verify docgen.vars.crates]
    // [verify docgen.vars.package]
    fn test_context_basic() {
        let spec = parse_fixture("basic-battery-pack");
        let descriptions = mock_descriptions();
        let ctx = build_context(&spec, &descriptions, "# Hello");

        // Package metadata
        assert_eq!(ctx.package.name, "basic-battery-pack");
        assert_eq!(ctx.package.version, "0.1.0");
        assert_eq!(ctx.package.description, "A simple test battery pack");
        assert_eq!(ctx.package.repository, "");

        // All 3 crates should be visible (no hidden config)
        let names: Vec<&str> = ctx.crates.iter().map(|c| c.name.as_str()).collect();
        assert_eq!(names, ["anyhow", "eyre", "thiserror"]);

        // Check a specific crate entry
        let anyhow = ctx.crates.iter().find(|c| c.name == "anyhow").unwrap();
        assert_eq!(anyhow.version, "1");
        assert_eq!(
            anyhow.description,
            "Flexible concrete Error type built on std::error::Error"
        );
        assert_eq!(anyhow.dep_kind, "dependencies");
    }

    #[test]
    // [verify docgen.hidden.excluded]
    fn test_context_hidden_excluded() {
        let spec = parse_fixture("fancy-battery-pack");
        let descriptions = mock_descriptions();
        let ctx = build_context(&spec, &descriptions, "");

        let names: Vec<&str> = ctx.crates.iter().map(|c| c.name.as_str()).collect();
        // serde, serde_json, cc should be hidden
        assert!(!names.contains(&"serde"), "serde should be hidden");
        assert!(
            !names.contains(&"serde_json"),
            "serde_json should be hidden"
        );
        assert!(!names.contains(&"cc"), "cc should be hidden");
        assert_data_eq!(
            names.join(", "),
            str!["assert_cmd, clap, console, dialoguer, indicatif, predicates"]
        );
    }

    #[test]
    // [verify docgen.vars.features]
    fn test_context_features() {
        let spec = parse_fixture("fancy-battery-pack");
        let descriptions = mock_descriptions();
        let ctx = build_context(&spec, &descriptions, "");

        let feature_names: Vec<&str> = ctx.features.iter().map(|f| f.name.as_str()).collect();
        assert_data_eq!(feature_names.join(", "), str!["default, fancy, indicators"]);

        let indicators = ctx
            .features
            .iter()
            .find(|f| f.name == "indicators")
            .unwrap();
        // Lexicographic order from `BTreeSet<FeatureRef>` iteration.
        assert_eq!(indicators.crates, vec!["console", "indicatif"]);
    }

    #[test]
    fn test_context_features_expand_local_aliases() {
        // Parse the embedded pack whose HAL features include a local critical-section alias.
        let manifest_path = fixtures_dir()
            .parent()
            .unwrap()
            .parent()
            .unwrap()
            .join("battery-packs/embedded-battery-pack/Cargo.toml");
        let spec = parse_battery_pack_from_path(&manifest_path).unwrap();
        let ctx = build_context(&spec, &BTreeMap::new(), "");

        // Render only real crate names so generated crates.io links always have valid targets.
        let stm32f0 = ctx
            .features
            .iter()
            .find(|feature| feature.name == "stm32f0")
            .unwrap();
        assert_data_eq!(
            stm32f0.crates.join(", "),
            str!["cortex-m, cortex-m-rt, critical-section, embedded-hal, stm32f0xx-hal"]
        );
    }

    #[test]
    // [verify docgen.vars.readme]
    fn test_context_readme() {
        let spec = parse_fixture("basic-battery-pack");
        let descriptions = mock_descriptions();
        let readme = "# My Battery Pack\n\nThis is great.";
        let ctx = build_context(&spec, &descriptions, readme);

        assert_eq!(ctx.readme, readme);
    }

    #[test]
    // [verify docgen.vars.crates]
    fn test_context_dep_kinds() {
        let spec = parse_fixture("fancy-battery-pack");
        let descriptions = mock_descriptions();
        let ctx = build_context(&spec, &descriptions, "");

        // clap is in [dependencies] (optional)
        let clap = ctx.crates.iter().find(|c| c.name == "clap").unwrap();
        assert_eq!(clap.dep_kind, "dependencies");

        // assert_cmd is in [dev-dependencies]
        let assert_cmd = ctx.crates.iter().find(|c| c.name == "assert_cmd").unwrap();
        assert_eq!(assert_cmd.dep_kind, "dev-dependencies");

        // cc is hidden (build-dep), so not present — no build-dep to test directly
        // but we can verify the hidden ones are truly excluded
        assert!(ctx.crates.iter().all(|c| c.name != "cc"));
    }

    // ================================================================
    // render_docs() tests
    // ================================================================

    /// Helper to build a minimal context for render tests.
    fn simple_context() -> DocsContext {
        DocsContext {
            crates: vec![
                CrateEntry {
                    name: "anyhow".into(),
                    version: "1".into(),
                    description: "Flexible concrete Error type".into(),
                    features: vec![],
                    dep_kind: "dependencies".into(),
                },
                CrateEntry {
                    name: "thiserror".into(),
                    version: "2".into(),
                    description: "derive(Error)".into(),
                    features: vec!["std".into()],
                    dep_kind: "dependencies".into(),
                },
            ],
            features: vec![FeatureEntry {
                name: "default".into(),
                crates: vec!["anyhow".into(), "thiserror".into()],
            }],
            categories: vec![],
            templates: vec![],
            readme: "# My Pack\n\nA great battery pack.".into(),
            package: PackageInfo {
                name: "test-battery-pack".into(),
                version: "0.1.0".into(),
                description: "A test pack".into(),
                repository: "https://github.com/example/test".into(),
            },
        }
    }

    #[test]
    // [verify docgen.helper.readme]
    fn test_render_readme_helper() {
        let ctx = simple_context();
        let output = render_docs("{{readme}}", &ctx).unwrap();
        assert_data_eq!(
            output,
            str![[r#"
# My Pack

A great battery pack.
"#]]
        )
    }

    #[test]
    // [verify docgen.helper.crate-table]
    fn test_render_crate_table() {
        let ctx = simple_context();
        let output = render_docs("{{crate-table}}", &ctx).unwrap();
        assert!(
            output.contains("| Name |"),
            "should render a markdown table"
        );
        assert_data_eq!(
            output,
            str![[r#"
## Battery pack contents

| Name | Description |
|------|-------------|
| [`anyhow`](https://crates.io/crates/anyhow) | Flexible concrete Error type |
| [`thiserror`](https://crates.io/crates/thiserror) | derive(Error) |


"#]]
        )
    }

    #[test]
    // [verify docgen.template.default]
    fn test_render_default_template() {
        let ctx = simple_context();
        let output = render_docs("{{readme}}\n\n{{crate-table}}", &ctx).unwrap();
        assert!(output.contains("# My Pack"), "should contain readme");
        assert!(output.contains("| Name |"), "should contain crate table");
        assert_data_eq!(
            output,
            str![[r#"
# My Pack

A great battery pack.

## Battery pack contents

| Name | Description |
|------|-------------|
| [`anyhow`](https://crates.io/crates/anyhow) | Flexible concrete Error type |
| [`thiserror`](https://crates.io/crates/thiserror) | derive(Error) |


"#]]
        )
    }

    #[test]
    // [verify docgen.template.custom]
    fn test_render_custom_template() {
        let ctx = simple_context();
        let template = r#"# {{package.name}} v{{package.version}}

{{package.description}}

## Crates

{{#each crates}}
- **{{name}}** ({{version}}): {{description}}
{{/each}}"#;
        let output = render_docs(template, &ctx).unwrap();

        assert!(
            output.contains("test-battery-pack v0.1.0"),
            "should render package name and version"
        );
        assert!(output.contains("**anyhow**"), "should list crates");
        assert_data_eq!(
            output,
            str![[r#"
# test-battery-pack v0.1.0

A test pack

## Crates

- **anyhow** (1): Flexible concrete Error type
- **thiserror** (2): derive(Error)

"#]]
        )
    }

    #[test]
    fn test_render_crate_table_empty() {
        let ctx = DocsContext {
            crates: vec![],
            features: vec![],
            categories: vec![],
            templates: vec![],
            readme: String::new(),
            package: PackageInfo {
                name: "empty-battery-pack".into(),
                version: "0.1.0".into(),
                description: "".into(),
                repository: "".into(),
            },
        };
        let output = render_docs("{{crate-table}}", &ctx).unwrap();
        assert_eq!(output, "");
    }

    #[test]
    // [verify docgen.helper.crate-table]
    fn test_render_crate_table_links() {
        let ctx = simple_context();
        let output = render_docs("{{crate-table}}", &ctx).unwrap();
        assert!(output.contains("[`anyhow`](https://crates.io/crates/anyhow)"));
        assert!(output.contains("[`thiserror`](https://crates.io/crates/thiserror)"));
        assert_data_eq!(
            output,
            str![[r#"
## Battery pack contents

| Name | Description |
|------|-------------|
| [`anyhow`](https://crates.io/crates/anyhow) | Flexible concrete Error type |
| [`thiserror`](https://crates.io/crates/thiserror) | derive(Error) |


"#]]
        );
    }

    #[test]
    fn test_render_no_html_escaping() {
        let ctx = DocsContext {
            readme: "Use `Option<T>` and `Result<T, E>` & more".into(),
            ..simple_context()
        };
        let output = render_docs("{{readme}}", &ctx).unwrap();
        // Angle brackets and ampersand should NOT be escaped
        assert!(!output.contains("&amp;"), "ampersand should not be escaped");
        assert!(
            !output.contains("&lt;"),
            "angle brackets should not be escaped"
        );
        assert_data_eq!(output, str!["Use `Option<T>` and `Result<T, E>` & more"]);
    }

    // ================================================================
    // Full pipeline tests (parse fixture → build context → render)
    // ================================================================

    #[test]
    // [verify docgen.template.handlebars]
    fn test_full_pipeline_basic() {
        let spec = parse_fixture("basic-battery-pack");
        let descriptions = mock_descriptions();
        let readme = "# basic-battery-pack\n\nError handling crates for Rust.";
        let ctx = build_context(&spec, &descriptions, readme);
        let output = render_docs("{{readme}}\n\n{{crate-table}}", &ctx).unwrap();

        assert!(
            output.contains("# basic-battery-pack"),
            "should contain readme"
        );
        assert!(output.contains("[`anyhow`]"), "should contain crate links");
        assert!(
            output.contains("[`eyre`]"),
            "should contain optional crate eyre"
        );
        assert_data_eq!(
            output,
            str![[r#"
# basic-battery-pack

Error handling crates for Rust.

## Battery pack contents

| Name | Description |
|------|-------------|
| [`anyhow`](https://crates.io/crates/anyhow) | Flexible concrete Error type built on std::error::Error |
| [`eyre`](https://crates.io/crates/eyre) | Flexible concrete Error type for easy idiomatic error handling |
| [`thiserror`](https://crates.io/crates/thiserror) | derive(Error) |


"#]]
        );
    }

    // ================================================================
    // True-by-construction rule verification
    // ================================================================

    #[test]
    // [verify docgen.build.lib-include]
    fn test_template_lib_rs_includes_generated_docs() {
        // True by construction: the `cargo bp new` template emits a lib.rs
        // containing `#![doc = include_str!(concat!(env!("OUT_DIR"), "/docs.md"))]`.
        // This test verifies the template file contains the expected directive.
        let template_dir = std::path::Path::new(env!("CARGO_MANIFEST_DIR"))
            .parent()
            .unwrap()
            .join("templates/default/src/lib.rs");
        let content = std::fs::read_to_string(&template_dir).unwrap();
        assert!(
            content.contains(r#"include_str!(concat!(env!("OUT_DIR"), "/docs.md"))"#),
            "template lib.rs must include generated docs from OUT_DIR"
        );
        assert_data_eq!(
            content,
            str![[r##"
#![doc = include_str!(concat!(env!("OUT_DIR"), "/docs.md"))]

#[cfg(test)]
mod tests {
    #[test]
    fn validate() {
        ::battery_pack::testing::validate(env!("CARGO_MANIFEST_DIR")).unwrap();
    }
}

"##]]
        );
    }

    #[test]
    // [verify docgen.helper.crate-table-update]
    fn test_crate_table_update_is_automatic() {
        // Documentation-only test. The spec says updating bphelper must
        // automatically update table rendering for all battery packs.
        // This is true by construction: `{{crate-table}}` is a Handlebars
        // helper registered at runtime by `render_docs()` in this crate.
        // Battery packs call `generate_docs()` → `render_docs()`, so
        // updating this crate updates the helper for all consumers. No
        // per-battery-pack code generation is involved. This architectural
        // invariant cannot be meaningfully unit-tested — the test below
        // merely confirms the helper is registered (already covered by
        // test_render_crate_table).
        let ctx = simple_context();
        let output = render_docs("{{crate-table}}", &ctx).unwrap();
        assert!(
            output.contains("| Name |"),
            "crate-table helper must be registered"
        );
        assert_data_eq!(
            output,
            str![[r#"
## Battery pack contents

| Name | Description |
|------|-------------|
| [`anyhow`](https://crates.io/crates/anyhow) | Flexible concrete Error type |
| [`thiserror`](https://crates.io/crates/thiserror) | derive(Error) |


"#]]
        );
    }

    // ================================================================
    // Full pipeline tests (parse fixture → build context → render)
    // ================================================================

    #[test]
    // [verify docgen.template.handlebars]
    // [verify docgen.hidden.excluded]
    fn test_full_pipeline_fancy() {
        let spec = parse_fixture("fancy-battery-pack");
        let descriptions = mock_descriptions();
        let readme = "# fancy-battery-pack\n\nCLI tools.";
        let ctx = build_context(&spec, &descriptions, readme);
        let output = render_docs("{{readme}}\n\n{{crate-table}}", &ctx).unwrap();

        // Hidden crates (serde*, cc) must not appear in the table
        assert!(!output.contains("serde"));
        assert!(!output.contains("| cc"));
        // Visible crates must appear
        assert!(output.contains("[`clap`]"));
        assert!(output.contains("[`indicatif`]"));
        // Dev-deps should also appear
        assert!(output.contains("[`assert_cmd`]"));
        assert!(output.contains("[`predicates`]"));
        assert_data_eq!(
            output,
            str![[r#"
# fancy-battery-pack

CLI tools.

## Battery pack contents

### Dependencies

| Name | Description |
|------|-------------|
| [`clap`](https://crates.io/crates/clap) | A simple to use, efficient, and full-featured Command Line Argument Parser |
| [`console`](https://crates.io/crates/console) | A terminal and console abstraction for Rust |
| [`dialoguer`](https://crates.io/crates/dialoguer) | A command line prompting library |
| [`indicatif`](https://crates.io/crates/indicatif) | A progress bar and CLI reporting library |

### Dev dependencies

| Name | Description |
|------|-------------|
| [`assert_cmd`](https://crates.io/crates/assert_cmd) | Easy command testing |
| [`predicates`](https://crates.io/crates/predicates) | Boolean predicate combinators |

### Templates

| Name | Description |
|------|-------------|
| `default` | Basic CLI app |
| `full` | Full-featured CLI with indicators |


"#]]
        );
    }

    // ================================================================
    // I/O integration tests (generate_docs_from_dir)
    // ================================================================

    /// Set up a tempdir with a battery pack's Cargo.toml, template, and README.
    fn setup_docgen_dir(manifest: &str, template: &str, readme: Option<&str>) -> tempfile::TempDir {
        let dir = tempfile::tempdir().unwrap();
        std::fs::write(dir.path().join("Cargo.toml"), manifest).unwrap();
        // cargo_metadata requires a target; give the pack an empty lib.
        std::fs::create_dir_all(dir.path().join("src")).unwrap();
        std::fs::write(dir.path().join("src/lib.rs"), "").unwrap();
        std::fs::write(dir.path().join("docs.handlebars.md"), template).unwrap();
        if let Some(readme) = readme {
            std::fs::write(dir.path().join("README.md"), readme).unwrap();
        }
        dir
    }

    #[test]
    // [verify docgen.build.trigger]
    fn test_generate_docs_writes_output_file() {
        let fixture = fixtures_dir().join("basic-battery-pack/Cargo.toml");
        let manifest = std::fs::read_to_string(&fixture).unwrap();
        let dir = setup_docgen_dir(&manifest, "{{readme}}\n\n{{crate-table}}", Some("# Hello"));

        let out_dir = tempfile::tempdir().unwrap();
        let descriptions = mock_descriptions();

        generate_docs_from_dir(
            dir.path().to_str().unwrap(),
            out_dir.path().to_str().unwrap(),
            &descriptions,
        )
        .unwrap();

        let output_path = out_dir.path().join("docs.md");
        assert!(output_path.exists(), "docs.md must be written to out_dir");

        let content = std::fs::read_to_string(&output_path).unwrap();
        assert!(
            content.contains("# Hello"),
            "output must contain rendered readme"
        );
        assert!(
            content.contains("[`anyhow`]"),
            "output must contain crate table"
        );
        assert_data_eq!(
            content,
            str![[r#"
# Hello

## Battery pack contents

| Name | Description |
|------|-------------|
| [`anyhow`](https://crates.io/crates/anyhow) | Flexible concrete Error type built on std::error::Error |
| [`eyre`](https://crates.io/crates/eyre) | Flexible concrete Error type for easy idiomatic error handling |
| [`thiserror`](https://crates.io/crates/thiserror) | derive(Error) |


"#]]
        );
    }

    #[test]
    // [verify docgen.build.template]
    fn test_generate_docs_reads_template_from_manifest_dir() {
        let fixture = fixtures_dir().join("basic-battery-pack/Cargo.toml");
        let manifest = std::fs::read_to_string(&fixture).unwrap();

        // Custom template — only uses package name, no readme or crate table.
        let dir = setup_docgen_dir(
            &manifest,
            "# Docs for {{package.name}}",
            Some("ignored readme"),
        );

        let out_dir = tempfile::tempdir().unwrap();
        let descriptions = mock_descriptions();

        generate_docs_from_dir(
            dir.path().to_str().unwrap(),
            out_dir.path().to_str().unwrap(),
            &descriptions,
        )
        .unwrap();

        let content = std::fs::read_to_string(out_dir.path().join("docs.md")).unwrap();
        assert_eq!(content, "# Docs for basic-battery-pack");
    }

    #[test]
    fn test_generate_docs_missing_template_errors() {
        let fixture = fixtures_dir().join("basic-battery-pack/Cargo.toml");
        let manifest = std::fs::read_to_string(&fixture).unwrap();

        // Set up dir with Cargo.toml but NO template file.
        let dir = tempfile::tempdir().unwrap();
        std::fs::write(dir.path().join("Cargo.toml"), &manifest).unwrap();
        std::fs::create_dir_all(dir.path().join("src")).unwrap();
        std::fs::write(dir.path().join("src/lib.rs"), "").unwrap();

        let out_dir = tempfile::tempdir().unwrap();
        let descriptions = mock_descriptions();

        let result = generate_docs_from_dir(
            dir.path().to_str().unwrap(),
            out_dir.path().to_str().unwrap(),
            &descriptions,
        );

        assert!(result.is_err(), "missing template must produce an error");
        let err = result.unwrap_err().to_string();
        assert_data_eq!(
            err,
            str!["reading [..]/docs.handlebars.md: [..] (os error 2)"]
        );
    }

    #[test]
    fn test_generate_docs_readme_optional() {
        let fixture = fixtures_dir().join("basic-battery-pack/Cargo.toml");
        let manifest = std::fs::read_to_string(&fixture).unwrap();

        // No README — should still succeed with empty readme in context.
        let dir = setup_docgen_dir(&manifest, "readme=[{{readme}}]", None);

        let out_dir = tempfile::tempdir().unwrap();
        let descriptions = mock_descriptions();

        generate_docs_from_dir(
            dir.path().to_str().unwrap(),
            out_dir.path().to_str().unwrap(),
            &descriptions,
        )
        .unwrap();

        let content = std::fs::read_to_string(out_dir.path().join("docs.md")).unwrap();
        assert_eq!(
            content, "readme=[]",
            "missing README should produce empty string"
        );
    }

    #[test]
    // [verify docgen.helper.crate-table-metadata]
    fn test_fetch_crate_descriptions_returns_workspace_packages() {
        // This test calls the real cargo metadata against our workspace.
        // It verifies that fetch_crate_descriptions() returns descriptions
        // for packages that exist in the workspace.
        let descriptions = fetch_crate_descriptions().unwrap();

        // bphelper-build is in our workspace and has a description.
        assert!(
            descriptions.contains_key("bphelper-build"),
            "must include workspace package bphelper-build"
        );
        assert_data_eq!(
            &descriptions["bphelper-build"],
            str!["Build-time documentation generation for battery packs"]
        );
    }
}
