//! Integration tests for `cargo bp list --json`.

use assert_cmd::Command;
use cargo_bp_script::{ListCommand, SCHEMA_VERSION, parse_list};
use std::path::{Path, PathBuf};

fn cargo_bp() -> Command {
    Command::new(assert_cmd::cargo::cargo_bin!("cargo-bp"))
}

fn fixtures_dir() -> PathBuf {
    Path::new(env!("CARGO_MANIFEST_DIR"))
        .parent()
        .unwrap()
        .join("battery-pack")
        .parent()
        .unwrap()
        .parent()
        .unwrap()
        .join("tests/fixtures")
}

#[test]
fn list_json_emits_valid_schema() {
    let output = cargo_bp()
        .args([
            "bp",
            "--crate-source",
            &fixtures_dir().to_string_lossy(),
            "list",
            "--json",
        ])
        .output()
        .expect("failed to run cargo-bp");

    assert!(
        output.status.success(),
        "stderr: {}",
        String::from_utf8_lossy(&output.stderr)
    );

    let report = parse_list(&output.stdout).unwrap_or_else(|err| {
        panic!(
            "parse_list failed: {err}\nraw stdout: {:?}",
            String::from_utf8_lossy(&output.stdout)
        )
    });

    assert_eq!(report.schema_version, SCHEMA_VERSION);
    assert!(report.filter.is_none());

    // The fixtures workspace has at least basic, fancy, and managed battery packs.
    assert!(
        report.packs.len() >= 3,
        "expected at least 3 packs, got {}",
        report.packs.len()
    );

    // Verify structure of a known pack.
    let fancy = report
        .packs
        .iter()
        .find(|p| p.short_name == "fancy")
        .expect("expected fancy-battery-pack in list");
    assert_eq!(fancy.name, "fancy-battery-pack");
    assert_eq!(fancy.version, "0.2.0");
    assert!(!fancy.description.is_empty());
}

#[test]
fn list_json_with_filter() {
    let output = cargo_bp()
        .args([
            "bp",
            "--crate-source",
            &fixtures_dir().to_string_lossy(),
            "list",
            "--json",
            "fancy",
        ])
        .output()
        .expect("failed to run cargo-bp");

    assert!(
        output.status.success(),
        "stderr: {}",
        String::from_utf8_lossy(&output.stderr)
    );

    let report = parse_list(&output.stdout).unwrap();
    assert_eq!(report.filter.as_deref(), Some("fancy"));
    assert_eq!(report.packs.len(), 1);
    assert_eq!(report.packs[0].short_name, "fancy");
}

#[test]
fn list_json_no_match_returns_empty_array() {
    let output = cargo_bp()
        .args([
            "bp",
            "--crate-source",
            &fixtures_dir().to_string_lossy(),
            "list",
            "--json",
            "nonexistent-xyz",
        ])
        .output()
        .expect("failed to run cargo-bp");

    assert!(
        output.status.success(),
        "stderr: {}",
        String::from_utf8_lossy(&output.stderr)
    );

    let report = parse_list(&output.stdout).unwrap();
    assert!(report.packs.is_empty());
    assert_eq!(report.filter.as_deref(), Some("nonexistent-xyz"));
}

#[test]
fn list_command_runner_returns_typed_report() {
    let report = ListCommand::new()
        .program(assert_cmd::cargo::cargo_bin!("cargo-bp"))
        .crate_source(fixtures_dir())
        .run()
        .expect("ListCommand::run failed");

    assert_eq!(report.schema_version, SCHEMA_VERSION);
    assert!(report.packs.len() >= 3);
    assert!(report.packs.iter().any(|p| p.short_name == "fancy"));
}
