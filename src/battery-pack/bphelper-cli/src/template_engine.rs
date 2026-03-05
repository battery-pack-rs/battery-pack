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
    /// The battery pack crate root (contains `templates/` dir).
    pub(crate) crate_root: PathBuf,
    /// Relative path to the template dir within the crate (e.g. `templates/default`).
    pub(crate) template_path: String,
    /// Project name (kebab-case).
    pub(crate) project_name: String,
    /// Output directory. The project will be created as a subdirectory named `project_name`.
    /// If `None`, uses the current directory.
    pub(crate) destination: Option<PathBuf>,
    /// Pre-set placeholder values (skip prompting for these).
    pub(crate) defines: BTreeMap<String, String>,
    /// Whether to run `git init` on the generated project.
    pub(crate) git_init: bool,
}

/// Generate a project from a battery pack template.
///
/// Returns the path to the generated project directory.
pub(crate) fn generate(opts: GenerateOpts) -> Result<PathBuf> {
    let template_dir = opts.crate_root.join(&opts.template_path);
    if !template_dir.is_dir() {
        bail!("template directory not found: {}", template_dir.display());
    }

    // Parse bp-template.toml (optional — templates without config still work)
    let config = load_config(&template_dir)?;

    // Resolve placeholder values
    let mut variables = BTreeMap::new();
    variables.insert("project_name".to_string(), opts.project_name.clone());
    variables.insert(
        "crate_name".to_string(),
        opts.project_name.replace('-', "_"),
    );
    resolve_placeholders(&config.placeholders, &opts.defines, &mut variables)?;

    // Set up MiniJinja environment with include support
    let env = build_jinja_env(&opts.crate_root, &variables)?;

    // Determine output directory
    let dest_base = opts.destination.unwrap_or_else(|| PathBuf::from("."));
    let project_dir = dest_base.join(&opts.project_name);
    if project_dir.exists() {
        bail!("destination already exists: {}", project_dir.display());
    }
    std::fs::create_dir_all(&project_dir)
        .with_context(|| format!("failed to create {}", project_dir.display()))?;

    let ignore_set: Vec<&str> = config.ignore.iter().map(|s| s.as_str()).collect();

    // Walk template directory and render files
    render_template_dir(&env, &template_dir, &project_dir, &ignore_set, &variables)?;

    // Process [[files]] includes
    for file_include in &config.files {
        let src_path = opts.crate_root.join(&file_include.src);
        let dest_path = project_dir.join(&file_include.dest);

        if !src_path.exists() {
            bail!("file include source not found: {}", src_path.display());
        }

        // Don't overwrite files from the template directory
        if dest_path.exists() {
            continue;
        }

        if let Some(parent) = dest_path.parent() {
            std::fs::create_dir_all(parent)?;
        }

        let content = std::fs::read_to_string(&src_path)
            .with_context(|| format!("failed to read {}", src_path.display()))?;
        let rendered = env
            .render_str(&content, minijinja::context! {})
            .with_context(|| format!("failed to render {}", src_path.display()))?;
        std::fs::write(&dest_path, rendered)
            .with_context(|| format!("failed to write {}", dest_path.display()))?;
    }

    // git init
    if opts.git_init {
        git_init(&project_dir)?;
    }

    Ok(project_dir)
}

fn load_config(template_dir: &Path) -> Result<BpTemplateConfig> {
    let config_path = template_dir.join("bp-template.toml");
    if !config_path.exists() {
        return Ok(BpTemplateConfig::default());
    }
    let content = std::fs::read_to_string(&config_path)
        .with_context(|| format!("failed to read {}", config_path.display()))?;
    toml::from_str(&content).with_context(|| format!("failed to parse {}", config_path.display()))
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

fn render_template_dir(
    env: &minijinja::Environment<'_>,
    template_dir: &Path,
    output_dir: &Path,
    ignore_set: &[&str],
    variables: &BTreeMap<String, String>,
) -> Result<()> {
    for entry in walkdir::WalkDir::new(template_dir) {
        let entry = entry?;
        let rel_path = entry.path().strip_prefix(template_dir)?;

        // Skip ignored files/folders
        if should_ignore(rel_path, ignore_set) {
            continue;
        }

        // The root bp-template.toml is the engine's own config and is never
        // included in output. Nested bp-template.toml files (e.g. inside a
        // scaffolded template directory) pass through normally.
        if rel_path == Path::new("bp-template.toml") {
            continue;
        }

        // Render template variables in the path (e.g. {{crate_name}}/mod.rs)
        let rendered_path = env.render_str(&rel_path.to_string_lossy(), minijinja::context! {})?;

        if entry.file_type().is_dir() {
            let dest = output_dir.join(&rendered_path);
            std::fs::create_dir_all(&dest)?;
            continue;
        }

        let dest = output_dir.join(&rendered_path);

        if let Some(parent) = dest.parent() {
            std::fs::create_dir_all(parent)?;
        }

        // Read and render file content
        let content = std::fs::read_to_string(entry.path())
            .with_context(|| format!("failed to read {}", entry.path().display()))?;
        let rendered = env
            .render_str(&content, minijinja::context! {})
            .with_context(|| format!("failed to render template {}", rel_path.display()))?;
        std::fs::write(&dest, rendered)
            .with_context(|| format!("failed to write {}", dest.display()))?;
    }
    Ok(())
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
        // bp-template.toml is excluded by a root-only check in render_template_dir,
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
}
