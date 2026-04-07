//! Tests for registry module — list, show, local discovery.
//!
//! Combined from: list.rs, show.rs.

// --- from list.rs ---

// Integration tests for `cargo bp list` with `--crate-source`.
//
// These tests use `CrateSource::Local` pointing at the test fixtures
// workspace to verify discovery and filtering without network access.
//
// Covers:
//   - cli.source.flag          — --crate-source accepts a workspace path
//   - cli.source.discover      — discovers crates ending in -battery-pack
//   - cli.source.replace       — no crates.io calls when source is local
//   - cli.list.query           — lists available battery packs
//   - cli.list.filter          — filters results by name pattern

use super::CrateSource;
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

fn format_summaries(packs: &[super::BatteryPackSummary]) -> String {
    packs
        .iter()
        .map(|bp| format!("{} {} -- {}", bp.name, bp.version, bp.description))
        .collect::<Vec<_>>()
        .join("\n")
}

// [verify cli.source.discover]
// [verify cli.list.query]
#[test]
fn list_discovers_local_battery_packs() {
    let source = CrateSource::Local(fixtures_dir());
    let packs = super::fetch_battery_pack_list(&source, None).unwrap();
    let formatted = format_summaries(&packs);
    assert!(
        formatted.contains("basic-battery-pack"),
        "Expected basic-battery-pack"
    );
    assert!(
        formatted.contains("broken-battery-pack"),
        "Expected broken-battery-pack"
    );
    assert!(
        formatted.contains("fancy-battery-pack"),
        "Expected fancy-battery-pack"
    );
    assert!(
        formatted.contains("managed-battery-pack"),
        "Expected managed-battery-pack"
    );
}

// [verify cli.list.filter]
#[test]
fn list_filter_narrows_results() {
    let source = CrateSource::Local(fixtures_dir());
    let packs = super::fetch_battery_pack_list(&source, Some("basic")).unwrap();
    let formatted = format_summaries(&packs);
    assert_eq!(packs.len(), 1, "Expected exactly 1 result");
    assert!(
        formatted.contains("basic-battery-pack"),
        "Expected basic-battery-pack"
    );
}

// [verify cli.list.filter]
#[test]
fn list_filter_no_match_returns_empty() {
    let source = CrateSource::Local(fixtures_dir());
    let packs = super::fetch_battery_pack_list(&source, Some("nonexistent")).unwrap();
    assert!(packs.is_empty());
}

// [verify cli.source.flag]
#[test]
fn list_invalid_workspace_path_errors() {
    let source = CrateSource::Local(PathBuf::from("/nonexistent/path"));
    let result = super::fetch_battery_pack_list(&source, None);
    assert!(result.is_err());
}

// [verify cli.source.discover]
#[test]
fn list_short_names_are_correct() {
    let source = CrateSource::Local(fixtures_dir());
    let packs = super::fetch_battery_pack_list(&source, None).unwrap();
    let short_names: Vec<&str> = packs.iter().map(|bp| bp.short_name.as_str()).collect();
    assert_eq!(short_names.len(), 4, "Expected 4 packs");
    assert!(short_names.contains(&"basic"), "Expected 'basic'");
    assert!(short_names.contains(&"broken"), "Expected 'broken'");
    assert!(short_names.contains(&"fancy"), "Expected 'fancy'");
    assert!(short_names.contains(&"managed"), "Expected 'managed'");
}

// --- from show.rs ---

// Integration tests for `cargo bp show`.

// [verify cli.show.hidden]
#[test]
fn show_detail_excludes_hidden_crates() {
    let fancy_path = fixtures_dir().join("fancy-battery-pack");
    let detail =
        super::fetch_battery_pack_detail("fancy", Some(fancy_path.to_str().unwrap())).unwrap();

    // hidden = ["serde*", "cc"] in the fancy fixture
    // Glob-matched deps excluded
    assert!(!detail.crates.contains(&"serde".to_string()));
    assert!(!detail.crates.contains(&"serde_json".to_string()));
    // Exact-matched build-dep excluded
    assert!(!detail.crates.contains(&"cc".to_string()));

    // Visible crates still present
    assert!(detail.crates.contains(&"clap".to_string()));
    assert!(detail.crates.contains(&"dialoguer".to_string()));
    assert!(detail.crates.contains(&"indicatif".to_string()));
    assert!(detail.crates.contains(&"console".to_string()));
    assert!(detail.crates.contains(&"assert_cmd".to_string()));
    assert!(detail.crates.contains(&"predicates".to_string()));
}

