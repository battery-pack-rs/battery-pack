//! A minimal ripgrep-like tool demonstrating cli-battery-pack.
//!
//! Run with: `cargo run --example mini-grep -- <pattern> [path]`

use std::fs::File;
use std::io::{BufRead, BufReader};
use std::path::PathBuf;

use cli_battery_pack::anyhow::{Context, Result};
use cli_battery_pack::clap::Parser;
use cli_battery_pack::console::style;
use cli_battery_pack::ignore::WalkBuilder;
use cli_battery_pack::regex::Regex;

/// A simple grep tool that respects .gitignore
#[derive(Parser)]
#[command(name = "mini-grep", version, about)]
struct Args {
    /// The pattern to search for (regex)
    pattern: String,

    /// Path to search (defaults to current directory)
    #[arg(default_value = ".")]
    path: PathBuf,

    /// Show line numbers
    #[arg(short = 'n', long)]
    line_numbers: bool,

    /// Case-insensitive search
    #[arg(short = 'i', long)]
    ignore_case: bool,
}

fn main() -> Result<()> {
    let args = Args::parse();

    // Build regex with case-insensitivity if requested
    let pattern = if args.ignore_case {
        format!("(?i){}", args.pattern)
    } else {
        args.pattern.clone()
    };
    let regex = Regex::new(&pattern).with_context(|| format!("Invalid regex: {}", args.pattern))?;

    // Walk directory, respecting .gitignore
    for entry in WalkBuilder::new(&args.path).build() {
        let entry = entry?;

        // Skip directories
        if entry.file_type().map(|t| t.is_dir()).unwrap_or(false) {
            continue;
        }

        let path = entry.path();

        // Try to open and search the file
        let file = match File::open(path) {
            Ok(f) => f,
            Err(_) => continue, // Skip files we can't read
        };

        let reader = BufReader::new(file);
        for (line_num, line) in reader.lines().enumerate() {
            let line = match line {
                Ok(l) => l,
                Err(_) => continue, // Skip binary/unreadable lines
            };

            if let Some(m) = regex.find(&line) {
                // Format: path:line:content with highlighted match
                let path_str = style(path.display()).cyan();
                let prefix = if args.line_numbers {
                    format!("{}:{}:", path_str, style(line_num + 1).green())
                } else {
                    format!("{}:", path_str)
                };

                // Highlight the match
                let before = &line[..m.start()];
                let matched = style(&line[m.start()..m.end()]).red().bold();
                let after = &line[m.end()..];

                println!("{}{}{}{}", prefix, before, matched, after);
            }
        }
    }

    Ok(())
}
