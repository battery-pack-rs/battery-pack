//! Tests for bp-managed dependency resolution.

use std::path::Path;

fn fixtures_dir() -> std::path::PathBuf {
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

/// Resolve bp-managed deps in a Cargo.toml string using the given fixture as the bp crate root.
fn resolve_with_fixture(cargo_toml: &str, bp_crate_root: &Path) -> anyhow::Result<String> {
    bphelper_cli::resolve_bp_managed_content(cargo_toml, bp_crate_root)
}

#[test]
fn resolve_bp_managed_resolves_versions() {
    use expect_test::expect;

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

    expect![[r#"
        [package]
        name = "my-app"
        version = "0.1.0"

        [dependencies]
        anyhow = "1"
        clap = { version = "4", features = ["derive"] }

        [build-dependencies]
        managed-battery-pack = "0.2.0"

        [package.metadata.battery-pack]
        managed-battery-pack = { features = ["default"] }
    "#]]
    .assert_eq(&result);
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
nonexistent = { bp-managed = true }

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
fn resolve_bp_managed_dotted_key_syntax() {
    use expect_test::expect;

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

    expect![[r#"
        [package]
        name = "my-app"
        version = "0.1.0"

        [dependencies]
        anyhow = "1"
        clap = { version = "4", features = ["derive"] }

        [build-dependencies]
        managed-battery-pack = "0.2.0"

        [package.metadata.battery-pack]
        managed-battery-pack = { features = ["default"] }
    "#]]
    .assert_eq(&result);
}
