//! CLI for battery-pack: create and manage battery packs.

use anyhow::{bail, Context, Result};
use cargo_generate::{GenerateArgs, TemplatePath, Vcs};
use clap::{Parser, Subcommand};
use serde::Deserialize;
use std::collections::BTreeMap;

const CRATES_IO_API: &str = "https://crates.io/api/v1/crates";

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
        /// Name of the battery pack to use as template source
        battery_pack: String,

        /// Name for the new project (prompted interactively if not provided)
        #[arg(long, short = 'n')]
        name: Option<String>,

        /// Which template to use (defaults to 'default', or prompts if multiple available)
        #[arg(long, short = 't')]
        template: Option<String>,

        /// Override the git repository (for development/testing)
        #[arg(long, hide = true)]
        git: Option<String>,

        /// Override with a local path (for development/testing)
        #[arg(long, hide = true)]
        path: Option<String>,
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
                git,
                path,
            } => new_from_battery_pack(&battery_pack, name, template, git, path),
        },
    }
}

// ============================================================================
// crates.io API types
// ============================================================================

#[derive(Deserialize)]
struct CratesIoResponse {
    #[serde(rename = "crate")]
    krate: CrateInfo,
}

#[derive(Deserialize)]
struct CrateInfo {
    repository: Option<String>,
}

// ============================================================================
// Battery pack metadata types
// ============================================================================

#[derive(Deserialize, Default)]
struct CargoManifest {
    package: Option<PackageSection>,
}

#[derive(Deserialize, Default)]
struct PackageSection {
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
// Implementation
// ============================================================================

fn new_from_battery_pack(
    battery_pack: &str,
    name: Option<String>,
    template: Option<String>,
    git_override: Option<String>,
    path_override: Option<String>,
) -> Result<()> {
    // Get the repository URL
    let repo_url = if let Some(path) = path_override {
        return generate_from_local(&path, battery_pack, name, template);
    } else if let Some(git) = git_override {
        git
    } else {
        get_repository_url(battery_pack)?
    };

    // Fetch the Cargo.toml from the repo to get template metadata
    let templates = fetch_template_metadata(&repo_url, battery_pack)?;

    // Resolve which template to use
    let template_path = resolve_template(&templates, template.as_deref())?;

    // Generate the project
    let args = GenerateArgs {
        template_path: TemplatePath {
            git: Some(repo_url),
            auto_path: Some(template_path),
            ..Default::default()
        },
        name,
        vcs: Some(Vcs::Git),
        ..Default::default()
    };

    cargo_generate::generate(args)?;

    Ok(())
}

fn generate_from_local(
    local_path: &str,
    battery_pack: &str,
    name: Option<String>,
    template: Option<String>,
) -> Result<()> {
    // Read local Cargo.toml
    let manifest_path = format!("{}/Cargo.toml", local_path);
    let manifest_content = std::fs::read_to_string(&manifest_path)
        .with_context(|| format!("Failed to read {}", manifest_path))?;

    let templates = parse_template_metadata(&manifest_content, battery_pack)?;
    let template_path = resolve_template(&templates, template.as_deref())?;

    let args = GenerateArgs {
        template_path: TemplatePath {
            path: Some(local_path.to_string()),
            auto_path: Some(template_path),
            ..Default::default()
        },
        name,
        vcs: Some(Vcs::Git),
        ..Default::default()
    };

    cargo_generate::generate(args)?;

    Ok(())
}

fn get_repository_url(crate_name: &str) -> Result<String> {
    let url = format!("{}/{}", CRATES_IO_API, crate_name);

    let client = reqwest::blocking::Client::builder()
        .user_agent("cargo-bp (https://github.com/battery-pack-rs/battery-pack)")
        .build()?;

    let response: CratesIoResponse = client
        .get(&url)
        .send()
        .with_context(|| format!("Failed to fetch crate info for '{}'", crate_name))?
        .json()
        .with_context(|| format!("Failed to parse crate info for '{}'", crate_name))?;

    response
        .krate
        .repository
        .ok_or_else(|| anyhow::anyhow!("Crate '{}' has no repository URL defined", crate_name))
}

fn fetch_template_metadata(
    repo_url: &str,
    crate_name: &str,
) -> Result<BTreeMap<String, TemplateConfig>> {
    // Convert GitHub repo URL to raw Cargo.toml URL
    let raw_url = if repo_url.contains("github.com") {
        repo_url
            .replace("github.com", "raw.githubusercontent.com")
            .trim_end_matches(".git")
            .to_string()
            + "/HEAD/Cargo.toml"
    } else {
        bail!(
            "Unsupported repository host. Currently only GitHub is supported: {}",
            repo_url
        );
    };

    let client = reqwest::blocking::Client::builder()
        .user_agent("cargo-bp (https://github.com/battery-pack-rs/battery-pack)")
        .build()?;

    let manifest_content = client
        .get(&raw_url)
        .send()
        .with_context(|| format!("Failed to fetch Cargo.toml from {}", raw_url))?
        .text()
        .with_context(|| "Failed to read Cargo.toml content")?;

    parse_template_metadata(&manifest_content, crate_name)
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
