//! Group 2 integration tests: full `add_battery_pack` flow with real fixtures.
//!
//! These tests create a temp project, call `add_battery_pack` with `--path`
//! pointing at test fixtures, then snapshot the written Cargo.toml sections
//! using expect-test.
//!
//! Covers the write side of:
//!   - cli.add.default-crates    — default deps appear in Cargo.toml
//!   - cli.add.features          — named feature crates appear
//!   - cli.add.no-default-features — only named feature crates, no defaults
//!   - cli.add.all-features      — all crates appear
//!   - cli.add.specific-crates   — only named crates appear
//!   - cli.add.unknown-crate     — unknown skipped, valid written
//!   - cli.add.target            — metadata lands in package vs workspace
//!   - cli.add.register          — battery pack in build-dependencies
//!   - cli.add.dep-kind          — dev-deps land in [dev-dependencies]

use expect_test::expect;
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

/// Create a temp project with a minimal Cargo.toml and return the temp dir.
fn make_temp_project() -> tempfile::TempDir {
    let tmp = tempfile::tempdir().unwrap();
    let cargo_toml = tmp.path().join("Cargo.toml");
    std::fs::write(
        &cargo_toml,
        r#"[package]
name = "test-project"
version = "0.1.0"
edition = "2021"
"#,
    )
    .unwrap();
    // Create src/main.rs so it's a valid crate
    std::fs::create_dir_all(tmp.path().join("src")).unwrap();
    std::fs::write(tmp.path().join("src/main.rs"), "fn main() {}\n").unwrap();
    tmp
}

/// Read back the raw Cargo.toml content.
fn read_cargo_toml(tmp: &tempfile::TempDir) -> String {
    std::fs::read_to_string(tmp.path().join("Cargo.toml")).unwrap()
}

/// Extract a TOML section by header name (e.g. "[dependencies]") from raw text.
/// Returns the section contents including the header, or an empty string if absent.
fn extract_section(toml_text: &str, section: &str) -> String {
    let mut result = String::new();
    let mut in_section = false;

    for line in toml_text.lines() {
        if line.trim() == section {
            in_section = true;
            result.push_str(line);
            result.push('\n');
            continue;
        }
        if in_section {
            // Stop at the next section header
            if line.starts_with('[') {
                break;
            }
            result.push_str(line);
            result.push('\n');
        }
    }
    result
}

/// Extract dotted-table sections like [package.metadata.battery-pack.X].
/// Since toml_edit may write these in different styles, we parse and re-format.
fn extract_metadata(toml_text: &str, bp_name: &str) -> String {
    let doc: toml::Value = toml::from_str(toml_text).unwrap();
    let bp_meta = doc
        .get("package")
        .and_then(|p| p.get("metadata"))
        .and_then(|m| m.get("battery-pack"))
        .and_then(|bp| bp.get(bp_name));

    match bp_meta {
        Some(val) => {
            // Pretty-print the metadata value
            format!(
                "[package.metadata.battery-pack.{bp_name}]\n{}",
                toml::to_string_pretty(val).unwrap()
            )
        }
        None => String::new(),
    }
}

#[derive(Clone, Copy)]
enum FeatureMode {
    Default,
    NoDefault,
    All,
}

/// Helper: call add_battery_pack with common defaults.
fn add(
    pack_name: &str,
    fixture: &str,
    features: &[&str],
    feature_mode: FeatureMode,
    specific_crates: &[&str],
    target: Option<bphelper_cli::AddTarget>,
    project_dir: &std::path::Path,
) {
    let (no_default_features, all_features) = match feature_mode {
        FeatureMode::Default => (false, false),
        FeatureMode::NoDefault => (true, false),
        FeatureMode::All => (false, true),
    };
    let fixture_path = fixtures_dir().join(fixture);
    let features: Vec<String> = features.iter().map(|s| s.to_string()).collect();
    let specific: Vec<String> = specific_crates.iter().map(|s| s.to_string()).collect();
    bphelper_cli::add_battery_pack(
        bphelper_cli::AddArgs {
            battery_pack: Some(pack_name.to_string()),
            crates: specific,
            features,
            no_default_features,
            all_features,
            target,
            path: Some(fixture_path.to_str().unwrap().to_string()),
        },
        &bphelper_cli::CrateSource::Registry,
        project_dir,
    )
    .unwrap();
}

// ============================================================================
// cli.add.register — battery pack appears in [build-dependencies]
// ============================================================================

