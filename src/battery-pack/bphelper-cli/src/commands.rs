//! Subcommand implementations, CLI arg types, and interactive picker.
//!
//! This module contains the `main()` entry point and all subcommand handlers.
//! Depends on `registry` and `manifest`.

use anyhow::{Context, Result, bail};
use bphelper_manifest::{ActiveFeatures, parse_battery_pack_from_path};
use clap::{CommandFactory, Parser, Subcommand};
use std::collections::{BTreeMap, BTreeSet};
use std::io::IsTerminal;
use std::path::{Path, PathBuf};

use crate::manifest::{
    add_dep_to_table, dep_kind_section, find_installed_bp_names, find_user_manifest,
    find_workspace_manifest, read_active_features_for_project, read_active_features_from_state,
    read_managed_deps_for_project, remove_battery_pack_state_entry, remove_deps_by_kind,
    should_upgrade_version, sync_dep_in_table, write_battery_pack_state, write_deps_by_kind,
    write_workspace_refs_by_kind,
};
use crate::registry::{
    CrateSource, InstalledPack, TemplateConfig, fetch_battery_pack_detail,
    fetch_battery_pack_detail_from_source, fetch_battery_pack_list, fetch_bp_spec,
    load_installed_bp_spec, resolve_crate_name, short_name,
};

// [impl cli.bare.help]
#[derive(Parser)]
#[command(name = "cargo-bp")]
#[command(bin_name = "cargo")]
#[command(version, about = "Create and manage battery packs", long_about = None)]
pub(crate) struct Cli {
    #[command(subcommand)]
    pub command: Commands,
}

#[derive(Subcommand)]
pub(crate) enum Commands {
    /// Battery pack commands
    Bp {
        // [impl cli.source.subcommands]
        /// Use a local workspace as the battery pack source (replaces crates.io)
        #[arg(long)]
        crate_source: Option<PathBuf>,

        /// Disable interactive prompts and TUI mode
        #[arg(long, short = 'N', global = true, env = "CARGO_BP_NON_INTERACTIVE")]
        non_interactive: bool,

        #[command(subcommand)]
        command: BpCommands,
    },
}

#[derive(Subcommand)]
pub(crate) enum BpCommands {
    /// Create a new project from a battery pack template
    New {
        /// Name of the battery pack (e.g., "cli" resolves to "cli-battery-pack")
        #[arg(add = clap_complete::ArgValueCompleter::new(crate::completions::registry_and_local_packs))]
        battery_pack: String,

        /// Name for the new project (prompted interactively if not provided)
        #[arg(long, short = 'n')]
        name: Option<String>,

        /// Which template to use (defaults to first available, or prompts if multiple)
        // [impl cli.new.template-flag]
        #[arg(long, short = 't', add = clap_complete::ArgValueCompleter::new(crate::completions::templates))]
        template: Option<String>,

        /// Use a local path instead of downloading from crates.io
        #[arg(long)]
        path: Option<String>,

        /// Set a template placeholder value (e.g., -d description="My project")
        #[arg(long = "define", short = 'd', value_parser = parse_define)]
        define: Vec<(String, String)>,
    },

    /// Add a battery pack and sync its dependencies.
    ///
    /// Without arguments, lists installed packs and suggests next steps.
    /// With a battery pack name, adds that specific pack (with an interactive picker
    /// for choosing crates if the pack has features or many dependencies).
    /// Re-running on an already-installed pack lets you edit the selection.
    #[command(visible_alias = "edit")]
    Add {
        /// Name of the battery pack (e.g., "cli" resolves to "cli-battery-pack").
        /// Omit to open the interactive manager.
        #[arg(add = clap_complete::ArgValueCompleter::new(crate::completions::registry_and_local_packs))]
        battery_pack: Option<String>,

        /// Specific crates to add from the battery pack (ignores defaults/features)
        #[arg(add = clap_complete::ArgValueCompleter::new(crate::completions::pack_crates))]
        crates: Vec<String>,

        // [impl cli.add.features]
        // [impl cli.add.features-multiple]
        /// Named features to enable (comma-separated or repeated)
        #[arg(long = "features", short = 'F', value_delimiter = ',', add = clap_complete::ArgValueCompleter::new(crate::completions::pack_features))]
        features: Vec<String>,

        // [impl cli.add.no-default-features]
        /// Skip the default crates; only add crates from named features
        #[arg(long)]
        no_default_features: bool,

        // [impl cli.add.all-features]
        /// Add every crate the battery pack offers
        #[arg(long)]
        all_features: bool,

        /// Use a local path instead of downloading from crates.io
        #[arg(long)]
        path: Option<String>,

        /// Apply a battery pack template to the current project (renders and merges files)
        #[arg(long, short = 't')]
        template: Option<String>,

        /// Set a template variable (e.g., -d ci_platform=github). Skips the prompt for that variable.
        #[arg(long = "define", short = 'd', value_parser = parse_define)]
        define: Vec<(String, String)>,

        /// Overwrite existing files without prompting (TOML and YAML files are always merged, never overwritten)
        #[arg(long)]
        overwrite: bool,
    },

    /// Update dependencies from installed battery packs
    Sync {
        // [impl cli.path.subcommands]
        /// Use a local path instead of downloading from crates.io
        #[arg(long)]
        path: Option<String>,
    },

    /// Remove a battery pack from the current project
    #[command(visible_alias = "remove")]
    Rm {
        /// Name of the battery pack to remove (e.g., "cli" resolves to "cli-battery-pack")
        #[arg(add = clap_complete::ArgValueCompleter::new(crate::completions::installed_packs))]
        battery_pack: String,

        /// Also remove dependencies that were added by the tool
        #[arg(long, conflicts_with = "keep_deps")]
        remove_deps: bool,

        /// Keep all dependencies (don't prompt)
        #[arg(long)]
        keep_deps: bool,
    },

    /// List available battery packs on crates.io
    #[command(visible_alias = "ls")]
    List {
        /// Filter by name (omit to list all battery packs)
        filter: Option<String>,

        /// Emit machine-readable JSON instead of the default text output
        #[arg(long)]
        json: bool,
    },

    /// Show detailed information about a battery pack
    #[command(visible_alias = "info")]
    Show {
        /// Name of the battery pack (e.g., "cli" resolves to "cli-battery-pack")
        #[arg(add = clap_complete::ArgValueCompleter::new(crate::completions::registry_and_local_packs))]
        battery_pack: String,

        /// Preview a specific template's rendered output
        // [impl cli.show.template-preview]
        #[arg(long, short = 't')]
        template: Option<String>,

        /// Use a local path instead of downloading from crates.io
        #[arg(long)]
        path: Option<String>,

        /// Set a template placeholder value (e.g., -d description="My project")
        #[arg(long = "define", short = 'd', value_parser = parse_define)]
        define: Vec<(String, String)>,

        /// Emit machine-readable JSON instead of the default text output
        #[arg(long, conflicts_with_all = ["template", "define"])]
        json: bool,
    },

    /// Show status of installed battery packs and version warnings
    #[command(visible_alias = "stat")]
    Status {
        // [impl cli.path.subcommands]
        /// Use a local path instead of downloading from crates.io
        #[arg(long)]
        path: Option<String>,

        /// Emit machine-readable JSON instead of the default text output
        // [impl cli.status.json]
        #[arg(long)]
        json: bool,
    },

    /// Check that installed battery packs match project dependencies
    Check {
        /// Use a local path instead of downloading from crates.io
        #[arg(long)]
        path: Option<String>,
    },

    /// Validate that the current battery pack is well-formed
    Validate {
        /// Path to the battery pack crate (defaults to current directory)
        #[arg(long)]
        path: Option<String>,
    },

    /// Print the one-line shell configuration to enable native shell completions
    Completions {
        /// Explicitly specify the shell (bash, zsh, fish)
        shell: Option<String>,
    },

    #[command(hide = true)]
    UpdateCache,
}

