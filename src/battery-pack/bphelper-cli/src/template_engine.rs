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
    resolve_placeholders(&config.placeholders, &opts.defines, &mut variables)?;
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
) -> Result<()> {
    let interactive = std::io::stdout().is_terminal();

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
mod tests {
    use super::*;

    // -- Config parsing --
    // [verify format.templates.engine]

    #[test]
    fn parse_config_full() {
        let toml = r#"
            ignore = ["hooks", ".git"]

            [placeholders.description]
            type = "string"
            prompt = "Describe it"
            default = "A thing"

            [[files]]
            src = "shared/LICENSE"
            dest = "LICENSE"
        "#;
        let config: BpTemplateConfig = toml::from_str(toml).unwrap();
        assert_eq!(config.ignore, vec!["hooks", ".git"]);
        assert_eq!(config.placeholders.len(), 1);
        let desc = &config.placeholders["description"];
        assert_eq!(desc.prompt.as_deref(), Some("Describe it"));
        assert_eq!(desc.default.as_deref(), Some("A thing"));
        assert_eq!(config.files.len(), 1);
        assert_eq!(config.files[0].src, "shared/LICENSE");
        assert_eq!(config.files[0].dest, "LICENSE");
    }

    #[test]
    fn parse_config_empty() {
        let config: BpTemplateConfig = toml::from_str("").unwrap();
        assert!(config.ignore.is_empty());
        assert!(config.placeholders.is_empty());
        assert!(config.files.is_empty());
    }

    #[test]
    fn parse_config_placeholder_defaults() {
        let toml = r#"
            [placeholders.name]
        "#;
        let config: BpTemplateConfig = toml::from_str(toml).unwrap();
        let p = &config.placeholders["name"];
        assert!(p.prompt.is_none());
        assert!(p.default.is_none());
        assert_eq!(p.placeholder_type, PlaceholderType::String);
    }

    // -- should_ignore --

    #[test]
    fn ignore_exact_match() {
        assert!(should_ignore(Path::new("hooks"), &["hooks"]));
    }

    #[test]
    fn ignore_nested_component() {
        assert!(should_ignore(
            Path::new("hooks/pre-script.rhai"),
            &["hooks"]
        ));
    }

    #[test]
    fn ignore_no_match() {
        assert!(!should_ignore(Path::new("src/main.rs"), &["hooks"]));
    }

    #[test]
    fn ignore_bp_template_toml() {
        // bp-template.toml is excluded by a root-only check in the render pipeline,
        // NOT via should_ignore. Nested bp-template.toml files pass through.
        assert!(!should_ignore(Path::new("bp-template.toml"), &["hooks"]));
        assert!(!should_ignore(
            Path::new("templates/default/bp-template.toml"),
            &["hooks"]
        ));
    }

    // -- resolve_placeholders --
    // [verify format.templates.placeholder-defaults]

    #[test]
    fn resolve_uses_define_over_default() {
        let mut defs = BTreeMap::new();
        defs.insert(
            "description".to_string(),
            PlaceholderDef {
                prompt: None,
                default: Some("fallback".to_string()),
                placeholder_type: PlaceholderType::String,
            },
        );
        let mut defines = BTreeMap::new();
        defines.insert("description".to_string(), "override".to_string());
        let mut vars = BTreeMap::new();

        resolve_placeholders(&defs, &defines, &mut vars).unwrap();
        assert_eq!(vars["description"], "override");
    }

    #[test]
    fn resolve_uses_default_non_interactive() {
        let mut defs = BTreeMap::new();
        defs.insert(
            "description".to_string(),
            PlaceholderDef {
                prompt: None,
                default: Some("fallback".to_string()),
                placeholder_type: PlaceholderType::String,
            },
        );
        let defines = BTreeMap::new();
        let mut vars = BTreeMap::new();

        // In test/CI, stdout is not a terminal, so non-interactive path is taken
        resolve_placeholders(&defs, &defines, &mut vars).unwrap();
        assert_eq!(vars["description"], "fallback");
    }

    #[test]
    fn resolve_no_default_non_interactive_errors() {
        let mut defs = BTreeMap::new();
        defs.insert(
            "description".to_string(),
            PlaceholderDef {
                prompt: Some("Describe it".to_string()),
                default: None,
                placeholder_type: PlaceholderType::String,
            },
        );
        let defines = BTreeMap::new();
        let mut vars = BTreeMap::new();

        let err = resolve_placeholders(&defs, &defines, &mut vars).unwrap_err();
        assert!(err.to_string().contains("description"));
        assert!(err.to_string().contains("no default"));
    }

    #[test]
    fn resolve_rejects_kebab_case_name() {
        let mut defs = BTreeMap::new();
        defs.insert(
            "my-thing".to_string(),
            PlaceholderDef {
                prompt: None,
                default: Some("val".to_string()),
                placeholder_type: PlaceholderType::String,
            },
        );
        let err = resolve_placeholders(&defs, &BTreeMap::new(), &mut BTreeMap::new()).unwrap_err();
        assert!(err.to_string().contains("my-thing"));
        assert!(err.to_string().contains("snake_case"));
    }

    // -- build_jinja_env --

    #[test]
    fn jinja_env_renders_variables() {
        let mut vars = BTreeMap::new();
        vars.insert("project_name".to_string(), "my-app".to_string());
        vars.insert("crate_name".to_string(), "my_app".to_string());

        let env = build_jinja_env(Path::new("."), &vars).unwrap();
        let result = env
            .render_str(
                "name = {{ project_name }}, crate = {{ crate_name }}",
                minijinja::context! {},
            )
            .unwrap();
        assert_eq!(result, "name = my-app, crate = my_app");
    }

    #[test]
    fn jinja_env_no_html_escaping() {
        let mut vars = BTreeMap::new();
        vars.insert(
            "description".to_string(),
            "A <bold> & cool thing".to_string(),
        );

        let env = build_jinja_env(Path::new("."), &vars).unwrap();
        let result = env
            .render_str("{{ description }}", minijinja::context! {})
            .unwrap();
        assert_eq!(result, "A <bold> & cool thing");
    }

    #[test]
    fn jinja_env_raw_block_passthrough() {
        let vars = BTreeMap::new();
        let env = build_jinja_env(Path::new("."), &vars).unwrap();
        let result = env
            .render_str(
                "{% raw %}{{ not_a_variable }}{% endraw %}",
                minijinja::context! {},
            )
            .unwrap();
        assert_eq!(result, "{{ not_a_variable }}");
    }

    // -- crate_name derivation --

    #[test]
    fn crate_name_derived_from_project_name() {
        let project_name = "my-cool-app";
        let crate_name = project_name.replace('-', "_");
        assert_eq!(crate_name, "my_cool_app");
    }

    #[test]
    fn parse_config_unsupported_type_errors() {
        let toml = r#"
            [placeholders.flag]
            type = "bool"
        "#;
        let err = toml::from_str::<BpTemplateConfig>(toml).unwrap_err();
        assert!(err.to_string().contains("unknown variant"), "{err}");
    }

    // -- preview --

    #[test]
    fn preview_renders_template_in_memory() {
        let fixtures = Path::new(env!("CARGO_MANIFEST_DIR"))
            .parent()
            .unwrap()
            .parent()
            .unwrap()
            .parent()
            .unwrap()
            .join("tests/fixtures/fancy-battery-pack");

        let opts = RenderOpts {
            crate_root: fixtures,
            template_path: "templates/default".to_string(),
            project_name: "my-project".to_string(),
            defines: BTreeMap::new(),
        };

        let files = preview(opts).unwrap();
        assert!(!files.is_empty(), "preview should produce files");

        // Should contain Cargo.toml with rendered project name
        let cargo = files.iter().find(|f| f.path == "Cargo.toml").unwrap();
        assert!(
            cargo.content.contains("my-project"),
            "Cargo.toml should contain rendered project name"
        );

        // Should contain src/main.rs
        assert!(
            files.iter().any(|f| f.path == "src/main.rs"),
            "should contain src/main.rs"
        );

        // Files should be sorted by path
        let paths: Vec<&str> = files.iter().map(|f| f.path.as_str()).collect();
        let mut sorted = paths.clone();
        sorted.sort();
        assert_eq!(paths, sorted, "files should be sorted by path");
    }

    #[test]
    fn preview_resolves_bp_managed_deps() {
        use expect_test::expect;

        let fixtures = Path::new(env!("CARGO_MANIFEST_DIR"))
            .parent()
            .unwrap()
            .parent()
            .unwrap()
            .parent()
            .unwrap()
            .join("tests/fixtures/managed-battery-pack");

        let opts = RenderOpts {
            crate_root: fixtures,
            template_path: "templates/default".to_string(),
            project_name: "my-project".to_string(),
            defines: BTreeMap::new(),
        };

        let files = preview(opts).unwrap();
        let cargo = files.iter().find(|f| f.path == "Cargo.toml").unwrap();

        expect![[r#"
            [package]
            name = "my-project"
            version = "0.1.0"
            edition = "2021"

            [dependencies]
            anyhow = "1"
            clap = { version = "4", features = ["derive"] }

            [build-dependencies]
            managed-battery-pack = "0.2.0"

            [package.metadata.battery-pack]
            managed-battery-pack = { features = ["default"] }
        "#]]
        .assert_eq(&cargo.content);
    }
}
