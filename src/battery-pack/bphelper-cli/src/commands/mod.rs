//! Subcommand implementations, CLI arg types, and interactive picker.
//!
//! This module contains the `main()` entry point and all subcommand handlers.
//! Depends on `registry` and `manifest`.

use anyhow::{Context, Result, bail};
use clap::{Parser, Subcommand};
use std::collections::{BTreeMap, BTreeSet};
use std::io::IsTerminal;
use std::path::{Path, PathBuf};

use crate::manifest::{
    MetadataLocation, add_dep_to_table, dep_kind_section, find_installed_bp_names,
    find_user_manifest, find_workspace_manifest, read_active_features_from, read_managed_deps_from,
    remove_deps_by_kind, resolve_metadata_location, should_upgrade_version, sync_dep_in_table,
    write_bp_features_to_doc, write_deps_by_kind, write_workspace_refs_by_kind,
};
use crate::registry::{
    CrateSource, InstalledPack, TemplateConfig, download_and_extract_crate,
    fetch_battery_pack_detail, fetch_battery_pack_detail_from_source, fetch_battery_pack_list,
    fetch_bp_spec, find_local_battery_pack_dir, load_installed_bp_spec, lookup_crate,
    resolve_crate_name, short_name,
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

        #[command(subcommand)]
        command: BpCommands,
    },
}

#[derive(Subcommand)]
pub(crate) enum BpCommands {
    /// Create a new project from a battery pack template
    New {
        /// Name of the battery pack (e.g., "cli" resolves to "cli-battery-pack")
        battery_pack: String,

        /// Name for the new project (prompted interactively if not provided)
        #[arg(long, short = 'n')]
        name: Option<String>,

        /// Which template to use (defaults to first available, or prompts if multiple)
        // [impl cli.new.template-flag]
        #[arg(long, short = 't')]
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
        battery_pack: Option<String>,

        /// Specific crates to add from the battery pack (ignores defaults/features)
        crates: Vec<String>,

        // [impl cli.add.features]
        // [impl cli.add.features-multiple]
        /// Named features to enable (comma-separated or repeated)
        #[arg(long = "features", short = 'F', value_delimiter = ',')]
        features: Vec<String>,

        // [impl cli.add.no-default-features]
        /// Skip the default crates; only add crates from named features
        #[arg(long)]
        no_default_features: bool,

        // [impl cli.add.all-features]
        /// Add every crate the battery pack offers
        #[arg(long)]
        all_features: bool,

        // [impl cli.add.target]
        /// Where to store the battery pack registration
        /// (workspace, package, or default)
        #[arg(long)]
        target: Option<AddTarget>,

        /// Use a local path instead of downloading from crates.io
        #[arg(long)]
        path: Option<String>,
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

        /// Disable interactive TUI mode
        #[arg(long)]
        non_interactive: bool,
    },

    /// Show detailed information about a battery pack
    #[command(visible_alias = "info")]
    Show {
        /// Name of the battery pack (e.g., "cli" resolves to "cli-battery-pack")
        battery_pack: String,

        /// Use a local path instead of downloading from crates.io
        #[arg(long)]
        path: Option<String>,

        /// Disable interactive TUI mode
        #[arg(long)]
        non_interactive: bool,
    },