pub fn main() -> Result<()> {
    clap_complete::env::CompleteEnv::with_factory(Cli::command).complete();
    let cli = Cli::parse();
    let project_dir = std::env::current_dir().context("Failed to get current directory")?;
    let interactive = std::io::stdout().is_terminal();

    match cli.command {
        Commands::Bp {
            crate_source,
            non_interactive,
            command,
        } => {
            if let Err(err) = sync_state_with_current_manifest(&project_dir) {
                eprintln!("warning: failed to prune battery-pack state: {err}");
            }
            let source = match crate_source {
                Some(path) => CrateSource::Local(path),
                None => CrateSource::Registry,
            };
            let interactive = interactive && !non_interactive;
            match command {
                BpCommands::New {
                    battery_pack,
                    name,
                    template,
                    path,
                    define,
                } => new_from_battery_pack(NewFromBpOpts {
                    battery_pack: &battery_pack,
                    name,
                    template,
                    path_override: path,
                    source: &source,
                    define: &define,
                    interactive,
                }),
                BpCommands::Add {
                    battery_pack,
                    crates,
                    features,
                    no_default_features,
                    all_features,
                    path,
                    template,
                    define,
                    overwrite,
                } => match (battery_pack, template) {
                    // Template merge: cargo bp add <pack> -t <template>
                    (Some(name), Some(tmpl)) => add_template(AddTemplateOpts {
                        battery_pack: &name,
                        template: &tmpl,
                        path_override: path.as_deref(),
                        source: &source,
                        project_dir: &project_dir,
                        defines: define.into_iter().collect(),
                        overwrite,
                        interactive,
                    }),
                    // Normal add: cargo bp add <pack>
                    (Some(name), None) => add_battery_pack(
                        &name,
                        &features,
                        no_default_features,
                        all_features,
                        &crates,
                        path.as_deref(),
                        &source,
                        &project_dir,
                    ),
                    (None, _) => show_add_help(&project_dir),
                },
                BpCommands::Sync { path } => {
                    sync_battery_packs(&project_dir, path.as_deref(), &source)
                }
                BpCommands::Rm {
                    battery_pack,
                    remove_deps,
                    keep_deps,
                } => remove_battery_pack(
                    &battery_pack,
                    remove_deps,
                    keep_deps,
                    interactive,
                    &project_dir,
                ),
                BpCommands::List { filter, json } => {
                    // [impl cli.list.interactive]
                    // [impl cli.list.non-interactive]
                    // [impl cli.list.json]
                    if json || !interactive {
                        list_battery_packs(&source, filter.as_deref(), json)
                    } else {
                        crate::tui::run_list(source, filter)
                    }
                }
                BpCommands::Show {
                    battery_pack,
                    template,
                    path,
                    define,
                    json,
                } => {
                    // [impl cli.show.json]
                    // [impl cli.show.non-interactive]
                    if json || (!interactive && template.is_none()) {
                        show_battery_pack(
                            &battery_pack,
                            path.as_deref(),
                            &source,
                            &project_dir,
                            json,
                        )
                    } else {
                        let show_opts = crate::tui::ShowOpts {
                            battery_pack: &battery_pack,
                            template: template.as_deref(),
                            path: path.as_deref(),
                            source,
                            defines: define.into_iter().collect(),
                        };
                        if interactive {
                            // [impl cli.show.interactive]
                            // [impl cli.show.template-preview]
                            crate::tui::run_show(show_opts)
                        } else {
                            // [impl cli.show.template-preview]
                            print_template_preview(&crate::template_engine::PreviewOpts {
                                battery_pack: show_opts.battery_pack,
                                template: show_opts.template.unwrap(),
                                path: show_opts.path,
                                source: &show_opts.source,
                                defines: show_opts.defines,
                            })
                        }
                    }
                }
                BpCommands::Status { path, json } => {
                    status_battery_packs(&project_dir, path.as_deref(), &source, json)
                }
                BpCommands::Check { path } => {
                    check_battery_packs(&project_dir, path.as_deref(), &source)
                }
                BpCommands::Validate { path } => {
                    crate::validate::validate_battery_pack_cmd(path.as_deref())
                }
                BpCommands::Completions { shell } => {
                    let shell_name = shell.unwrap_or_else(|| {
                        std::env::var("SHELL")
                            .ok()
                            .and_then(|s| s.rsplit('/').next().map(|s| s.to_string()))
                            .unwrap_or_else(|| "bash".to_string())
                    });
                    println!("source <(COMPLETE={} cargo-bp)", shell_name);
                    Ok(())
                }
                BpCommands::UpdateCache => {
                    let _ = crate::registry::update_cache();
                    Ok(())
                }
            }
        }
    }
}

/// Preflight: keep `battery-pack.toml` managed-deps aligned with current Cargo.toml.
pub(crate) fn sync_state_with_current_manifest(project_dir: &Path) -> Result<usize> {
    let metadata = match cargo_metadata::MetadataCommand::new()
        .current_dir(project_dir)
        .no_deps()
        .exec()
    {
        Ok(m) => m,
        Err(_) => return Ok(0),
    };

    // In a multi-package workspace, cargo metadata returns all members
    // regardless of current_dir. Match by canonicalized path to find
    // the package whose Cargo.toml lives in project_dir.
    let project_dir = project_dir
        .canonicalize()
        .unwrap_or_else(|_| project_dir.to_path_buf());

    let package = metadata.packages.iter().find(|p| {
        p.manifest_path.parent().and_then(|d| d.canonicalize().ok()) == Some(project_dir.clone())
    });

    let Some(package) = package else {
        eprintln!(
            "warning: no package found matching {}",
            project_dir.display()
        );
        return Ok(0);
    };

    let user_manifest_path: PathBuf = package.manifest_path.clone().into();
    let user_manifest_content =
        std::fs::read_to_string(&user_manifest_path).context("Failed to read Cargo.toml")?;
    crate::manifest::prune_state_managed_deps_for_manifest(
        &user_manifest_path,
        &user_manifest_content,
    )
}

// ============================================================================
// Implementation
// ============================================================================

/// Input options for [`new_from_battery_pack`].
struct NewFromBpOpts<'a> {
    battery_pack: &'a str,
    name: Option<String>,
    template: Option<String>,
    path_override: Option<String>,
    source: &'a CrateSource,
    define: &'a [(String, String)],
    interactive: bool,
}

// [impl cli.new.template]
// [impl cli.new.name-flag]
// [impl cli.new.name-prompt]
// [impl cli.path.flag]
// [impl cli.source.replace]
fn new_from_battery_pack(opts: NewFromBpOpts<'_>) -> Result<()> {
    if !opts.interactive && opts.name.is_none() {
        bail!("--name is required in non-interactive mode");
    }

    let new_opts = NewOpts {
        battery_pack: opts.battery_pack.to_string(),
        name: opts.name,
        defines: opts.define.iter().cloned().collect(),
        interactive: opts.interactive,
    };

    // --path takes precedence over --crate-source
    if let Some(path) = opts.path_override {
        return generate_from_local(new_opts, &path, opts.template);
    }

    let crate_name = resolve_crate_name(opts.battery_pack);
    let resolved = crate::registry::resolve_crate_dir(opts.battery_pack, None, opts.source)?;

    // Read template metadata from the Cargo.toml
    let manifest_path = resolved.dir.join("Cargo.toml");
    let templates = parse_template_metadata(&manifest_path, &crate_name)?;

    // Resolve which template to use
    let template_path = resolve_template(&templates, opts.template.as_deref(), opts.interactive)?;

    // Generate the project from the crate directory
    generate_from_path(new_opts, &resolved.dir, &template_path)
}

/// Result of resolving which crates to add from a battery pack.
pub(crate) enum ResolvedAdd {
    /// Resolved to a concrete set of crates (no interactive picker needed).
    Crates {
        active_features: BTreeSet<String>,
        crates: BTreeMap<String, bphelper_manifest::CrateSpec>,
    },
    /// The caller should show the interactive picker.
    Interactive,
}

/// Pure resolution logic for `cargo bp add` flags.
///
/// Given the battery pack spec and the CLI flags, determines which crates
/// to install. Returns `ResolvedAdd::Interactive` when the picker should
/// be shown (no explicit flags, TTY, meaningful choices).
///
/// When `specific_crates` is non-empty, unknown crate names are reported
/// to stderr and skipped; valid ones proceed.
// [impl cli.add.specific-crates]
// [impl cli.add.unknown-crate]
// [impl cli.add.default-crates]
// [impl cli.add.features]
// [impl cli.add.no-default-features]
// [impl cli.add.all-features]
pub(crate) fn resolve_add_crates(
    bp_spec: &bphelper_manifest::BatteryPackSpec,
    bp_name: &str,
    with_features: &[String],
    no_default_features: bool,
    all_features: bool,
    specific_crates: &[String],
) -> ResolvedAdd {
    if !specific_crates.is_empty() {
        // Explicit crate selection — ignores defaults and features.
        let mut selected = BTreeMap::new();
        for crate_name_arg in specific_crates {
            if let Some(spec) = bp_spec.crates.get(crate_name_arg.as_str()) {
                selected.insert(crate_name_arg.clone(), spec.clone());
            } else {
                eprintln!(
                    "error: crate '{}' not found in battery pack '{}'",
                    crate_name_arg, bp_name
                );
            }
        }
        return ResolvedAdd::Crates {
            active_features: BTreeSet::new(),
            crates: selected,
        };
    }

    if all_features {
        // [impl format.hidden.effect]
        let crates = bp_spec.resolve_for_features(&ActiveFeatures::All);

        return ResolvedAdd::Crates {
            active_features: BTreeSet::from(["all".to_string()]),
            crates,
        };
    }

    // When no explicit flags narrow the selection and the pack has
    // meaningful choices, signal that the caller may want to show
    // the interactive picker.
    if !no_default_features && with_features.is_empty() && bp_spec.has_meaningful_choices() {
        return ResolvedAdd::Interactive;
    }

    let mut features: BTreeSet<String> = if no_default_features {
        BTreeSet::new()
    } else {
        BTreeSet::from(["default".to_string()])
    };
    features.extend(with_features.iter().cloned());

    // When no features are active (--no-default-features with no -F),
    // return empty rather than calling resolve_crates(&[]) which
    // falls back to defaults.
    if features.is_empty() {
        return ResolvedAdd::Crates {
            active_features: features,
            crates: BTreeMap::new(),
        };
    }

    let str_features: Vec<&str> = features.iter().map(|s| s.as_str()).collect();
    let crates = bp_spec.resolve_crates(&str_features);
    ResolvedAdd::Crates {
        active_features: features,
        crates,
    }
}

// [impl cli.add.register]
// [impl cli.add.dep-kind]
// ============================================================================
// Template merge: cargo bp add <pack> -t <template>
// ============================================================================

/// Options for `cargo bp add <pack> -t <template>`.
struct AddTemplateOpts<'a> {
    battery_pack: &'a str,
    template: &'a str,
    path_override: Option<&'a str>,
    source: &'a CrateSource,
    project_dir: &'a Path,
    defines: BTreeMap<String, String>,
    overwrite: bool,
    interactive: bool,
}

