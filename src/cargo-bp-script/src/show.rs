//! Schema for `cargo bp show --json` output.
//!
//! These types are the stable, machine-consumable representation of
//! `cargo bp show <pack>`. They are emitted by the CLI when invoked
//! with `--json` and parsed by the [`runner`](crate::runner) module.
//!
//! # Construction
//!
//! ```
//! use cargo_bp_script::{ShowReport, OwnerInfo, TemplateInfo, ExampleInfo, FeatureInfo};
//!
//! let report = ShowReport::new("cli", "cli-battery-pack", "0.3.0")
//!     .with_description("Opinionated CLI starter kit")
//!     .with_crate("clap")
//!     .with_crate("indicatif")
//!     .with_owner(OwnerInfo::new("rustacean").with_name("Ferris"))
//!     .with_feature(FeatureInfo::new("fancy").with_crate("dialoguer"))
//!     .with_template(TemplateInfo::new("default").with_description("Minimal CLI app"))
//!     .with_example(ExampleInfo::new("mini-grep").with_description("Grep clone"));
//! assert_eq!(report.crates.len(), 2);
//! ```

use serde::{Deserialize, Serialize};

use crate::SCHEMA_VERSION;

/// Top-level report emitted by `cargo bp show --json`.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[non_exhaustive]
pub struct ShowReport {
    /// Schema version. Currently always `"1"`.
    pub schema_version: String,

    /// Short name without the `-battery-pack` suffix, e.g. `"cli"`.
    pub short_name: String,

    /// Full crate name, e.g. `"cli-battery-pack"`.
    pub name: String,

    /// Version of the battery pack.
    pub version: String,

    /// Description of the battery pack.
    pub description: String,

    /// Repository URL (if known).
    pub repository: Option<String>,

    /// Pack authors/owners.
    pub owners: Vec<OwnerInfo>,

    /// Default crates provided by this battery pack.
    pub crates: Vec<String>,

    /// Battery packs this one extends.
    pub extends: Vec<String>,

    /// Named features and the crates they include.
    pub features: Vec<FeatureInfo>,

    /// Categories declared by this battery pack, with their members.
    #[serde(default, skip_serializing_if = "Vec::is_empty")]
    pub categories: Vec<CategoryInfo>,

    /// Available templates.
    pub templates: Vec<TemplateInfo>,

    /// Available examples.
    pub examples: Vec<ExampleInfo>,

    /// Crates from this pack that are currently installed in the user's
    /// project. Empty when not run inside a project or when the pack
    /// isn't installed.
    #[serde(default, skip_serializing_if = "Vec::is_empty")]
    pub installed_crates: Vec<String>,

    /// Features from this pack that are currently active in the user's
    /// project. Empty when not run inside a project or when the pack
    /// isn't installed.
    #[serde(default, skip_serializing_if = "Vec::is_empty")]
    pub active_features: Vec<String>,
}

/// Information about a battery pack owner/author.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[non_exhaustive]
pub struct OwnerInfo {
    /// Login/username.
    pub login: String,

    /// Display name (if known).
    pub name: Option<String>,
}

/// A named feature and the crates it provides.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[non_exhaustive]
pub struct FeatureInfo {
    /// Feature name (e.g. `"fancy"`).
    pub name: String,

    /// Crates activated by this feature.
    pub crates: Vec<String>,
}

/// How many members of a category may be selected.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize, Default)]
#[serde(rename_all = "kebab-case")]
pub enum PickModeInfo {
    /// Any number of members may be selected.
    #[default]
    Any,
    /// At most one member may be selected.
    AtMostOne,
}

/// A declared category and its member items.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[non_exhaustive]
pub struct CategoryInfo {
    /// Category key (e.g. `"hal"`).
    pub name: String,

    /// Display title (falls back to the key when unset).
    pub title: Option<String>,

    /// Explanatory description.
    pub description: Option<String>,

    /// Selection constraint.
    pub pick: PickModeInfo,

    /// Member item names (features, dependencies, and templates).
    pub members: Vec<String>,
}

/// Information about an available template.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[non_exhaustive]
pub struct TemplateInfo {
    /// Template name.
    pub name: String,

    /// Short description (if available).
    pub description: Option<String>,
}

/// Information about an available example.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[non_exhaustive]
pub struct ExampleInfo {
    /// Example name.
    pub name: String,

    /// Short description (if available).
    pub description: Option<String>,
}

// ============================================================================
// Builders
// ============================================================================

impl ShowReport {
    /// Start building a report with the current [`SCHEMA_VERSION`].
    pub fn new(
        short_name: impl Into<String>,
        name: impl Into<String>,
        version: impl Into<String>,
    ) -> Self {
        Self {
            schema_version: SCHEMA_VERSION.to_string(),
            short_name: short_name.into(),
            name: name.into(),
            version: version.into(),
            description: String::new(),
            repository: None,
            owners: Vec::new(),
            crates: Vec::new(),
            extends: Vec::new(),
            features: Vec::new(),
            categories: Vec::new(),
            templates: Vec::new(),
            examples: Vec::new(),
            installed_crates: Vec::new(),
            active_features: Vec::new(),
        }
    }

    /// Set the description.
    pub fn with_description(mut self, description: impl Into<String>) -> Self {
        self.description = description.into();
        self
    }

    /// Set the repository URL.
    pub fn with_repository(mut self, repository: impl Into<String>) -> Self {
        self.repository = Some(repository.into());
        self
    }

    /// Append a single owner.
    pub fn with_owner(mut self, owner: OwnerInfo) -> Self {
        self.owners.push(owner);
        self
    }