    /// Show status of installed battery packs and version warnings
    #[command(visible_alias = "stat")]
    Status {
        // [impl cli.path.subcommands]
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
}

// [impl cli.add.target]
#[derive(Debug, Clone, Copy, PartialEq, Eq, clap::ValueEnum)]
pub(crate) enum AddTarget {
    /// Register in `workspace.metadata.battery-pack`.
    Workspace,
    /// Register in `package.metadata.battery-pack`.
    Package,
    /// Use workspace if a workspace root exists, otherwise package
    Default,
}

pub fn main() -> Result<()> {
    let cli = Cli::parse();
    let project_dir = std::env::current_dir().context("Failed to get current directory")?;
    let interactive = std::io::stdout().is_terminal();

    match cli.command {
        Commands::Bp {
            crate_source,
            command,
        } => {
            let source = match crate_source {
                Some(path) => CrateSource::Local(path),
                None => CrateSource::Registry,
            };
            match command {
                BpCommands::New {
                    battery_pack,
                    name,
                    template,
                    path,
                    define,
                } => new_from_battery_pack(&battery_pack, name, template, path, &source, &define),
                BpCommands::Add {
                    battery_pack,
                    crates,
                    features,
                    no_default_features,
                    all_features,
                    target,
                    path,
                } => match battery_pack {
                    Some(name) => add_battery_pack(
                        &name,
                        &features,
                        no_default_features,
                        all_features,
                        &crates,
                        target,
                        path.as_deref(),
                        &source,
                        &project_dir,
                    ),
                    None => show_add_help(&project_dir),
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
                BpCommands::List {
                    filter,
                    non_interactive,
                } => {
                    // [impl cli.list.interactive]
                    // [impl cli.list.non-interactive]
                    if !non_interactive && interactive {
                        crate::tui::run_list(source, filter)
                    } else {
                        // [impl cli.list.query]
                        // [impl cli.list.filter]
                        print_battery_pack_list(&source, filter.as_deref())
                    }
                }
                BpCommands::Show {
                    battery_pack,
                    path,
                    non_interactive,
                } => {
                    // [impl cli.show.interactive]
                    // [impl cli.show.non-interactive]
                    if !non_interactive && interactive {
                        crate::tui::run_show(&battery_pack, path.as_deref(), source)
                    } else {
                        print_battery_pack_detail(
                            &battery_pack,
                            path.as_deref(),
                            &source,
                            &project_dir,
                        )
                    }
                }
                BpCommands::Status { path } => {
                    status_battery_packs(&project_dir, path.as_deref(), &source)
                }
                BpCommands::Validate { path } => {
                    crate::validate::validate_battery_pack_cmd(path.as_deref())
                }
            }
        }
    }
}

// ============================================================================
// Implementation
// ============================================================================

// [impl cli.new.template]
// [impl cli.new.name-flag]
// [impl cli.new.name-prompt]
// [impl cli.path.flag]
// [impl cli.source.replace]
fn new_from_battery_pack(
    battery_pack: &str,
    name: Option<String>,
    template: Option<String>,
    path_override: Option<String>,
    source: &CrateSource,
    define: &[(String, String)],
) -> Result<()> {
    let defines: std::collections::BTreeMap<String, String> = define.iter().cloned().collect();

    // --path takes precedence over --crate-source
    if let Some(path) = path_override {
        return generate_from_local(battery_pack, &path, name, template, defines);
    }

    let crate_name = resolve_crate_name(battery_pack);

    // Locate the crate directory based on source
    let crate_dir: PathBuf;
    let _temp_dir: Option<tempfile::TempDir>; // keep alive for Registry
    match source {
        CrateSource::Registry => {
            let crate_info = lookup_crate(&crate_name)?;
            let temp = download_and_extract_crate(&crate_name, &crate_info.version)?;
            crate_dir = temp
                .path()
                .join(format!("{}-{}", crate_name, crate_info.version));
            _temp_dir = Some(temp);
        }
        CrateSource::Local(workspace_dir) => {
            crate_dir = find_local_battery_pack_dir(workspace_dir, &crate_name)?;
            _temp_dir = None;
        }
    }

    // Read template metadata from the Cargo.toml
    let manifest_path = crate_dir.join("Cargo.toml");
    let manifest_content = std::fs::read_to_string(&manifest_path)
        .with_context(|| format!("Failed to read {}", manifest_path.display()))?;
    let templates = parse_template_metadata(&manifest_content, &crate_name)?;

    // Resolve which template to use
    let template_path = resolve_template(&templates, template.as_deref())?;

    // Generate the project from the crate directory
    generate_from_path(battery_pack, &crate_dir, &template_path, name, defines)
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
        return ResolvedAdd::Crates {
            active_features: BTreeSet::from(["all".to_string()]),
            crates: bp_spec.resolve_all_visible(),
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
    target: Option<AddTarget>,
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
        let manifest_content = std::fs::read_to_string(&manifest_path)
            .with_context(|| format!("Failed to read {}", manifest_path.display()))?;
        let spec = bphelper_manifest::parse_battery_pack(&manifest_content)
            .map_err(|e| anyhow::anyhow!("Failed to parse battery pack '{}': {}", crate_name, e))?;
        (None, spec)
    } else {
        fetch_bp_spec(source, name)?
    };

    // Step 2: Determine which crates to install — interactive picker, explicit flags, or defaults.
    // No manifest changes have been made yet, so cancellation is free.
    let resolved = resolve_add_crates(
        &bp_spec,
        &crate_name,
        with_features,
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
    let user_manifest_path = find_user_manifest(project_dir)?;
    let user_manifest_content =
        std::fs::read_to_string(&user_manifest_path).context("Failed to read Cargo.toml")?;
    // [impl manifest.toml.preserve]
    let mut user_doc: toml_edit::DocumentMut = user_manifest_content
        .parse()
        .context("Failed to parse Cargo.toml")?;

    // [impl manifest.register.workspace-default]
    let workspace_manifest = find_workspace_manifest(&user_manifest_path)?;

    // Add battery pack to [build-dependencies]
    let build_deps =
        user_doc["build-dependencies"].or_insert(toml_edit::Item::Table(toml_edit::Table::new()));
    if let Some(table) = build_deps.as_table_mut() {
        if let Some(local_path) = path {
            let mut dep = toml_edit::InlineTable::new();
            dep.insert("path", toml_edit::Value::from(local_path));
            table.insert(
                &crate_name,
                toml_edit::Item::Value(toml_edit::Value::InlineTable(dep)),
            );
        } else if workspace_manifest.is_some() {
            let mut dep = toml_edit::InlineTable::new();
            dep.insert("workspace", toml_edit::Value::from(true));
            table.insert(
                &crate_name,
                toml_edit::Item::Value(toml_edit::Value::InlineTable(dep)),
            );
        } else {
            let version = bp_version
                .as_ref()
                .context("battery pack version not available (--path without workspace)")?;
            table.insert(&crate_name, toml_edit::value(version));
        }
    }

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

    // [impl manifest.register.location]
    // [impl manifest.register.format]
    // [impl manifest.features.storage]
    // [impl cli.add.target]
    // Edit semantics: remove deselected crates from previous installation
    let metadata_location = resolve_metadata_location(&user_manifest_path)?;
    let prev_managed =
        read_managed_deps_from(&metadata_location, &user_manifest_content, &crate_name);
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

    // Record active features — location depends on --target flag
    let managed_deps = new_crate_names;
    let use_workspace_metadata = match target {
        Some(AddTarget::Workspace) => true,
        Some(AddTarget::Package) => false,
        Some(AddTarget::Default) | None => workspace_manifest.is_some(),
    };

    if use_workspace_metadata {
        if let Some(ref mut doc) = ws_doc {
            write_bp_features_to_doc(
                doc,
                &["workspace", "metadata"],
                &crate_name,
                &active_features,
                Some(&managed_deps),
            );
        } else {
            bail!("--target=workspace requires a workspace, but none was found");
        }
    } else {
        write_bp_features_to_doc(
            &mut user_doc,
            &["package", "metadata"],
            &crate_name,
            &active_features,
            Some(&managed_deps),
        );
    }

    // Write workspace Cargo.toml once (deps + metadata combined)
    if let (Some(ws_path), Some(doc)) = (&workspace_manifest, &ws_doc) {
        // [impl manifest.toml.preserve]
        std::fs::write(ws_path, doc.to_string()).context("Failed to write workspace Cargo.toml")?;
    }

    // Write the final Cargo.toml
    // [impl manifest.toml.preserve]
    std::fs::write(&user_manifest_path, user_doc.to_string())
        .context("Failed to write Cargo.toml")?;

    // Create/modify build.rs
    let build_rs_path = user_manifest_path
        .parent()
        .unwrap_or(Path::new("."))
        .join("build.rs");
    update_build_rs(&build_rs_path, &crate_name)?;

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
    metadata_location: &MetadataLocation,
    user_manifest_content: &str,
) -> BTreeSet<String> {
    let mut shared = BTreeSet::new();
    for other_bp in all_bp_names {
        if other_bp == current_bp {
            continue;
        }
        if let Some(other_managed) =
            read_managed_deps_from(metadata_location, user_manifest_content, other_bp)
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

    let metadata_location = resolve_metadata_location(&user_manifest_path)?;
    let managed_deps =
        read_managed_deps_from(&metadata_location, &user_manifest_content, &crate_name);

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
                &metadata_location,
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
            &metadata_location,
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

            // Remove metadata from workspace if that's where it lives
            if matches!(metadata_location, MetadataLocation::Workspace { .. })
                && let Some(bp_table) = ws_doc
                    .get_mut("workspace")
                    .and_then(|w| w.get_mut("metadata"))
                    .and_then(|m| m.get_mut("battery-pack"))
                    .and_then(|bp| bp.as_table_mut())
            {
                bp_table.remove(&crate_name);
            }

            std::fs::write(ws_path, ws_doc.to_string())
                .context("Failed to write workspace Cargo.toml")?;
        }

        if !safe.is_empty() {
            println!("Removed {} dependency(ies)", safe.len());
        }
    }

    // Remove metadata from package if that's where it lives
    if matches!(metadata_location, MetadataLocation::Package)
        && let Some(bp_table) = user_doc
            .get_mut("package")
            .and_then(|p| p.get_mut("metadata"))
            .and_then(|m| m.get_mut("battery-pack"))
            .and_then(|bp| bp.as_table_mut())
    {
        bp_table.remove(&crate_name);
    }

    std::fs::write(&user_manifest_path, user_doc.to_string())
        .context("Failed to write Cargo.toml")?;

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
    let metadata_location = resolve_metadata_location(&user_manifest_path)?;
    let mut total_changes = 0;

    for bp_name in &bp_names {
        // Get the battery pack spec
        let bp_spec = load_installed_bp_spec(bp_name, path, source)?;

        // Read active features from the correct metadata location
        let active_features =
            read_active_features_from(&metadata_location, &user_manifest_content, bp_name);

        // [impl format.hidden.effect]
        let expected = bp_spec.resolve_for_features(&active_features);

        // Compute managed-deps: migrate old-format or merge new crates
        let existing_managed =
            read_managed_deps_from(&metadata_location, &user_manifest_content, bp_name);
        let expected_names: BTreeSet<String> = expected.keys().cloned().collect();
        let managed_deps = match existing_managed {
            None => expected_names, // migration: populate from resolved crates
            Some(mut set) => {
                set.extend(expected_names);
                set
            }
        };

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

            // Write managed-deps to workspace metadata if that's where it lives
            if matches!(metadata_location, MetadataLocation::Workspace { .. }) {
                write_bp_features_to_doc(
                    &mut ws_doc,
                    &["workspace", "metadata"],
                    bp_name,
                    &active_features,
                    Some(&managed_deps),
                );
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

        // Write managed-deps to package metadata if that's where it lives
        if matches!(metadata_location, MetadataLocation::Package) {
            write_bp_features_to_doc(
                &mut user_doc,
                &["package", "metadata"],
                bp_name,
                &active_features,
                Some(&managed_deps),
            );
        }
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
    let features: Vec<(&String, &BTreeSet<String>)> = bp_spec
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
            .filter(|c| !bp_spec.is_hidden(c))
            .cloned()
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
                .filter(|c| !bp_spec.is_hidden(c))
                .all(|c| default_crates.contains(c))
        } else {
            // Feature is checked if all its visible members are pre-selected
            feat_crates
                .iter()
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

    // Determine which features and crates are selected
    let selected_set: BTreeSet<usize> = selected_indices.into_iter().collect();
    let mut selected_crates: BTreeSet<String> = BTreeSet::new();

    for (i, item) in items.iter().enumerate() {
        if !selected_set.contains(&i) {
            continue;
        }
        match item {
            PickerItem::Feature(feat_name) => {
                if let Some(members) = bp_spec.features.get(feat_name) {
                    for c in members {
                        if !bp_spec.is_hidden(c) {
                            selected_crates.insert(c.clone());
                        }
                    }
                }
            }
            PickerItem::Crate(name) => {
                selected_crates.insert(name.clone());
            }
        }
    }

    // Build the result
    let mut crates = BTreeMap::new();
    for name in &selected_crates {
        if let Some(spec) = bp_spec.crates.get(name) {
            crates.insert(name.clone(), spec.clone());
        }
    }

    // Determine which features are fully selected
    let mut active_features = BTreeSet::new();
    // Check if all default crates are selected
    let default_members = bp_spec.features.get("default");
    if default_members.is_some_and(|members| {
        members
            .iter()
            .filter(|c| !bp_spec.is_hidden(c))
            .all(|c| selected_crates.contains(c))
    }) {
        active_features.insert("default".to_string());
    }
    for (feat_name, feat_crates) in &bp_spec.features {
        if feat_name == "default" {
            continue;
        }
        if feat_crates
            .iter()
            .filter(|c| !bp_spec.is_hidden(c))
            .all(|c| selected_crates.contains(c))
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

/// Update or create build.rs to include a validate() call.
fn update_build_rs(build_rs_path: &Path, crate_name: &str) -> Result<()> {
    let crate_ident = crate_name.replace('-', "_");
    let validate_call = format!("{}::validate();", crate_ident);

    if build_rs_path.exists() {
        let content = std::fs::read_to_string(build_rs_path).context("Failed to read build.rs")?;

        // Check if validate call is already present
        if content.contains(&validate_call) {
            return Ok(());
        }

        // Verify the file parses as valid Rust with syn
        let file: syn::File = syn::parse_str(&content).context("Failed to parse build.rs")?;

        // Check that a main function exists
        let has_main = file
            .items
            .iter()
            .any(|item| matches!(item, syn::Item::Fn(func) if func.sig.ident == "main"));

        if has_main {
            // Find the closing brace of main using string manipulation
            let lines: Vec<&str> = content.lines().collect();
            let mut insert_line = None;
            let mut brace_depth: i32 = 0;
            let mut in_main = false;

            for (i, line) in lines.iter().enumerate() {
                if line.contains("fn main") {
                    in_main = true;
                    brace_depth = 0;
                }
                if in_main {
                    for ch in line.chars() {
                        if ch == '{' {
                            brace_depth += 1;
                        } else if ch == '}' {
                            brace_depth -= 1;
                            if brace_depth == 0 {
                                insert_line = Some(i);
                                in_main = false;
                                break;
                            }
                        }
                    }
                }
            }

            if let Some(line_idx) = insert_line {
                let mut new_lines: Vec<String> = lines.iter().map(|l| l.to_string()).collect();
                new_lines.insert(line_idx, format!("    {}", validate_call));
                std::fs::write(build_rs_path, new_lines.join("\n") + "\n")
                    .context("Failed to write build.rs")?;
                return Ok(());
            }
        }

        // Fallback: no main function found or couldn't locate closing brace
        bail!(
            "Could not find fn main() in build.rs. Please add `{}` manually.",
            validate_call
        );
    } else {
        // Create new build.rs
        let content = format!("fn main() {{\n    {}\n}}\n", validate_call);
        std::fs::write(build_rs_path, content).context("Failed to create build.rs")?;
    }

    Ok(())
}

fn generate_from_local(
    battery_pack: &str,
    local_path: &str,
    name: Option<String>,
    template: Option<String>,
    defines: std::collections::BTreeMap<String, String>,
) -> Result<()> {
    let local_path = Path::new(local_path);

    // Read local Cargo.toml
    let manifest_path = local_path.join("Cargo.toml");
    let manifest_content = std::fs::read_to_string(&manifest_path)
        .with_context(|| format!("Failed to read {}", manifest_path.display()))?;

    let crate_name = local_path
        .file_name()
        .and_then(|s| s.to_str())
        .unwrap_or("unknown");
    let templates = parse_template_metadata(&manifest_content, crate_name)?;
    let template_path = resolve_template(&templates, template.as_deref())?;

    generate_from_path(battery_pack, local_path, &template_path, name, defines)
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

fn generate_from_path(
    battery_pack: &str,
    crate_path: &Path,
    template_path: &str,
    name: Option<String>,
    defines: std::collections::BTreeMap<String, String>,
) -> Result<()> {
    let raw = prompt_project_name(name)?;
    let project_name = if battery_pack == "battery-pack" {
        ensure_battery_pack_suffix(raw)
    } else {
        raw
    };

    let opts = crate::template_engine::GenerateOpts {
        render: crate::template_engine::RenderOpts {
            crate_root: crate_path.to_path_buf(),
            template_path: template_path.to_string(),
            project_name,
            defines,
            interactive_override: None,
        },
        destination: None,
        git_init: true,
    };

    crate::template_engine::generate(opts)?;

    Ok(())
}

/// Parse a `key=value` string for clap's `value_parser`.
fn parse_define(s: &str) -> Result<(String, String), String> {
    let (key, value) = s
        .split_once('=')
        .ok_or_else(|| format!("invalid define '{s}': expected key=value"))?;
    Ok((key.to_string(), value.to_string()))
}

fn parse_template_metadata(
    manifest_content: &str,
    crate_name: &str,
) -> Result<BTreeMap<String, TemplateConfig>> {
    let spec = bphelper_manifest::parse_battery_pack(manifest_content)
        .map_err(|e| anyhow::anyhow!("Failed to parse Cargo.toml: {}", e))?;

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
                // Only one template, use it
                let (_, config) = templates.iter().next().unwrap();
                Ok(config.path.clone())
            } else if let Some(config) = templates.get("default") {
                // Multiple templates, but there's a 'default'
                Ok(config.path.clone())
            } else {
                // Multiple templates, no default - prompt user to pick
                prompt_for_template(templates)
            }
        }
    }
}

fn prompt_for_template(templates: &BTreeMap<String, TemplateConfig>) -> Result<String> {
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
    if !std::io::stdout().is_terminal() {
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

fn print_battery_pack_list(source: &CrateSource, filter: Option<&str>) -> Result<()> {
    use console::style;

    let battery_packs = fetch_battery_pack_list(source, filter)?;

    if battery_packs.is_empty() {
        match filter {
            Some(q) => println!("No battery packs found matching '{}'", q),
            None => println!("No battery packs found"),
        }
        return Ok(());
    }

    // Find the longest name for alignment
    let max_name_len = battery_packs
        .iter()
        .map(|c| c.short_name.len())
        .max()
        .unwrap_or(0);

    let max_version_len = battery_packs
        .iter()
        .map(|c| c.version.len())
        .max()
        .unwrap_or(0);

    println!();
    for bp in &battery_packs {
        let desc = bp.description.lines().next().unwrap_or("");

        // Pad strings manually, then apply colors (ANSI codes break width formatting)
        let name_padded = format!("{:<width$}", bp.short_name, width = max_name_len);
        let ver_padded = format!("{:<width$}", bp.version, width = max_version_len);

        println!(
            "  {}  {}  {}",
            style(name_padded).green().bold(),
            style(ver_padded).dim(),
            desc,
        );
    }
    println!();

    println!(
        "{}",
        style(format!("Found {} battery pack(s)", battery_packs.len())).dim()
    );

    Ok(())
}

/// Read installed state (managed-deps and active features) for a battery pack.
/// Returns empty sets if not in a project or pack not installed.
fn read_installed_state(
    project_dir: &Path,
    crate_name: &str,
) -> (BTreeSet<String>, BTreeSet<String>) {
    let empty = (BTreeSet::new(), BTreeSet::new());
    let Ok(manifest_path) = find_user_manifest(project_dir) else {
        return empty;
    };
    let Ok(content) = std::fs::read_to_string(&manifest_path) else {
        return empty;
    };
    let Ok(location) = resolve_metadata_location(&manifest_path) else {
        return empty;
    };
    let managed = read_managed_deps_from(&location, &content, crate_name).unwrap_or_default();
    let features = read_active_features_from(&location, &content, crate_name);
    (managed, features)
}

fn print_battery_pack_detail(
    name: &str,
    path: Option<&str>,
    source: &CrateSource,
    project_dir: &Path,
) -> Result<()> {
    use console::style;

    // --path takes precedence over --crate-source
    let detail = if path.is_some() {
        fetch_battery_pack_detail(name, path)?
    } else {
        fetch_battery_pack_detail_from_source(source, name)?
    };

    // Read installed state from the project (if available)
    let crate_name = resolve_crate_name(name);
    let (managed_deps, active_features) = read_installed_state(project_dir, &crate_name);

    // Header
    println!();
    println!(
        "{} {}",
        style(&detail.name).green().bold(),
        style(&detail.version).dim()
    );
    if !detail.description.is_empty() {
        println!("{}", detail.description);
    }

    // Authors
    if !detail.owners.is_empty() {
        println!();
        println!("{}", style("Authors:").bold());
        for owner in &detail.owners {
            if let Some(name) = &owner.name {
                println!("  {} ({})", name, owner.login);
            } else {
                println!("  {}", owner.login);
            }
        }
    }

    // Crates
    if !detail.crates.is_empty() {
        println!();
        println!("{}", style("Crates:").bold());
        for dep in &detail.crates {
            let marker = if managed_deps.contains(dep) {
                format!(" {}", style("✓").green())
            } else {
                String::new()
            };
            println!("  {}{}", dep, marker);
        }
    }

    // Features
    if !detail.features.is_empty() {
        println!();
        println!("{}", style("Features:").bold());
        for (feat_name, members) in &detail.features {
            let marker = if active_features.contains(feat_name) {
                format!(" {}", style("✓").green())
            } else {
                String::new()
            };
            println!(
                "  {} → {}{}",
                style(feat_name).cyan(),
                members.join(", "),
                marker
            );
        }
    }

    // Extends
    if !detail.extends.is_empty() {
        println!();
        println!("{}", style("Extends:").bold());
        for dep in &detail.extends {
            println!("  {}", dep);
        }
    }

    // Templates
    if !detail.templates.is_empty() {
        println!();
        println!("{}", style("Templates:").bold());
        let max_name_len = detail
            .templates
            .iter()
            .map(|t| t.name.len())
            .max()
            .unwrap_or(0);
        for tmpl in &detail.templates {
            let name_padded = format!("{:<width$}", tmpl.name, width = max_name_len);
            if let Some(desc) = &tmpl.description {
                println!("  {}  {}", style(name_padded).cyan(), desc);
            } else {
                println!("  {}", style(name_padded).cyan());
            }
        }
    }

    // [impl format.examples.browsable]
    // Examples
    if !detail.examples.is_empty() {
        println!();
        println!("{}", style("Examples:").bold());
        let max_name_len = detail
            .examples
            .iter()
            .map(|e| e.name.len())
            .max()
            .unwrap_or(0);
        for example in &detail.examples {
            let name_padded = format!("{:<width$}", example.name, width = max_name_len);
            if let Some(desc) = &example.description {
                println!("  {}  {}", style(name_padded).magenta(), desc);
            } else {
                println!("  {}", style(name_padded).magenta());
            }
        }
    }

    // Install hints
    println!();
    println!("{}", style("Install:").bold());
    println!("  cargo bp add {}", detail.short_name);
    println!("  cargo bp new {}", detail.short_name);
    println!();

    Ok(())
}

// ============================================================================
// Status command
// ============================================================================

// [impl cli.status.list]
// [impl cli.status.version-warn]
// [impl cli.status.no-project]
// [impl cli.source.subcommands]
// [impl cli.path.subcommands]
fn status_battery_packs(
    project_dir: &Path,
    path: Option<&str>,
    source: &CrateSource,
) -> Result<()> {
    use console::style;

    // [impl cli.status.no-project]
    let user_manifest_path =
        find_user_manifest(project_dir).context("are you inside a Rust project?")?;
    let user_manifest_content =
        std::fs::read_to_string(&user_manifest_path).context("Failed to read Cargo.toml")?;

    // Inline the load_installed_packs logic to avoid re-reading the manifest.
    let bp_names = find_installed_bp_names(&user_manifest_content)?;
    let metadata_location = resolve_metadata_location(&user_manifest_path)?;
    let packs: Vec<InstalledPack> = bp_names
        .into_iter()
        .map(|bp_name| {
            let spec = load_installed_bp_spec(&bp_name, path, source)?;
            let active_features =
                read_active_features_from(&metadata_location, &user_manifest_content, &bp_name);
            Ok(InstalledPack {
                short_name: short_name(&bp_name).to_string(),
                version: spec.version.clone(),
                spec,
                active_features,
            })
        })
        .collect::<Result<_>>()?;

    if packs.is_empty() {
        println!("No battery packs installed.");
        return Ok(());
    }

    // Build a map of the user's actual dependency versions so we can compare.
    let user_versions = collect_user_dep_versions(&user_manifest_path, &user_manifest_content)?;

    let mut any_warnings = false;

    for pack in &packs {
        // [impl cli.status.list]
        println!(
            "{} ({})",
            style(&pack.short_name).bold(),
            style(&pack.version).dim(),
        );

        // Resolve which crates are expected for this pack's active features.
        let expected = pack.spec.resolve_for_features(&pack.active_features);

        let mut pack_warnings = Vec::new();
        for (dep_name, dep_spec) in &expected {
            if dep_spec.version.is_empty() {
                continue;
            }
            if let Some(user_version) = user_versions.get(dep_name.as_str()) {
                // [impl cli.status.version-warn]
                if should_upgrade_version(user_version, &dep_spec.version) {
                    pack_warnings.push((
                        dep_name.as_str(),
                        user_version.as_str(),
                        dep_spec.version.as_str(),
                    ));
                }
            }
        }

        if pack_warnings.is_empty() {
            println!("  {} all dependencies up to date", style("✓").green());
        } else {
            any_warnings = true;
            for (dep, current, recommended) in &pack_warnings {
                println!(
                    "  {} {}: {} → {} recommended",
                    style("⚠").yellow(),
                    dep,
                    style(current).red(),
                    style(recommended).green(),
                );
            }
        }
    }

    if any_warnings {
        println!();
        println!("Run {} to update.", style("cargo bp sync").bold());
    }

    Ok(())
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