/// Warn if the git working tree has uncommitted changes.
///
/// Silently passes if git is not installed or the directory is not a git repo.
/// In interactive mode, warns and asks for confirmation. In non-interactive
/// mode, refuses unless `overwrite` is true.
fn check_git_clean(project_dir: &Path, interactive: bool, overwrite: bool) -> Result<()> {
    let output = std::process::Command::new("git")
        .args(["status", "--porcelain"])
        .current_dir(project_dir)
        .output();

    let output = match output {
        Ok(o) if o.status.success() => o,
        // Not a git repo or git not installed: skip the check.
        _ => return Ok(()),
    };

    let status = String::from_utf8_lossy(&output.stdout);
    if status.trim().is_empty() {
        return Ok(());
    }

    if overwrite {
        // User explicitly accepted risk with --overwrite.
        return Ok(());
    }

    eprintln!(
        "warning: git working tree has uncommitted changes. \
         Template merge may be hard to undo."
    );

    if !interactive {
        bail!(
            "Working tree has uncommitted changes. \
             Commit or stash first, or pass --overwrite to proceed."
        );
    }

    let proceed = dialoguer::Confirm::new()
        .with_prompt("Proceed anyway?")
        .default(false)
        .interact()
        .context("prompt failed")?;

    if !proceed {
        bail!("Aborted.");
    }

    Ok(())
}

/// Merge a battery pack template into the current project.
///
/// Renders the template to memory, then applies each file using format-aware
/// merge strategies: TOML merge for Cargo.toml, YAML merge for workflow files,
/// and skip/overwrite for everything else.
fn add_template(opts: AddTemplateOpts<'_>) -> Result<()> {
    // Warn if the git working tree is dirty so the user can undo changes.
    check_git_clean(opts.project_dir, opts.interactive, opts.overwrite)?;

    let crate_name = resolve_crate_name(opts.battery_pack);

    // Resolve the battery pack directory.
    // Keep `_resolved` alive so its TempDir is not dropped before we finish
    // reading from the extracted crate directory.
    let _resolved;
    let crate_dir = if let Some(local_path) = opts.path_override {
        PathBuf::from(local_path)
    } else {
        _resolved = crate::registry::resolve_crate_dir(opts.battery_pack, None, opts.source)?;
        _resolved.dir.clone()
    };

    // Read template metadata and resolve which template to use.
    let manifest_path = crate_dir.join("Cargo.toml");
    let templates = parse_template_metadata(&manifest_path, &crate_name)?;
    let template_path = resolve_template(&templates, Some(opts.template), opts.interactive)?;

    // Load post-merge hints before moving template_path.
    let hints = crate::template_engine::load_template_hints(&crate_dir, &template_path);

    // Infer project_name from the current Cargo.toml or directory name.
    let project_name = infer_project_name(opts.project_dir)?;

    // Render the template to memory.
    let interactive_override = if opts.interactive { None } else { Some(false) };
    let render_opts = crate::template_engine::RenderOpts {
        crate_root: crate_dir,
        template_path,
        project_name,
        defines: opts.defines,
        interactive_override,
    };
    let files = crate::template_engine::preview(render_opts)?;

    // Apply rendered files with format-aware merging.
    let apply_opts = crate::merge::ApplyOpts {
        project_dir: opts.project_dir.to_path_buf(),
        overwrite: opts.overwrite,
        interactive: opts.interactive,
    };
    let results = crate::merge::apply_rendered_files(&files, &apply_opts)?;
    crate::merge::print_summary(&results);

    // Print post-merge hints if the template defines any.
    if !hints.is_empty() {
        eprintln!();
        eprintln!("Next steps:");
        for hint in &hints {
            eprintln!("  {hint}");
        }
    }

    Ok(())
}

/// Infer the project name from the current Cargo.toml or directory name.
fn infer_project_name(project_dir: &Path) -> Result<String> {
    let cargo_toml = project_dir.join("Cargo.toml");
    if let Ok(content) = std::fs::read_to_string(&cargo_toml)
        && let Ok(doc) = content.parse::<toml_edit::DocumentMut>()
        && let Some(name) = doc
            .get("package")
            .and_then(|p| p.get("name"))
            .and_then(|n| n.as_str())
    {
        return Ok(name.to_string());
    }

    // Fallback: directory name.
    Ok(project_dir
        .file_name()
        .and_then(|s| s.to_str())
        .unwrap_or("my-project")
        .to_string())
}

// ============================================================================
// Dependency add: cargo bp add <pack>
// ============================================================================

// [impl cli.add.specific-crates]
// [impl cli.add.unknown-crate]
// [impl manifest.register.location]
// [impl manifest.register.format]
// [impl manifest.features.storage]
// [impl manifest.deps.add]
// [impl manifest.deps.version-features]
#[allow(clippy::too_many_arguments)]
pub(crate) fn add_battery_pack(
    name: &str,
    with_features: &[String],
    no_default_features: bool,
    all_features: bool,
    specific_crates: &[String],
    path: Option<&str>,
    source: &CrateSource,
    project_dir: &Path,
) -> Result<()> {
    let crate_name = resolve_crate_name(name);

    // Step 1: Read the battery pack spec WITHOUT modifying any manifests.
    // --path takes precedence over --crate-source.
    // [impl cli.path.flag]
    // [impl cli.path.no-resolve]
    // [impl cli.source.replace]
    let (bp_version, bp_spec) = if let Some(local_path) = path {
        let manifest_path = Path::new(local_path).join("Cargo.toml");
        let spec = parse_battery_pack_from_path(&manifest_path)
            .with_context(|| format!("Failed to parse battery pack '{}'", crate_name))?;

        (None, spec)
    } else {
        fetch_bp_spec(source, name)?
    };

    // Step 2: Determine which crates to install — interactive picker, explicit flags, or defaults.
    // No manifest changes have been made yet, so cancellation is free.
    //
    // Merge previously stored features so that re-adding with `-F bar` is
    // additive rather than replacing the existing feature set.
    // Skip merging when the user explicitly narrows (--no-default-features,
    // --all-features, or specific crates) since those signal a fresh selection.
    let user_manifest_path = find_user_manifest(project_dir)?;
    // If the user isn't resetting with --no-default-features or --all-features or specific
    // crates, merge their -F flags with the previously stored feature set.
    let (merged_features, all_features) = if !no_default_features
        && !all_features
        && specific_crates.is_empty()
    {
        if let Some(existing) = read_active_features_from_state(&user_manifest_path, &crate_name) {
            match existing {
                // Previously had --all-features: keep that (adding more features is a no-op).
                bphelper_manifest::ActiveFeatures::All => (vec![], true),
                bphelper_manifest::ActiveFeatures::Subset(set) => {
                    let mut merged: Vec<String> = set.into_iter().collect();
                    for f in with_features {
                        if !merged.contains(f) {
                            merged.push(f.clone());
                        }
                    }
                    (merged, false)
                }
            }
        } else {
            (with_features.to_vec(), false)
        }
    } else {
        (with_features.to_vec(), all_features)
    };

    let resolved = resolve_add_crates(
        &bp_spec,
        &crate_name,
        &merged_features,
        no_default_features,
        all_features,
        specific_crates,
    );
    let (active_features, crates_to_sync) = match resolved {
        ResolvedAdd::Crates {
            active_features,
            crates,
        } => (active_features, crates),
        ResolvedAdd::Interactive if std::io::stdout().is_terminal() => {
            // Pre-select crates already in the project (edit mode)
            let pre_selected = compute_pre_selection(&bp_spec, project_dir);
            match pick_crates_interactive(&bp_spec, &pre_selected)? {
                Some(result) => (result.active_features, result.crates),
                None => {
                    println!("Cancelled.");
                    return Ok(());
                }
            }
        }
        ResolvedAdd::Interactive => {
            // Non-interactive fallback: use defaults
            let crates = bp_spec.resolve_crates(&["default"]);
            (BTreeSet::from(["default".to_string()]), crates)
        }
    };

    if crates_to_sync.is_empty() {
        println!("No crates selected.");
        return Ok(());
    }

    // Step 3: Now write everything — build-dep, workspace deps, crate deps, metadata.
    let user_manifest_content =
        std::fs::read_to_string(&user_manifest_path).context("Failed to read Cargo.toml")?;
    // [impl manifest.toml.preserve]
    let mut user_doc: toml_edit::DocumentMut = user_manifest_content
        .parse()
        .context("Failed to parse Cargo.toml")?;

    // [impl manifest.register.workspace-default]
    let workspace_manifest = find_workspace_manifest(&user_manifest_path)?;

    // [impl manifest.deps.workspace]
    // Add crate dependencies + workspace deps (including the battery pack itself).
    // Load workspace doc once; both deps and metadata are written to it before a
    // single flush at the end (avoids a double read-modify-write).
    let mut ws_doc: Option<toml_edit::DocumentMut> = if let Some(ref ws_path) = workspace_manifest {
        let ws_content =
            std::fs::read_to_string(ws_path).context("Failed to read workspace Cargo.toml")?;
        Some(
            ws_content
                .parse()
                .context("Failed to parse workspace Cargo.toml")?,
        )
    } else {
        None
    };

    if let Some(ref mut doc) = ws_doc {
        let ws_deps = doc["workspace"]["dependencies"]
            .or_insert(toml_edit::Item::Table(toml_edit::Table::new()));
        if let Some(ws_table) = ws_deps.as_table_mut() {
            // Add the battery pack itself to workspace deps
            if let Some(local_path) = path {
                let mut dep = toml_edit::InlineTable::new();
                dep.insert("path", toml_edit::Value::from(local_path));
                ws_table.insert(
                    &crate_name,
                    toml_edit::Item::Value(toml_edit::Value::InlineTable(dep)),
                );
            } else {
                let version = bp_version
                    .as_ref()
                    .context("battery pack version not available (--path without workspace)")?;
                ws_table.insert(&crate_name, toml_edit::value(version));
            }
            // Add the resolved crate dependencies
            for (dep_name, dep_spec) in &crates_to_sync {
                add_dep_to_table(ws_table, dep_name, dep_spec);
            }
        }

        // [impl cli.add.dep-kind]
        write_workspace_refs_by_kind(&mut user_doc, &crates_to_sync, false);
    } else {
        // [impl manifest.deps.no-workspace]
        // [impl cli.add.dep-kind]
        write_deps_by_kind(&mut user_doc, &crates_to_sync, false);
    }

    // Edit semantics: remove deselected crates from previous installation
    let prev_managed =
        read_managed_deps_for_project(&user_manifest_path, &user_manifest_content, &crate_name);
    let new_crate_names: BTreeSet<String> = crates_to_sync.keys().cloned().collect();
    let mut removed_count = 0;

    if let Some(prev) = &prev_managed {
        // Find crates that were previously managed but are no longer selected
        let to_remove: BTreeMap<String, bphelper_manifest::CrateSpec> = prev
            .iter()
            .filter(|name| !new_crate_names.contains(name.as_str()))
            .filter_map(|name| {
                bp_spec
                    .crates
                    .get(name)
                    .map(|spec| (name.clone(), spec.clone()))
            })
            .collect();

        if !to_remove.is_empty() {
            if let Some(ref mut doc) = ws_doc {
                // Remove from workspace deps
                let ws_deps = doc["workspace"]["dependencies"].as_table_mut();
                if let Some(ws_table) = ws_deps {
                    for name in to_remove.keys() {
                        ws_table.remove(name);
                    }
                }
            }
            removed_count = remove_deps_by_kind(&mut user_doc, &to_remove);
        }
    }

    // Write workspace Cargo.toml once (deps combined)
    if let (Some(ws_path), Some(doc)) = (&workspace_manifest, &ws_doc) {
        // [impl manifest.toml.preserve]
        std::fs::write(ws_path, doc.to_string()).context("Failed to write workspace Cargo.toml")?;
    }

    // Write the final Cargo.toml
    // [impl manifest.toml.preserve]
    std::fs::write(&user_manifest_path, user_doc.to_string())
        .context("Failed to write Cargo.toml")?;

    write_battery_pack_state(
        &user_manifest_path,
        &crate_name,
        &(&active_features).into(),
        &crates_to_sync,
    )?;

    println!(
        "Added {} with {} crate(s)",
        crate_name,
        crates_to_sync.len()
    );
    for dep_name in crates_to_sync.keys() {
        println!("  + {}", dep_name);
    }
    if removed_count > 0 {
        println!("Removed {} deselected crate(s)", removed_count);
    }

    Ok(())
}

