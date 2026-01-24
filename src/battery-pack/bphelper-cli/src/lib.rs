//! CLI for battery-pack: create and manage battery packs.

use anyhow::{Context, Result, bail};
use cargo_generate::{GenerateArgs, TemplatePath, Vcs};
use clap::{Parser, Subcommand};
use flate2::read::GzDecoder;
use serde::Deserialize;
use std::collections::BTreeMap;
use std::io::IsTerminal;
use std::path::Path;
use tar::Archive;

mod tui;

const CRATES_IO_API: &str = "https://crates.io/api/v1/crates";
const CRATES_IO_CDN: &str = "https://static.crates.io/crates";

#[derive(Parser)]
#[command(name = "cargo-bp")]
#[command(bin_name = "cargo")]
#[command(version, about = "Create and manage battery packs", long_about = None)]
pub struct Cli {
    #[command(subcommand)]
    pub command: Commands,
}

#[derive(Subcommand)]
pub enum Commands {
    /// Battery pack commands
    Bp {
        #[command(subcommand)]
        command: BpCommands,
    },
}

#[derive(Subcommand)]
pub enum BpCommands {
    /// Create a new project from a battery pack template
    New {
        /// Name of the battery pack (e.g., "cli" resolves to "cli-battery-pack")
        battery_pack: String,

        /// Name for the new project (prompted interactively if not provided)
        #[arg(long, short = 'n')]
        name: Option<String>,

        /// Which template to use (defaults to first available, or prompts if multiple)
        #[arg(long, short = 't')]
        template: Option<String>,

        /// Use a local path instead of downloading from crates.io
        #[arg(long)]
        path: Option<String>,
    },

    /// Add a battery pack as a dependency
    Add {
        /// Name of the battery pack (e.g., "cli" resolves to "cli-battery-pack")
        battery_pack: String,

        /// Features to enable
        #[arg(long, short = 'F')]
        features: Vec<String>,
    },

    /// List available battery packs on crates.io
    List {
        /// Filter by name (omit to list all battery packs)
        filter: Option<String>,

        /// Disable interactive TUI mode
        #[arg(long)]
        non_interactive: bool,
    },

    /// Show detailed information about a battery pack
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
}

/// Main entry point for the CLI.
pub fn main() -> Result<()> {
    let cli = Cli::parse();

    match cli.command {
        Commands::Bp { command } => match command {
            BpCommands::New {
                battery_pack,
                name,
                template,
                path,
            } => new_from_battery_pack(&battery_pack, name, template, path),
            BpCommands::Add {
                battery_pack,
                features,
            } => add_battery_pack(&battery_pack, &features),
            BpCommands::List {
                filter,
                non_interactive,
            } => {
                if !non_interactive && std::io::stdout().is_terminal() {
                    tui::run_list(filter)
                } else {
                    print_battery_pack_list(filter.as_deref())
                }
            }
            BpCommands::Show {
                battery_pack,
                path,
                non_interactive,
            } => {
                if !non_interactive && std::io::stdout().is_terminal() {
                    tui::run_show(&battery_pack, path.as_deref())
                } else {
                    print_battery_pack_detail(&battery_pack, path.as_deref())
                }
            }
        },
    }
}

// ============================================================================
// crates.io API types
// ============================================================================

#[derive(Deserialize)]
struct CratesIoResponse {
    versions: Vec<VersionInfo>,
}

#[derive(Deserialize)]
struct VersionInfo {
    num: String,
    yanked: bool,
}

#[derive(Deserialize)]
struct SearchResponse {
    crates: Vec<SearchCrate>,
}

#[derive(Deserialize)]
struct SearchCrate {
    name: String,
    max_version: String,
    description: Option<String>,
}

// ============================================================================
// Battery pack metadata types (from Cargo.toml)
// ============================================================================

#[derive(Deserialize, Default)]
struct CargoManifest {
    package: Option<PackageSection>,
    #[serde(default)]
    dependencies: BTreeMap<String, toml::Value>,
}

