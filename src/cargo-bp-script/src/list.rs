//! Schema for `cargo bp list --json` output.
//!
//! These types are the stable, machine-consumable representation of
//! `cargo bp list`. They are emitted by the CLI when invoked with
//! `--json` and parsed by the [`runner`](crate::runner) module.
//!
//! # Construction
//!
//! ```
//! use cargo_bp_script::{ListReport, PackSummary};
//!
//! let report = ListReport::new()
//!     .with_pack(PackSummary::new("cli", "cli-battery-pack", "0.3.0")
//!         .with_description("Opinionated CLI starter kit"));
//! assert_eq!(report.packs.len(), 1);
//! ```

use serde::{Deserialize, Serialize};

use crate::SCHEMA_VERSION;

/// Top-level report emitted by `cargo bp list --json`.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[non_exhaustive]
pub struct ListReport {
    /// Schema version. Currently always `"1"`.
    pub schema_version: String,

    /// Optional filter that was applied (if any).
    pub filter: Option<String>,

    /// Available battery packs matching the filter.
    pub packs: Vec<PackSummary>,
}

/// Summary of a single available battery pack.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[non_exhaustive]
pub struct PackSummary {
    /// Short name without the `-battery-pack` suffix, e.g. `"cli"`.
    pub short_name: String,

    /// Full crate name, e.g. `"cli-battery-pack"`.
    pub name: String,

    /// Latest version on the registry.
    pub version: String,

    /// One-line description of the battery pack.
    pub description: String,
}

// ============================================================================
// Builders
// ============================================================================

impl ListReport {
    /// Start building a report with the current [`SCHEMA_VERSION`] and
    /// no packs.
    pub fn new() -> Self {
        Self {
            schema_version: SCHEMA_VERSION.to_string(),
            filter: None,
            packs: Vec::new(),
        }
    }

    /// Set the filter that was applied.
    pub fn with_filter(mut self, filter: impl Into<String>) -> Self {
        self.filter = Some(filter.into());
        self
    }

    /// Append a single pack summary.
    pub fn with_pack(mut self, pack: PackSummary) -> Self {
        self.packs.push(pack);
        self
    }

    /// Extend the report with multiple pack summaries.
    pub fn with_packs(mut self, packs: impl IntoIterator<Item = PackSummary>) -> Self {
        self.packs.extend(packs);
        self
    }
}

impl Default for ListReport {
    fn default() -> Self {
        Self::new()
    }
}

impl PackSummary {
    /// Build a [`PackSummary`] from its required fields.
    pub fn new(
        short_name: impl Into<String>,
        name: impl Into<String>,
        version: impl Into<String>,
    ) -> Self {
        Self {
            short_name: short_name.into(),
            name: name.into(),
            version: version.into(),
            description: String::new(),
        }
    }

    /// Set the description.
    pub fn with_description(mut self, description: impl Into<String>) -> Self {
        self.description = description.into();
        self
    }
}
