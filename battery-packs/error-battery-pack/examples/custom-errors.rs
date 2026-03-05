//! Defining custom error types with thiserror.
//!
//! Run with: `cargo run --example custom-errors -p error-battery-pack`

use anyhow::Result;
use thiserror::Error;

/// A custom error type for a config parser.
///
/// Use thiserror in library code where callers need to match on
/// specific error variants. Use anyhow in application code where
/// you just need to propagate errors with context.
#[derive(Debug, Error)]
enum ConfigError {
    #[error("failed to read config file")]
    Io(#[from] std::io::Error),

    #[error("invalid port number: {0}")]
    InvalidPort(#[from] std::num::ParseIntError),

    #[error("port {0} is out of range (must be 1024..65535)")]
    PortOutOfRange(u16),
}

/// Parse a port number from a config file.
///
/// Returns a typed `ConfigError` so callers can match on the variant.
fn parse_port(path: &str) -> std::result::Result<u16, ConfigError> {
    let content = std::fs::read_to_string(path)?; // io::Error -> ConfigError::Io via #[from]
    let port: u16 = content.trim().parse()?; // ParseIntError -> ConfigError::InvalidPort via #[from]

    if !(1024..=65535).contains(&port) {
        return Err(ConfigError::PortOutOfRange(port));
    }

    Ok(port)
}

fn main() -> Result<()> {
    // thiserror errors convert into anyhow automatically
    let port = parse_port("config.txt")?;
    println!("Server starting on port {port}");
    Ok(())
}
