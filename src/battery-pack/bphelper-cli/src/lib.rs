//! CLI for battery-pack: create and manage battery packs.

mod commands;
mod completions;
pub(crate) mod manifest;
pub(crate) mod merge;
pub(crate) mod registry;
pub(crate) mod template_engine;
mod tui;
mod validate;

// The only true public API
pub use commands::main;
pub use registry::resolve_bp_managed_content;
pub use validate::validate_templates;

/// A rendered file from a template preview.
#[non_exhaustive]
pub struct PreviewFile {
    pub path: String,
    pub content: String,
}

/// Builder for previewing a template.
///
/// ```rust,ignore
/// let files = battery_pack::testing::PreviewBuilder::new(env!("CARGO_MANIFEST_DIR"))
///     .template("templates/full")
///     .project_name("my-project")
///     .define("fuzzing", "true")
///     .preview()
///     .unwrap();
/// ```
pub struct PreviewBuilder {
    crate_root: std::path::PathBuf,
    template_path: String,
    project_name: String,
    defines: std::collections::BTreeMap<String, String>,
}

impl PreviewBuilder {
    pub fn new(crate_root: impl Into<std::path::PathBuf>) -> Self {
        Self {
            crate_root: crate_root.into(),
            template_path: String::new(),
            project_name: "test-project".to_string(),
            defines: std::collections::BTreeMap::new(),
        }
    }

    pub fn template(mut self, path: impl Into<String>) -> Self {
        self.template_path = path.into();
        self
    }

    pub fn project_name(mut self, name: impl Into<String>) -> Self {
        self.project_name = name.into();
        self
    }

    pub fn define(mut self, key: impl Into<String>, value: impl Into<String>) -> Self {
        self.defines.insert(key.into(), value.into());
        self
    }

    pub fn preview(self) -> anyhow::Result<Vec<PreviewFile>> {
        let opts = template_engine::RenderOpts {
            crate_root: self.crate_root,
            template_path: self.template_path,
            project_name: self.project_name,
            defines: self.defines,
            interactive_override: Some(false),
        };
        let files = template_engine::preview(opts)?;
        Ok(files
            .into_iter()
            .map(|f| PreviewFile {
                path: f.path,
                content: f.content,
            })
            .collect())
    }
}
