//! Basic error handling with anyhow.
//!
//! Run with: `cargo run --example basic -p error-battery-pack`

use anyhow::{Context, Result};

fn read_config() -> Result<u16> {
    let content = std::fs::read_to_string("config.txt").context("failed to read config.txt")?;

    let port: u16 = content
        .trim()
        .parse()
        .context("config.txt must contain a valid port number")?;

    Ok(port)
}

fn main() -> Result<()> {
    match read_config() {
        Ok(port) => println!("Server starting on port {port}"),
        Err(e) => {
            // anyhow formats the full error chain with {:#}
            println!("Error: {e:#}");
        }
    }

    Ok(())
}
