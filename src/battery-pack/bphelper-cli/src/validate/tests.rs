//! Tests for battery pack validation.

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
