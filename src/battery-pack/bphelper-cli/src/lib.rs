//! CLI for battery-pack: create and manage battery packs.

mod commands;
pub(crate) mod manifest;
pub(crate) mod registry;
pub(crate) mod template_engine;
mod tui;
mod validate;
mod completions;

// The only true public API
pub use commands::main;
pub use registry::resolve_bp_managed_content;
pub use validate::validate_templates;