/// Show a helpful message when `cargo bp add` is run without arguments.
/// Determine which managed deps are safe to remove (not shared with other packs).
pub(crate) fn deps_safe_to_remove(
    managed_deps: &BTreeSet<String>,
    all_bp_names: &[String],
    current_bp: &str,
    user_manifest_path: &Path,
    user_manifest_content: &str,
) -> BTreeSet<String> {
    let mut shared = BTreeSet::new();
    for other_bp in all_bp_names {
        if other_bp == current_bp {
            continue;
        }
        if let Some(other_managed) =
            read_managed_deps_for_project(user_manifest_path, user_manifest_content, other_bp)
        {
            shared.extend(other_managed.intersection(managed_deps).cloned());
        }
    }
    managed_deps.difference(&shared).cloned().collect()
}

fn remove_battery_pack(
    name: &str,
    remove_deps: bool,
    keep_deps: bool,
    interactive: bool,
    project_dir: &Path,
) -> Result<()> {
    let crate_name = resolve_crate_name(name);
    let user_manifest_path = find_user_manifest(project_dir)?;
    let user_manifest_content =
        std::fs::read_to_string(&user_manifest_path).context("Failed to read Cargo.toml")?;

    // Verify the pack is installed
    let bp_names = find_installed_bp_names(&user_manifest_content)?;
    if !bp_names.contains(&crate_name) {
        bail!("Battery pack '{}' is not installed", crate_name);
    }

    let managed_deps =
        read_managed_deps_for_project(&user_manifest_path, &user_manifest_content, &crate_name);

    // Determine which deps to remove
    let should_remove_deps = if let Some(ref managed) = managed_deps {
        if remove_deps {
            true
        } else if keep_deps {
            false
        } else if interactive {
            let safe = deps_safe_to_remove(
                managed,
                &bp_names,
                &crate_name,
                &user_manifest_path,
                &user_manifest_content,
            );
            if safe.is_empty() {
                false
            } else {
                println!("The following dependencies were added by {}:", crate_name);
                for dep in &safe {
                    println!("  {}", dep);
                }
                dialoguer::Confirm::new()
                    .with_prompt("Also remove these dependencies?")
                    .default(false)
                    .interact()
                    .unwrap_or(false)
            }
        } else {
            false // non-TTY default
        }
    } else {
        // Pre-migration: no managed-deps, don't touch deps
        false
    };

    let mut user_doc: toml_edit::DocumentMut = user_manifest_content
        .parse()
        .context("Failed to parse Cargo.toml")?;

    let workspace_manifest = find_workspace_manifest(&user_manifest_path)?;

    // Remove battery pack from [build-dependencies]
    if let Some(table) = user_doc
        .get_mut("build-dependencies")
        .and_then(|t| t.as_table_mut())
    {
        table.remove(&crate_name);
    }

    // Remove managed deps if confirmed
    if should_remove_deps && let Some(ref managed) = managed_deps {
        let safe = deps_safe_to_remove(
            managed,
            &bp_names,
            &crate_name,
            &user_manifest_path,
            &user_manifest_content,
        );

        // Remove from user doc (all dep sections)
        for section in ["dependencies", "dev-dependencies"] {
            if let Some(table) = user_doc.get_mut(section).and_then(|t| t.as_table_mut()) {
                for dep in &safe {
                    table.remove(dep.as_str());
                }
            }
        }

        // Remove from workspace deps
        if let Some(ref ws_path) = workspace_manifest {
            let ws_content =
                std::fs::read_to_string(ws_path).context("Failed to read workspace Cargo.toml")?;
            let mut ws_doc: toml_edit::DocumentMut = ws_content
                .parse()
                .context("Failed to parse workspace Cargo.toml")?;

            if let Some(ws_table) = ws_doc
                .get_mut("workspace")
                .and_then(|w| w.get_mut("dependencies"))
                .and_then(|d| d.as_table_mut())
            {
                for dep in &safe {
                    ws_table.remove(dep.as_str());
                }
                // Also remove the battery pack itself from workspace deps
                ws_table.remove(&crate_name);
            }

            std::fs::write(ws_path, ws_doc.to_string())
                .context("Failed to write workspace Cargo.toml")?;
        }

        if !safe.is_empty() {
            println!("Removed {} dependency(ies)", safe.len());
        }
    }

    std::fs::write(&user_manifest_path, user_doc.to_string())
        .context("Failed to write Cargo.toml")?;

    if let Err(e) = remove_battery_pack_state_entry(&user_manifest_path, &crate_name) {
        eprintln!("warning: failed to update battery-pack.toml: {e}");
    }

    // Clean up build.rs
    let build_rs_path = user_manifest_path
        .parent()
        .unwrap_or(Path::new("."))
        .join("build.rs");
    cleanup_build_rs(&build_rs_path, &crate_name)?;

    println!("Removed {}", crate_name);
    Ok(())
}

/// Remove a validate() call from build.rs. If the file becomes an empty main,
/// delete it entirely.
fn cleanup_build_rs(build_rs_path: &Path, crate_name: &str) -> Result<()> {
    if !build_rs_path.exists() {
        return Ok(());
    }

    let content = std::fs::read_to_string(build_rs_path).context("Failed to read build.rs")?;
    let crate_ident = crate_name.replace('-', "_");
    let validate_call = format!("{}::validate();", crate_ident);

    if !content.contains(&validate_call) {
        return Ok(()); // Nothing to remove
    }

    // Remove the line containing the validate call
    let new_lines: Vec<&str> = content
        .lines()
        .filter(|line| !line.trim().starts_with(&validate_call))
        .collect();
    let new_content = new_lines.join("\n") + "\n";

    // Check if the remaining content is just an empty main
    let trimmed = new_content.replace(char::is_whitespace, "");
    if trimmed == "fnmain(){}" {
        std::fs::remove_file(build_rs_path).context("Failed to delete build.rs")?;
    } else {
        std::fs::write(build_rs_path, new_content).context("Failed to write build.rs")?;
    }

    Ok(())
}

fn show_add_help(project_dir: &Path) -> Result<()> {
    let manifest_path = find_user_manifest(project_dir);
    let installed = manifest_path.ok().and_then(|p| {
        let content = std::fs::read_to_string(&p).ok()?;
        find_installed_bp_names(&content).ok()
    });

    match installed.as_deref() {
        Some(names) if !names.is_empty() => {
            println!("Installed battery packs:");
            for name in names {
                println!("  {}", short_name(name));
            }
            println!();
            println!("To add crates or features, run:");
            println!("  cargo bp add <name>");
        }
        _ => {
            println!("No battery packs installed.");
        }
    }

    println!();
    println!("To discover and install new packs, run:");
    println!("  cargo bp ls");

    Ok(())
}

// [impl cli.sync.update-versions]
// [impl cli.sync.add-features]
// [impl cli.sync.add-crates]
// [impl cli.source.subcommands]