// [verify cli.show.hidden]
#[test]
fn show_detail_no_hidden_returns_all_crates() {
    let basic_path = fixtures_dir().join("basic-battery-pack");
    let detail =
        super::fetch_battery_pack_detail("basic", Some(basic_path.to_str().unwrap())).unwrap();

    // basic fixture has no hidden config — all crates should appear
    assert!(detail.crates.contains(&"anyhow".to_string()));
    assert!(detail.crates.contains(&"thiserror".to_string()));
    assert!(detail.crates.contains(&"eyre".to_string()));
    assert_eq!(detail.crates.len(), 3);
}

// --- from bp_managed.rs ---

// Tests for bp-managed dependency resolution.

use std::path::Path;

/// Resolve bp-managed deps in a Cargo.toml string using the given fixture as the bp crate root.
fn resolve_with_fixture(cargo_toml: &str, bp_crate_root: &Path) -> anyhow::Result<String> {
    super::resolve_bp_managed_content(cargo_toml, bp_crate_root)
}

#[test]
fn resolve_bp_managed_resolves_versions() {
    let bp_root = fixtures_dir().join("managed-battery-pack");
    let cargo_toml = r#"[package]
name = "my-app"
version = "0.1.0"

[dependencies]
anyhow.bp-managed = true
clap.bp-managed = true

[build-dependencies]
managed-battery-pack.bp-managed = true

[package.metadata.battery-pack]
managed-battery-pack = { features = ["default"] }
"#;

    let result = resolve_with_fixture(cargo_toml, &bp_root).unwrap();

    assert!(
        result.contains(r#"name = "my-app""#),
        "Expected package name"
    );
    assert!(
        result.contains(r#"anyhow = "1""#),
        "Expected anyhow version"
    );
    assert!(
        result.contains(r#"clap = { version = "4""#),
        "Expected clap version"
    );
    assert!(
        result.contains(r#"managed-battery-pack = "0.2.0""#),
        "Expected managed-battery-pack version"
    );
}

#[test]
fn resolve_bp_managed_resolves_dev_and_build_deps() {
    let bp_root = fixtures_dir().join("managed-battery-pack");
    let cargo_toml = r#"[package]
name = "my-app"
version = "0.1.0"

[dependencies]
anyhow.bp-managed = true

[dev-dependencies]
insta.bp-managed = true

[build-dependencies]
cc.bp-managed = true

[package.metadata.battery-pack]
managed-battery-pack = { features = ["default"] }
"#;

    let result = resolve_with_fixture(cargo_toml, &bp_root).unwrap();

    assert!(
        result.contains(r#"name = "my-app""#),
        "Expected package name"
    );
    assert!(result.contains(r#"anyhow = "1""#), "Expected anyhow");
    assert!(result.contains(r#"insta = "1.34""#), "Expected insta");
    assert!(result.contains(r#"cc = "1.0""#), "Expected cc");
}

#[test]
fn resolve_bp_managed_errors_on_version_and_bp_managed() {
    let bp_root = fixtures_dir().join("managed-battery-pack");
    let cargo_toml = r#"[package]
name = "my-app"
version = "0.1.0"

[dependencies]
anyhow = { version = "1", bp-managed = true }

[package.metadata.battery-pack]
managed-battery-pack = { features = ["default"] }
"#;

    let err = resolve_with_fixture(cargo_toml, &bp_root).unwrap_err();
    assert!(
        err.to_string().contains("bp-managed") && err.to_string().contains("conflicting keys"),
        "should error on both bp-managed and version: {err}"
    );
}

#[test]
fn resolve_bp_managed_leaves_explicit_versions_untouched() {
    let bp_root = fixtures_dir().join("managed-battery-pack");
    let cargo_toml = r#"[package]
name = "my-app"
version = "0.1.0"

[dependencies]
serde = "1.0.200"

[package.metadata.battery-pack]
managed-battery-pack = { features = ["default"] }
"#;

    let result = resolve_with_fixture(cargo_toml, &bp_root).unwrap();
    assert!(
        result.contains(r#"serde = "1.0.200""#),
        "explicit version should be untouched: {result}"
    );
}

#[test]
fn resolve_bp_managed_errors_on_unresolvable_dep() {
    let bp_root = fixtures_dir().join("managed-battery-pack");
    let cargo_toml = r#"[package]
name = "my-app"
version = "0.1.0"

[dependencies]
nonexistent.bp-managed = true

[package.metadata.battery-pack]
managed-battery-pack = { features = ["default"] }
"#;

    let err = resolve_with_fixture(cargo_toml, &bp_root).unwrap_err();
    assert!(
        err.to_string().contains("nonexistent")
            && err.to_string().contains("no battery pack provides it"),
        "should error on unresolvable dep: {err}"
    );
}

#[test]
fn resolve_bp_managed_noop_without_managed_deps() {
    let bp_root = fixtures_dir().join("managed-battery-pack");
    let cargo_toml = r#"[package]
name = "my-app"
version = "0.1.0"

[dependencies]
serde = "1"
"#;

    let result = resolve_with_fixture(cargo_toml, &bp_root).unwrap();
    assert!(
        result.contains(r#"serde = "1""#),
        "should be unchanged: {result}"
    );
}

#[test]
fn resolve_bp_managed_errors_on_features_and_bp_managed() {
    let bp_root = fixtures_dir().join("managed-battery-pack");
    let cargo_toml = r#"[package]
name = "my-app"
version = "0.1.0"

[dependencies]
clap = { bp-managed = true, features = ["derive"] }

[package.metadata.battery-pack]
managed-battery-pack = { features = ["default"] }
"#;

    let err = resolve_with_fixture(cargo_toml, &bp_root).unwrap_err();
    assert!(
        err.to_string().contains("conflicting keys") && err.to_string().contains("features"),
        "should error on bp-managed with features: {err}"
    );
}

#[test]
fn resolve_bp_managed_errors_on_no_default_features_and_bp_managed() {
    let bp_root = fixtures_dir().join("managed-battery-pack");
    let cargo_toml = r#"[package]
name = "my-app"
version = "0.1.0"

[dependencies]
clap = { bp-managed = true, default-features = false }

[package.metadata.battery-pack]
managed-battery-pack = { features = ["default"] }
"#;

    let err = resolve_with_fixture(cargo_toml, &bp_root).unwrap_err();
    assert!(
        err.to_string().contains("conflicting keys")
            && err.to_string().contains("default-features"),
        "should error on bp-managed with default-features: {err}"
    );
}

#[test]
fn resolve_bp_managed_inline_table_syntax() {
    let bp_root = fixtures_dir().join("managed-battery-pack");
    let cargo_toml = r#"[package]
name = "my-app"
version = "0.1.0"

[dependencies]
anyhow = { bp-managed = true }
clap = { bp-managed = true }

[build-dependencies]
managed-battery-pack = { bp-managed = true }

[package.metadata.battery-pack]
managed-battery-pack = { features = ["default"] }
"#;

    let result = resolve_with_fixture(cargo_toml, &bp_root).unwrap();

    assert!(
        result.contains(r#"name = "my-app""#),
        "Expected package name"
    );
    assert!(result.contains(r#"anyhow = "1""#), "Expected anyhow");
    assert!(
        result.contains(r#"clap = { version = "4""#),
        "Expected clap"
    );
    assert!(
        result.contains(r#"managed-battery-pack = "0.2.0""#),
        "Expected managed-battery-pack"
    );
}
