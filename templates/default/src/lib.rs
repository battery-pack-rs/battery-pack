//! {{project-name}}: A curated battery pack.
//!
//! This battery pack provides a curated set of crates for ...
//!
//! # Usage
//!
//! Add to your `Cargo.toml`:
//!
//! ```toml
//! [dependencies]
//! {{project-name}} = "0.1"
//! ```
//!
//! Then use the re-exported crates:
//!
//! ```rust,ignore
//! use {{crate_name}}::*;
//! ```

include!(concat!(env!("OUT_DIR"), "/facade.rs"));