fn sync_battery_packs(project_dir: &Path, path: Option<&str>, source: &CrateSource) -> Result<()> {
    let user_manifest_path = find_user_manifest(project_dir)?;
    let user_manifest_content =
        std::fs::read_to_string(&user_manifest_path).context("Failed to read Cargo.toml")?;

    let bp_names = find_installed_bp_names(&user_manifest_content)?;

    if bp_names.is_empty() {
        println!("No battery packs installed.");
        return Ok(());
    }

    // [impl manifest.toml.preserve]
    let mut user_doc: toml_edit::DocumentMut = user_manifest_content
        .parse()
        .context("Failed to parse Cargo.toml")?;

    let workspace_manifest = find_workspace_manifest(&user_manifest_path)?;
    let mut total_changes = 0;

    for bp_name in &bp_names {
        // Get the battery pack spec
        let bp_spec = load_installed_bp_spec(bp_name, path, source)?;

        let active_features =
            read_active_features_for_project(&user_manifest_path, &user_manifest_content, bp_name);

        // [impl format.hidden.effect]
        let expected = bp_spec.resolve_for_features(&active_features);

        // [impl manifest.deps.workspace]
        // Sync each crate
        if let Some(ref ws_path) = workspace_manifest {
            let ws_content =
                std::fs::read_to_string(ws_path).context("Failed to read workspace Cargo.toml")?;
            // [impl manifest.toml.preserve]
            let mut ws_doc: toml_edit::DocumentMut = ws_content
                .parse()
                .context("Failed to parse workspace Cargo.toml")?;

            let ws_deps = ws_doc["workspace"]["dependencies"]
                .or_insert(toml_edit::Item::Table(toml_edit::Table::new()));
            if let Some(ws_table) = ws_deps.as_table_mut() {
                for (dep_name, dep_spec) in &expected {
                    if sync_dep_in_table(ws_table, dep_name, dep_spec) {
                        total_changes += 1;
                        println!("  ~ {} (updated in workspace)", dep_name);
                    }
                }
            }

            // [impl manifest.toml.preserve]
            std::fs::write(ws_path, ws_doc.to_string())
                .context("Failed to write workspace Cargo.toml")?;

            // Ensure crate-level references exist in the correct sections
            // [impl cli.add.dep-kind]
            let refs_added = write_workspace_refs_by_kind(&mut user_doc, &expected, true);
            total_changes += refs_added;
        } else {
            // [impl manifest.deps.no-workspace]
            // [impl cli.add.dep-kind]
            for (dep_name, dep_spec) in &expected {
                let section = dep_kind_section(dep_spec.dep_kind);
                let table =
                    user_doc[section].or_insert(toml_edit::Item::Table(toml_edit::Table::new()));
                if let Some(table) = table.as_table_mut() {
                    if !table.contains_key(dep_name) {
                        add_dep_to_table(table, dep_name, dep_spec);
                        total_changes += 1;
                        println!("  + {}", dep_name);
                    } else if sync_dep_in_table(table, dep_name, dep_spec) {
                        total_changes += 1;
                        println!("  ~ {}", dep_name);
                    }
                }
            }
        }
        write_battery_pack_state(&user_manifest_path, bp_name, &active_features, &expected)?;
    }

    // [impl manifest.toml.preserve]
    std::fs::write(&user_manifest_path, user_doc.to_string())
        .context("Failed to write Cargo.toml")?;

    if total_changes == 0 {
        println!("All dependencies are up to date.");
    } else {
        println!("Synced {} change(s).", total_changes);
    }

    Ok(())
}

// ============================================================================
// Interactive crate picker
// ============================================================================

/// Compute which crates from a battery pack are already present in the project.
///
/// Returns an empty set when the pack is not yet installed (fresh install),
/// which causes the picker to fall back to the pack's default feature set.
fn compute_pre_selection(
    bp_spec: &bphelper_manifest::BatteryPackSpec,
    project_dir: &Path,
) -> BTreeSet<String> {
    let Ok(manifest_path) = find_user_manifest(project_dir) else {
        return BTreeSet::new();
    };
    let Ok(content) = std::fs::read_to_string(&manifest_path) else {
        return BTreeSet::new();
    };
    let Ok(versions) = collect_user_dep_versions(&manifest_path, &content) else {
        return BTreeSet::new();
    };

    // A crate is pre-selected if it appears in the project's dependencies
    bp_spec
        .crates
        .keys()
        .filter(|name| versions.contains_key(name.as_str()))
        .cloned()
        .collect()
}

/// Represents the result of an interactive crate selection.
pub(crate) struct PickerResult {
    /// The resolved crates to install (name -> dep spec with merged features).
    pub crates: BTreeMap<String, bphelper_manifest::CrateSpec>,
    /// Which feature names are fully selected (for metadata recording).
    pub active_features: BTreeSet<String>,
}

/// An item in the picker — either a feature or an individual crate.
enum PickerItem {
    Feature(String), // feature name
    Crate(String),   // crate name
}

/// Show an interactive multi-select picker for choosing which crates to install.
///
/// Features are listed first, then individual crates. `pre_selected` contains
/// crate names already present in the project (for edit mode); when empty,
/// the pack's default feature set is used for initial selection.
///
/// Returns `None` if the user cancels.
fn pick_crates_interactive(
    bp_spec: &bphelper_manifest::BatteryPackSpec,
    pre_selected: &BTreeSet<String>,
) -> Result<Option<PickerResult>> {
    use console::style;
    use dialoguer::MultiSelect;

    // Collect non-default features with their member crates
    let features: Vec<(&String, &BTreeSet<bphelper_manifest::FeatureRef>)> = bp_spec
        .features
        .iter()
        .filter(|(name, _)| name.as_str() != "default")
        .collect();

    // Collect all visible crates
    let visible_crates: Vec<(&String, &bphelper_manifest::CrateSpec)> = bp_spec
        .crates
        .iter()
        .filter(|(name, _)| !bp_spec.is_hidden(name))
        .collect();

    if visible_crates.is_empty() {
        bail!("Battery pack has no crates to add");
    }

    let use_defaults = pre_selected.is_empty();
    let default_crates: BTreeSet<String> = if use_defaults {
        bp_spec
            .resolve_crates(&["default"])
            .keys()
            .cloned()
            .collect()
    } else {
        BTreeSet::new()
    };

    // Build picker items: features first, then crates
    let mut items: Vec<PickerItem> = Vec::new();
    let mut labels: Vec<String> = Vec::new();
    let mut defaults: Vec<bool> = Vec::new();

    for (feat_name, feat_crates) in &features {
        let member_list = feat_crates
            .iter()
            .map(|fref| fref.dep_name())
            .filter(|c| !bp_spec.is_hidden(c))
            .collect::<Vec<_>>()
            .join(", ");
        labels.push(format!(
            "✦ {} {}",
            feat_name,
            style(format!("[{}]", member_list)).dim()
        ));
        let checked = if use_defaults {
            // Feature is checked if all its visible members are in defaults
            feat_crates
                .iter()
                .map(|fref| fref.dep_name())
                .filter(|c| !bp_spec.is_hidden(c))
                .all(|c| default_crates.contains(c))
        } else {
            // Feature is checked if all its visible members are pre-selected
            feat_crates
                .iter()
                .map(|fref| fref.dep_name())
                .filter(|c| !bp_spec.is_hidden(c))
                .all(|c| pre_selected.contains(c))
        };
        defaults.push(checked);
        items.push(PickerItem::Feature(feat_name.to_string()));
    }

    for (crate_name, spec) in &visible_crates {
        let version_info = if spec.features.is_empty() {
            format!("({})", spec.version)
        } else {
            format!(
                "({}, features: {})",
                spec.version,
                spec.features
                    .iter()
                    .map(|s| s.as_str())
                    .collect::<Vec<_>>()
                    .join(", ")
            )
        };
        labels.push(format!("  {} {}", crate_name, style(&version_info).dim()));
        let checked = if use_defaults {
            default_crates.contains(crate_name.as_str())
        } else {
            pre_selected.contains(crate_name.as_str())
        };
        defaults.push(checked);
        items.push(PickerItem::Crate(crate_name.to_string()));
    }

    // Show the picker
    println!();
    println!(
        "  {} v{}",
        style(&bp_spec.name).green().bold(),
        style(&bp_spec.version).dim()
    );
    println!();

    let selections = MultiSelect::new()
        .with_prompt("Select features and crates")
        .items(&labels)
        .defaults(&defaults)
        .interact_opt()
        .context("Failed to show crate picker")?;

    let Some(selected_indices) = selections else {
        return Ok(None);
    };

    // Split the picker selection into feature names and direct crate names.
    let selected_set: BTreeSet<usize> = selected_indices.into_iter().collect();
    let mut picked_features = Vec::new();
    let mut picked_crates = BTreeSet::new();

    for (index, item) in items.iter().enumerate() {
        if !selected_set.contains(&index) {
            continue;
        }

        match item {
            PickerItem::Feature(name) => picked_features.push(name.clone()),
            PickerItem::Crate(name) => {
                picked_crates.insert(name.clone());
            }
        }
    }

    // Route picked features through the main resolver so per-dep features (`serde/derive`)
    // weak refs (`serde?/derive`), and nested refs (`bundle = ["fancy"]`) reach the
    // result with correct CrateSpec.features.
    let feature_strs = picked_features
        .iter()
        .map(String::as_str)
        .collect::<Vec<&str>>();

    let mut crates = if feature_strs.is_empty() {
        BTreeMap::new()
    } else {
        bp_spec.resolve_crates(&feature_strs)
    };

    // Layer in directly-picked crates (their declared spec -- picked by name, not via
    // a feature gate, so no per-dep feature accumulation).
    for name in &picked_crates {
        if bp_spec.is_hidden(name) {
            continue;
        }

        if let Some(spec) = bp_spec.crates.get(name) {
            crates.entry(name.clone()).or_insert_with(|| spec.clone());
        }
    }

    // Mark a feature active when every visible member crate it would activate is in the resolved set.
    let resolved_names = crates
        .keys()
        .map(String::as_str)
        .collect::<BTreeSet<&str>>();
    let mut active_features = BTreeSet::new();
    let default_members = bp_spec.features.get("default");
    if default_members.is_some_and(|members| {
        members
            .iter()
            .map(|fref| fref.dep_name())
            .filter(|crate_name| !bp_spec.is_hidden(crate_name))
            .all(|crate_name| resolved_names.contains(crate_name))
    }) {
        active_features.insert("default".to_string());
    }

    for (feat_name, feat_crates) in &bp_spec.features {
        if feat_name == "default" {
            continue;
        }
        if feat_crates
            .iter()
            .map(|r| r.dep_name())
            .filter(|c| !bp_spec.is_hidden(c))
            .all(|c| resolved_names.contains(c))
        {
            active_features.insert(feat_name.clone());
        }
    }

    Ok(Some(PickerResult {
        crates,
        active_features,
    }))
}

