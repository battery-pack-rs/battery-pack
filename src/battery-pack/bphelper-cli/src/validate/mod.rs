//! Battery pack validation: structure checks and template compilation.

use anyhow::{Context, Result, bail};
use std::path::Path;

// ============================================================================
// Validate command
// ============================================================================

// [impl cli.validate.purpose]
// [impl cli.validate.default-path]
pub(crate) fn validate_battery_pack_cmd(path: Option<&str>) -> Result<()> {
    let crate_root = match path {
        Some(p) => std::path::PathBuf::from(p),
        None => std::env::current_dir().context("failed to get current directory")?,
    };

    let cargo_toml = crate_root.join("Cargo.toml");
    let content = std::fs::read_to_string(&cargo_toml)
        .with_context(|| format!("failed to read {}", cargo_toml.display()))?;

    // Check for virtual/workspace manifest before attempting battery pack parse
    let raw: toml::Value = toml::from_str(&content)
        .with_context(|| format!("failed to parse {}", cargo_toml.display()))?;
    if raw.get("package").is_none() {
        if raw.get("workspace").is_some() {
            // [impl cli.validate.workspace-error]
            bail!(
                "{} is a workspace manifest, not a battery pack crate.\n\
                 Run this from a battery pack crate directory, or use --path to point to one.",
                cargo_toml.display()
            );
        } else {
            // [impl cli.validate.no-package]
            bail!(
                "{} has no [package] section — is this a battery pack crate?",
                cargo_toml.display()
            );
        }
    }

    let spec = bphelper_manifest::parse_battery_pack(&content)
        .with_context(|| format!("failed to parse {}", cargo_toml.display()))?;

    // [impl cli.validate.checks]
    let mut report = spec.validate_spec();
    report.merge(bphelper_manifest::validate_on_disk(&spec, &crate_root));

    // [impl cli.validate.clean]
    if report.is_clean() {
        validate_templates(crate_root.to_str().unwrap_or("."))?;
        println!("{} is valid", spec.name);
        return Ok(());
    }

    // [impl cli.validate.severity]
    // [impl cli.validate.rule-id]
    let mut errors = 0;
    let mut warnings = 0;
    for diag in &report.diagnostics {
        match diag.severity {
            bphelper_manifest::Severity::Error => {
                eprintln!("error[{}]: {}", diag.rule, diag.message);
                errors += 1;
            }
            bphelper_manifest::Severity::Warning => {
                eprintln!("warning[{}]: {}", diag.rule, diag.message);
                warnings += 1;
            }
        }
    }

    // [impl cli.validate.errors]
    if errors > 0 {
        bail!(
            "validation failed: {} error(s), {} warning(s)",
            errors,
            warnings
        );
    }

    // [impl cli.validate.warnings-only]
    // Warnings only — still succeeds
    validate_templates(crate_root.to_str().unwrap_or("."))?;
    println!("{} is valid ({} warning(s))", spec.name, warnings);
    Ok(())
}

