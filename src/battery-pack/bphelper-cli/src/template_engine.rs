//! MiniJinja-based template engine for `cargo bp new`.
//!
//! Supports:
//! - `bp-template.toml` configuration (ignore, placeholders, file includes)
//! - MiniJinja syntax (`{{ project_name }}`, `{% include %}`, etc.)
//! - Pre-set variable overrides (for non-interactive / test usage)

use anyhow::{Context, Result, bail};
use bphelper_manifest::parse_battery_pack_from_path;
use serde::Deserialize;
use std::collections::{BTreeMap, BTreeSet};
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
    /// Options for a `select` placeholder: either a literal list
    /// (`options = ["a", "b"]`) or a category reference
    /// (`options.category = "allocator"`).
    #[serde(default)]
    options: Option<OptionsSource>,
}

/// Where a `select` placeholder's options come from.
#[derive(Debug, Deserialize)]
#[serde(untagged)]
enum OptionsSource {
    /// A literal list of option strings.
    Literal(Vec<String>),
    /// The members of a battery pack category.
    Category { category: String },
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
    /// Feature names the user selected in the picker. A category-linked
    /// `select` placeholder whose category contains one of these is pre-filled
    /// without prompting.
    pub(crate) active_features: BTreeSet<String>,
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

    // A category-linked placeholder that the picker already resolved must not
    // be shadowed by the synthesized fallback below, so skip those here; the
    // prefill is applied inside `resolve_placeholders`.
    let resolved = resolve_option_sources(&opts, &config)?;
    let has_prefill = |name: &str| resolved.get(name).is_some_and(|r| r.prefill.is_some());

    // For preview, fall back to "<name>" for placeholders without a default
    // so the preview always renders.
    for (name, def) in &config.placeholders {
        if has_prefill(name) {
            continue;
        }
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
    let mut env = build_jinja_env(crate_root, variables)?;
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

        // Normalize to forward slashes so rendered paths are OS-agnostic.
        let rendered_path = env
            .render_str(&rel_path.to_string_lossy(), minijinja::context! {})?
            .replace('\\', "/");
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
        // Whitespace trimming and trailing-newline keeping apply to Rust only, where rustfmt also
        // normalizes the result. Other formats (YAML, TOML) are newline-sensitive and keep the
        // legacy behavior, so their templates still use `{%- -%}` for whitespace control.
        let is_rust = rel_path.extension().and_then(|e| e.to_str()) == Some("rs");
        env.set_trim_blocks(is_rust);
        env.set_lstrip_blocks(is_rust);
        env.set_keep_trailing_newline(is_rust);
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
        let is_rust = file_include.dest.ends_with(".rs");
        env.set_trim_blocks(is_rust);
        env.set_lstrip_blocks(is_rust);
        env.set_keep_trailing_newline(is_rust);
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

    // Map each directory to its rendered battery-pack.toml, the active-pack/feature source for
    // resolution. Falls back to the Cargo.toml metadata section inside the resolver when absent.
    let parent_dir = |p: &str| -> String {
        std::path::Path::new(p)
            .parent()
            .map(|d| d.to_string_lossy().into_owned())
            .unwrap_or_default()
    };
    let bp_states: std::collections::HashMap<String, String> = files
        .iter()
        .filter(|f| std::path::Path::new(&f.path).file_name() == Some("battery-pack.toml".as_ref()))
        .map(|f| (parent_dir(&f.path), f.content.clone()))
        .collect();

    // Resolve bp-managed dependencies in all rendered Cargo.toml files.
    // Resolution can fail for nested templates (e.g. battery-pack-of-battery-packs)
    // that reference battery packs not yet published, so we warn instead of failing.
    for file in files
        .iter_mut()
        .filter(|f| std::path::Path::new(&f.path).file_name() == Some("Cargo.toml".as_ref()))
    {
        let state = bp_states.get(&parent_dir(&file.path)).map(String::as_str);
        match crate::resolve_bp_managed_content(&file.content, crate_root, state) {
            Ok(resolved) => file.content = resolved,
            Err(e) => eprintln!(
                "warning: failed to resolve bp-managed deps in {}: {e:#}",
                file.path
            ),
        }
    }

    // Normalize rendered Rust through rustfmt so generated output is formatting-stable regardless
    // of template whitespace. No-ops if rustfmt is missing or rejects the input.
    for file in files.iter_mut().filter(|f| f.path.ends_with(".rs")) {
        file.content = rustfmt_rust(std::mem::take(&mut file.content));
    }

    Ok(files)
}

/// Formats Rust source with rustfmt (edition 2024) over stdin/stdout, with no temporary files so
/// it works for previews too. Returns the input unchanged if rustfmt is unavailable or fails, so
/// generation never hard-depends on a rustfmt install.
fn rustfmt_rust(source: String) -> String {
    use std::io::{Read, Write};
    use std::process::{Command, Stdio};

    let mut child = match Command::new("rustfmt")
        .args(["--edition", "2024", "--emit", "stdout", "--quiet"])
        .stdin(Stdio::piped())
        .stdout(Stdio::piped())
        .stderr(Stdio::null())
        .spawn()
    {
        Ok(child) => child,
        Err(_) => return source,
    };

    // Feed stdin from another thread so a full stdout pipe can't deadlock the write.
    let mut stdin = child.stdin.take().expect("stdin is piped");
    let input = source.clone();
    let writer = std::thread::spawn(move || {
        let _ = stdin.write_all(input.as_bytes());
    });

    let mut formatted = String::new();
    let read_ok = child
        .stdout
        .take()
        .expect("stdout is piped")
        .read_to_string(&mut formatted)
        .is_ok();
    let _ = writer.join();

    match child.wait() {
        Ok(status) if status.success() && read_ok => formatted,
        _ => source,
    }
}
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