// ============================================================================
// build.rs manipulation
// ============================================================================

/// Shared options for `cargo bp new` generation.
struct NewOpts {
    battery_pack: String,
    name: Option<String>,
    defines: BTreeMap<String, String>,
    interactive: bool,
}

fn generate_from_local(opts: NewOpts, local_path: &str, template: Option<String>) -> Result<()> {
    let local_path = Path::new(local_path);

    // Read local Cargo.toml
    let manifest_path = local_path.join("Cargo.toml");

    let crate_name = local_path
        .file_name()
        .and_then(|s| s.to_str())
        .unwrap_or("unknown");
    let templates = parse_template_metadata(&manifest_path, crate_name)?;
    let template_path = resolve_template(&templates, template.as_deref(), opts.interactive)?;

    generate_from_path(opts, local_path, &template_path)
}

/// Prompt for a project name if not provided.
fn prompt_project_name(name: Option<String>) -> Result<String> {
    match name {
        Some(n) => Ok(n),
        None => dialoguer::Input::<String>::new()
            .with_prompt("Project name")
            .interact_text()
            .context("Failed to read project name"),
    }
}

/// Ensure a project name ends with `-battery-pack`.
fn ensure_battery_pack_suffix(name: String) -> String {
    if name.ends_with("-battery-pack") {
        name
    } else {
        let fixed = format!("{}-battery-pack", name);
        println!("Renaming project to: {}", fixed);
        fixed
    }
}

fn generate_from_path(opts: NewOpts, crate_path: &Path, template_path: &str) -> Result<()> {
    let raw = prompt_project_name(opts.name)?;
    let project_name = if opts.battery_pack == "battery-pack" {
        ensure_battery_pack_suffix(raw)
    } else {
        raw
    };

    let interactive_override = if opts.interactive { None } else { Some(false) };

    let gen_opts = crate::template_engine::GenerateOpts {
        render: crate::template_engine::RenderOpts {
            crate_root: crate_path.to_path_buf(),
            template_path: template_path.to_string(),
            project_name,
            defines: opts.defines,
            interactive_override,
        },
        destination: None,
        git_init: true,
    };

    crate::template_engine::generate(gen_opts)?;

    Ok(())
}

/// Parse a `key=value` string for clap's `value_parser`.
fn parse_define(s: &str) -> Result<(String, String), String> {
    match s.split_once('=') {
        Some((key, value)) => Ok((key.to_string(), value.to_string())),
        None => Ok((s.to_string(), "true".to_string())),
    }
}

fn parse_template_metadata(
    manifest_path: &Path,
    crate_name: &str,
) -> Result<BTreeMap<String, TemplateConfig>> {
    let spec = parse_battery_pack_from_path(manifest_path).context("Failed to parse Cargo.toml")?;

    if spec.templates.is_empty() {
        bail!(
            "Battery pack '{}' has no templates defined in [package.metadata.battery.templates]",
            crate_name
        );
    }

    Ok(spec.templates)
}

// [impl format.templates.selection]
// [impl cli.new.template-select]
pub(crate) fn resolve_template(
    templates: &BTreeMap<String, TemplateConfig>,
    requested: Option<&str>,
    interactive: bool,
) -> Result<String> {
    match requested {
        Some(name) => {
            let config = templates.get(name).ok_or_else(|| {
                let available: Vec<_> = templates.keys().map(|s| s.as_str()).collect();
                anyhow::anyhow!(
                    "Template '{}' not found. Available templates: {}",
                    name,
                    available.join(", ")
                )
            })?;
            Ok(config.path.clone())
        }
        None => {
            if templates.len() == 1 {
                let (_, config) = templates.iter().next().unwrap();
                Ok(config.path.clone())
            } else if let Some(config) = templates.get("default") {
                Ok(config.path.clone())
            } else {
                prompt_for_template(templates, interactive)
            }
        }
    }
}

fn prompt_for_template(
    templates: &BTreeMap<String, TemplateConfig>,
    interactive: bool,
) -> Result<String> {
    use dialoguer::{Select, theme::ColorfulTheme};

    // Build display items with descriptions
    let items: Vec<String> = templates
        .iter()
        .map(|(name, config)| {
            if let Some(desc) = &config.description {
                format!("{} - {}", name, desc)
            } else {
                name.clone()
            }
        })
        .collect();

    // Check if we're in a TTY for interactive mode
    if !interactive || !std::io::stdout().is_terminal() {
        // Non-interactive: list templates and bail
        println!("Available templates:");
        for item in &items {
            println!("  {}", item);
        }
        bail!("Multiple templates available. Please specify one with --template <name>");
    }

    // Interactive: show selector
    let selection = Select::with_theme(&ColorfulTheme::default())
        .with_prompt("Select a template")
        .items(&items)
        .default(0)
        .interact()
        .context("Failed to select template")?;

    // Get the selected template's path
    let (_, config) = templates
        .iter()
        .nth(selection)
        .ok_or_else(|| anyhow::anyhow!("Invalid template selection"))?;
    Ok(config.path.clone())
}

// ============================================================================
// List command
// ============================================================================
//
// Like `status`, the list command is split into:
//   1. `build_list_report`  — pure data, returns a `ListReport`.
//   2. `render_list_json`   — serializes the report.
//   3. `render_list_text`   — pretty-prints the report.
// `list_battery_packs` is a thin dispatcher that picks one renderer.

// [impl cli.list.json]
// [impl cli.list.non-interactive]
fn list_battery_packs(source: &CrateSource, filter: Option<&str>, json: bool) -> Result<()> {
    let report = build_list_report(source, filter)?;
    let stdout = std::io::stdout();
    let mut out = stdout.lock();
    if json {
        render_list_json(&report, &mut out).context("Failed to write list JSON")?;
    } else {
        render_list_text(&report, &mut out).context("Failed to render list")?;
    }
    Ok(())
}

/// Build a [`cargo_bp_script::ListReport`] from the registry. Pure data.
fn build_list_report(
    source: &CrateSource,
    filter: Option<&str>,
) -> Result<cargo_bp_script::ListReport> {
    let battery_packs = fetch_battery_pack_list(source, filter)?;

    let mut report = cargo_bp_script::ListReport::new();
    if let Some(f) = filter {
        report = report.with_filter(f);
    }
    report = report.with_packs(battery_packs.iter().map(|bp| {
        cargo_bp_script::PackSummary::new(&bp.short_name, &bp.name, &bp.version)
            .with_description(&bp.description)
    }));
    Ok(report)
}

/// Serialize a [`ListReport`] as JSON to `w`, with a trailing newline.
fn render_list_json(
    report: &cargo_bp_script::ListReport,
    w: &mut impl std::io::Write,
) -> std::io::Result<()> {
    serde_json::to_writer(&mut *w, report)?;
    writeln!(w)?;
    Ok(())
}

/// Pretty-print a [`ListReport`] for human consumption to `w`.
fn render_list_text(
    report: &cargo_bp_script::ListReport,
    w: &mut impl std::io::Write,
) -> std::io::Result<()> {
    use console::style;

    if report.packs.is_empty() {
        match &report.filter {
            Some(q) => writeln!(w, "No battery packs found matching '{}'", q)?,
            None => writeln!(w, "No battery packs found")?,
        }
        return Ok(());
    }

    // Find the longest name for alignment
    let max_name_len = report
        .packs
        .iter()
        .map(|p| p.short_name.len())
        .max()
        .unwrap_or(0);

    let max_version_len = report
        .packs
        .iter()
        .map(|p| p.version.len())
        .max()
        .unwrap_or(0);

    writeln!(w)?;
    for pack in &report.packs {
        let desc = pack.description.lines().next().unwrap_or("");

        // Pad strings manually, then apply colors (ANSI codes break width formatting)
        let name_padded = format!("{:<width$}", pack.short_name, width = max_name_len);
        let ver_padded = format!("{:<width$}", pack.version, width = max_version_len);

        writeln!(
            w,
            "  {}  {}  {}",
            style(name_padded).green().bold(),
            style(ver_padded).dim(),
            desc,
        )?;
    }
    writeln!(w)?;

    writeln!(
        w,
        "{}",
        style(format!("Found {} battery pack(s)", report.packs.len())).dim()
    )?;

    Ok(())
}

// ============================================================================
// Show command
// ============================================================================
//
// Like `status`, the show command is split into:
//   1. `build_show_report`  — pure data, returns a `ShowReport`.
//   2. `render_show_json`   — serializes the report.
//   3. `render_show_text`   — pretty-prints the report.
// `show_battery_pack` is a thin dispatcher that picks one renderer.

