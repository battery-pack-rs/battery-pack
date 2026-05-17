//! Integration tests for `cargo bp status --json`.
//!
//! Exercises both the binary directly (via `assert_cmd`) and the
//! `cargo-bp-script` runner that wraps it.

use assert_cmd::Command;
use cargo_bp_script::{SCHEMA_VERSION, StatusCommand, parse_status};
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

/// Build a temp project that has `fancy-battery-pack` registered as a
/// build-dep and an outdated `clap` regular dep that should produce a
/// version warning relative to the fixture's recommendation.
fn make_project_with_outdated_clap() -> tempfile::TempDir {
    let tmp = tempfile::tempdir().unwrap();
    std::fs::write(
        tmp.path().join("Cargo.toml"),
        r#"
[package]
name = "test-consumer"
version = "0.1.0"
edition = "2021"

[dependencies]
clap = "3.0"

[build-dependencies]
fancy-battery-pack = "0.2.0"
"#,
    )
    .unwrap();
    // Stub src/lib.rs so the manifest is plausibly a real crate.
    std::fs::create_dir_all(tmp.path().join("src")).unwrap();
    std::fs::write(tmp.path().join("src/lib.rs"), "").unwrap();
    tmp
}

#[test]
fn status_json_emits_valid_schema() {
    let tmp = make_project_with_outdated_clap();
    let fixture = fixtures_dir().join("fancy-battery-pack");

    let output = cargo_bp()
        .args([
            "bp",
            "status",
            "--json",
            "--path",
            &fixture.to_string_lossy(),
        ])
        .current_dir(tmp.path())
        .output()
        .expect("failed to run cargo-bp");

    assert!(
        output.status.success(),
        "stderr: {}",
        String::from_utf8_lossy(&output.stderr)
    );

    // --- Parse via the script crate so we exercise `parse_status`.
    let report = parse_status(&output.stdout).unwrap_or_else(|err| {
        panic!(
            "parse_status failed: {err}\nraw stdout: {:?}",
            output.stdout
        )
    });

    assert_eq!(report.schema_version, SCHEMA_VERSION);
    // Compare via canonicalisation to avoid macOS `/var` vs `/private/var`
    // surprises when comparing tempdir paths.
    let expected_manifest = tmp.path().join("Cargo.toml").canonicalize().unwrap();
    let got_manifest = report.project.manifest_path.canonicalize().unwrap();
    assert_eq!(
        got_manifest, expected_manifest,
        "manifest_path should point at the project's Cargo.toml",
    );

    // We registered exactly one battery pack (fancy).
    assert_eq!(report.packs.len(), 1, "expected one installed pack");
    let pack = &report.packs[0];
    assert_eq!(pack.short_name, "fancy");
    assert_eq!(pack.name, "fancy-battery-pack");
    assert_eq!(pack.version, "0.2.0");

    // The fixture recommends clap 4 → user has 3.0 → expect a warning.
    let clap_warning = pack
        .warnings
        .iter()
        .find(|w| w.crate_name == "clap")
        .expect("expected a clap version warning");
    assert_eq!(clap_warning.current_version, "3.0");
    assert!(
        clap_warning.recommended_version.starts_with('4'),
        "fancy-battery-pack recommends a 4.x clap, got: {}",
        clap_warning.recommended_version,
    );
}

#[test]
fn status_command_runner_returns_typed_report() {
    let tmp = make_project_with_outdated_clap();
    let fixture = fixtures_dir().join("fancy-battery-pack");

    // Use the locally-built `cargo-bp` binary directly so we don't need
    // it on PATH for this test to work.
    let report = StatusCommand::new()
        .program(assert_cmd::cargo::cargo_bin!("cargo-bp"))
        .cwd(tmp.path())
        .path(&fixture)
        .run()
        .expect("StatusCommand::run failed");

    assert_eq!(report.schema_version, SCHEMA_VERSION);
    assert_eq!(report.packs.len(), 1);
    assert_eq!(report.packs[0].name, "fancy-battery-pack");
    assert!(
        report.packs[0]
            .warnings
            .iter()
            .any(|w| w.crate_name == "clap"),
        "expected a clap version warning",
    );
}

#[test]
fn status_json_outside_project_fails_cleanly() {
    let tmp = tempfile::tempdir().unwrap();

    let output = cargo_bp()
        .args(["bp", "status", "--json"])
        .current_dir(tmp.path())
        .output()
        .expect("failed to run cargo-bp");

    // No Cargo.toml → should error, and the error should travel to stderr,
    // leaving stdout empty so the script runner's `parse_status` doesn't
    // get a misleading half-payload.
    assert!(!output.status.success());
    assert!(
        output.stdout.is_empty(),
        "stdout should be empty on error, got: {:?}",
        String::from_utf8_lossy(&output.stdout),
    );
}

#[test]
fn status_json_no_packs_emits_empty_packs_array() {
    let tmp = tempfile::tempdir().unwrap();
    std::fs::write(
        tmp.path().join("Cargo.toml"),
        r#"
[package]
name = "no-packs"
version = "0.1.0"
edition = "2021"

[dependencies]
serde = "1"
"#,
    )
    .unwrap();
    std::fs::create_dir_all(tmp.path().join("src")).unwrap();
    std::fs::write(tmp.path().join("src/lib.rs"), "").unwrap();

    let output = cargo_bp()
        .args(["bp", "status", "--json"])
        .current_dir(tmp.path())
        .output()
        .expect("failed to run cargo-bp");

    assert!(
        output.status.success(),
        "stderr: {}",
        String::from_utf8_lossy(&output.stderr)
    );

    let report =
        parse_status(&output.stdout).expect("parse_status should succeed for empty packs case");
    assert!(
        report.packs.is_empty(),
        "expected no packs, got {} packs",
        report.packs.len(),
    );
    assert_eq!(report.schema_version, SCHEMA_VERSION);
}
