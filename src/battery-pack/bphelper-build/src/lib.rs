//! Build-time documentation generation for battery packs.
//!
//! Renders Handlebars templates with battery pack metadata to produce
//! documentation for docs.rs. The rendering pipeline is split into
//! pure functions (`build_context`, `render_docs`) for testability,
//! with `generate_docs` as the I/O entry point for build.rs.

use bphelper_manifest::{BatteryPackSpec, PickMode, parse_battery_pack_from_path};
use serde::Serialize;
use std::{collections::BTreeMap, path::Path};

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
        .map(|(name, refs)| FeatureEntry {
            name: name.clone(),
            crates: refs.iter().map(|r| r.dep_name().to_string()).collect(),
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
                    let mut crate_names: Vec<String> = spec
                        .features
                        .get(feat_name)
                        .map(|refs| refs.iter().map(|r| r.dep_name().to_string()).collect())
                        .unwrap_or_default();
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
mod tests;
