//! battery-pack: Framework for building battery packs.
//!
//! Battery packs are curated collections of crates that work well together,
//! re-exported through a single facade for easy consumption.
//!
//! # For Battery Pack Authors
//!
//! Use `cargo battery new my-pack` to create a new battery pack, or add the
//! following to an existing crate:
//!
//! **Cargo.toml:**
//! ```toml
//! [package.metadata.battery]
//! schema_version = 1
//!
//! [build-dependencies]
//! battery-pack = "0.1"
//! ```
//!
//! **build.rs:**
//! ```rust,ignore
//! fn main() -> Result<(), battery_pack::build::Error> {
//!     battery_pack::build::generate_facade()
//! }
//! ```
//!
//! **lib.rs:**
//! ```rust,ignore
//! include!(concat!(env!("OUT_DIR"), "/facade.rs"));
//! ```

pub use battery_pack_build as build;