// [verify cli.add.register]
#[test]
fn add_registers_build_dep() {
    let tmp = make_temp_project();
    add(
        "basic",
        "basic-battery-pack",
        &["default"],
        FeatureMode::Default,
        &[],
        None,
        tmp.path(),
    );

    let content = read_cargo_toml(&tmp);
    let build_deps = extract_section(&content, "[build-dependencies]");

    // The path will differ per run, so we check structure not exact path
    assert!(
        build_deps.contains("basic-battery-pack"),
        "battery pack should appear in [build-dependencies]: {build_deps}"
    );
    assert!(
        build_deps.contains("path ="),
        "should be a path dependency: {build_deps}"
    );
}

// ============================================================================
// cli.add.default-crates — default crates written to Cargo.toml
// ============================================================================

// [verify cli.add.default-crates]
#[test]
fn add_default_crates_basic() {
    let tmp = make_temp_project();
    add(
        "basic",
        "basic-battery-pack",
        &["default"],
        FeatureMode::Default,
        &[],
        None,
        tmp.path(),
    );

    let content = read_cargo_toml(&tmp);
    let deps = extract_section(&content, "[dependencies]");

    expect![[r#"
        [dependencies]
        anyhow = "1"
        thiserror = "2"
    "#]]
    .assert_eq(&deps);
}

// ============================================================================
// cli.add.features — named feature crates written
// ============================================================================

// [verify cli.add.features]
#[test]
fn add_with_named_feature_writes_deps() {
    let tmp = make_temp_project();
    add(
        "fancy",
        "fancy-battery-pack",
        &["indicators"],
        FeatureMode::Default,
        &[],
        None,
        tmp.path(),
    );

    let content = read_cargo_toml(&tmp);
    let deps = extract_section(&content, "[dependencies]");

    expect![[r#"
        [dependencies]
        clap = { version = "4", features = ["derive"] }
        console = "0.15"
        dialoguer = "0.11"
        indicatif = "0.17"
    "#]]
    .assert_eq(&deps);
}

// [verify cli.add.features]
#[test]
fn add_with_named_feature_records_metadata() {
    let tmp = make_temp_project();
    add(
        "fancy",
        "fancy-battery-pack",
        &["indicators"],
        FeatureMode::Default,
        &[],
        None,
        tmp.path(),
    );

    let content = read_cargo_toml(&tmp);
    let meta = extract_metadata(&content, "fancy-battery-pack");

    expect![[r#"
        [package.metadata.battery-pack.fancy-battery-pack]
        features = [
            "default",
            "indicators",
        ]
    "#]]
    .assert_eq(&meta);
}

// ============================================================================
// cli.add.no-default-features — only named feature crates, no defaults
// ============================================================================

// [verify cli.add.no-default-features]
#[test]
fn add_no_default_features_with_feature() {
    let tmp = make_temp_project();
    add(
        "fancy",
        "fancy-battery-pack",
        &["indicators"],
        FeatureMode::NoDefault,
        &[],
        None,
        tmp.path(),
    );

    let content = read_cargo_toml(&tmp);
    let deps = extract_section(&content, "[dependencies]");

    expect![[r#"
        [dependencies]
        console = "0.15"
        indicatif = "0.17"
    "#]]
    .assert_eq(&deps);
}

// [verify cli.add.no-default-features]
#[test]
fn add_no_default_features_alone_writes_no_deps() {
    let tmp = make_temp_project();
    add(
        "basic",
        "basic-battery-pack",
        &[],
        FeatureMode::NoDefault,
        &[],
        None,
        tmp.path(),
    );

    let content = read_cargo_toml(&tmp);
    let deps = extract_section(&content, "[dependencies]");

    expect![""].assert_eq(&deps);
}

// ============================================================================
// cli.add.all-features — all crates written
// ============================================================================

// [verify cli.add.all-features]
#[test]
fn add_all_features_basic() {
    let tmp = make_temp_project();
    add(
        "basic",
        "basic-battery-pack",
        &[],
        FeatureMode::All,
        &[],
        None,
        tmp.path(),
    );

    let content = read_cargo_toml(&tmp);
    let deps = extract_section(&content, "[dependencies]");

    expect![[r#"
        [dependencies]
        anyhow = "1"
        eyre = "0.6"
        thiserror = "2"
    "#]]
    .assert_eq(&deps);
}

// [verify cli.add.all-features]
#[test]
fn add_all_features_records_metadata() {
    let tmp = make_temp_project();
    add(
        "basic",
        "basic-battery-pack",
        &[],
        FeatureMode::All,
        &[],
        None,
        tmp.path(),
    );

    let content = read_cargo_toml(&tmp);
    let meta = extract_metadata(&content, "basic-battery-pack");

    expect![[r#"
        [package.metadata.battery-pack.basic-battery-pack]
        features = ["all"]
    "#]]
    .assert_eq(&meta);
}

// [verify cli.add.all-features]
#[test]
fn add_all_features_fancy() {
    let tmp = make_temp_project();
    add(
        "fancy",
        "fancy-battery-pack",
        &[],
        FeatureMode::All,
        &[],
        None,
        tmp.path(),
    );

    let content = read_cargo_toml(&tmp);
    let deps = extract_section(&content, "[dependencies]");
    let dev_deps = extract_section(&content, "[dev-dependencies]");
    let build_deps = extract_section(&content, "[build-dependencies]");

    // Normal deps in [dependencies] — hidden crates (serde*, cc) filtered out
    // [verify format.hidden.effect]
    expect![[r#"
        [dependencies]
        clap = { version = "4", features = ["derive"] }
        console = "0.15"
        dialoguer = "0.11"
        indicatif = "0.17"
    "#]]
    .assert_eq(&deps);

    // Dev-deps land in [dev-dependencies]
    // [verify cli.add.dep-kind]
    expect![[r#"
        [dev-dependencies]
        assert_cmd = "2.0"
        predicates = "3.0"

    "#]]
    .assert_eq(&dev_deps);

    // Build-deps: only the battery pack itself (cc is hidden)
    // [verify format.hidden.effect]
    assert!(
        !build_deps.contains("cc = \"1.0\""),
        "cc is hidden and should not appear in [build-dependencies]: {build_deps}"
    );
}

// ============================================================================
// cli.add.specific-crates — only named crates written
// ============================================================================

// [verify cli.add.specific-crates]
#[test]
fn add_specific_crates_writes_only_named() {
    let tmp = make_temp_project();
    add(
        "fancy",
        "fancy-battery-pack",
        &[],
        FeatureMode::Default,
        &["clap"],
        None,
        tmp.path(),
    );

    let content = read_cargo_toml(&tmp);
    let deps = extract_section(&content, "[dependencies]");

    expect![[r#"
        [dependencies]
        clap = { version = "4", features = ["derive"] }
    "#]]
    .assert_eq(&deps);
}

// ============================================================================
// cli.add.unknown-crate — unknown skipped, valid written
// ============================================================================

// [verify cli.add.unknown-crate]
#[test]
fn add_unknown_crate_writes_valid_ones() {
    let tmp = make_temp_project();
    add(
        "fancy",
        "fancy-battery-pack",
        &[],
        FeatureMode::Default,
        &["nonexistent", "clap"],
        None,
        tmp.path(),
    );

    let content = read_cargo_toml(&tmp);
    let deps = extract_section(&content, "[dependencies]");

    expect![[r#"
        [dependencies]
        clap = { version = "4", features = ["derive"] }
    "#]]
    .assert_eq(&deps);
}

// ============================================================================
// cli.add.target — metadata location
// ============================================================================

// [verify cli.add.target]
#[test]
fn add_target_package_writes_metadata() {
    let tmp = make_temp_project();
    add(
        "basic",
        "basic-battery-pack",
        &["default"],
        FeatureMode::Default,
        &[],
        Some(bphelper_cli::AddTarget::Package),
        tmp.path(),
    );

    let content = read_cargo_toml(&tmp);
    let meta = extract_metadata(&content, "basic-battery-pack");

    expect![[r#"
        [package.metadata.battery-pack.basic-battery-pack]
        features = ["default"]
    "#]]
    .assert_eq(&meta);
}

// ============================================================================
// build.rs creation
// ============================================================================

// [verify cli.add.register]
#[test]
fn add_creates_build_rs() {
    let tmp = make_temp_project();
    add(
        "basic",
        "basic-battery-pack",
        &["default"],
        FeatureMode::Default,
        &[],
        None,
        tmp.path(),
    );

    let build_rs = std::fs::read_to_string(tmp.path().join("build.rs")).unwrap();

    expect![[r#"
        fn main() {
            basic_battery_pack::validate();
        }
    "#]]
    .assert_eq(&build_rs);
}

// ============================================================================
// Idempotency
// ============================================================================

// [verify cli.add.idempotent]
#[test]
fn add_twice_is_idempotent() {
    let tmp = make_temp_project();
    add(
        "basic",
        "basic-battery-pack",
        &["default"],
        FeatureMode::Default,
        &[],
        None,
        tmp.path(),
    );
    let first_content = read_cargo_toml(&tmp);

    add(
        "basic",
        "basic-battery-pack",
        &["default"],
        FeatureMode::Default,
        &[],
        None,
        tmp.path(),
    );
    let second_content = read_cargo_toml(&tmp);

    // The Cargo.toml should be identical after adding twice
    assert_eq!(
        first_content, second_content,
        "adding twice should be idempotent"
    );

    // build.rs should have exactly one validate call
    let build_rs = std::fs::read_to_string(tmp.path().join("build.rs")).unwrap();
    expect![[r#"
        fn main() {
            basic_battery_pack::validate();
        }
    "#]]
    .assert_eq(&build_rs);
}