    /// Extend with multiple owners.
    pub fn with_owners(mut self, owners: impl IntoIterator<Item = OwnerInfo>) -> Self {
        self.owners.extend(owners);
        self
    }

    /// Append a single crate name.
    pub fn with_crate(mut self, crate_name: impl Into<String>) -> Self {
        self.crates.push(crate_name.into());
        self
    }

    /// Extend with multiple crate names.
    pub fn with_crates<I, S>(mut self, crates: I) -> Self
    where
        I: IntoIterator<Item = S>,
        S: Into<String>,
    {
        self.crates.extend(crates.into_iter().map(Into::into));
        self
    }

    /// Append a single extends entry.
    pub fn with_extends(mut self, extends: impl Into<String>) -> Self {
        self.extends.push(extends.into());
        self
    }

    /// Append a single feature.
    pub fn with_feature(mut self, feature: FeatureInfo) -> Self {
        self.features.push(feature);
        self
    }

    /// Extend with multiple features.
    pub fn with_features(mut self, features: impl IntoIterator<Item = FeatureInfo>) -> Self {
        self.features.extend(features);
        self
    }

    /// Append a single category.
    pub fn with_category(mut self, category: CategoryInfo) -> Self {
        self.categories.push(category);
        self
    }

    /// Extend with multiple categories.
    pub fn with_categories(mut self, categories: impl IntoIterator<Item = CategoryInfo>) -> Self {
        self.categories.extend(categories);
        self
    }

    /// Append a single template.
    pub fn with_template(mut self, template: TemplateInfo) -> Self {
        self.templates.push(template);
        self
    }

    /// Extend with multiple templates.
    pub fn with_templates(mut self, templates: impl IntoIterator<Item = TemplateInfo>) -> Self {
        self.templates.extend(templates);
        self
    }

    /// Append a single example.
    pub fn with_example(mut self, example: ExampleInfo) -> Self {
        self.examples.push(example);
        self
    }

    /// Extend with multiple examples.
    pub fn with_examples(mut self, examples: impl IntoIterator<Item = ExampleInfo>) -> Self {
        self.examples.extend(examples);
        self
    }

    /// Set the crates from this pack that are currently installed.
    pub fn with_installed_crates<I, S>(mut self, crates: I) -> Self
    where
        I: IntoIterator<Item = S>,
        S: Into<String>,
    {
        self.installed_crates
            .extend(crates.into_iter().map(Into::into));
        self
    }

    /// Set the features from this pack that are currently active.
    pub fn with_active_features<I, S>(mut self, features: I) -> Self
    where
        I: IntoIterator<Item = S>,
        S: Into<String>,
    {
        self.active_features
            .extend(features.into_iter().map(Into::into));
        self
    }
}

impl OwnerInfo {
    /// Build an [`OwnerInfo`] from a login.
    pub fn new(login: impl Into<String>) -> Self {
        Self {
            login: login.into(),
            name: None,
        }
    }

    /// Set the display name.
    pub fn with_name(mut self, name: impl Into<String>) -> Self {
        self.name = Some(name.into());
        self
    }
}

impl FeatureInfo {
    /// Build a [`FeatureInfo`] from its name with no crates.
    pub fn new(name: impl Into<String>) -> Self {
        Self {
            name: name.into(),
            crates: Vec::new(),
        }
    }

    /// Append a single crate to this feature.
    pub fn with_crate(mut self, crate_name: impl Into<String>) -> Self {
        self.crates.push(crate_name.into());
        self
    }

    /// Extend with multiple crates.
    pub fn with_crates<I, S>(mut self, crates: I) -> Self
    where
        I: IntoIterator<Item = S>,
        S: Into<String>,
    {
        self.crates.extend(crates.into_iter().map(Into::into));
        self
    }
}

impl CategoryInfo {
    /// Build a [`CategoryInfo`] from its key with the default (`Any`) pick mode.
    pub fn new(name: impl Into<String>) -> Self {
        Self {
            name: name.into(),
            title: None,
            description: None,
            pick: PickModeInfo::Any,
            members: Vec::new(),
        }
    }

    /// Set the display title.
    pub fn with_title(mut self, title: impl Into<String>) -> Self {
        self.title = Some(title.into());
        self
    }

    /// Set the description.
    pub fn with_description(mut self, description: impl Into<String>) -> Self {
        self.description = Some(description.into());
        self
    }

    /// Set the pick mode.
    pub fn with_pick(mut self, pick: PickModeInfo) -> Self {
        self.pick = pick;
        self
    }

    /// Extend with multiple member names.
    pub fn with_members<I, S>(mut self, members: I) -> Self
    where
        I: IntoIterator<Item = S>,
        S: Into<String>,
    {
        self.members.extend(members.into_iter().map(Into::into));
        self
    }
}

impl TemplateInfo {
    /// Build a [`TemplateInfo`] from its name.
    pub fn new(name: impl Into<String>) -> Self {
        Self {
            name: name.into(),
            description: None,
        }
    }

    /// Set the description.
    pub fn with_description(mut self, description: impl Into<String>) -> Self {
        self.description = Some(description.into());
        self
    }
}

impl ExampleInfo {
    /// Build an [`ExampleInfo`] from its name.
    pub fn new(name: impl Into<String>) -> Self {
        Self {
            name: name.into(),
            description: None,
        }
    }

    /// Set the description.
    pub fn with_description(mut self, description: impl Into<String>) -> Self {
        self.description = Some(description.into());
        self
    }
}