#[derive(Deserialize, Default)]
struct PackageSection {
    name: Option<String>,
    version: Option<String>,
    description: Option<String>,
    repository: Option<String>,
    metadata: Option<PackageMetadata>,
}

#[derive(Deserialize, Default)]
struct PackageMetadata {
    battery: Option<BatteryMetadata>,
}

#[derive(Deserialize, Default)]
struct BatteryMetadata {
    #[serde(default)]
    templates: BTreeMap<String, TemplateConfig>,
}

#[derive(Deserialize)]
struct TemplateConfig {
    path: String,
    #[serde(default)]
    description: Option<String>,
}

// ============================================================================
// crates.io owner types
// ============================================================================

#[derive(Deserialize)]
struct OwnersResponse {
    users: Vec<Owner>,
}

#[derive(Deserialize, Clone)]
struct Owner {
    login: String,
    name: Option<String>,
}

// ============================================================================
// GitHub API types
// ============================================================================

#[derive(Deserialize)]
struct GitHubTreeResponse {
    tree: Vec<GitHubTreeEntry>,
    #[serde(default)]
    #[allow(dead_code)]
    truncated: bool,
}

#[derive(Deserialize)]
struct GitHubTreeEntry {
    path: String,
}

// ============================================================================
// Shared data types (used by both TUI and text output)
// ============================================================================

/// Summary info for displaying in a list
#[derive(Clone)]
pub struct BatteryPackSummary {
    pub name: String,
    pub short_name: String,
    pub version: String,
    pub description: String,
}

/// Detailed battery pack info
#[derive(Clone)]
pub struct BatteryPackDetail {
    pub name: String,
    pub short_name: String,
    pub version: String,
    pub description: String,
    pub repository: Option<String>,
    pub owners: Vec<OwnerInfo>,
    pub crates: Vec<String>,
    pub extends: Vec<String>,
    pub templates: Vec<TemplateInfo>,
    pub examples: Vec<ExampleInfo>,
}

#[derive(Clone)]
pub struct OwnerInfo {
    pub login: String,
    pub name: Option<String>,
}

impl From<Owner> for OwnerInfo {
    fn from(o: Owner) -> Self {
        Self {
            login: o.login,
            name: o.name,
        }
    }
}

#[derive(Clone)]
pub struct TemplateInfo {
    pub name: String,
    pub path: String,
    pub description: Option<String>,
    /// Full path in the repository (e.g., "src/cli-battery-pack/templates/simple")
    /// Resolved by searching the GitHub tree API
    pub repo_path: Option<String>,
}

#[derive(Clone)]
pub struct ExampleInfo {
    pub name: String,
    pub description: Option<String>,
    /// Full path in the repository (e.g., "src/cli-battery-pack/examples/mini-grep.rs")
    /// Resolved by searching the GitHub tree API
    pub repo_path: Option<String>,
}

// ============================================================================
// Implementation
// ============================================================================

fn new_from_battery_pack(
    battery_pack: &str,
    name: Option<String>,
    template: Option<String>,
    path_override: Option<String>,
) -> Result<()> {
    // If using local path, generate directly from there
    if let Some(path) = path_override {
        return generate_from_local(&path, name, template);
    }

    // Resolve the crate name (add -battery-pack suffix if needed)
    let crate_name = resolve_crate_name(battery_pack);

    // Look up the crate on crates.io and get the latest version
    let crate_info = lookup_crate(&crate_name)?;

    // Download and extract the crate to a temp directory
    let temp_dir = download_and_extract_crate(&crate_name, &crate_info.version)?;
    let crate_dir = temp_dir
        .path()
        .join(format!("{}-{}", crate_name, crate_info.version));

    // Read template metadata from the extracted Cargo.toml
    let manifest_path = crate_dir.join("Cargo.toml");
    let manifest_content = std::fs::read_to_string(&manifest_path)
        .with_context(|| format!("Failed to read {}", manifest_path.display()))?;
    let templates = parse_template_metadata(&manifest_content, &crate_name)?;

    // Resolve which template to use
    let template_path = resolve_template(&templates, template.as_deref())?;

    // Generate the project from the extracted crate
    generate_from_path(&crate_dir, &template_path, name)
}

