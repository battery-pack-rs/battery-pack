//! MiniJinja-based template engine for `cargo bp new`.
//!
//! Supports:
//! - `bp-template.toml` configuration (ignore, placeholders, file includes)
//! - MiniJinja syntax (`{{ project_name }}`, `{% include %}`, etc.)
//! - Pre-set variable overrides (for non-interactive / test usage)

use anyhow::{Context, Result, bail};
use serde::Deserialize;
use std::collections::BTreeMap;
use std::io::IsTerminal;
use std::path::{Path, PathBuf};

/// Parsed `bp-template.toml` configuration.
#[derive(Debug, Deserialize, Default)]
struct BpTemplateConfig {
    /// Files/folders to exclude from output entirely.
    #[serde(default)]
    ignore: Vec<String>,

    /// Placeholder definitions.
    #[serde(default)]
    placeholders: BTreeMap<String, PlaceholderDef>,

    /// Whole-file includes: copy src -> dest in the generated project.
    #[serde(default)]
    files: Vec<FileInclude>,
}

#[derive(Debug, Deserialize)]
struct PlaceholderDef {
    #[serde(default)]
    prompt: Option<String>,
    #[serde(default)]
    default: Option<String>,
    #[serde(default, rename = "type")]
    placeholder_type: PlaceholderType,
}

#[derive(Debug, Default, Deserialize, PartialEq)]
#[serde(rename_all = "lowercase")]
enum PlaceholderType {
    #[default]
    String,
    // TODO: support more types (bool, etc).
}

#[derive(Debug, Deserialize)]
struct FileInclude {
    src: String,
    dest: String,
}

/// Options for template generation.
pub(crate) struct GenerateOpts {
    /// Shared rendering options.
    pub(crate) render: RenderOpts,
    /// Output directory. The project will be created as a subdirectory named `project_name`.
    /// If `None`, uses the current directory.
    pub(crate) destination: Option<PathBuf>,
    /// Whether to run `git init` on the generated project.
    pub(crate) git_init: bool,
}

/// Shared options for rendering a template (used by both preview and generate).
pub(crate) struct RenderOpts {
    /// The battery pack crate root (contains `templates/` dir).
    pub(crate) crate_root: PathBuf,
    /// Relative path to the template dir within the crate (e.g. `templates/default`).
    pub(crate) template_path: String,
    /// Project name (kebab-case).
    pub(crate) project_name: String,
    /// Pre-set placeholder values (skip prompting for these).
    pub(crate) defines: BTreeMap<String, String>,
    /// Force treating the context as interactive or not, used to avoid prompting for input during tests,
    /// used to make sure we don't prompt for input during tests
    pub(crate) interactive_override: Option<bool>,
}

/// A rendered file from a template preview.
pub(crate) struct RenderedFile {
    /// Relative path within the generated project.
    pub(crate) path: String,
    /// Rendered file content.
    pub(crate) content: String,
}

/// Render a template and return the files in memory without writing to disk.
pub(crate) fn preview(mut opts: RenderOpts) -> Result<Vec<RenderedFile>> {
    let (template_dir, config) = load_config(&opts)?;
    // For preview, fall back to "<name>" for placeholders without a default
    // so the preview always renders.
    for (name, def) in &config.placeholders {
        opts.defines
            .entry(name.clone())
            .or_insert_with(|| def.default.clone().unwrap_or_else(|| format!("<{name}>")));
    }

    let variables = prepare_render(&opts, &config)?;
    render(&opts.crate_root, &template_dir, &config, &variables)
}