/// Validate that each template in a battery pack generates a project that compiles
/// and passes tests.
///
/// For each template declared in the battery pack's metadata:
/// 1. Generates a project into a temporary directory
/// 2. Runs `cargo check` to verify it compiles
/// 3. Runs `cargo test` to verify tests pass
///
/// Compiled artifacts are cached in `<target_dir>/bp-validate/` so that
/// subsequent runs are faster.
// [impl cli.validate.templates]
// [impl cli.validate.templates.cache]
pub fn validate_templates(manifest_dir: &str) -> Result<()> {
    let manifest_dir = Path::new(manifest_dir);
    let cargo_toml = manifest_dir.join("Cargo.toml");
    let content = std::fs::read_to_string(&cargo_toml)
        .with_context(|| format!("failed to read {}", cargo_toml.display()))?;

    let crate_name = manifest_dir
        .file_name()
        .and_then(|s| s.to_str())
        .unwrap_or("unknown");
    let spec = bphelper_manifest::parse_battery_pack(&content)
        .map_err(|e| anyhow::anyhow!("failed to parse {}: {e}", cargo_toml.display()))?;

    if spec.templates.is_empty() {
        // [impl cli.validate.templates.none]
        println!("no templates to validate");
        return Ok(());
    }

    // Stable target dir for caching compiled artifacts across runs.
    let metadata = cargo_metadata::MetadataCommand::new()
        .manifest_path(&cargo_toml)
        .no_deps()
        .exec()
        .context("failed to run cargo metadata")?;
    let shared_target_dir = metadata.target_directory.join("bp-validate");

    for (name, template) in &spec.templates {
        println!("validating template '{name}'...");

        let tmp = tempfile::tempdir().context("failed to create temp directory")?;

        let project_name = format!("bp-validate-{name}");

        let opts = crate::template_engine::GenerateOpts {
            render: crate::template_engine::RenderOpts {
                crate_root: manifest_dir.to_path_buf(),
                template_path: template.path.clone(),
                project_name,
                defines: std::collections::BTreeMap::new(),
                interactive_override: Some(false),
            },
            destination: Some(tmp.path().to_path_buf()),
            git_init: false,
        };

        let project_dir = crate::template_engine::generate(opts)
            .with_context(|| format!("failed to generate template '{name}'"))?;

        write_crates_io_patches(&project_dir, &metadata)?;

        // cargo check
        let output = std::process::Command::new("cargo")
            .args(["check"])
            .env("CARGO_TARGET_DIR", &*shared_target_dir)
            .current_dir(&project_dir)
            .output()
            .context("failed to run cargo check")?;
        anyhow::ensure!(
            output.status.success(),
            "cargo check failed for template '{name}':\n{}",
            String::from_utf8_lossy(&output.stderr)
        );

        // cargo test
        let output = std::process::Command::new("cargo")
            .args(["test"])
            .env("CARGO_TARGET_DIR", &*shared_target_dir)
            .current_dir(&project_dir)
            .output()
            .context("failed to run cargo test")?;
        anyhow::ensure!(
            output.status.success(),
            "cargo test failed for template '{name}':\n{}",
            String::from_utf8_lossy(&output.stderr)
        );

        println!("template '{name}' ok");
    }

    println!(
        "all {} template(s) for '{}' validated successfully",
        spec.templates.len(),
        crate_name
    );
    Ok(())
}

/// Write a `.cargo/config.toml` that patches crates-io dependencies with local
/// workspace packages, so template validation builds against current source.
// [impl cli.validate.templates.patch]
fn write_crates_io_patches(project_dir: &Path, metadata: &cargo_metadata::Metadata) -> Result<()> {
    let mut patches = String::from("[patch.crates-io]\n");
    for pkg in &metadata.workspace_packages() {
        let path = pkg.manifest_path.parent().unwrap();
        patches.push_str(&format!("{} = {{ path = \"{}\" }}\n", pkg.name, path));
    }

    // Forward any existing patches from the battery pack's .cargo/config.toml
    // so transitive dependencies (e.g. battery-pack with feature flags) resolve
    // against local source when running in a patched development environment.
    let parent_config = metadata.workspace_root.join(".cargo/config.toml");
    if let Ok(content) = std::fs::read_to_string(&parent_config)
        && let Ok(parsed) = content.parse::<toml::Table>()
        && let Some(toml::Value::Table(patch_section)) = parsed.get("patch")
        && let Some(toml::Value::Table(crates_io)) = patch_section.get("crates-io")
    {
        for (name, value) in crates_io {
            // Skip packages already covered by workspace members
            if metadata
                .workspace_packages()
                .iter()
                .any(|p| p.name == *name)
            {
                continue;
            }
            patches.push_str(&format!("{name} = {value}\n"));
        }
    }

    let cargo_dir = project_dir.join(".cargo");
    std::fs::create_dir_all(&cargo_dir)
        .with_context(|| format!("failed to create {}", cargo_dir.display()))?;
    std::fs::write(cargo_dir.join("config.toml"), patches)
        .context("failed to write .cargo/config.toml")?;
    Ok(())
}

#[cfg(test)]
mod tests;