fn add_battery_pack(name: &str, features: &[String]) -> Result<()> {
    let crate_name = resolve_crate_name(name);
    let short = short_name(&crate_name);

    // Verify the crate exists on crates.io
    lookup_crate(&crate_name)?;

    // Build cargo add command: cargo add cli-battery-pack --rename cli
    let mut cmd = std::process::Command::new("cargo");
    cmd.arg("add").arg(&crate_name);

    // Rename to the short name (e.g., cli-battery-pack -> cli)
    cmd.arg("--rename").arg(short);

    // Add features if specified
    for feature in features {
        cmd.arg("--features").arg(feature);
    }

    let status = cmd.status().context("Failed to run cargo add")?;

    if !status.success() {
        bail!("cargo add failed");
    }

    Ok(())
}

fn generate_from_local(
    local_path: &str,
    name: Option<String>,
    template: Option<String>,
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

    generate_from_path(local_path, &template_path, name)
}

fn generate_from_path(crate_path: &Path, template_path: &str, name: Option<String>) -> Result<()> {
    let args = GenerateArgs {
        template_path: TemplatePath {
            path: Some(crate_path.to_string_lossy().into_owned()),
            auto_path: Some(template_path.to_string()),
            ..Default::default()
        },
        name,
        vcs: Some(Vcs::Git),
        ..Default::default()
    };

    cargo_generate::generate(args)?;

    Ok(())
}

/// Info about a crate from crates.io
struct CrateMetadata {
    version: String,
}

/// Look up a crate on crates.io and return its metadata
fn lookup_crate(crate_name: &str) -> Result<CrateMetadata> {
    let client = reqwest::blocking::Client::builder()
        .user_agent("cargo-bp (https://github.com/battery-pack-rs/battery-pack)")
        .build()?;

    let url = format!("{}/{}", CRATES_IO_API, crate_name);
    let response = client
        .get(&url)
        .send()
        .with_context(|| format!("Failed to query crates.io for '{}'", crate_name))?;

    if !response.status().is_success() {
        bail!(
            "Crate '{}' not found on crates.io (status: {})",
            crate_name,
            response.status()
        );
    }

    let parsed: CratesIoResponse = response
        .json()
        .with_context(|| format!("Failed to parse crates.io response for '{}'", crate_name))?;

    // Find the latest non-yanked version
    let version = parsed
        .versions
        .iter()
        .find(|v| !v.yanked)
        .map(|v| v.num.clone())
        .ok_or_else(|| anyhow::anyhow!("No non-yanked versions found for '{}'", crate_name))?;

    Ok(CrateMetadata { version })
}

/// Download a crate tarball and extract it to a temp directory
fn download_and_extract_crate(crate_name: &str, version: &str) -> Result<tempfile::TempDir> {
    let client = reqwest::blocking::Client::builder()
        .user_agent("cargo-bp (https://github.com/battery-pack-rs/battery-pack)")
        .build()?;

    // Download from CDN: https://static.crates.io/crates/{name}/{name}-{version}.crate
    let url = format!(
        "{}/{}/{}-{}.crate",
        CRATES_IO_CDN, crate_name, crate_name, version
    );

    let response = client
        .get(&url)
        .send()
        .with_context(|| format!("Failed to download crate from {}", url))?;

    if !response.status().is_success() {
        bail!(
            "Failed to download '{}' version {} (status: {})",
            crate_name,
            version,
            response.status()
        );
    }

    let bytes = response
        .bytes()
        .with_context(|| "Failed to read crate tarball")?;

    // Create temp directory and extract
    let temp_dir = tempfile::tempdir().with_context(|| "Failed to create temp directory")?;

    let decoder = GzDecoder::new(&bytes[..]);
    let mut archive = Archive::new(decoder);
    archive
        .unpack(temp_dir.path())
        .with_context(|| "Failed to extract crate tarball")?;

    Ok(temp_dir)
}