/// Generate a project from a battery pack template.
///
/// Returns the path to the generated project directory.
pub(crate) fn generate(opts: GenerateOpts) -> Result<PathBuf> {
    let (template_dir, config) = load_config(&opts.render)?;
    let variables = prepare_render(&opts.render, &config)?;

    let files = render(&opts.render.crate_root, &template_dir, &config, &variables)?;

    // Write rendered files to disk
    let dest_base = opts.destination.unwrap_or_else(|| PathBuf::from("."));
    let project_dir = dest_base.join(&opts.render.project_name);
    if project_dir.exists() {
        bail!("destination already exists: {}", project_dir.display());
    }

    for file in &files {
        let dest = project_dir.join(&file.path);
        if let Some(parent) = dest.parent() {
            std::fs::create_dir_all(parent)?;
        }
        std::fs::write(&dest, &file.content)
            .with_context(|| format!("failed to write {}", dest.display()))?;
    }

    if opts.git_init {
        git_init(&project_dir)?;
    }

    Ok(project_dir)
}

/// Shared rendering pipeline: resolves templates and file includes into memory.
fn render(
    crate_root: &Path,
    template_dir: &Path,
    config: &BpTemplateConfig,
    variables: &BTreeMap<String, String>,
) -> Result<Vec<RenderedFile>> {
    let env = build_jinja_env(crate_root, variables)?;
    let ignore_set: Vec<&str> = config.ignore.iter().map(|s| s.as_str()).collect();

    let mut files = Vec::new();

    for entry in walkdir::WalkDir::new(template_dir) {
        let entry = entry?;
        let rel_path = entry.path().strip_prefix(template_dir)?;

        if should_ignore(rel_path, &ignore_set) {
            continue;
        }
        if rel_path == Path::new("bp-template.toml") {
            continue;
        }
        if entry.file_type().is_dir() {
            continue;
        }

        let rendered_path = env.render_str(&rel_path.to_string_lossy(), minijinja::context! {})?;
        let content = std::fs::read_to_string(entry.path())
            .with_context(|| format!("failed to read {}", entry.path().display()))?;
        let rendered = env
            .render_str(&content, minijinja::context! {})
            .with_context(|| format!("failed to render template {}", rel_path.display()))?;

        files.push(RenderedFile {
            path: rendered_path,
            content: rendered,
        });
    }

    // Process [[files]] includes
    for file_include in &config.files {
        let src_path = crate_root.join(&file_include.src);
        if !src_path.exists() {
            bail!("file include source not found: {}", src_path.display());
        }
        if files.iter().any(|f| f.path == file_include.dest) {
            continue;
        }
        let content = std::fs::read_to_string(&src_path)
            .with_context(|| format!("failed to read {}", src_path.display()))?;
        let rendered = env
            .render_str(&content, minijinja::context! {})
            .with_context(|| format!("failed to render {}", src_path.display()))?;
        files.push(RenderedFile {
            path: file_include.dest.clone(),
            content: rendered,
        });
    }

    files.sort_by(|a, b| a.path.cmp(&b.path));

    // Resolve bp-managed dependencies in all rendered Cargo.toml files.
    // Resolution is best-effort: nested template Cargo.toml files (e.g. in a
    // battery-pack-of-battery-packs) may reference battery packs that don't
    // exist yet, so we silently skip files that fail to resolve.
    for file in files.iter_mut().filter(|f| f.path.ends_with("Cargo.toml")) {
        if let Ok(resolved) = crate::resolve_bp_managed_content(&file.content, crate_root) {
            file.content = resolved;
        }
    }

    Ok(files)
}

/// Resolve template variables from render options and config.
fn prepare_render(
    opts: &RenderOpts,
    config: &BpTemplateConfig,
) -> Result<BTreeMap<String, String>> {
    let mut variables = BTreeMap::new();
    variables.insert("project_name".to_string(), opts.project_name.clone());
    variables.insert(
        "crate_name".to_string(),
        opts.project_name.replace('-', "_"),
    );
    resolve_placeholders(
        &config.placeholders,
        &opts.defines,
        &mut variables,
        opts.interactive_override,
    )?;
    Ok(variables)
}

fn load_config(opts: &RenderOpts) -> Result<(PathBuf, BpTemplateConfig)> {
    let template_dir = opts.crate_root.join(&opts.template_path);
    if !template_dir.is_dir() {
        bail!("template directory not found: {}", template_dir.display());
    }
    let config_path = template_dir.join("bp-template.toml");
    if !config_path.exists() {
        return Ok((template_dir, BpTemplateConfig::default()));
    }
    let content = std::fs::read_to_string(&config_path)
        .with_context(|| format!("failed to read {}", config_path.display()))?;
    let config = toml::from_str(&content)
        .with_context(|| format!("failed to parse {}", config_path.display()))?;
    Ok((template_dir, config))
}

