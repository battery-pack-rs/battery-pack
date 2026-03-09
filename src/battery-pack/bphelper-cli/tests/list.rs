//! Integration tests for `cargo bp list` with `--crate-source`.
//!
//! These tests use `CrateSource::Local` pointing at the test fixtures
//! workspace to verify discovery and filtering without network access.
//!
//! Covers:
//!   - cli.source.flag          — --crate-source accepts a workspace path
//!   - cli.source.discover      — discovers crates ending in -battery-pack
//!   - cli.source.replace       — no crates.io calls when source is local
//!   - cli.list.query           — lists available battery packs
//!   - cli.list.filter          — filters results by name pattern

use bphelper_cli::CrateSource;
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

fn format_summaries(packs: &[bphelper_cli::BatteryPackSummary]) -> String {
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
    let packs = bphelper_cli::fetch_battery_pack_list(&source, None).unwrap();
    let formatted = format_summaries(&packs);
    expect![[r#"
        basic-battery-pack 0.1.0 -- A simple test battery pack
        broken-battery-pack 0.1.0 -- A deliberately broken battery pack for testing validation
        fancy-battery-pack 0.2.0 -- A feature-rich test battery pack
        managed-battery-pack 0.2.0 -- A test battery pack for bp-managed resolution"#]]
    .assert_eq(&formatted);
}

// [verify cli.list.filter]
#[test]
fn list_filter_narrows_results() {
    let source = CrateSource::Local(fixtures_dir());
    let packs = bphelper_cli::fetch_battery_pack_list(&source, Some("basic")).unwrap();
    let formatted = format_summaries(&packs);
    expect![[r#"
        basic-battery-pack 0.1.0 -- A simple test battery pack"#]]
    .assert_eq(&formatted);
}

// [verify cli.list.filter]
#[test]
fn list_filter_no_match_returns_empty() {
    let source = CrateSource::Local(fixtures_dir());
    let packs = bphelper_cli::fetch_battery_pack_list(&source, Some("nonexistent")).unwrap();
    assert!(packs.is_empty());
}

// [verify cli.source.flag]
#[test]
fn list_invalid_workspace_path_errors() {
    let source = CrateSource::Local(PathBuf::from("/nonexistent/path"));
    let result = bphelper_cli::fetch_battery_pack_list(&source, None);
    assert!(result.is_err());
}

// [verify cli.source.discover]
#[test]
fn list_short_names_are_correct() {
    let source = CrateSource::Local(fixtures_dir());
    let packs = bphelper_cli::fetch_battery_pack_list(&source, None).unwrap();
    let short_names: Vec<&str> = packs.iter().map(|bp| bp.short_name.as_str()).collect();
    expect![[r#"
        [
            "basic",
            "broken",
            "fancy",
            "managed",
        ]"#]]
    .assert_eq(&format!("{:#?}", short_names));
}
