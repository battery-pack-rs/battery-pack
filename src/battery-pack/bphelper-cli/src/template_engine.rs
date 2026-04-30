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

    /// Post-merge hints shown after `cargo bp add -t`.
    #[serde(default)]
    hints: Vec<Hint>,
}

#[derive(Debug, Deserialize)]
struct PlaceholderDef {
    #[serde(default)]
    prompt: Option<String>,
    #[serde(default)]
    default: Option<String>,
    #[serde(default, rename = "type")]
    placeholder_type: PlaceholderType,
    /// Valid options for `select` type placeholders.
    #[serde(default)]
    options: Vec<String>,
}

#[derive(Debug, Default, Deserialize, PartialEq)]
#[serde(rename_all = "lowercase")]
enum PlaceholderType {
    #[default]
    String,
    Bool,
    Select,
}

#[derive(Debug, Deserialize)]
struct FileInclude {
    src: String,
    dest: String,
}

/// A post-merge hint shown to the user after `cargo bp add -t`.
#[derive(Debug, Deserialize)]
struct Hint {
    message: String,
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
#[derive(Debug)]
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

/// Load post-merge hints from a template's `bp-template.toml`.
///
/// Returns an empty vec if the template has no hints or no config file.
pub(crate) fn load_template_hints(crate_root: &Path, template_path: &str) -> Vec<String> {
    let config_path = crate_root.join(template_path).join("bp-template.toml");
    let Ok(content) = std::fs::read_to_string(&config_path) else {
        return Vec::new();
    };
    let Ok(config) = toml::from_str::<BpTemplateConfig>(&content) else {
        return Vec::new();
    };
    config.hints.into_iter().map(|h| h.message).collect()
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
    prefetch_pin_github_actions(crate_root);
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
        // Map _Cargo.toml → Cargo.toml so templates can ship Cargo.toml files
        // without cargo treating the directory as a separate crate boundary
        // during `cargo package`. Skip paths under templates/ so that battery
        // packs which generate other battery packs (e.g. with_template)
        // preserve _Cargo.toml in their scaffolded template directories.
        let rendered_path = if rendered_path.starts_with("templates/") {
            rendered_path
        } else {
            rendered_path.replace("_Cargo.toml", "Cargo.toml")
        };
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
    // TODO: support conditional file includes (e.g. a `condition` field evaluated as a
    // MiniJinja expression, or grouped tables like [[files.fuzzing]]) so battery pack
    // authors don't need to wrap entire file contents in {% if %}...{% endif %}.
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

    // Filter out files whose rendered content is empty (e.g. wrapped in {% if false %}...{% endif %}).
    files.retain(|f| !f.content.trim().is_empty());

    // Resolve bp-managed dependencies in all rendered Cargo.toml files.
    // Resolution can fail for nested templates (e.g. battery-pack-of-battery-packs)
    // that reference battery packs not yet published, so we warn instead of failing.
    for file in files
        .iter_mut()
        .filter(|f| std::path::Path::new(&f.path).file_name() == Some("Cargo.toml".as_ref()))
    {
        match crate::resolve_bp_managed_content(&file.content, crate_root) {
            Ok(resolved) => file.content = resolved,
            Err(e) => eprintln!(
                "warning: failed to resolve bp-managed deps in {}: {e:#}",
                file.path
            ),
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
            PlaceholderType::Bool => {
                if interactive {
                    let prompt = def.prompt.as_deref().unwrap_or(name);
                    let default_val = def
                        .default
                        .as_deref()
                        .map(|d| d.eq_ignore_ascii_case("true"))
                        .unwrap_or(false);
                    let val = dialoguer::Confirm::new()
                        .with_prompt(prompt)
                        .default(default_val)
                        .interact()
                        .with_context(|| format!("failed to read placeholder '{name}'"))?;
                    val.to_string()
                } else {
                    def.default.clone().unwrap_or_else(|| "false".to_string())
                }
            }
            PlaceholderType::Select => {
                if def.options.is_empty() {
                    bail!("select placeholder '{name}' has no options");
                }
                if interactive {
                    let prompt = def.prompt.as_deref().unwrap_or(name);
                    let default_idx = def
                        .default
                        .as_ref()
                        .and_then(|d| def.options.iter().position(|o| o == d))
                        .unwrap_or(0);
                    let idx = dialoguer::Select::new()
                        .with_prompt(prompt)
                        .items(&def.options)
                        .default(default_idx)
                        .interact()
                        .with_context(|| format!("failed to read placeholder '{name}'"))?;
                    def.options[idx].clone()
                } else {
                    let val = def.default.clone().ok_or_else(|| {
                        anyhow::anyhow!("placeholder '{name}' has no default and no value provided")
                    })?;
                    if !def.options.contains(&val) {
                        bail!(
                            "placeholder '{name}' default '{}' is not in options: {:?}",
                            val,
                            def.options
                        );
                    }
                    val
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

    // Register all variables as globals.
    // "true"/"false" strings are registered as actual bools so {% if flag %} works.
    for (key, value) in variables {
        match value.as_str() {
            "true" => env.add_global(key.clone(), true),
            "false" => env.add_global(key.clone(), false),
            _ => env.add_global(key.clone(), value.clone()),
        }
    }

    // Register pin_github_action — reads from the global cache (populated by prefetch or on-demand).
    env.add_function(
        "pin_github_action",
        |owner_repo: &str, tag: &str, subpath: Option<&str>| -> String {
            let key = (owner_repo.to_string(), tag.to_string());
            let mut cache = PIN_GITHUB_ACTION_CACHE.lock().unwrap();
            let resolved = if let Some(cached) = cache.get(&key) {
                cached.clone()
            } else {
                let result =
                    resolve_latest_tag(owner_repo, tag).or_else(|_| resolve_ref(owner_repo, tag));
                let output = match result {
                    Ok((sha, resolved_tag)) => format!("{owner_repo}@{sha} # {resolved_tag}"),
                    Err(_) => {
                        let url = format!("https://github.com/{owner_repo}.git");
                        format!(
                            "{owner_repo}@could-not-resolve-git-sha-for-{tag} \
                         # TODO: run 'git ls-remote --tags {url} \"refs/tags/{tag}.*\"' and pin"
                        )
                    }
                };
                cache.insert(key, output.clone());
                output
            };
            // Insert subpath between owner/repo and @sha
            if let Some(sub) = subpath {
                resolved.replacen(owner_repo, &format!("{owner_repo}/{sub}"), 1)
            } else {
                resolved
            }
        },
    );

    // Register rust_stable_version() — returns the current stable Rust version (e.g. "1.91.1").
    env.add_function("rust_stable_version", rust_stable_version);

    Ok(env)
}

/// Returns the current stable Rust version from `rustc --version`.
fn rust_stable_version() -> String {
    let output = std::process::Command::new("rustc")
        .args(["--version"])
        .output()
        .expect("rustc must be on PATH");
    let s = String::from_utf8_lossy(&output.stdout);
    s.split_whitespace()
        .nth(1)
        .expect("unexpected rustc --version output")
        .to_string()
}

/// Global cache for resolved pin_github_action results. Persists across preview/render calls.
static PIN_GITHUB_ACTION_CACHE: std::sync::LazyLock<
    std::sync::Mutex<std::collections::HashMap<(String, String), String>>,
> = std::sync::LazyLock::new(|| std::sync::Mutex::new(std::collections::HashMap::new()));

/// Pre-resolve all `pin_github_action` calls in parallel, then register the cached results.
fn prefetch_pin_github_actions(crate_root: &Path) {
    use std::collections::HashSet;

    let re = regex::Regex::new(r#"pin_github_action\(\s*"([^"]+)"\s*,\s*"([^"]+)"\s*\)"#).unwrap();
    let mut pairs: HashSet<(String, String)> = HashSet::new();

    // Scan all templates and snippets for pin_github_action calls.
    // We scan broadly because templates can {% include %} from other template dirs.
    for dir in [crate_root.join("templates"), crate_root.join("snippets")] {
        if !dir.is_dir() {
            continue;
        }
        for entry in walkdir::WalkDir::new(dir).into_iter().flatten() {
            if entry.file_type().is_file() {
                let Ok(content) = std::fs::read_to_string(entry.path()) else {
                    continue;
                };
                for cap in re.captures_iter(&content) {
                    pairs.insert((cap[1].to_string(), cap[2].to_string()));
                }
            }
        }
    }

    if pairs.is_empty() {
        return;
    }

    // Filter out already-cached pairs.
    {
        let cache = PIN_GITHUB_ACTION_CACHE.lock().unwrap();
        pairs.retain(|key| !cache.contains_key(key));
    }

    // Resolve uncached pairs in parallel.
    if !pairs.is_empty() {
        std::thread::scope(|s| {
            for (owner_repo, tag) in &pairs {
                let owner_repo = owner_repo.clone();
                let tag = tag.clone();
                s.spawn(move || {
                    let result = resolve_latest_tag(&owner_repo, &tag)
                        .or_else(|_| resolve_ref(&owner_repo, &tag));
                    let output = match result {
                        Ok((sha, resolved_tag)) => {
                            format!("{owner_repo}@{sha} # {resolved_tag}")
                        }
                        Err(_) => {
                            let url = format!("https://github.com/{owner_repo}.git");
                            format!(
                                "{owner_repo}@could-not-resolve-git-sha-for-{tag} \
                                 # TODO: run 'git ls-remote --tags {url} \"refs/tags/{tag}.*\"' and pin"
                            )
                        }
                    };
                    PIN_GITHUB_ACTION_CACHE
                        .lock()
                        .unwrap()
                        .insert((owner_repo, tag), output);
                });
            }
        });
    }
}

/// Find the latest semver tag matching a prefix and return (sha, tag_name).
///
/// For `tag = "v4"`, lists all `v4*` tags, parses semver, picks the highest,
/// and returns its SHA. Falls back to the exact tag if no semver matches exist.
fn resolve_latest_tag(owner_repo: &str, tag: &str) -> Result<(String, String)> {
    let url = format!("https://github.com/{owner_repo}.git");
    let output = std::process::Command::new("git")
        .args(["ls-remote", "--tags", &url, &format!("refs/tags/{tag}*")])
        .output()
        .context("failed to run git ls-remote")?;

    if !output.status.success() {
        bail!("git ls-remote failed for {owner_repo}@{tag}");
    }

    let stdout = String::from_utf8_lossy(&output.stdout);

    // Parse all tags into (sha, tag_name) pairs.
    // For annotated tags, prefer the ^{} (dereferenced) line.
    let mut tags: Vec<(String, String)> = Vec::new();
    for line in stdout.lines() {
        let Some((sha, ref_name)) = line.split_once('\t') else {
            continue;
        };
        let tag_name = ref_name.strip_prefix("refs/tags/").unwrap_or(ref_name);

        // ^{} is the dereferenced commit for annotated tags — update the SHA
        if let Some(base) = tag_name.strip_suffix("^{}") {
            if let Some(entry) = tags.iter_mut().find(|(_, t)| t == base) {
                entry.0 = sha.to_string();
            } else {
                tags.push((sha.to_string(), base.to_string()));
            }
        } else {
            tags.push((sha.to_string(), tag_name.to_string()));
        }
    }

    if tags.is_empty() {
        bail!("no tags matching '{tag}*' found in {owner_repo}");
    }

    // Find the highest semver tag.
    let best = tags
        .iter()
        .filter_map(|(sha, name)| {
            let version_str = name.strip_prefix('v').unwrap_or(name);
            let version = semver::Version::parse(version_str).ok()?;
            Some((sha.clone(), name.clone(), version))
        })
        .max_by(|(_, _, a), (_, _, b)| a.cmp(b));

    match best {
        Some((sha, name, _)) => Ok((sha, name)),
        // No semver tags found — fall back to exact tag match
        None => tags
            .into_iter()
            .find(|(_, name)| name == tag)
            .ok_or_else(|| anyhow::anyhow!("tag '{tag}' not found in {owner_repo}")),
    }
}

/// Resolve a non-tag ref (branch name like "stable", "master") to its commit SHA.
fn resolve_ref(owner_repo: &str, ref_name: &str) -> Result<(String, String)> {
    let url = format!("https://github.com/{owner_repo}.git");
    let output = std::process::Command::new("git")
        .args(["ls-remote", &url, ref_name])
        .output()
        .context("failed to run git ls-remote")?;

    if !output.status.success() {
        bail!("git ls-remote failed for {owner_repo}@{ref_name}");
    }

    let stdout = String::from_utf8_lossy(&output.stdout);
    let sha = stdout
        .lines()
        .find_map(|line| {
            let (sha, _) = line.split_once('\t')?;
            Some(sha.to_string())
        })
        .ok_or_else(|| anyhow::anyhow!("ref '{ref_name}' not found in {owner_repo}"))?;

    Ok((sha, ref_name.to_string()))
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

/// Options for previewing a battery pack template.
pub(crate) struct PreviewOpts<'a> {
    pub battery_pack: &'a str,
    pub template: &'a str,
    pub path: Option<&'a str>,
    pub source: &'a crate::registry::CrateSource,
    pub defines: BTreeMap<String, String>,
}

/// Resolve a battery pack template and render a preview.
///
/// Handles crate-dir resolution, manifest parsing, template lookup, and
/// rendering. Returns the rendered files and the resolved crate name.
pub(crate) fn preview_template(opts: &PreviewOpts<'_>) -> Result<(String, Vec<RenderedFile>)> {
    let crate_name = crate::registry::resolve_crate_name(opts.battery_pack);
    let resolved = crate::registry::resolve_crate_dir(opts.battery_pack, opts.path, opts.source)?;

    let manifest_path = resolved.dir.join("Cargo.toml");
    let manifest_content = std::fs::read_to_string(&manifest_path)
        .with_context(|| format!("Failed to read {}", manifest_path.display()))?;
    let spec = bphelper_manifest::parse_battery_pack(&manifest_content)
        .map_err(|e| anyhow::anyhow!("Failed to parse battery pack: {e}"))?;
    let tmpl = spec.templates.get(opts.template).ok_or_else(|| {
        let available: Vec<_> = spec.templates.keys().map(|s| s.as_str()).collect();
        anyhow::anyhow!(
            "Template '{}' not found. Available: {}",
            opts.template,
            available.join(", ")
        )
    })?;

    let opts = RenderOpts {
        crate_root: resolved.dir,
        template_path: tmpl.path.clone(),
        project_name: "my-project".to_string(),
        defines: opts.defines.clone(),
        interactive_override: Some(false),
    };
    let files = preview(opts)?;
    Ok((crate_name, files))
}

#[cfg(test)]
mod tests;