    // Resolve category-linked options into concrete lists, and pre-fill any
    // placeholder whose category the user already chose from in the picker.
    let resolved = resolve_option_sources(opts, config)?;

    resolve_placeholders(
        &config.placeholders,
        &resolved,
        &opts.defines,
        &mut variables,
        opts.interactive_override,
    )?;
    Ok(variables)
}

/// The concrete options and any picker-derived pre-fill for one placeholder.
#[derive(Debug)]
struct ResolvedOptions {
    /// The option list a `select` placeholder chooses from.
    options: Vec<String>,
    /// A value chosen for the user in the picker (skips the prompt).
    prefill: Option<String>,
}

/// Resolve every placeholder's `options` into a concrete list.
///
/// Literal options pass through unchanged. Category options derive their list
/// from the battery pack's category members (parsed from `crate_root`); if the
/// user already selected a member in the picker, it becomes the pre-fill value.
fn resolve_option_sources(
    opts: &RenderOpts,
    config: &BpTemplateConfig,
) -> Result<BTreeMap<String, ResolvedOptions>> {
    // Only parse the pack spec when a category-linked placeholder needs it.
    let needs_spec = config
        .placeholders
        .values()
        .any(|def| matches!(def.options, Some(OptionsSource::Category { .. })));
    let spec = if needs_spec {
        Some(
            parse_battery_pack_from_path(&opts.crate_root.join("Cargo.toml"))
                .context("failed to parse battery pack for category-linked placeholder")?,
        )
    } else {
        None
    };

    let mut resolved = BTreeMap::new();
    for (name, def) in &config.placeholders {
        match &def.options {
            Some(OptionsSource::Literal(list)) => {
                resolved.insert(
                    name.clone(),
                    ResolvedOptions {
                        options: list.clone(),
                        prefill: None,
                    },
                );
            }
            Some(OptionsSource::Category { category }) => {
                let spec = spec
                    .as_ref()
                    .expect("spec parsed when a category placeholder exists");
                if !spec.categories.contains_key(category) {
                    bail!("placeholder '{name}' references undefined category '{category}'");
                }
                let members = spec.items_in_category(category);
                // Pre-fill from the picker: the first category member the user activated.
                let prefill = members
                    .iter()
                    .find(|m| opts.active_features.contains(*m))
                    .cloned();
                resolved.insert(
                    name.clone(),
                    ResolvedOptions {
                        options: members,
                        prefill,
                    },
                );
            }
            None => {}
        }
    }
    Ok(resolved)
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
    resolved_options: &BTreeMap<String, ResolvedOptions>,
    defines: &BTreeMap<String, String>,
    variables: &mut BTreeMap<String, String>,
    interactive_override: Option<bool>,
) -> Result<()> {
    let interactive = if let Some(interactive) = interactive_override {
        interactive
    } else {
        std::io::stdout().is_terminal()
    };

    // Empty option list for placeholders with no `options` (non-select types).
    let no_options: Vec<String> = Vec::new();

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

        // A category-linked placeholder the user already chose in the picker is
        // filled without prompting.
        if let Some(prefill) = resolved_options.get(name).and_then(|r| r.prefill.as_ref()) {
            variables.insert(name.clone(), prefill.clone());
            continue;
        }

        // The concrete option list for a `select` placeholder.
        let options = resolved_options
            .get(name)
            .map(|r| &r.options)
            .unwrap_or(&no_options);

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
                if options.is_empty() {
                    bail!("select placeholder '{name}' has no options");
                }
                if interactive {
                    let prompt = def.prompt.as_deref().unwrap_or(name);
                    let default_idx = def
                        .default
                        .as_ref()
                        .and_then(|d| options.iter().position(|o| o == d))
                        .unwrap_or(0);
                    let idx = dialoguer::Select::new()
                        .with_prompt(prompt)
                        .items(options)
                        .default(default_idx)
                        .interact()
                        .with_context(|| format!("failed to read placeholder '{name}'"))?;
                    options[idx].clone()
                } else {
                    let val = def.default.clone().ok_or_else(|| {
                        anyhow::anyhow!("placeholder '{name}' has no default and no value provided")
                    })?;
                    if !options.contains(&val) {
                        bail!(
                            "placeholder '{name}' default '{}' is not in options: {:?}",
                            val,
                            options
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
    let spec =
        parse_battery_pack_from_path(&manifest_path).context("Failed to parse battery pack")?;

    let temp_spec = spec.templates.get(opts.template).ok_or_else(|| {
        let available: Vec<_> = spec.templates.keys().map(|s| s.as_str()).collect();
        anyhow::anyhow!(
            "Template '{}' not found. Available: {}",
            opts.template,
            available.join(", ")
        )
    })?;

    let opts = RenderOpts {
        crate_root: resolved.dir,
        template_path: temp_spec.path.clone(),
        project_name: "my-project".to_string(),
        defines: opts.defines.clone(),
        active_features: std::collections::BTreeSet::new(),
        interactive_override: Some(false),
    };
    let files = preview(opts)?;
    Ok((crate_name, files))
}

#[cfg(test)]
mod tests {
    use indoc::indoc;
    use snapbox::{ToDebug, assert_data_eq, str};

    use super::*;

    /// Test wrapper: derive `resolved_options` from each placeholder's literal
    /// options, then run the real resolver. Category-linked options are resolved
    /// in `prepare_render`, so these unit tests only exercise literal lists.
    fn test_resolve(
        defs: &BTreeMap<String, PlaceholderDef>,
        defines: &BTreeMap<String, String>,
        variables: &mut BTreeMap<String, String>,
        interactive_override: Option<bool>,
    ) -> Result<()> {
        let resolved: BTreeMap<String, ResolvedOptions> = defs
            .iter()
            .filter_map(|(name, def)| match &def.options {
                Some(OptionsSource::Literal(list)) => Some((
                    name.clone(),
                    ResolvedOptions {
                        options: list.clone(),
                        prefill: None,
                    },
                )),
                _ => None,
            })
            .collect();
        resolve_placeholders(defs, &resolved, defines, variables, interactive_override)
    }

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
        assert_data_eq!(
            p.to_debug(),
            str![[r#"
PlaceholderDef {
    prompt: None,
    default: None,
    placeholder_type: String,
    options: None,
}

"#]]
        );
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
                options: None,
            },
        );
        let mut defines = BTreeMap::new();
        defines.insert("description".to_string(), "override".to_string());
        let mut vars = BTreeMap::new();

        test_resolve(&defs, &defines, &mut vars, Some(false)).unwrap();
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
                options: None,
            },
        );
        let defines = BTreeMap::new();
        let mut vars = BTreeMap::new();

        // In test/CI, stdout is not a terminal, so non-interactive path is taken
        test_resolve(&defs, &defines, &mut vars, Some(false)).unwrap();
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
                options: None,
            },
        );
        let defines = BTreeMap::new();
        let mut vars = BTreeMap::new();

        let err = test_resolve(&defs, &defines, &mut vars, Some(false)).unwrap_err();
        assert_data_eq!(
            err.to_string(),
            str!["placeholder 'description' has no default and no value provided"]
        );
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
                options: None,
            },
        );
        let err =
            test_resolve(&defs, &BTreeMap::new(), &mut BTreeMap::new(), Some(false)).unwrap_err();
        assert_data_eq!(
            err.to_string(),
            str![
                "placeholder 'my-thing' contains '-'; use snake_case (MiniJinja treats '-' as minus)"
            ]
        );
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
    fn parse_config_bool_placeholder() {
        let toml = r#"
            [placeholders.flag]
            type = "bool"
            prompt = "Enable it?"
            default = "true"
        "#;
        let config: BpTemplateConfig = toml::from_str(toml).unwrap();
        let p = &config.placeholders["flag"];
        assert_eq!(p.placeholder_type, PlaceholderType::Bool);
        assert_eq!(p.default.as_deref(), Some("true"));
    }

    #[test]
    fn parse_config_select_placeholder() {
        let toml = r#"
            [placeholders.platform]
            type = "select"
            prompt = "CI platform"
            options = ["github", "gitlab"]
            default = "github"
        "#;
        let config: BpTemplateConfig = toml::from_str(toml).unwrap();
        let p = &config.placeholders["platform"];
        assert_eq!(p.placeholder_type, PlaceholderType::Select);
        assert!(
            matches!(&p.options, Some(OptionsSource::Literal(v)) if v == &["github", "gitlab"])
        );
        assert_eq!(p.default.as_deref(), Some("github"));
    }

    #[test]
    fn parse_config_unknown_type_errors() {
        let toml = r#"
            [placeholders.flag]
            type = "number"
        "#;
        let err = toml::from_str::<BpTemplateConfig>(toml).unwrap_err();
        assert_data_eq!(
            err.to_string(),
            str![[r#"
TOML parse error at line 3, column 20
  |
3 |             type = "number"
  |                    ^^^^^^^^
unknown variant `number`, expected one of `string`, `bool`, `select`

"#]]
        );
    }

    // -- Bool placeholder resolution --

    #[test]
    fn resolve_bool_true_non_interactive() {
        let mut defs = BTreeMap::new();
        defs.insert(
            "flag".to_string(),
            PlaceholderDef {
                prompt: None,
                default: Some("true".to_string()),
                placeholder_type: PlaceholderType::Bool,
                options: None,
            },
        );
        let mut vars = BTreeMap::new();
        test_resolve(&defs, &BTreeMap::new(), &mut vars, Some(false)).unwrap();
        assert_eq!(vars["flag"], "true");
    }

    #[test]
    fn resolve_bool_false_non_interactive() {
        let mut defs = BTreeMap::new();
        defs.insert(
            "flag".to_string(),
            PlaceholderDef {
                prompt: None,
                default: Some("false".to_string()),
                placeholder_type: PlaceholderType::Bool,
                options: None,
            },
        );
        let mut vars = BTreeMap::new();
        test_resolve(&defs, &BTreeMap::new(), &mut vars, Some(false)).unwrap();
        assert_eq!(vars["flag"], "false");
    }

    #[test]
    fn resolve_bool_no_default_is_false() {
        let mut defs = BTreeMap::new();
        defs.insert(
            "flag".to_string(),
            PlaceholderDef {
                prompt: None,
                default: None,
                placeholder_type: PlaceholderType::Bool,
                options: None,
            },
        );
        let mut vars = BTreeMap::new();
        test_resolve(&defs, &BTreeMap::new(), &mut vars, Some(false)).unwrap();
        assert_eq!(vars["flag"], "false");
    }

    #[test]
    fn resolve_bool_define_override() {
        let mut defs = BTreeMap::new();
        defs.insert(
            "flag".to_string(),
            PlaceholderDef {
                prompt: None,
                default: Some("false".to_string()),
                placeholder_type: PlaceholderType::Bool,
                options: None,
            },
        );
        let mut defines = BTreeMap::new();
        defines.insert("flag".to_string(), "true".to_string());
        let mut vars = BTreeMap::new();
        test_resolve(&defs, &defines, &mut vars, Some(false)).unwrap();
        assert_eq!(vars["flag"], "true");
    }

    // -- Select placeholder resolution --

    #[test]
    fn resolve_select_non_interactive() {
        let mut defs = BTreeMap::new();
        defs.insert(
            "platform".to_string(),
            PlaceholderDef {
                prompt: None,
                default: Some("github".to_string()),
                placeholder_type: PlaceholderType::Select,
                options: Some(OptionsSource::Literal(vec![
                    "github".to_string(),
                    "gitlab".to_string(),
                ])),
            },
        );
        let mut vars = BTreeMap::new();
        test_resolve(&defs, &BTreeMap::new(), &mut vars, Some(false)).unwrap();
        assert_eq!(vars["platform"], "github");
    }

    #[test]
    fn resolve_select_invalid_default_errors() {
        let mut defs = BTreeMap::new();
        defs.insert(
            "platform".to_string(),
            PlaceholderDef {
                prompt: None,
                default: Some("bitbucket".to_string()),
                placeholder_type: PlaceholderType::Select,
                options: Some(OptionsSource::Literal(vec![
                    "github".to_string(),
                    "gitlab".to_string(),
                ])),
            },
        );
        let err =
            test_resolve(&defs, &BTreeMap::new(), &mut BTreeMap::new(), Some(false)).unwrap_err();
        assert!(err.to_string().contains("bitbucket"), "{err}");
        assert!(err.to_string().contains("not in options"), "{err}");
    }

    #[test]
    fn resolve_select_empty_options_errors() {
        let mut defs = BTreeMap::new();
        defs.insert(
            "platform".to_string(),
            PlaceholderDef {
                prompt: None,
                default: Some("github".to_string()),
                placeholder_type: PlaceholderType::Select,
                options: None,
            },
        );
        let err =
            test_resolve(&defs, &BTreeMap::new(), &mut BTreeMap::new(), Some(false)).unwrap_err();
        assert!(err.to_string().contains("no options"), "{err}");
    }

    #[test]
    fn resolve_select_define_override() {
        let mut defs = BTreeMap::new();
        defs.insert(
            "platform".to_string(),
            PlaceholderDef {
                prompt: None,
                default: Some("github".to_string()),
                placeholder_type: PlaceholderType::Select,
                options: Some(OptionsSource::Literal(vec![
                    "github".to_string(),
                    "gitlab".to_string(),
                ])),
            },
        );
        let mut defines = BTreeMap::new();
        defines.insert("platform".to_string(), "gitlab".to_string());
        let mut vars = BTreeMap::new();
        test_resolve(&defs, &defines, &mut vars, Some(false)).unwrap();
        assert_eq!(vars["platform"], "gitlab");
    }

    // -- Bool values in MiniJinja --

    #[test]
    fn jinja_bool_true_is_truthy() {
        let mut vars = BTreeMap::new();
        vars.insert("flag".to_string(), "true".to_string());
        let env = build_jinja_env(Path::new("."), &vars).unwrap();
        let result = env
            .render_str(
                "{% if flag %}yes{% else %}no{% endif %}",
                minijinja::context! {},
            )
            .unwrap();
        assert_eq!(result, "yes");
    }

    #[test]
    fn jinja_bool_false_is_falsy() {
        let mut vars = BTreeMap::new();
        vars.insert("flag".to_string(), "false".to_string());
        let env = build_jinja_env(Path::new("."), &vars).unwrap();
        let result = env
            .render_str(
                "{% if flag %}yes{% else %}no{% endif %}",
                minijinja::context! {},
            )
            .unwrap();
        assert_eq!(result, "no");
    }

    // -- pin_github_action --

    #[test]
    fn rust_stable_version_returns_semver() {
        let version = rust_stable_version();
        let parts: Vec<&str> = version.split('.').collect();
        assert!(parts.len() >= 2, "should be semver-like: {version}");
        assert!(
            parts[0].parse::<u32>().is_ok(),
            "major should be numeric: {version}"
        );
        assert!(
            parts[1].parse::<u32>().is_ok(),
            "minor should be numeric: {version}"
        );
    }

    #[test]
    fn rust_stable_version_available_in_jinja() {
        let vars = BTreeMap::new();
        let env = build_jinja_env(Path::new("."), &vars).unwrap();
        let result = env
            .render_str("{{ rust_stable_version() }}", minijinja::context! {})
            .unwrap();
        assert!(result.contains('.'), "should contain a dot: {result}");
        assert!(!result.is_empty(), "should not be empty");
    }

    // -- pin_github_action tests (require network, run with `cargo test -- --ignored`) --

    #[test]
    fn pin_github_action_resolves_known_repo() {
        let vars = BTreeMap::new();
        let env = build_jinja_env(Path::new("."), &vars).unwrap();
        let result = env
            .render_str(
                r#"{{ pin_github_action("actions/checkout", "v4") }}"#,
                minijinja::context! {},
            )
            .unwrap();
        assert!(
            result.starts_with("actions/checkout@"),
            "should start with owner/repo@: {result}"
        );
        assert!(
            !result.contains("could-not-resolve"),
            "should resolve successfully: {result}"
        );
        let comment = result.split("# ").nth(1).unwrap();
        assert!(
            comment.contains('.'),
            "should resolve to a specific version: {comment}"
        );
        let sha = result.split('@').nth(1).unwrap().split(' ').next().unwrap();
        assert_eq!(sha.len(), 40, "SHA should be 40 chars: {sha}");
    }

    #[test]
    fn pin_github_action_bad_repo_returns_error_marker() {
        let vars = BTreeMap::new();
        let env = build_jinja_env(Path::new("."), &vars).unwrap();
        let result = env
            .render_str(
                r#"{{ pin_github_action("nonexistent-owner-xyz/nonexistent-repo-xyz", "v999") }}"#,
                minijinja::context! {},
            )
            .unwrap();
        assert!(
            result.contains("could-not-resolve"),
            "should contain error marker: {result}"
        );
    }

    #[test]
    fn pin_github_action_subpath() {
        let vars = BTreeMap::new();
        let env = build_jinja_env(Path::new("."), &vars).unwrap();
        let result = env
            .render_str(
                r#"{{ pin_github_action("github/codeql-action", "v3", "upload-sarif") }}"#,
                minijinja::context! {},
            )
            .unwrap();
        assert!(
            result.contains("github/codeql-action/upload-sarif@"),
            "should contain subpath: {result}"
        );
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
            active_features: std::collections::BTreeSet::new(),
            interactive_override: None,
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
    fn preview_maps_underscore_cargo_toml_to_cargo_toml() {
        let tmp = tempfile::tempdir().unwrap();
        let crate_root = tmp.path();

        std::fs::write(
            crate_root.join("Cargo.toml"),
            "[package]\nname = \"test-bp\"\nversion = \"0.1.0\"\nkeywords = [\"battery-pack\"]\n",
        )
        .unwrap();
        std::fs::create_dir_all(crate_root.join("src")).unwrap();
        std::fs::write(crate_root.join("src/lib.rs"), "").unwrap();

        let tpl = crate_root.join("templates/default");
        std::fs::create_dir_all(tpl.join("src")).unwrap();
        std::fs::write(
            tpl.join("_Cargo.toml"),
            "[package]\nname = \"{{ project_name }}\"\n",
        )
        .unwrap();
        std::fs::write(tpl.join("src/main.rs"), "fn main() {}\n").unwrap();

        let opts = RenderOpts {
            crate_root: crate_root.to_path_buf(),
            template_path: "templates/default".to_string(),
            project_name: "my-app".to_string(),
            defines: BTreeMap::new(),
            active_features: std::collections::BTreeSet::new(),
            interactive_override: Some(false),
        };

        let files = preview(opts).unwrap();
        let mut paths: Vec<&str> = files.iter().map(|f| f.path.as_str()).collect();
        paths.sort();
        assert_data_eq!(
            paths.join("\n"),
            str![[r#"
Cargo.toml
src/main.rs"#]]
        );
        assert!(
            !files.iter().any(|f| f.path.contains("_Cargo")),
            "_Cargo.toml should not appear in output"
        );

        let cargo = files.iter().find(|f| f.path == "Cargo.toml").unwrap();
        assert_data_eq!(
            &cargo.content,
            str![[r#"
[package]
name = "my-app"
"#]]
        );
    }

    #[test]
    fn preview_preserves_underscore_cargo_toml_under_templates_dir() {
        let tmp = tempfile::tempdir().unwrap();
        let crate_root = tmp.path();

        std::fs::write(
            crate_root.join("Cargo.toml"),
            "[package]\nname = \"test-bp\"\nversion = \"0.1.0\"\nkeywords = [\"battery-pack\"]\n",
        )
        .unwrap();
        std::fs::create_dir_all(crate_root.join("src")).unwrap();
        std::fs::write(crate_root.join("src/lib.rs"), "").unwrap();

        let tpl = crate_root.join("tpl");
        std::fs::create_dir_all(tpl.join("templates/default/src")).unwrap();
        std::fs::write(tpl.join("templates/default/_Cargo.toml"), "[package]\n").unwrap();
        std::fs::write(tpl.join("templates/default/src/main.rs"), "fn main() {}\n").unwrap();

        let opts = RenderOpts {
            crate_root: crate_root.to_path_buf(),
            template_path: "tpl".to_string(),
            project_name: "my-app".to_string(),
            defines: BTreeMap::new(),
            active_features: std::collections::BTreeSet::new(),
            interactive_override: Some(false),
        };

        let files = preview(opts).unwrap();
        let mut paths: Vec<&str> = files.iter().map(|f| f.path.as_str()).collect();
        paths.sort();
        assert_data_eq!(
            paths.join("\n"),
            str![[r#"
templates/default/_Cargo.toml
templates/default/src/main.rs"#]]
        );
    }

    #[test]
    fn preview_resolves_bp_managed_deps() {
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
            active_features: std::collections::BTreeSet::new(),
            interactive_override: None,
        };

        let files = preview(opts).unwrap();
        let cargo = files.iter().find(|f| f.path == "Cargo.toml").unwrap();

        assert!(
            cargo.content.contains(r#"name = "my-project""#),
            "Expected package name"
        );
        assert!(cargo.content.contains("anyhow"), "Expected anyhow");
        assert!(
            cargo.content.contains("managed-battery-pack"),
            "Expected managed dependency"
        );
    }

    #[test]
    fn preview_warns_on_unresolvable_bp_managed_dep() {
        let tmp = tempfile::tempdir().unwrap();
        let crate_root = tmp.path();

        std::fs::write(
            crate_root.join("Cargo.toml"),
            indoc! {r#"
                [package]
                name = "fake-battery-pack"
                version = "0.1.0"
                edition = "2021"
                description = "test"
                keywords = ["battery-pack"]

                [package.metadata.battery.templates]
                default = { path = "templates/default", description = "test" }
            "#},
        )
        .unwrap();

        std::fs::create_dir_all(crate_root.join("src")).unwrap();
        std::fs::write(crate_root.join("src/lib.rs"), "").unwrap();

        let tpl_dir = crate_root.join("templates/default");
        std::fs::create_dir_all(&tpl_dir).unwrap();
        std::fs::write(
            tpl_dir.join("Cargo.toml"),
            indoc! {r#"
                [package]
                name = "{{ project_name }}"
                version = "0.1.0"
                edition = "2021"

                [dependencies]
                nonexistent-crate.bp-managed = true

                [package.metadata.battery-pack]
                fake-battery-pack = { features = ["default"] }
            "#},
        )
        .unwrap();

        let opts = RenderOpts {
            crate_root: crate_root.to_path_buf(),
            template_path: "templates/default".to_string(),
            project_name: "test-project".to_string(),
            defines: BTreeMap::new(),
            active_features: std::collections::BTreeSet::new(),
            interactive_override: Some(false),
        };

        // Should succeed (warn, not error) since the battery pack may not exist yet
        let files = preview(opts).unwrap();
        let cargo = files.iter().find(|f| f.path == "Cargo.toml").unwrap();
        assert_data_eq!(
            &cargo.content,
            str![[r#"
[package]
name = "test-project"
version = "0.1.0"
edition = "2021"

[dependencies]
nonexistent-crate.bp-managed = true

[package.metadata.battery-pack]
fake-battery-pack = { features = ["default"] }
"#]]
        );
    }

    #[test]
    fn preview_template_resolves_and_renders() {
        let fixtures = Path::new(env!("CARGO_MANIFEST_DIR"))
            .parent()
            .unwrap()
            .parent()
            .unwrap()
            .parent()
            .unwrap()
            .join("tests/fixtures/fancy-battery-pack");

        let source = crate::registry::CrateSource::Local(fixtures.parent().unwrap().to_path_buf());
        let opts = PreviewOpts {
            battery_pack: "fancy",
            template: "default",
            path: None,
            source: &source,
            defines: BTreeMap::new(),
        };

        let (crate_name, files) = preview_template(&opts).unwrap();
        assert_eq!(crate_name, "fancy-battery-pack");
        assert!(!files.is_empty());
        assert!(files.iter().any(|f| f.path == "Cargo.toml"));
        assert!(files.iter().any(|f| f.path == "src/main.rs"));
    }

    // -- path normalization --

    #[test]
    fn preview_normalizes_backslashes_and_applies_cargo_toml_rename_correctly() {
        let tmp = tempfile::tempdir().unwrap();
        let crate_root = tmp.path();

        std::fs::write(
            crate_root.join("Cargo.toml"),
            "[package]\nname = \"test-bp\"\nversion = \"0.1.0\"\nkeywords = [\"battery-pack\"]\n",
        )
        .unwrap();
        std::fs::create_dir_all(crate_root.join("src")).unwrap();
        std::fs::write(crate_root.join("src/lib.rs"), "").unwrap();

        // Create a template directory with a subdirectory whose name contains a
        // backslash. On Linux this is a valid filename character, simulating what
        // Windows path separators would look like if they leaked into the path.
        let tpl = crate_root.join("tpl");
        let backslash_dir = tpl.join("sub\\dir");
        std::fs::create_dir_all(&backslash_dir).unwrap();
        std::fs::write(backslash_dir.join("hello.txt"), "hi\n").unwrap();

        // _Cargo.toml at root: should be renamed to Cargo.toml
        std::fs::write(tpl.join("_Cargo.toml"), "[package]\n").unwrap();

        // _Cargo.toml under templates/: should be preserved as _Cargo.toml
        std::fs::create_dir_all(tpl.join("templates/inner")).unwrap();
        std::fs::write(tpl.join("templates/inner/_Cargo.toml"), "[package]\n").unwrap();

        let opts = RenderOpts {
            crate_root: crate_root.to_path_buf(),
            template_path: "tpl".to_string(),
            project_name: "my-app".to_string(),
            defines: BTreeMap::new(),
            active_features: std::collections::BTreeSet::new(),
            interactive_override: Some(false),
        };

        let files = preview(opts).unwrap();
        let paths: Vec<&str> = files.iter().map(|f| f.path.as_str()).collect();

        // Backslash in directory name is normalized to forward slash
        assert!(
            paths.contains(&"sub/dir/hello.txt"),
            "backslash should be normalized to forward slash: {paths:?}"
        );

        // _Cargo.toml at root is renamed to Cargo.toml
        assert!(
            paths.contains(&"Cargo.toml"),
            "_Cargo.toml at root should become Cargo.toml: {paths:?}"
        );
        assert!(
            !paths.contains(&"_Cargo.toml"),
            "_Cargo.toml should not appear at root: {paths:?}"
        );

        // _Cargo.toml under templates/ is preserved
        assert!(
            paths.contains(&"templates/inner/_Cargo.toml"),
            "_Cargo.toml under templates/ should be preserved: {paths:?}"
        );
    }

    // -- Category-linked select placeholders (Phase 6) --

    /// The category-battery-pack fixture directory.
    fn category_fixture() -> PathBuf {
        Path::new(env!("CARGO_MANIFEST_DIR"))
            .parent()
            .unwrap()
            .parent()
            .unwrap()
            .parent()
            .unwrap()
            .join("tests/fixtures/category-battery-pack")
    }

    /// Build a config with one `select` placeholder linked to `category`.
    fn category_placeholder_config(category: &str) -> BpTemplateConfig {
        let toml = format!(
            r#"
        [placeholders.util]
        type = "select"
        prompt = "Utility"
        options.category = "{category}"
        "#
        );
        toml::from_str(&toml).unwrap()
    }

    // [verify parse.options-category-in-template]
    #[test]
    fn parse_options_category_source() {
        let config = category_placeholder_config("utils");
        let p = &config.placeholders["util"];
        assert!(matches!(
            &p.options,
            Some(OptionsSource::Category { category }) if category == "utils"
        ));
    }

    // [verify parse.options-literal-array]
    #[test]
    fn parse_options_literal_source() {
        let toml = r#"
        [placeholders.util]
        type = "select"
        options = ["a", "b", "c"]
        "#;
        let config: BpTemplateConfig = toml::from_str(toml).unwrap();
        assert!(matches!(
            &config.placeholders["util"].options,
            Some(OptionsSource::Literal(v)) if v == &["a", "b", "c"]
        ));
    }

    // [verify template.options-category-derives-list]
    // [verify template.options-category-dep-uses-dep-name]
    #[test]
    fn options_category_derives_members() {
        let config = category_placeholder_config("utils");
        let opts = RenderOpts {
            crate_root: category_fixture(),
            template_path: "templates/blinky".to_string(),
            project_name: "p".to_string(),
            defines: BTreeMap::new(),
            active_features: BTreeSet::new(),
            interactive_override: Some(false),
        };
        let resolved = resolve_option_sources(&opts, &config).unwrap();
        // `utils` contains the `logging` feature and the `heapless` dependency.
        assert_eq!(resolved["util"].options, vec!["heapless", "logging"]);
        assert!(resolved["util"].prefill.is_none());
    }

    // [verify template.options-category-prefill-from-picker]
    #[test]
    fn options_category_prefills_from_active_features() {
        let config = category_placeholder_config("utils");
        let opts = RenderOpts {
            crate_root: category_fixture(),
            template_path: "templates/blinky".to_string(),
            project_name: "p".to_string(),
            defines: BTreeMap::new(),
            active_features: BTreeSet::from(["logging".to_string()]),
            interactive_override: Some(false),
        };
        let resolved = resolve_option_sources(&opts, &config).unwrap();
        assert_eq!(resolved["util"].prefill.as_deref(), Some("logging"));

        // End to end: prepare_render should fill the variable without a default.
        let vars = prepare_render(&opts, &config).unwrap();
        assert_eq!(vars["util"], "logging");
    }

    // [verify template.options-category-prefill-from-picker]
    #[test]
    fn preview_applies_category_prefill_over_fallback() {
        // preview() synthesizes a `<name>` fallback for undefaulted placeholders;
        // that must not shadow the picker-derived prefill. Renders the real blinky
        // template (which references `{{ util }}`) with `logging` active.
        let opts = RenderOpts {
            crate_root: category_fixture(),
            template_path: "templates/blinky".to_string(),
            project_name: "p".to_string(),
            defines: BTreeMap::new(),
            active_features: BTreeSet::from(["logging".to_string()]),
            interactive_override: Some(false),
        };
        let files = preview(opts).unwrap();
        let main_rs = files
            .iter()
            .find(|f| f.path == "src/main.rs")
            .expect("blinky renders src/main.rs");
        assert!(
            main_rs.content.contains("utility: logging"),
            "prefill should win over the <util> fallback:\n{}",
            main_rs.content
        );
        assert!(
            !main_rs.content.contains("<util>"),
            "the synthesized fallback must not appear:\n{}",
            main_rs.content
        );
    }

    // [verify template.options-category-unknown-error]
    #[test]
    fn options_category_unknown_errors() {
        let config = category_placeholder_config("nonexistent");
        let opts = RenderOpts {
            crate_root: category_fixture(),
            template_path: "templates/blinky".to_string(),
            project_name: "p".to_string(),
            defines: BTreeMap::new(),
            active_features: BTreeSet::new(),
            interactive_override: Some(false),
        };
        let err = resolve_option_sources(&opts, &config).unwrap_err();
        assert!(
            err.to_string().contains("nonexistent"),
            "error should name the category: {err}"
        );
    }
}
