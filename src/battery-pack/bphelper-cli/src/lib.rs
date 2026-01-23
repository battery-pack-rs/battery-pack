//! CLI for battery-pack: create and manage battery packs.

use anyhow::{bail, Context, Result};
use cargo_generate::{GenerateArgs, TemplatePath, Vcs};
use clap::{Parser, Subcommand};
use flate2::read::GzDecoder;
use serde::Deserialize;
use std::collections::BTreeMap;
use std::path::Path;
use tar::Archive;

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

    /// Search for battery packs on crates.io
    Search {
        /// Search query (omit to list all battery packs)
        query: Option<String>,
    },

    /// Show detailed information about a battery pack
    Show {
        /// Name of the battery pack (e.g., "cli" resolves to "cli-battery-pack")
        battery_pack: String,
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
            BpCommands::Search { query } => search_battery_packs(query.as_deref()),
            BpCommands::Show { battery_pack } => show_battery_pack(&battery_pack),
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
    description: Option<String>,
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

#[derive(Deserialize)]
struct Owner {
    login: String,
    name: Option<String>,
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
    let crate_dir = temp_dir.path().join(format!("{}-{}", crate_name, crate_info.version));

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
fn download_and_extract_crate(
    crate_name: &str,
    version: &str,
) -> Result<tempfile::TempDir> {
    let client = reqwest::blocking::Client::builder()
        .user_agent("cargo-bp (https://github.com/battery-pack-rs/battery-pack)")
        .build()?;

    // Download from CDN: https://static.crates.io/crates/{name}/{name}-{version}.crate
    let url = format!("{}/{}/{}-{}.crate", CRATES_IO_CDN, crate_name, crate_name, version);

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
                // Multiple templates, no default - list them
                println!("Available templates:");
                for (name, config) in templates {
                    if let Some(desc) = &config.description {
                        println!("  {} - {}", name, desc);
                    } else {
                        println!("  {}", name);
                    }
                }
                bail!("Multiple templates available. Please specify one with --template <name>");
            }
        }
    }
}

fn search_battery_packs(query: Option<&str>) -> Result<()> {
    use console::style;

    let client = reqwest::blocking::Client::builder()
        .user_agent("cargo-bp (https://github.com/battery-pack-rs/battery-pack)")
        .build()?;

    // Build the search URL with keyword filter
    let url = match query {
        Some(q) => format!(
            "{CRATES_IO_API}?q={}&keyword=battery-pack&per_page=50",
            urlencoding::encode(q)
        ),
        None => format!("{CRATES_IO_API}?keyword=battery-pack&per_page=50"),
    };

    let response = client
        .get(&url)
        .send()
        .context("Failed to search crates.io")?;

    if !response.status().is_success() {
        bail!("Search failed (status: {})", response.status());
    }

    let parsed: SearchResponse = response.json().context("Failed to parse search response")?;

    // Filter to only crates whose name ends with "-battery-pack"
    let battery_packs: Vec<_> = parsed
        .crates
        .into_iter()
        .filter(|c| c.name.ends_with("-battery-pack"))
        .collect();

    if battery_packs.is_empty() {
        match query {
            Some(q) => println!("No battery packs found matching '{}'", q),
            None => println!("No battery packs found"),
        }
        return Ok(());
    }

    // Find the longest name for alignment
    let max_name_len = battery_packs
        .iter()
        .map(|c| short_name(&c.name).len())
        .max()
        .unwrap_or(0);

    let max_version_len = battery_packs
        .iter()
        .map(|c| c.max_version.len())
        .max()
        .unwrap_or(0);

    println!();
    for krate in &battery_packs {
        let short = short_name(&krate.name);
        let desc = krate
            .description
            .as_deref()
            .unwrap_or("")
            .lines()
            .next()
            .unwrap_or("");

        // Pad strings manually, then apply colors (ANSI codes break width formatting)
        let name_padded = format!("{:<width$}", short, width = max_name_len);
        let ver_padded = format!("{:<width$}", krate.max_version, width = max_version_len);

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

fn show_battery_pack(name: &str) -> Result<()> {
    use console::style;

    let crate_name = resolve_crate_name(name);
    let short = short_name(&crate_name);

    // Look up crate info and download
    let crate_info = lookup_crate(&crate_name)?;
    let temp_dir = download_and_extract_crate(&crate_name, &crate_info.version)?;
    let crate_dir = temp_dir
        .path()
        .join(format!("{}-{}", crate_name, crate_info.version));

    // Read and parse Cargo.toml
    let manifest_path = crate_dir.join("Cargo.toml");
    let manifest_content = std::fs::read_to_string(&manifest_path)
        .with_context(|| format!("Failed to read {}", manifest_path.display()))?;
    let manifest: CargoManifest =
        toml::from_str(&manifest_content).with_context(|| "Failed to parse Cargo.toml")?;

    // Fetch owners from crates.io
    let owners = fetch_owners(&crate_name)?;

    // Extract info
    let package = manifest.package.unwrap_or_default();
    let description = package.description.as_deref().unwrap_or("");
    let battery = package
        .metadata
        .and_then(|m| m.battery)
        .unwrap_or_default();

    // Header
    println!();
    println!(
        "{} {}",
        style(&crate_name).green().bold(),
        style(&crate_info.version).dim()
    );
    if !description.is_empty() {
        println!("{}", description);
    }

    // Authors
    if !owners.is_empty() {
        println!();
        println!("{}", style("Authors:").bold());
        for owner in &owners {
            if let Some(name) = &owner.name {
                println!("  {} ({})", name, owner.login);
            } else {
                println!("  {}", owner.login);
            }
        }
    }

    // Dependencies (split into battery packs and regular crates)
    let mut extends: Vec<&str> = Vec::new();
    let mut crates: Vec<&str> = Vec::new();

    for dep_name in manifest.dependencies.keys() {
        if dep_name.ends_with("-battery-pack") {
            extends.push(dep_name);
        } else if dep_name != "battery-pack" {
            crates.push(dep_name);
        }
    }

    if !crates.is_empty() {
        println!();
        println!("{}", style("Crates:").bold());
        for dep in &crates {
            println!("  {}", dep);
        }
    }

    if !extends.is_empty() {
        println!();
        println!("{}", style("Extends:").bold());
        for dep in &extends {
            println!("  {}", short_name(dep));
        }
    }

    // Templates
    if !battery.templates.is_empty() {
        println!();
        println!("{}", style("Templates:").bold());
        let max_name_len = battery.templates.keys().map(|k| k.len()).max().unwrap_or(0);
        for (name, config) in &battery.templates {
            let name_padded = format!("{:<width$}", name, width = max_name_len);
            if let Some(desc) = &config.description {
                println!("  {}  {}", style(name_padded).cyan(), desc);
            } else {
                println!("  {}", style(name_padded).cyan());
            }
        }
    }

    // Install hints
    println!();
    println!("{}", style("Install:").bold());
    println!("  cargo bp add {}", short);
    println!("  cargo bp new {}", short);
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