// [impl cli.show.json]
// [impl cli.show.non-interactive]
fn show_battery_pack(
    name: &str,
    path: Option<&str>,
    source: &CrateSource,
    project_dir: &Path,
    json: bool,
) -> Result<()> {
    let report = build_show_report(name, path, source, project_dir)?;
    let stdout = std::io::stdout();
    let mut out = stdout.lock();
    if json {
        render_show_json(&report, &mut out).context("Failed to write show JSON")?;
    } else {
        render_show_text(&report, &mut out).context("Failed to render show")?;
    }
    Ok(())
}

/// Build a [`cargo_bp_script::ShowReport`] from the registry. Pure data.
fn build_show_report(
    name: &str,
    path: Option<&str>,
    source: &CrateSource,
    project_dir: &Path,
) -> Result<cargo_bp_script::ShowReport> {
    // --path takes precedence over --crate-source
    let detail = if path.is_some() {
        fetch_battery_pack_detail(name, path)?
    } else {
        fetch_battery_pack_detail_from_source(source, name)?
    };

    let mut report =
        cargo_bp_script::ShowReport::new(&detail.short_name, &detail.name, &detail.version)
            .with_description(&detail.description);

    if let Some(repo) = &detail.repository {
        report = report.with_repository(repo);
    }

    // Owners
    report = report.with_owners(detail.owners.iter().map(|o| {
        let mut info = cargo_bp_script::OwnerInfo::new(&o.login);
        if let Some(n) = &o.name {
            info = info.with_name(n);
        }
        info
    }));

    // Crates
    report = report.with_crates(detail.crates.iter().map(|s| s.as_str()));

    // Extends
    for ext in &detail.extends {
        report = report.with_extends(ext);
    }

    // Features
    report = report.with_features(detail.features.iter().map(|(feat_name, members)| {
        cargo_bp_script::FeatureInfo::new(feat_name).with_crates(members.iter().map(|s| s.as_str()))
    }));

    // Templates
    report = report.with_templates(detail.templates.iter().map(|t| {
        let mut info = cargo_bp_script::TemplateInfo::new(&t.name);
        if let Some(d) = &t.description {
            info = info.with_description(d);
        }
        info
    }));

    // Examples
    report = report.with_examples(detail.examples.iter().map(|e| {
        let mut info = cargo_bp_script::ExampleInfo::new(&e.name);
        if let Some(d) = &e.description {
            info = info.with_description(d);
        }
        info
    }));

    // Installed state from the current project (if available)
    let crate_name = resolve_crate_name(name);
    let (managed_deps, active_features) = read_installed_state(project_dir, &crate_name);
    if !managed_deps.is_empty() {
        report = report.with_installed_crates(managed_deps);
    }
    match &active_features {
        bphelper_manifest::ActiveFeatures::All => {
            report = report.with_active_features(["all".to_string()]);
        }
        bphelper_manifest::ActiveFeatures::Subset(features) if !features.is_empty() => {
            report = report.with_active_features(features.iter().cloned());
        }
        _ => {}
    }

    Ok(report)
}

/// Read installed state (managed-deps and active features) for a battery pack.
/// Returns empty managed-deps set and active features for the pack.
fn read_installed_state(
    project_dir: &Path,
    crate_name: &str,
) -> (BTreeSet<String>, bphelper_manifest::ActiveFeatures) {
    let empty = (
        BTreeSet::new(),
        bphelper_manifest::ActiveFeatures::Subset(BTreeSet::from(["default".to_string()])),
    );
    let Ok(manifest_path) = find_user_manifest(project_dir) else {
        return empty;
    };
    let Ok(content) = std::fs::read_to_string(&manifest_path) else {
        return empty;
    };
    let managed =
        read_managed_deps_for_project(&manifest_path, &content, crate_name).unwrap_or_default();
    let features = read_active_features_for_project(&manifest_path, &content, crate_name);
    (managed, features)
}

/// Serialize a [`ShowReport`] as JSON to `w`, with a trailing newline.
fn render_show_json(
    report: &cargo_bp_script::ShowReport,
    w: &mut impl std::io::Write,
) -> std::io::Result<()> {
    serde_json::to_writer(&mut *w, report)?;
    writeln!(w)?;
    Ok(())
}

/// Pretty-print a [`ShowReport`] for human consumption to `w`.
fn render_show_text(
    report: &cargo_bp_script::ShowReport,
    w: &mut impl std::io::Write,
) -> std::io::Result<()> {
    use console::style;

    // Header
    writeln!(w)?;
    writeln!(
        w,
        "{} {}",
        style(&report.name).green().bold(),
        style(&report.version).dim()
    )?;
    if !report.description.is_empty() {
        writeln!(w, "{}", report.description)?;
    }

    // Authors
    if !report.owners.is_empty() {
        writeln!(w)?;
        writeln!(w, "{}", style("Authors:").bold())?;
        for owner in &report.owners {
            if let Some(name) = &owner.name {
                writeln!(w, "  {} ({})", name, owner.login)?;
            } else {
                writeln!(w, "  {}", owner.login)?;
            }
        }
    }

    // Crates
    if !report.crates.is_empty() {
        writeln!(w)?;
        writeln!(w, "{}", style("Crates:").bold())?;
        for dep in &report.crates {
            let marker = if report.installed_crates.contains(dep) {
                format!(" {}", style("✓").green())
            } else {
                String::new()
            };
            writeln!(w, "  {}{}", dep, marker)?;
        }
    }

    // Features
    if !report.features.is_empty() {
        writeln!(w)?;
        writeln!(w, "{}", style("Features:").bold())?;
        for feat in &report.features {
            let marker = if report.active_features.contains(&feat.name) {
                format!(" {}", style("✓").green())
            } else {
                String::new()
            };
            writeln!(
                w,
                "  {} → {}{}",
                style(&feat.name).cyan(),
                feat.crates.join(", "),
                marker
            )?;
        }
    }

    // Extends
    if !report.extends.is_empty() {
        writeln!(w)?;
        writeln!(w, "{}", style("Extends:").bold())?;
        for dep in &report.extends {
            writeln!(w, "  {}", dep)?;
        }
    }

    // Templates
    if !report.templates.is_empty() {
        writeln!(w)?;
        writeln!(w, "{}", style("Templates:").bold())?;
        let max_name_len = report
            .templates
            .iter()
            .map(|t| t.name.len())
            .max()
            .unwrap_or(0);
        for tmpl in &report.templates {
            let name_padded = format!("{:<width$}", tmpl.name, width = max_name_len);
            if let Some(desc) = &tmpl.description {
                writeln!(w, "  {}  {}", style(name_padded).cyan(), desc)?;
            } else {
                writeln!(w, "  {}", style(name_padded).cyan())?;
            }
        }
    }

    // [impl format.examples.browsable]
    // Examples
    if !report.examples.is_empty() {
        writeln!(w)?;
        writeln!(w, "{}", style("Examples:").bold())?;
        let max_name_len = report
            .examples
            .iter()
            .map(|e| e.name.len())
            .max()
            .unwrap_or(0);
        for example in &report.examples {
            let name_padded = format!("{:<width$}", example.name, width = max_name_len);
            if let Some(desc) = &example.description {
                writeln!(w, "  {}  {}", style(name_padded).magenta(), desc)?;
            } else {
                writeln!(w, "  {}", style(name_padded).magenta())?;
            }
        }
    }

    // Install hints
    writeln!(w)?;
    writeln!(w, "{}", style("Install:").bold())?;
    writeln!(w, "  cargo bp add {}", report.short_name)?;
    writeln!(w, "  cargo bp new {}", report.short_name)?;
    writeln!(w)?;

    Ok(())
}

// [impl cli.show.template-preview]
fn print_template_preview(opts: &crate::template_engine::PreviewOpts<'_>) -> Result<()> {
    let (_crate_name, files) = crate::template_engine::preview_template(opts)?;

    for file in &files {
        println!("── {} ──", file.path);
        println!("{}", file.content);
        println!();
    }

    Ok(())
}

// ============================================================================
// Status command
// ============================================================================
//
// Status is split into three pieces so that the text and JSON outputs are
// guaranteed to read from the same data:
//   1. `build_status_report` — pure data, returns a `StatusReport`.
//   2. `render_status_json`  — serializes the report to a writer.
//   3. `render_status_text`  — pretty-prints the report to a writer.
// `status_battery_packs` is a thin dispatcher that picks one renderer.

// [impl cli.status.list]
// [impl cli.status.version-warn]
// [impl cli.status.no-project]
// [impl cli.status.json]
// [impl cli.source.subcommands]
// [impl cli.path.subcommands]
fn status_battery_packs(
    project_dir: &Path,
    path: Option<&str>,
    source: &CrateSource,
    json: bool,
) -> Result<()> {
    let report = build_status_report(project_dir, path, source)?;
    let stdout = std::io::stdout();
    let mut out = stdout.lock();
    if json {
        render_status_json(&report, &mut out).context("Failed to write status JSON")?;
    } else {
        render_status_text(&report, &mut out).context("Failed to render status")?;
    }
    Ok(())
}

/// Serialize a [`StatusReport`] as JSON to `w`, with a trailing newline.
///
/// Used for `cargo bp status --json`. Stable, machine-consumable output;
/// the schema lives in `cargo-bp-script`.
// [impl cli.status.json]
fn render_status_json(
    report: &cargo_bp_script::StatusReport,
    w: &mut impl std::io::Write,
) -> std::io::Result<()> {
    serde_json::to_writer(&mut *w, report)?;
    writeln!(w)?;
    Ok(())
}

