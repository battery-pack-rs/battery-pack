//! Battery pack validation: structure checks, packaging, and template compilation.

use anyhow::{Context, Result, bail};
use bphelper_manifest::parse_battery_pack_from_path;
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

    let spec = parse_battery_pack_from_path(&cargo_toml)
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

    let crate_name = manifest_dir
        .file_name()
        .and_then(|s| s.to_str())
        .unwrap_or("unknown");
    let spec = parse_battery_pack_from_path(&cargo_toml)
        .with_context(|| format!("failed to parse {}", cargo_toml.display()))?;

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
        build_and_test_template(
            &packaged_dir,
            &template.path,
            name,
            std::collections::BTreeMap::new(),
            shared_target_dir.as_std_path(),
            &metadata,
        )?;
        println!("template '{name}' ok");
        validated += 1;
    }

    println!(
        "{} template(s) validated, {} skipped for '{}'",
        validated, skipped, crate_name
    );
    Ok(())
}

/// Generate one template with explicit placeholder values, then `cargo check` + `cargo test` it.
///
/// Generates from the source tree (not the packaged tarball) so battery pack authors can
/// exercise feature/placeholder combinations beyond the template defaults in their own tests.
pub fn validate_template_with(
    manifest_dir: &str,
    template_name: &str,
    defines: &[(&str, &str)],
) -> Result<()> {
    let manifest_dir = Path::new(manifest_dir);
    let cargo_toml = manifest_dir.join("Cargo.toml");
    let spec = parse_battery_pack_from_path(&cargo_toml)
        .with_context(|| format!("failed to parse {}", cargo_toml.display()))?;

    let template = spec.templates.get(template_name).with_context(|| {
        format!(
            "template '{template_name}' not found in {}",
            cargo_toml.display()
        )
    })?;

    let metadata = cargo_metadata::MetadataCommand::new()
        .manifest_path(&cargo_toml)
        .no_deps()
        .exec()
        .context("failed to run cargo metadata")?;
    let shared_target_dir = metadata.target_directory.join("bp-validate");

    let defines = defines
        .iter()
        .map(|(k, v)| (k.to_string(), v.to_string()))
        .collect();
    build_and_test_template(
        manifest_dir,
        &template.path,
        template_name,
        defines,
        shared_target_dir.as_std_path(),
        &metadata,
    )
}

/// Generate a template into a temp dir with the given placeholders, then run
/// `cargo check` and `cargo test` against it.
fn build_and_test_template(
    crate_root: &Path,
    template_path: &str,
    label: &str,
    defines: std::collections::BTreeMap<String, String>,
    shared_target_dir: &Path,
    metadata: &cargo_metadata::Metadata,
) -> Result<()> {
    let tmp = tempfile::tempdir().context("failed to create temp directory")?;
    let opts = crate::template_engine::GenerateOpts {
        render: crate::template_engine::RenderOpts {
            crate_root: crate_root.to_path_buf(),
            template_path: template_path.to_string(),
            project_name: format!("bp-validate-{label}"),
            defines,
            active_features: std::collections::BTreeSet::new(),
            interactive_override: Some(false),
        },
        destination: Some(tmp.path().to_path_buf()),
        git_init: false,
    };
    let project_dir = crate::template_engine::generate(opts)
        .with_context(|| format!("failed to generate template '{label}'"))?;

    write_crates_io_patches(&project_dir, metadata)?;

    // Failure details (assertions, panics) go to stdout, not stderr. `fmt --check` keeps generated
    // output rustfmt-clean so a freshly scaffolded project does not greet the user with a diff.
    for args in [
        ["check"].as_slice(),
        ["test"].as_slice(),
        ["fmt", "--check"].as_slice(),
    ] {
        let output = std::process::Command::new("cargo")
            .args(args)
            .env("CARGO_TARGET_DIR", shared_target_dir)
            .current_dir(&project_dir)
            .output()
            .with_context(|| format!("failed to run cargo {}", args.join(" ")))?;
        anyhow::ensure!(
            output.status.success(),
            "cargo {} failed for template '{label}':\n{}\n{}",
            args.join(" "),
            String::from_utf8_lossy(&output.stdout),
            String::from_utf8_lossy(&output.stderr)
        );
    }
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
        active_features: std::collections::BTreeSet::new(),
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
        patches.push_str(&format!("{} = {{ path = \'{}\' }}\n", pkg.name, path));
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
    // A template may ship its own .cargo/config.toml (e.g. `[build] rustflags`). Append the
    // patch table rather than overwriting it, so those settings survive validation.
    let config_path = cargo_dir.join("config.toml");
    let existing = std::fs::read_to_string(&config_path).unwrap_or_default();
    std::fs::write(&config_path, merge_patches_into_config(&existing, &patches))
        .context("failed to write .cargo/config.toml")?;
    Ok(())
}