fn resolve_placeholders(
    defs: &BTreeMap<String, PlaceholderDef>,
    defines: &BTreeMap<String, String>,
    variables: &mut BTreeMap<String, String>,
    interactive_override: Option<bool>,
) -> Result<()> {
    let interactive = if let Some(interactive) = interactive_override {
        interactive
    } else {
        std::io::stdout().is_terminal()
    };

    for (name, def) in defs {
        // MiniJinja parses `-` as the minus operator, so kebab-case names
        // would be silently unreachable in templates without extra user handling.
        // better to avoid the footgun.
        if name.contains('-') {
            bail!(
                "placeholder '{name}' contains '-'; use snake_case (MiniJinja treats '-' as minus)"
            );
        }

        // Check pre-set overrides first
        if let Some(value) = defines.get(name) {
            variables.insert(name.clone(), value.clone());
            continue;
        }

        let value = match def.placeholder_type {
            PlaceholderType::String => {
                if interactive {
                    let prompt = def.prompt.as_deref().unwrap_or(name);
                    let mut builder = dialoguer::Input::<String>::new().with_prompt(prompt);
                    if let Some(default) = &def.default {
                        builder = builder.default(default.clone());
                    }
                    builder
                        .interact_text()
                        .with_context(|| format!("failed to read placeholder '{name}'"))?
                } else {
                    def.default.clone().ok_or_else(|| {
                        anyhow::anyhow!("placeholder '{name}' has no default and no value provided")
                    })?
                }
            }
        };
        variables.insert(name.clone(), value);
    }
    Ok(())
}

fn build_jinja_env(
    crate_root: &Path,
    variables: &BTreeMap<String, String>,
) -> Result<minijinja::Environment<'static>> {
    let mut env = minijinja::Environment::new();

    // Disable auto-escaping — we're generating Rust/TOML, not HTML
    env.set_auto_escape_callback(|_| minijinja::AutoEscape::None);

    // Set up include path resolution relative to crate root
    let crate_root = crate_root.to_path_buf();
    env.set_loader(move |name| {
        let path = crate_root.join(name);
        match std::fs::read_to_string(&path) {
            Ok(content) => Ok(Some(content)),
            Err(e) if e.kind() == std::io::ErrorKind::NotFound => Ok(None),
            Err(e) => Err(minijinja::Error::new(
                minijinja::ErrorKind::InvalidOperation,
                format!("failed to read include '{name}': {e}"),
            )),
        }
    });

    // Register all variables as globals
    for (key, value) in variables {
        env.add_global(key.clone(), value.clone());
    }

    Ok(env)
}

/// Check if a relative path should be ignored.
fn should_ignore(rel_path: &Path, ignore_set: &[&str]) -> bool {
    // Check each component of the path against the ignore set
    for component in rel_path.components() {
        let name = component.as_os_str().to_string_lossy();
        if ignore_set.iter().any(|&pattern| pattern == name) {
            return true;
        }
    }
    false
}

fn git_init(project_dir: &Path) -> Result<()> {
    let output = std::process::Command::new("git")
        .args(["init"])
        .current_dir(project_dir)
        .output()
        .context("failed to run `git init` — is git installed and on PATH?")?;
    if !output.status.success() {
        bail!(
            "git init failed:\n{}",
            String::from_utf8_lossy(&output.stderr)
        );
    }

    let output = std::process::Command::new("git")
        .args(["add", "."])
        .current_dir(project_dir)
        .output()
        .context("failed to run `git add .`")?;
    if !output.status.success() {
        bail!(
            "git add failed:\n{}",
            String::from_utf8_lossy(&output.stderr)
        );
    }

    Ok(())
}

#[cfg(test)]
mod tests;
