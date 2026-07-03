//! Schema for `cargo bp status --json` output.
//!
//! These types are the stable, machine-consumable representation of
//! `cargo bp status`. They are emitted by the CLI when invoked with
//! `--json` and parsed by the [`runner`](crate::runner) module.
//!
//! # Construction
//!
//! Each type follows the same `new(required)` + chainable `with_*`
//! setters pattern. Required fields are positional arguments to
//! `new`; optional / collection-shaped fields are populated via
//! `with_*` methods. New fields added in future schema versions get
//! a new `with_*` setter rather than changing `new()`'s signature,
//! so producers don't break.
//!
//! ```
//! use cargo_bp_script::{
//!     DependencyWarning, InstalledPackStatus, ProjectInfo, StatusReport,
//! };
//!
//! let report = StatusReport::new(ProjectInfo::new("/path/to/Cargo.toml"))
//!     .with_pack(
//!         InstalledPackStatus::new("cli", "cli-battery-pack", "0.3.0")
//!             .with_active_feature("default")
//!             .with_warning(DependencyWarning::new("clap", "4.4.0", "4.5.0")),
//!     );
//! assert_eq!(report.packs.len(), 1);
//! ```

use serde::{Deserialize, Serialize};
use std::path::PathBuf;

/// Current JSON schema version emitted by `cargo bp status --json`.
///
/// Bumped on any breaking change to the schema. Consumers may use
/// [`StatusReport::schema_version`] to detect the version they
/// received and adapt accordingly.
pub const SCHEMA_VERSION: &str = "1";

/// Top-level report emitted by `cargo bp status --json`.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[non_exhaustive]
pub struct StatusReport {
    /// Schema version. Currently always `"1"`.
    pub schema_version: String,

    /// Information about the project that was inspected.
    pub project: ProjectInfo,

    /// All installed battery packs, in stable order (sorted by short name).
    pub packs: Vec<InstalledPackStatus>,
}

/// Information about the project whose status was inspected.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[non_exhaustive]
pub struct ProjectInfo {
    /// Path to the `Cargo.toml` that was inspected.
    pub manifest_path: PathBuf,
}

/// Status of a single installed battery pack.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[non_exhaustive]
pub struct InstalledPackStatus {
    /// Short name without the `-battery-pack` suffix, e.g. `"cli"`.
    pub short_name: String,

    /// Full crate name, e.g. `"cli-battery-pack"`.
    pub name: String,

    /// Registered version of the battery pack as recorded in the
    /// project's metadata.
    pub version: String,

    /// Active features for this pack in the user's project.
    ///
    /// Sorted alphabetically.
    pub active_features: Vec<String>,

    /// Templates that have been applied from this battery pack.
    #[serde(default, skip_serializing_if = "Vec::is_empty")]
    pub applied_templates: Vec<String>,

    /// Per-dependency warnings for this pack — each entry indicates
    /// a dependency whose user-side version is older than what the
    /// battery pack recommends.
    ///
    /// An empty vector means all dependencies are up to date.
    pub warnings: Vec<DependencyWarning>,
}

/// A single version-drift warning for a battery pack dependency.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[non_exhaustive]
pub struct DependencyWarning {
    /// Crate name (e.g. `"clap"`).
    pub crate_name: String,
    /// Current version pinned in the user's `Cargo.toml` (or workspace).
    pub current_version: String,
    /// Version recommended by the battery pack.
    pub recommended_version: String,
}

// ============================================================================
// Builders
// ============================================================================
//
// All schema types follow the same shape:
//   - `new(required_fields...)` for the smallest valid value.
//   - `with_<field>` for adding a single item to a collection-shaped field.
//   - `with_<plural>` for extending a collection with an iterator.
// New fields added in future schema versions gain a new `with_*` method
// instead of changing `new()`'s signature, so producers stay forward-compatible.

impl StatusReport {
    /// Start building a report with the current [`SCHEMA_VERSION`] and
    /// no packs. Add packs with [`with_pack`](Self::with_pack) /
    /// [`with_packs`](Self::with_packs).
    pub fn new(project: ProjectInfo) -> Self {
        Self {
            schema_version: SCHEMA_VERSION.to_string(),
            project,
            packs: Vec::new(),
        }
    }

    /// Append a single installed-pack status.
    pub fn with_pack(mut self, pack: InstalledPackStatus) -> Self {
        self.packs.push(pack);
        self
    }

    /// Extend the report with multiple installed-pack statuses.
    pub fn with_packs(mut self, packs: impl IntoIterator<Item = InstalledPackStatus>) -> Self {
        self.packs.extend(packs);
        self
    }
}

impl ProjectInfo {
    /// Build a [`ProjectInfo`] from the inspected manifest path.
    pub fn new(manifest_path: impl Into<PathBuf>) -> Self {
        Self {
            manifest_path: manifest_path.into(),
        }
    }
}

impl InstalledPackStatus {
    /// Start building an installed-pack status with no active features
    /// or warnings. Use [`with_active_feature`](Self::with_active_feature) /
    /// [`with_warning`](Self::with_warning) (and their plural variants)
    /// to populate the rest.
    pub fn new(
        short_name: impl Into<String>,
        name: impl Into<String>,
        version: impl Into<String>,
    ) -> Self {
        Self {
            short_name: short_name.into(),
            name: name.into(),
            version: version.into(),
            active_features: Vec::new(),
            applied_templates: Vec::new(),
            warnings: Vec::new(),
        }
    }

    /// Append a single active feature.
    pub fn with_active_feature(mut self, feature: impl Into<String>) -> Self {
        self.active_features.push(feature.into());
        self
    }

    /// Extend the active features list from any iterable of string-likes.
    pub fn with_active_features<I, S>(mut self, features: I) -> Self
    where
        I: IntoIterator<Item = S>,
        S: Into<String>,
    {
        self.active_features
            .extend(features.into_iter().map(Into::into));
        self
    }

    /// Append applied template names.
    pub fn with_applied_templates<I, S>(mut self, templates: I) -> Self
    where
        I: IntoIterator<Item = S>,
        S: Into<String>,
    {
        self.applied_templates
            .extend(templates.into_iter().map(Into::into));
        self
    }

    /// Append a single dependency warning.
    pub fn with_warning(mut self, warning: DependencyWarning) -> Self {
        self.warnings.push(warning);
        self
    }

    /// Extend the warnings list with multiple dependency warnings.
    pub fn with_warnings(mut self, warnings: impl IntoIterator<Item = DependencyWarning>) -> Self {
        self.warnings.extend(warnings);
        self
    }
}

impl DependencyWarning {
    /// Build a [`DependencyWarning`] from its current required fields.
    pub fn new(
        crate_name: impl Into<String>,
        current_version: impl Into<String>,
        recommended_version: impl Into<String>,
    ) -> Self {
        Self {
            crate_name: crate_name.into(),
            current_version: current_version.into(),
            recommended_version: recommended_version.into(),
        }
    }
}
