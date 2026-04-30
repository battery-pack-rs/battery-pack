//! Battery pack validation: structure checks, packaging, and template compilation.

use anyhow::{Context, Result, bail};
use std::path::Path;

/// Sentinel error: template validation was skipped (not a real failure).
#[derive(Debug)]
struct SkipValidation;

impl std::fmt::Display for SkipValidation {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        f.write_str("template validation skipped")
    }
}

impl std::error::Error for SkipValidation {}

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
        validate(crate_root.to_str().unwrap_or("."))?;
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
    validate(crate_root.to_str().unwrap_or("."))?;
    println!("{} is valid ({} warning(s))", spec.name, warnings);
    Ok(())
}

/// Validate a battery pack by packaging it and building templates from the tarball.
///
/// This ensures that what users download from crates.io actually works:
/// 1. Runs `cargo package` to produce the `.crate` tarball
/// 2. Extracts it to a temp directory
/// 3. For each template that produces a `Cargo.toml` (i.e., a full project),
///    generates a project and runs `cargo check` + `cargo test`
/// 4. Templates without a `Cargo.toml` (partial scaffolds) are skipped
///
/// Compiled artifacts are cached in `<target_dir>/bp-validate/` so that
/// subsequent runs are faster.
pub fn validate(manifest_dir: &str) -> Result<()> {
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

    // Package the crate and extract the tarball so we validate what users get.
    // During multi-crate releases, path dependency versions may not yet be
    // published, causing cargo package to fail. Fall back to validating
    // directly from the source tree (less precise, but still catches
    // template render and build errors).
    let (packaged_dir, from_tarball) = match package_and_extract(manifest_dir, &metadata) {
        Ok(dir) => (dir, true),
        Err(e) if e.downcast_ref::<SkipValidation>().is_some() => {
            println!(
                "note: cargo package failed (path dependency not yet published), \
                 validating from source tree"
            );
            (manifest_dir.to_path_buf(), false)
        }
        Err(e) => return Err(e),
    };

    // Build a hint so users can inspect the tarball manually on failure.
    let tarball_hint = format!(
        "To inspect the packaged tarball:\n  \
         cargo package --allow-dirty --no-verify\n  \
         tar tzf {}/package/{}-{}.crate",
        metadata.target_directory,
        crate_name,
        metadata
            .packages
            .iter()
            .find(|p| p.manifest_path == cargo_toml)
            .map(|p| p.version.to_string())
            .unwrap_or_else(|| "VERSION".to_string())
    );

    let mut validated = 0;
    let mut skipped = 0;

    for (name, template) in &spec.templates {
        // Render the template from the packaged tarball. This catches missing
        // files (snippets, includes) even for partial scaffolds.
        let files = try_render(&packaged_dir, &template.path).with_context(|| {
            if from_tarball {
                format!(
                    "template '{name}' failed to render from the packaged tarball.\n\
                     This usually means files are missing from the published crate \
                     (e.g. Cargo.toml in a template directory causes cargo to exclude it).\n\
                     {tarball_hint}"
                )
            } else {
                format!("template '{name}' failed to render from the source tree")
            }
        })?;

        let is_full_project = files.iter().any(|f| f.path == "Cargo.toml");
        if !is_full_project {
            println!("template '{name}' renders ok (partial scaffold, skipping build)");
            skipped += 1;
            continue;
        }

        println!("validating template '{name}'...");

        let tmp = tempfile::tempdir().context("failed to create temp directory")?;
        let project_name = format!("bp-validate-{name}");

        let opts = crate::template_engine::GenerateOpts {
            render: crate::template_engine::RenderOpts {
                crate_root: packaged_dir.clone(),
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
            "cargo check failed for template '{name}':\n{}\n{}",
            String::from_utf8_lossy(&output.stdout),
            String::from_utf8_lossy(&output.stderr)
        );

        // cargo test
        // Failure details (assertions, panics) go to stdout, not stderr.
        let output = std::process::Command::new("cargo")
            .args(["test"])
            .env("CARGO_TARGET_DIR", &*shared_target_dir)
            .current_dir(&project_dir)
            .output()
            .context("failed to run cargo test")?;
        anyhow::ensure!(
            output.status.success(),
            "cargo test failed for template '{name}':\n{}\n{}",
            String::from_utf8_lossy(&output.stdout),
            String::from_utf8_lossy(&output.stderr)
        );

        println!("template '{name}' ok");
        validated += 1;
    }

    println!(
        "{} template(s) validated, {} skipped for '{}'",
        validated, skipped, crate_name
    );
    Ok(())
}

/// Package the crate into a tarball and extract it to a temp directory.
/// Returns the path to the extracted crate root.
fn package_and_extract(
    crate_root: &Path,
    metadata: &cargo_metadata::Metadata,
) -> Result<std::path::PathBuf> {
    // Find the package name and version from metadata
    let manifest_path = crate_root.join("Cargo.toml");
    let pkg = metadata
        .packages
        .iter()
        .find(|p| p.manifest_path == manifest_path)
        .context("could not find package in cargo metadata")?;

    let target_dir = &metadata.target_directory;
    let crate_file = target_dir
        .join("package")
        .join(format!("{}-{}.crate", pkg.name, pkg.version));

    // --no-verify: skip the redundant build; we validate the tarball ourselves.
    let output = std::process::Command::new("cargo")
        .args(["package", "--allow-dirty", "--no-verify"])
        .current_dir(crate_root)
        .output()
        .context("failed to run cargo package")?;
    if !output.status.success() {
        let stderr = String::from_utf8_lossy(&output.stderr);
        let workspace_names: Vec<_> = metadata
            .workspace_packages()
            .iter()
            .map(|p| p.name.to_string())
            .collect();
        if is_unpublished_workspace_dep(&stderr, &workspace_names) {
            println!(
                "note: cargo package failed (workspace dependency not yet \
                 published), falling back to source tree validation"
            );
            return Err(SkipValidation.into());
        }
        bail!("cargo package failed:\n{stderr}");
    }

    // Extract the tarball
    let temp_dir = tempfile::tempdir().context("failed to create temp directory")?;
    let crate_bytes =
        std::fs::read(&crate_file).with_context(|| format!("failed to read {crate_file}"))?;
    let decoder = flate2::read::GzDecoder::new(&crate_bytes[..]);
    let mut archive = tar::Archive::new(decoder);
    archive
        .unpack(temp_dir.path())
        .context("failed to extract crate tarball")?;

    let extracted = temp_dir
        .path()
        .join(format!("{}-{}", pkg.name, pkg.version));
    anyhow::ensure!(
        extracted.is_dir(),
        "extracted crate directory not found: {}",
        extracted.display()
    );

    // Leak the temp dir so it lives long enough for validation
    let path = extracted.to_path_buf();
    std::mem::forget(temp_dir);
    Ok(path)
}

/// Render a template from the packaged crate. Returns the rendered file list.
/// Fails if any file or include is missing from the tarball.
fn try_render(
    crate_root: &Path,
    template_path: &str,
) -> Result<Vec<crate::template_engine::RenderedFile>> {
    let opts = crate::template_engine::RenderOpts {
        crate_root: crate_root.to_path_buf(),
        template_path: template_path.to_string(),
        project_name: "bp-validate-probe".to_string(),
        defines: std::collections::BTreeMap::new(),
        interactive_override: Some(false),
    };
    crate::template_engine::preview(opts)
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

/// Check whether a `cargo package` failure is caused by a workspace path
/// dependency whose version hasn't been published to crates.io yet.
/// During a multi-crate release (e.g. release-plz), workspace dependencies
/// may be bumped to versions not yet published. This is expected and not
/// something template validation can or should catch. A missing external
/// crate is still a real error.
fn is_unpublished_workspace_dep(stderr: &str, workspace_names: &[String]) -> bool {
    stderr.contains("failed to select a version for the requirement")
        && workspace_names
            .iter()
            .any(|name| stderr.contains(&format!("`{name} = ")))
}

#[cfg(test)]
mod tests;
