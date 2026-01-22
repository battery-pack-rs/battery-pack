//! # cli-bp: CLI Battery Pack
//!
//! A curated collection of crates for building command-line applications in Rust.
//!
//! ## Included crates
//!
//! - **clap** — argument parsing with derive macros
//! - **anyhow** — easy error handling for applications
//! - **thiserror** — derive macros for custom error types
//! - **tracing** + **tracing-subscriber** — structured logging
//! - **console** — terminal styling and colors
//! - **indicatif** — progress bars and spinners
//! - **dialoguer** — interactive prompts
//!
//! ## Usage
//!
//! ```rust,ignore
//! use cli_bp::{clap::Parser, anyhow::Result};
//!
//! #[derive(Parser)]
//! struct Args {
//!     name: String,
//! }
//!
//! fn main() -> Result<()> {
//!     let args = Args::parse();
//!     println!("Hello, {}!", args.name);
//!     Ok(())
//! }
//! ```

// Generated facade - re-exports curated crates based on Cargo.toml metadata
include!(concat!(env!("OUT_DIR"), "/facade.rs"));
