//! battery-pack: Framework for building and using battery packs.
//!
//! Battery packs are curated collections of crates that work well together.
//! The CLI (`cargo bp`) syncs real dependencies into your Cargo.toml,
//! and this library provides build-time documentation generation and
//! drift validation.
//!
//! # For Battery Pack Authors
//!
//! Your `build.rs` generates documentation:
//!
//! ```rust,ignore
//! fn main() {
//!     battery_pack::build::generate_docs().unwrap();
//! }
//! ```
//!
//! Your `lib.rs` includes the generated docs:
//!
//! ```rust,ignore
//! #![doc = include_str!(concat!(env!("OUT_DIR"), "/docs.md"))]
//! ```

pub use bphelper_manifest::{BatteryPackSpec, CrateSpec, DepKind};

/// Build-time documentation generation.
///
/// Use from your battery pack's `build.rs`:
///
/// ```rust,ignore
/// fn main() {
///     battery_pack::build::generate_docs().unwrap();
/// }
/// ```
///
/// See the [docgen spec](https://battery-pack-rs.github.io/battery-pack/spec/docgen.html)
/// for details on templates and helpers.
#[cfg(feature = "build")]
pub mod build {
    pub use bphelper_build::{
        CrateEntry, DocsContext, Error, FeatureEntry, PackageInfo, build_context, generate_docs,
        render_docs,
    };
}

/// Test utilities for battery pack authors.
///
/// In your `src/lib.rs`:
///
/// ```rust,ignore
/// #[cfg(test)]
/// mod tests {
///     #[test]
///     fn validate_templates() {
///         battery_pack::testing::validate_templates(env!("CARGO_MANIFEST_DIR")).unwrap();
///     }
/// }
/// ```
#[cfg(feature = "cli")]
pub mod testing {
    pub use bphelper_cli::validate_templates;
}

#[cfg(test)]
mod tests {
    #[test]
    #[cfg(feature = "cli")]
    fn validate_templates() {
        battery_pack::testing::validate_templates(env!("CARGO_MANIFEST_DIR")).unwrap();
    }
}