/// Append a `[patch.crates-io]` block to an existing `.cargo/config.toml` body, preserving
/// any tables (like `[build]`) the template already defined.
fn merge_patches_into_config(existing: &str, patches: &str) -> String {
    if existing.trim().is_empty() {
        return patches.to_string();
    }
    let mut out = existing.trim_end().to_string();
    out.push_str("\n\n");
    out.push_str(patches);
    out
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
mod tests {
    //! Tests for battery pack validation.

    use indoc::indoc;
    use snapbox::{assert_data_eq, str};
    use std::path::PathBuf;

    fn fixtures_dir() -> PathBuf {
        let manifest_dir = std::path::Path::new(env!("CARGO_MANIFEST_DIR"));
        manifest_dir
            .parent()
            .unwrap()
            .parent()
            .unwrap()
            .parent()
            .unwrap()
            .join("tests/fixtures")
    }

    // [verify cli.validate.clean]
    // [verify cli.validate.checks]
    #[test]
    fn validate_basic_fixture_is_clean() {
        let fixture = fixtures_dir().join("basic-battery-pack");
        let result = super::validate_battery_pack_cmd(Some(fixture.to_str().unwrap()));
        assert!(result.is_ok(), "basic-battery-pack should validate cleanly");
    }

    // [verify cli.validate.clean]
    // [verify cli.validate.checks]
    #[test]
    fn validate_fancy_fixture_is_clean() {
        let fixture = fixtures_dir().join("fancy-battery-pack");
        let result = super::validate_battery_pack_cmd(Some(fixture.to_str().unwrap()));
        assert!(result.is_ok(), "fancy-battery-pack should validate cleanly");
    }

    // [verify cli.validate.errors]
    // [verify cli.validate.severity]
    // [verify cli.validate.rule-id]
    #[test]
    fn validate_broken_fixture_fails() {
        let fixture = fixtures_dir().join("broken-battery-pack");
        let result = super::validate_battery_pack_cmd(Some(fixture.to_str().unwrap()));
        assert!(
            result.is_err(),
            "broken-battery-pack should fail validation"
        );
        let err = result.unwrap_err().to_string();
        assert_data_eq!(err, str!["validation failed: 3 error(s), 2 warning(s)"]);
    }

    // [verify cli.validate.workspace-error]
    #[test]
    fn validate_workspace_manifest_fails() {
        let fixture = fixtures_dir();
        // The fixtures directory itself has a workspace Cargo.toml
        let result = super::validate_battery_pack_cmd(Some(fixture.to_str().unwrap()));
        assert!(result.is_err(), "workspace manifest should fail");
        let err = result.unwrap_err().to_string();
        assert_data_eq!(
            err,
            str![[r#"
[..]/tests/fixtures/Cargo.toml is a workspace manifest, not a battery pack crate.
Run this from a battery pack crate directory, or use --path to point to one.
"#]]
        );
    }

    // [verify cli.validate.no-package]
    #[test]
    fn validate_nonexistent_path_fails() {
        let result = super::validate_battery_pack_cmd(Some("/nonexistent/path"));
        assert!(result.is_err(), "nonexistent path should fail");
    }

    // [verify cli.validate.default-path]
    #[test]
    fn validate_uses_path_argument() {
        // Verify --path correctly targets a specific directory rather than cwd
        let fixture = fixtures_dir().join("basic-battery-pack");
        let result = super::validate_battery_pack_cmd(Some(fixture.to_str().unwrap()));
        assert!(
            result.is_ok(),
            "explicit --path to a valid fixture should succeed"
        );
    }

    // [verify cli.validate.default-path]
    #[test]
    #[ignore = "causes race conditions, refactor to not rely on setting env::current_dir()"]
    fn validate_defaults_to_current_directory() {
        let fixture = fixtures_dir().join("fancy-battery-pack");
        let original_dir = std::env::current_dir().unwrap();
        std::env::set_current_dir(&fixture).unwrap();
        let result = super::validate_battery_pack_cmd(None);
        std::env::set_current_dir(&original_dir).unwrap();
        assert!(
            result.is_ok(),
            "validate with None (cwd = fancy-battery-pack) should succeed: {:?}",
            result.unwrap_err()
        );
    }

    // [verify format.crate.repository]
    #[test]
    fn validate_fixture_without_repository_warns_but_passes() {
        let fixture = fixtures_dir().join("basic-battery-pack");
        let result = super::validate_battery_pack_cmd(Some(fixture.to_str().unwrap()));
        assert!(
            result.is_ok(),
            "basic-battery-pack should pass validation (warnings only): {:?}",
            result.unwrap_err()
        );
    }

    // [verify format.crate.repository]
    #[test]
    fn validate_fixture_with_repository_no_warning() {
        let fixture = fixtures_dir().join("fancy-battery-pack");
        let result = super::validate_battery_pack_cmd(Some(fixture.to_str().unwrap()));
        assert!(
            result.is_ok(),
            "fancy-battery-pack should validate cleanly: {:?}",
            result.unwrap_err()
        );
    }

    #[test]
    fn validate_template_test_failure_includes_stdout() {
        // Regression test for https://github.com/battery-pack-rs/battery-pack/issues/73
        let tmp = tempfile::tempdir().unwrap();
        let bp = tmp.path().join("test-bp");

        std::fs::create_dir_all(bp.join("src")).unwrap();
        std::fs::create_dir_all(bp.join("templates/default/src")).unwrap();
        std::fs::create_dir_all(bp.join("templates/default/tests")).unwrap();

        std::fs::write(
            bp.join("Cargo.toml"),
            indoc! {r#"
                [package]
                name = "test-battery-pack"
                version = "0.1.0"
                edition = "2021"
                description = "test"
                keywords = ["battery-pack"]

                [package.metadata.battery.templates]
                default = { path = "templates/default", description = "test" }
            "#},
        )
        .unwrap();
        std::fs::write(bp.join("src/lib.rs"), "").unwrap();

        std::fs::write(
            bp.join("templates/default/_Cargo.toml"),
            indoc! {r#"
                [package]
                name = "{{ project_name }}"
                version = "0.1.0"
                edition = "2021"
            "#},
        )
        .unwrap();
        std::fs::write(bp.join("templates/default/src/main.rs"), "fn main() {}\n").unwrap();
        std::fs::write(
            bp.join("templates/default/tests/broken.rs"),
            indoc! {r#"
                #[test]
                fn this_test_fails() {
                    assert_eq!("expected", "actual", "THIS DETAIL SHOULD BE VISIBLE");
                }
            "#},
        )
        .unwrap();

        let err = super::validate(bp.to_str().unwrap()).unwrap_err();
        let msg = format!("{err:#}");

        snapbox::assert_data_eq!(
            msg,
            snapbox::str![[r#"
cargo test failed for template 'default':

running 0 tests

test result: ok. 0 passed; 0 failed; 0 ignored; 0 measured; 0 filtered out; finished in [..]s


running 1 test
test this_test_fails ... FAILED

failures:

---- this_test_fails stdout ----

thread 'this_test_fails' ([..]) panicked at tests/broken.rs:3:5:
assertion `left == right` failed: THIS DETAIL SHOULD BE VISIBLE
  left: "expected"
 right: "actual"
note: run with `RUST_BACKTRACE=1` environment variable to display a backtrace


failures:
    this_test_fails

test result: FAILED. 0 passed; 1 failed; 0 ignored; 0 measured; 0 filtered out; finished in [..]s

...
"#]]
        );
    }

    #[test]
    fn validate_catches_excluded_template_file() {
        // A template file excluded from the tarball via `package.exclude` should
        // cause validate to fail, proving the packaging round-trip
        // actually catches missing files.
        let tmp = tempfile::tempdir().unwrap();
        let bp = tmp.path().join("test-bp");

        std::fs::create_dir_all(bp.join("src")).unwrap();
        std::fs::create_dir_all(bp.join("templates/default/src")).unwrap();

        std::fs::write(
            bp.join("Cargo.toml"),
            indoc! {r#"
                [package]
                name = "test-battery-pack"
                version = "0.1.0"
                edition = "2021"
                description = "test"
                keywords = ["battery-pack"]
                exclude = ["templates/default/src/main.rs"]

                [package.metadata.battery.templates]
                default = { path = "templates/default", description = "test" }
            "#},
        )
        .unwrap();
        std::fs::write(bp.join("src/lib.rs"), "").unwrap();

        std::fs::write(
            bp.join("templates/default/_Cargo.toml"),
            indoc! {r#"
                [package]
                name = "{{ project_name }}"
                version = "0.1.0"
                edition = "2021"
            "#},
        )
        .unwrap();
        std::fs::write(bp.join("templates/default/src/main.rs"), "fn main() {}\n").unwrap();

        let err = super::validate(bp.to_str().unwrap()).unwrap_err();
        let msg = format!("{err:#}");
        assert!(
            msg.contains("cargo check failed"),
            "expected build failure from excluded file, got: {msg}"
        );
    }

    #[test]
    fn unpublished_workspace_dep_is_detected() {
        let stderr = r#"error: failed to prepare local package for uploading

Caused by:
  failed to select a version for the requirement `bphelper-build = "^0.4.7"`
  candidate versions found which didn't match: 0.4.6, 0.4.5, 0.4.4, ...
  location searched: crates.io index
  required by package `battery-pack v0.5.2`"#;
        let workspace = vec!["bphelper-build".to_string(), "battery-pack".to_string()];
        assert!(super::is_unpublished_workspace_dep(stderr, &workspace));
    }

    #[test]
    fn unpublished_external_dep_is_not_skipped() {
        let stderr = r#"error: failed to prepare local package for uploading

Caused by:
  failed to select a version for the requirement `serde = "^99.0.0"`
  candidate versions found which didn't match: 1.0.219, ...
  location searched: crates.io index
  required by package `battery-pack v0.5.2`"#;
        let workspace = vec!["bphelper-build".to_string(), "battery-pack".to_string()];
        assert!(!super::is_unpublished_workspace_dep(stderr, &workspace));
    }

    #[test]
    fn merge_patches_preserves_existing_config() {
        let existing = "[build]\nrustflags = [\"--cfg\", \"tokio_unstable\"]\n";
        let patches = "[patch.crates-io]\nfoo = { path = \"/x\" }\n";
        let merged = super::merge_patches_into_config(existing, patches);
        // Both tables survive and the result is valid TOML.
        assert!(
            merged.contains("tokio_unstable"),
            "existing build flags kept"
        );
        assert!(merged.contains("[patch.crates-io]"), "patch table appended");
        let parsed: toml::Table = merged.parse().expect("merged config is valid TOML");
        assert!(parsed.contains_key("build") && parsed.contains_key("patch"));
    }

    #[test]
    fn merge_patches_into_empty_config() {
        let merged = super::merge_patches_into_config("", "[patch.crates-io]\n");
        assert_eq!(merged, "[patch.crates-io]\n");
    }
}