fn parse_template_metadata(
    manifest_content: &str,
    crate_name: &str,
) -> Result<BTreeMap<String, TemplateConfig>> {
    let manifest: CargoManifest =
        toml::from_str(manifest_content).with_context(|| "Failed to parse Cargo.toml")?;

    let templates = manifest
        .package
        .and_then(|p| p.metadata)
        .and_then(|m| m.battery)
        .map(|b| b.templates)
        .unwrap_or_default();

    if templates.is_empty() {
        bail!(
            "Battery pack '{}' has no templates defined in [package.metadata.battery.templates]",
            crate_name
        );
    }

    Ok(templates)
}

fn resolve_template(
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
    let (_, config) = templates.iter().nth(selection).unwrap();
    Ok(config.path.clone())
}

/// Fetch battery pack list from crates.io
pub fn fetch_battery_pack_list(filter: Option<&str>) -> Result<Vec<BatteryPackSummary>> {
    let client = reqwest::blocking::Client::builder()
        .user_agent("cargo-bp (https://github.com/battery-pack-rs/battery-pack)")
        .build()?;

    // Build the search URL with keyword filter
    let url = match filter {
        Some(q) => format!(
            "{CRATES_IO_API}?q={}&keyword=battery-pack&per_page=50",
            urlencoding::encode(q)
        ),
        None => format!("{CRATES_IO_API}?keyword=battery-pack&per_page=50"),
    };

    let response = client
        .get(&url)
        .send()
        .context("Failed to query crates.io")?;

    if !response.status().is_success() {
        bail!(
            "Failed to list battery packs (status: {})",
            response.status()
        );
    }

    let parsed: SearchResponse = response.json().context("Failed to parse response")?;

    // Filter to only crates whose name ends with "-battery-pack"
    let battery_packs = parsed
        .crates
        .into_iter()
        .filter(|c| c.name.ends_with("-battery-pack"))
        .map(|c| BatteryPackSummary {
            short_name: short_name(&c.name).to_string(),
            name: c.name,
            version: c.max_version,
            description: c.description.unwrap_or_default(),
        })
        .collect();

    Ok(battery_packs)
}

