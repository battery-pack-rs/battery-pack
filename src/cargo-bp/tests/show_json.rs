//! Integration tests for `cargo bp show --json`.

use assert_cmd::Command;
use cargo_bp_script::{SCHEMA_VERSION, ShowCommand, parse_show};
use snapbox::{assert_data_eq, str};
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
fn show_json_emits_valid_schema() {
    let fixture = fixtures_dir().join("fancy-battery-pack");

    let output = cargo_bp()
        .args([
            "bp",
            "show",
            "--json",
            "--path",
            &fixture.to_string_lossy(),
            "fancy",
        ])
        .output()
        .expect("failed to run cargo-bp");

    assert!(
        output.status.success(),
        "stderr: {}",
        String::from_utf8_lossy(&output.stderr)
    );

    let report = parse_show(&output.stdout).unwrap_or_else(|err| {
        panic!(
            "parse_show failed: {err}\nraw stdout: {:?}",
            String::from_utf8_lossy(&output.stdout)
        )
    });

    assert_eq!(report.schema_version, SCHEMA_VERSION);
    assert_eq!(report.short_name, "fancy");
    assert_eq!(report.name, "fancy-battery-pack");
    assert_eq!(report.version, "0.2.0");
    assert_eq!(report.description, "A feature-rich test battery pack");
    assert_eq!(
        report.repository.as_deref(),
        Some("https://github.com/example/fancy")
    );

    // Crates: hidden deps (serde*, cc) should be excluded.
    assert!(
        report.crates.contains(&"clap".to_string()),
        "expected clap in crates, got {:?}",
        report.crates
    );
    assert!(
        !report.crates.contains(&"serde".to_string()),
        "serde should be hidden"
    );
    assert!(
        !report.crates.contains(&"cc".to_string()),
        "cc should be hidden"
    );

    // Features
    assert!(
        report.features.iter().any(|f| f.name == "indicators"),
        "expected indicators feature, got {:?}",
        report.features
    );

    // Templates
    assert!(
        report.templates.len() >= 2,
        "expected at least 2 templates, got {}",
        report.templates.len()
    );
    let default_tmpl = report
        .templates
        .iter()
        .find(|t| t.name == "default")
        .expect("expected 'default' template");
    assert_eq!(default_tmpl.description.as_deref(), Some("Basic CLI app"));
}

#[test]
fn show_json_via_crate_source() {
    let output = cargo_bp()
        .args([
            "bp",
            "--crate-source",
            &fixtures_dir().to_string_lossy(),
            "show",
            "--json",
            "basic",
        ])
        .output()
        .expect("failed to run cargo-bp");

    assert!(
        output.status.success(),
        "stderr: {}",
        String::from_utf8_lossy(&output.stderr)
    );

    let stdout = String::from_utf8_lossy(&output.stdout);
    assert_data_eq!(
        stdout.as_ref(),
        str![[r#"
{"schema_version":"1","short_name":"basic","name":"basic-battery-pack","version":"0.1.0","description":"A simple test battery pack","repository":null,"owners":[],"crates":["anyhow","eyre","thiserror"],"extends":[],"features":[{"name":"all-errors","crates":["anyhow","eyre","thiserror"]},{"name":"default","crates":["anyhow","thiserror"]}],"templates":[],"examples":[],"active_features":["default"]}

"#]]
    );
}

#[test]
fn show_json_unknown_pack_fails() {
    let output = cargo_bp()
        .args([
            "bp",
            "--crate-source",
            &fixtures_dir().to_string_lossy(),
            "show",
            "--json",
            "nonexistent-xyz",
        ])
        .output()
        .expect("failed to run cargo-bp");

    assert!(!output.status.success());
    assert!(
        output.stdout.is_empty(),
        "stdout should be empty on error, got: {:?}",
        String::from_utf8_lossy(&output.stdout),
    );
}

#[test]
fn show_json_conflicts_with_template() {
    let fixture = fixtures_dir().join("fancy-battery-pack");

    let output = cargo_bp()
        .args([
            "bp",
            "show",
            "--json",
            "--template",
            "default",
            "--path",
            &fixture.to_string_lossy(),
            "fancy",
        ])
        .output()
        .expect("failed to run cargo-bp");

    assert!(
        !output.status.success(),
        "should fail when --json and --template are combined"
    );
    let stderr = String::from_utf8_lossy(&output.stderr);
    assert!(
        stderr.contains("cannot be used with"),
        "expected conflict error, got: {stderr}"
    );
}

#[test]
fn show_command_runner_returns_typed_report() {
    let fixture = fixtures_dir().join("fancy-battery-pack");

    let report = ShowCommand::new("fancy")
        .program(assert_cmd::cargo::cargo_bin!("cargo-bp"))
        .path(&fixture)
        .run()
        .expect("ShowCommand::run failed");

    assert_eq!(report.schema_version, SCHEMA_VERSION);
    assert_eq!(report.name, "fancy-battery-pack");
    assert!(!report.crates.is_empty());
    assert!(!report.templates.is_empty());
}