/// Pretty-print a [`StatusReport`] for human consumption to `w`.
///
/// Reads the same data the JSON renderer reads, so JSON and text output
/// are guaranteed to be consistent at the data layer.
// [impl cli.status.list]
// [impl cli.status.version-warn]
fn render_status_text(
    report: &cargo_bp_script::StatusReport,
    w: &mut impl std::io::Write,
) -> std::io::Result<()> {
    use console::style;

    if report.packs.is_empty() {
        writeln!(w, "No battery packs installed.")?;
        return Ok(());
    }

    let mut any_warnings = false;
    for pack in &report.packs {
        // [impl cli.status.list]
        writeln!(
            w,
            "{} ({})",
            style(&pack.short_name).bold(),
            style(&pack.version).dim(),
        )?;

        if pack.warnings.is_empty() {
            writeln!(w, "  {} all dependencies up to date", style("✓").green())?;
        } else {
            any_warnings = true;
            for warning in &pack.warnings {
                // [impl cli.status.version-warn]
                writeln!(
                    w,
                    "  {} {}: {} → {} recommended",
                    style("⚠").yellow(),
                    warning.crate_name,
                    style(&warning.current_version).red(),
                    style(&warning.recommended_version).green(),
                )?;
            }
        }
    }

    if any_warnings {
        writeln!(w)?;
        writeln!(w, "Run {} to update.", style("cargo bp sync").bold())?;
    }

    Ok(())
}

/// Build a [`cargo_bp_script::StatusReport`] for the project rooted at
/// `project_dir`. Pure data — no terminal output. Both the text renderer
/// and the `--json` mode consume the same report.
// [impl cli.status.list]
// [impl cli.status.version-warn]
// [impl cli.status.no-project]
fn build_status_report(
    project_dir: &Path,
    path: Option<&str>,
    source: &CrateSource,
) -> Result<cargo_bp_script::StatusReport> {
    // [impl cli.status.no-project]
    let user_manifest_path =
        find_user_manifest(project_dir).context("are you inside a Rust project?")?;
    let user_manifest_content =
        std::fs::read_to_string(&user_manifest_path).context("Failed to read Cargo.toml")?;

    // Inline the load_installed_packs logic to avoid re-reading the manifest.
    let bp_names = find_installed_bp_names(&user_manifest_content)?;
    let packs: Vec<InstalledPack> = bp_names
        .into_iter()
        .map(|bp_name| {
            let spec = load_installed_bp_spec(&bp_name, path, source)?;
            let active_features = read_active_features_for_project(
                &user_manifest_path,
                &user_manifest_content,
                &bp_name,
            );
            Ok(InstalledPack {
                short_name: short_name(&bp_name).to_string(),
                version: spec.version.clone(),
                spec,
                active_features,
            })
        })
        .collect::<Result<_>>()?;

    // Build a map of the user's actual dependency versions so we can compare.
    // (Cheap to compute even when packs is empty; keeps the structure simple.)
    let user_versions = collect_user_dep_versions(&user_manifest_path, &user_manifest_content)?;

    // --- Map each installed pack into its `InstalledPackStatus`.
    let pack_statuses: Vec<cargo_bp_script::InstalledPackStatus> = packs
        .iter()
        .map(|pack| {
            // Resolve which crates are expected for this pack's active features.
            let expected = pack.spec.resolve_for_features(&pack.active_features);

            // [impl cli.status.version-warn]
            let warnings = expected.iter().filter_map(|(dep_name, dep_spec)| {
                if dep_spec.version.is_empty() {
                    return None;
                }
                let user_version = user_versions.get(dep_name.as_str())?;
                if !should_upgrade_version(user_version, &dep_spec.version) {
                    return None;
                }
                Some(cargo_bp_script::DependencyWarning::new(
                    dep_name.clone(),
                    user_version.clone(),
                    dep_spec.version.clone(),
                ))
            });

            let feature_strings: Vec<String> = match &pack.active_features {
                bphelper_manifest::ActiveFeatures::All => vec!["all".to_string()],
                bphelper_manifest::ActiveFeatures::Subset(set) => set.iter().cloned().collect(),
            };

            cargo_bp_script::InstalledPackStatus::new(
                &pack.short_name,
                &pack.spec.name,
                &pack.version,
            )
            .with_active_features(feature_strings)
            .with_warnings(warnings)
        })
        .collect();

    Ok(
        cargo_bp_script::StatusReport::new(cargo_bp_script::ProjectInfo::new(user_manifest_path))
            .with_packs(pack_statuses),
    )
}

fn check_battery_packs(
    project_dir: &Path,
    _path: Option<&str>,
    source: &CrateSource,
) -> Result<()> {
    let user_manifest_path = find_user_manifest(project_dir)?;
    let user_manifest_content =
        std::fs::read_to_string(&user_manifest_path).context("Failed to read Cargo.toml")?;

    // For now, use build-dependencies to find battery packs (this will be updated when metadata reading is improved)
    let bp_names = find_installed_bp_names(&user_manifest_content)?;

    if bp_names.is_empty() {
        println!("No battery packs installed.");
        return Ok(());
    }

    println!("Checking {} installed battery pack(s)...", bp_names.len());

    // Get user's current dependency versions
    let user_versions = collect_user_dep_versions(&user_manifest_path, &user_manifest_content)?;

    let mut all_valid = true;

    for bp_name in &bp_names {
        print!("  {} ... ", bp_name);

        // Get the battery pack spec
        let (_version, spec) = match crate::registry::fetch_bp_spec(source, bp_name) {
            Ok(result) => result,
            Err(e) => {
                println!("❌ Failed to load spec: {}", e);
                all_valid = false;
                continue;
            }
        };

        // Check for version drift
        let mut warnings = Vec::new();
        for (crate_name, crate_spec) in &spec.crates {
            if let Some(user_version) = user_versions.get(crate_name)
                && is_older_version(user_version, &crate_spec.version)
            {
                warnings.push(format!(
                    "{}: {} → {}",
                    crate_name, user_version, crate_spec.version
                ));
            }
        }

        if warnings.is_empty() {
            println!("✅ OK");
        } else {
            println!("⚠️  Outdated versions:");
            for warning in warnings {
                println!("    {}", warning);
            }
            all_valid = false;
        }
    }

    if all_valid {
        println!("\nAll battery packs are up to date! ✅");
    } else {
        println!("\nSome dependencies are outdated. Run `cargo bp sync` to update. ⚠️");
    }

    Ok(())
}

fn is_older_version(user_version: &str, recommended_version: &str) -> bool {
    // Simple version comparison - parse as semver if possible
    match (
        semver::Version::parse(user_version),
        semver::Version::parse(recommended_version),
    ) {
        (Ok(user), Ok(recommended)) => user < recommended,
        _ => false, // If we can't parse, assume it's fine
    }
}

/// Collect the user's actual dependency versions from Cargo.toml (and workspace deps if applicable).
///
/// Returns a map of `crate_name → version_string`.
pub(crate) fn collect_user_dep_versions(
    user_manifest_path: &Path,
    user_manifest_content: &str,
) -> Result<BTreeMap<String, String>> {
    let raw: toml::Value =
        toml::from_str(user_manifest_content).context("Failed to parse Cargo.toml")?;

    let mut versions = BTreeMap::new();

    // Read workspace dependency versions (if applicable).
    let ws_versions = if let Some(ws_path) = find_workspace_manifest(user_manifest_path)? {
        let ws_content =
            std::fs::read_to_string(&ws_path).context("Failed to read workspace Cargo.toml")?;
        let ws_raw: toml::Value =
            toml::from_str(&ws_content).context("Failed to parse workspace Cargo.toml")?;
        extract_versions_from_table(
            ws_raw
                .get("workspace")
                .and_then(|w| w.get("dependencies"))
                .and_then(|d| d.as_table()),
        )
    } else {
        BTreeMap::new()
    };

    // Collect from each dependency section.
    for section in ["dependencies", "dev-dependencies", "build-dependencies"] {
        let table = raw.get(section).and_then(|d| d.as_table());
        let Some(table) = table else { continue };
        for (name, value) in table {
            if versions.contains_key(name) {
                continue; // first section wins
            }
            if let Some(version) = extract_version_from_dep(value) {
                versions.insert(name.clone(), version);
            } else if is_workspace_ref(value) {
                // Resolve from workspace deps.
                if let Some(ws_ver) = ws_versions.get(name) {
                    versions.insert(name.clone(), ws_ver.clone());
                }
            }
        }
    }

    Ok(versions)
}

/// Extract version strings from a TOML dependency table.
fn extract_versions_from_table(
    table: Option<&toml::map::Map<String, toml::Value>>,
) -> BTreeMap<String, String> {
    let Some(table) = table else {
        return BTreeMap::new();
    };
    let mut versions = BTreeMap::new();
    for (name, value) in table {
        if let Some(version) = extract_version_from_dep(value) {
            versions.insert(name.clone(), version);
        }
    }
    versions
}

/// Extract the version string from a single dependency value.
///
/// Handles both `crate = "1.0"` and `crate = { version = "1.0", ... }`.
fn extract_version_from_dep(value: &toml::Value) -> Option<String> {
    match value {
        toml::Value::String(s) => Some(s.clone()),
        toml::Value::Table(t) => t
            .get("version")
            .and_then(|v| v.as_str())
            .map(|s| s.to_string()),
        _ => None,
    }
}

/// Check if a dependency entry is a workspace reference (`{ workspace = true }`).
fn is_workspace_ref(value: &toml::Value) -> bool {
    match value {
        toml::Value::Table(t) => t
            .get("workspace")
            .and_then(|v| v.as_bool())
            .unwrap_or(false),
        _ => false,
    }
}

#[cfg(test)]
mod tests;
