//! Schema and process runner for scripting `cargo bp` commands.
//!
//! See the [crate README](https://crates.io/crates/cargo-bp-script)
//! for a high-level overview.
//!
//! # Quick start
//!
//! Spawn `cargo bp status --json` and parse the result:
//!
//! ```no_run
//! use cargo_bp_script::StatusCommand;
//!
//! let report = StatusCommand::new().run()?;
//! for pack in &report.packs {
//!     println!("{} {}", pack.short_name, pack.version);
//! }
//! # Ok::<(), cargo_bp_script::Error>(())
//! ```
//!
//! # Construction
//!
//! All schema types use a `new(required)` + chainable `with_*` setters
//! pattern, which lets the schema grow without breaking producers:
//!
//! ```
//! use cargo_bp_script::{
//!     DependencyWarning, InstalledPackStatus, ProjectInfo, StatusReport,
//! };
//!
//! let report = StatusReport::new(ProjectInfo::new("Cargo.toml"))
//!     .with_pack(
//!         InstalledPackStatus::new("cli", "cli-battery-pack", "0.3.0")
//!             .with_active_feature("default")
//!             .with_warning(DependencyWarning::new("clap", "4.4.0", "4.5.0")),
//!     );
//! assert_eq!(report.packs.len(), 1);
//! ```

#![deny(missing_docs)]

pub mod runner;
pub mod status;

// Re-export the most commonly used items at the crate root for
// ergonomic access. The full API stays addressable via the modules.
pub use runner::{Error, StatusCommand, parse_status};
pub use status::{
    DependencyWarning, InstalledPackStatus, ProjectInfo, SCHEMA_VERSION, StatusReport,
};

#[cfg(test)]
mod tests {
    use super::*;

    /// JSON serialisation round-trips through `parse_status` when reports
    /// are built via the chained-builder API.
    #[test]
    fn round_trip_status_report_via_builders() {
        let report = StatusReport::new(ProjectInfo::new("/tmp/proj/Cargo.toml"))
            .with_pack(
                InstalledPackStatus::new("cli", "cli-battery-pack", "0.3.0")
                    .with_active_feature("default")
                    .with_warning(DependencyWarning::new("clap", "4.4.0", "4.5.0")),
            )
            .with_pack(InstalledPackStatus::new(
                "error",
                "error-battery-pack",
                "0.2.0",
            ));

        let bytes = serde_json::to_vec(&report).expect("serialize");
        let parsed = parse_status(&bytes).expect("parse_status");
        assert_eq!(parsed, report);
    }

    /// `with_packs` extends — it does not replace.
    #[test]
    fn with_packs_extends() {
        let pack_a = InstalledPackStatus::new("a", "a-battery-pack", "0.1.0");
        let pack_b = InstalledPackStatus::new("b", "b-battery-pack", "0.1.0");
        let pack_c = InstalledPackStatus::new("c", "c-battery-pack", "0.1.0");

        let report = StatusReport::new(ProjectInfo::new("Cargo.toml"))
            .with_pack(pack_a.clone())
            .with_packs([pack_b.clone(), pack_c.clone()]);

        assert_eq!(report.packs, vec![pack_a, pack_b, pack_c]);
    }

    /// `with_active_features` accepts any string-likes (and extends).
    #[test]
    fn with_active_features_accepts_string_likes() {
        let pack = InstalledPackStatus::new("cli", "cli-battery-pack", "0.3.0")
            .with_active_feature("default")
            .with_active_features(["fancy", "indicators"])
            .with_active_features(vec!["color".to_string()]);

        assert_eq!(
            pack.active_features,
            vec!["default", "fancy", "indicators", "color"],
        );
    }

    /// Fresh reports advertise the current `SCHEMA_VERSION`.
    #[test]
    fn schema_version_is_set() {
        let report = StatusReport::new(ProjectInfo::new("Cargo.toml"));
        assert_eq!(report.schema_version, SCHEMA_VERSION);
        assert!(report.packs.is_empty());
    }

    /// `parse_status` surfaces malformed input as `Error::Parse`.
    #[test]
    fn parse_status_rejects_garbage() {
        let err = parse_status(b"not json").unwrap_err();
        assert!(matches!(err, Error::Parse { .. }), "got {err:?}");
    }
}