fn print_battery_pack_list(filter: Option<&str>) -> Result<()> {
    use console::style;

    let battery_packs = fetch_battery_pack_list(filter)?;

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

/// Convert "cli-battery-pack" to "cli" for display
fn short_name(crate_name: &str) -> &str {
    crate_name
        .strip_suffix("-battery-pack")
        .unwrap_or(crate_name)
}

/// Convert "cli" to "cli-battery-pack" (adds suffix if not already present)
fn resolve_crate_name(name: &str) -> String {
    if name.ends_with("-battery-pack") {
        name.to_string()
    } else {
        format!("{}-battery-pack", name)
    }
}

/// Fetch detailed battery pack info from crates.io or a local path
pub fn fetch_battery_pack_detail(name: &str, path: Option<&str>) -> Result<BatteryPackDetail> {
    // If path is provided, use local directory
    if let Some(local_path) = path {
        return fetch_battery_pack_detail_from_path(local_path);
    }

    let crate_name = resolve_crate_name(name);

    // Look up crate info and download
    let crate_info = lookup_crate(&crate_name)?;
    let temp_dir = download_and_extract_crate(&crate_name, &crate_info.version)?;
    let crate_dir = temp_dir
        .path()
        .join(format!("{}-{}", crate_name, crate_info.version));

    // Fetch owners from crates.io
    let owners = fetch_owners(&crate_name)?;

    build_battery_pack_detail(
        &crate_dir,
        crate_name,
        crate_info.version,
        owners,
    )
}

/// Fetch detailed battery pack info from a local path
fn fetch_battery_pack_detail_from_path(path: &str) -> Result<BatteryPackDetail> {
    let crate_dir = std::path::Path::new(path);

    // Read Cargo.toml to extract name and version
    let manifest_path = crate_dir.join("Cargo.toml");
    let manifest_content = std::fs::read_to_string(&manifest_path)
        .with_context(|| format!("Failed to read {}", manifest_path.display()))?;
    let manifest: CargoManifest =
        toml::from_str(&manifest_content).with_context(|| "Failed to parse Cargo.toml")?;

    let package = manifest.package.unwrap_or_default();
    let crate_name = package
        .name
        .clone()
        .unwrap_or_else(|| "unknown".to_string());
    let version = package
        .version
        .clone()
        .unwrap_or_else(|| "0.0.0".to_string());

    build_battery_pack_detail(
        crate_dir,
        crate_name,
        version,
        Vec::new(), // No owners for local path
    )
}

/// Helper function to build BatteryPackDetail from already-resolved parameters.
/// Contains shared logic for both crates.io and local path sources.
fn build_battery_pack_detail(
    crate_dir: &Path,
    crate_name: String,
    version: String,
    owners: Vec<Owner>,
) -> Result<BatteryPackDetail> {
    // Read and parse Cargo.toml
    let manifest_path = crate_dir.join("Cargo.toml");
    let manifest_content = std::fs::read_to_string(&manifest_path)
        .with_context(|| format!("Failed to read {}", manifest_path.display()))?;
    let manifest: CargoManifest =
        toml::from_str(&manifest_content).with_context(|| "Failed to parse Cargo.toml")?;

    // Extract info
    let package = manifest.package.unwrap_or_default();
    let description = package.description.clone().unwrap_or_default();
    let repository = package.repository.clone();
    let battery = package.metadata.and_then(|m| m.battery).unwrap_or_default();

    // Split dependencies into battery packs and regular crates
    let mut extends = Vec::new();
    let mut crates = Vec::new();

    for dep_name in manifest.dependencies.keys() {
        if dep_name.ends_with("-battery-pack") {
            extends.push(short_name(dep_name).to_string());
        } else if dep_name != "battery-pack" {
            crates.push(dep_name.clone());
        }
    }

    // Fetch the GitHub repository tree to resolve paths
    let repo_tree = repository.as_ref().and_then(|r| fetch_github_tree(r));

    // Convert templates with resolved repo paths
    let templates = battery
        .templates
        .into_iter()
        .map(|(name, config)| {
            let repo_path = repo_tree
                .as_ref()
                .and_then(|tree| find_template_path(tree, &config.path));
            TemplateInfo {
                name,
                path: config.path,
                description: config.description,
                repo_path,
            }
        })
        .collect();

    // Scan examples directory
    let examples = scan_examples(crate_dir, repo_tree.as_deref());

    Ok(BatteryPackDetail {
        short_name: short_name(&crate_name).to_string(),
        name: crate_name,
        version,
        description,
        repository,
        owners: owners.into_iter().map(OwnerInfo::from).collect(),
        crates,
        extends,
        templates,
        examples,
    })
}

fn print_battery_pack_detail(name: &str, path: Option<&str>) -> Result<()> {
    use console::style;

    let detail = fetch_battery_pack_detail(name, path)?;

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
            println!("  {}", dep);
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

fn fetch_owners(crate_name: &str) -> Result<Vec<Owner>> {
    let client = reqwest::blocking::Client::builder()
        .user_agent("cargo-bp (https://github.com/battery-pack-rs/battery-pack)")
        .build()?;

    let url = format!("{}/{}/owners", CRATES_IO_API, crate_name);
    let response = client
        .get(&url)
        .send()
        .with_context(|| format!("Failed to fetch owners for '{}'", crate_name))?;

    if !response.status().is_success() {
        // Not fatal - just return empty
        return Ok(Vec::new());
    }

    let parsed: OwnersResponse = response
        .json()
        .with_context(|| "Failed to parse owners response")?;

    Ok(parsed.users)
}

/// Scan the examples directory and extract example info.
/// If a GitHub tree is provided, resolves the full repository path for each example.
fn scan_examples(crate_dir: &std::path::Path, repo_tree: Option<&[String]>) -> Vec<ExampleInfo> {
    let examples_dir = crate_dir.join("examples");
    if !examples_dir.exists() {
        return Vec::new();
    }

    let mut examples = Vec::new();

    if let Ok(entries) = std::fs::read_dir(&examples_dir) {
        for entry in entries.flatten() {
            let path = entry.path();
            if path.extension().is_some_and(|ext| ext == "rs") {
                if let Some(name) = path.file_stem().and_then(|s| s.to_str()) {
                    let description = extract_example_description(&path);
                    let repo_path = repo_tree.and_then(|tree| find_example_path(tree, name));
                    examples.push(ExampleInfo {
                        name: name.to_string(),
                        description,
                        repo_path,
                    });
                }
            }
        }
    }

    // Sort by name
    examples.sort_by(|a, b| a.name.cmp(&b.name));
    examples
}

/// Extract description from the first doc comment in an example file
fn extract_example_description(path: &std::path::Path) -> Option<String> {
    let content = std::fs::read_to_string(path).ok()?;

    // Look for //! doc comments at the start
    for line in content.lines() {
        let trimmed = line.trim();
        if trimmed.starts_with("//!") {
            let desc = trimmed.strip_prefix("//!").unwrap_or("").trim();
            if !desc.is_empty() {
                return Some(desc.to_string());
            }
        } else if !trimmed.is_empty() && !trimmed.starts_with("//") {
            // Stop at first non-comment, non-empty line
            break;
        }
    }
    None
}

/// Fetch the repository tree from GitHub API.
/// Returns a list of all file paths in the repository.
fn fetch_github_tree(repository: &str) -> Option<Vec<String>> {
    // Parse GitHub URL: https://github.com/owner/repo
    let gh_path = repository
        .strip_prefix("https://github.com/")
        .or_else(|| repository.strip_prefix("http://github.com/"))?;
    let gh_path = gh_path.strip_suffix(".git").unwrap_or(gh_path);
    let gh_path = gh_path.trim_end_matches('/');

    let client = reqwest::blocking::Client::builder()
        .user_agent("cargo-bp (https://github.com/battery-pack-rs/battery-pack)")
        .build()
        .ok()?;

    // Fetch the tree recursively using the main branch
    let url = format!(
        "https://api.github.com/repos/{}/git/trees/main?recursive=1",
        gh_path
    );

    let response = client.get(&url).send().ok()?;
    if !response.status().is_success() {
        return None;
    }

    let tree_response: GitHubTreeResponse = response.json().ok()?;

    // Extract all paths (both blobs/files and trees/directories)
    Some(
        tree_response
            .tree
            .into_iter()
            .map(|e| e.path)
            .collect(),
    )
}

/// Find the full repository path for an example file.
/// Searches the tree for a file matching "examples/{name}.rs".
fn find_example_path(tree: &[String], example_name: &str) -> Option<String> {
    let suffix = format!("examples/{}.rs", example_name);
    tree.iter()
        .find(|path| path.ends_with(&suffix))
        .cloned()
}

/// Find the full repository path for a template directory.
/// Searches the tree for a path matching "templates/{name}" or "{name}".
fn find_template_path(tree: &[String], template_path: &str) -> Option<String> {
    // The template path from config might be "templates/simple" or just the relative path
    tree.iter()
        .find(|path| path.ends_with(template_path))
        .cloned()
}
