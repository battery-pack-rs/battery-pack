//! cli-battery-pack CLI — initialize projects, view docs, etc.

use clap::{Parser, Subcommand};
use cli_battery_pack::{anyhow, clap};

#[derive(Parser)]
#[command(name = "cli-battery-pack")]
#[command(about = "CLI Battery Pack - curated crates for building CLIs")]
struct Cli {
    #[command(subcommand)]
    command: Commands,
}

#[derive(Subcommand)]
enum Commands {
    /// Initialize a new CLI project from a template
    Init {
        /// Project name
        name: String,
        /// Template to use (simple, subcmds)
        #[arg(short, long, default_value = "simple")]
        template: String,
    },
    /// List available templates
    Templates,
    /// Show a skill (guidance for humans and AI)
    Skill {
        /// Skill name
        name: String,
    },
    /// List available skills
    Skills,
}

fn main() -> anyhow::Result<()> {
    let cli = Cli::parse();

    match cli.command {
        Commands::Init { name, template } => {
            println!(
                "TODO: Initialize project '{}' with template '{}'",
                name, template
            );
            // Will use cargo-generate here
        }
        Commands::Templates => {
            println!("Available templates:");
            println!("  simple   - Minimal CLI with argument parsing");
            println!("  subcmds  - CLI with subcommands");
        }
        Commands::Skill { name } => {
            println!("TODO: Show skill '{}'", name);
        }
        Commands::Skills => {
            println!("TODO: List available skills");
        }
    }

    Ok(())
}
