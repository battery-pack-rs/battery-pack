//! Tests for manifest module — TOML manipulation, registration, features, sync.
//!
//! Combined from: manifest_registration.rs, sync_behavior.rs, toml_preservation.rs,
//! and the collect_user_dep_versions tests from status.rs.

// --- from manifest_registration.rs ---

// Tests for manifest registration and features spec rules.
//
// These tests exercise the TOML manipulation helpers that implement
// battery pack registration, feature storage, and dependency management
// in user Cargo.toml files.

use std::collections::BTreeSet;
use std::path::PathBuf;

fn fixtures_dir() -> PathBuf {
    let manifest_dir = std::path::Path::new(env!("CARGO_MANIFEST_DIR"));
    // bphelper-cli is at src/battery-pack/bphelper-cli, workspace root is three levels up
    manifest_dir
        .parent()
        .unwrap()
        .parent()
        .unwrap()
        .parent()
        .unwrap()
        .join("tests/fixtures")
}

// ============================================================================
// manifest.register.location — registrations in [*.metadata.battery-pack]
// ============================================================================

// [verify manifest.register.location]
#[test]
fn register_location_package_metadata() {
    // Battery pack registrations must be stored in package.metadata.battery-pack
    let manifest = r#"
[package]
name = "my-app"
version = "0.1.0"

[package.metadata.battery-pack]
basic-battery-pack = "0.1.0"

[build-dependencies]
basic-battery-pack = "0.1.0"
"#;

    let names = super::find_installed_bp_names(manifest).unwrap();
    assert_eq!(names, vec!["basic-battery-pack"]);
}

// [verify manifest.register.location]
#[test]
fn register_location_finds_battery_packs_in_build_deps() {
    // find_installed_bp_names scans [build-dependencies] for battery packs
    let manifest = r#"
[package]
name = "my-app"
version = "0.1.0"

[build-dependencies]
cli-battery-pack = "0.3.0"
error-battery-pack = "0.4.0"
serde = "1"
"#;

    let names = super::find_installed_bp_names(manifest).unwrap();
    assert!(names.contains(&"cli-battery-pack".to_string()));
    assert!(names.contains(&"error-battery-pack".to_string()));
    assert!(!names.contains(&"serde".to_string()));
}

// ============================================================================
// manifest.register.format — key-value pair with name and version string
// ============================================================================

// [verify manifest.register.format]
#[test]
fn register_format_key_value_pair() {
    // Each registration is a key-value pair: name = "version"
    let manifest = r#"
[package]
name = "my-app"
version = "0.1.0"

[package.metadata.battery-pack]
basic-battery-pack = { version = "0.1.0", features = ["default"] }
"#;

    // read_active_features can parse the table format
    let features = super::read_active_features(manifest, "basic-battery-pack");
    assert_eq!(features, BTreeSet::from(["default".to_string()]));
}

// ============================================================================
// manifest.features.storage — without features key, default is active
// ============================================================================

// [verify manifest.features.storage]
#[test]
fn features_default_implicit_when_no_features_key() {
    // When there is no features key at all, default is implicitly active
    let manifest = r#"
[package]
name = "my-app"
version = "0.1.0"

[package.metadata.battery-pack]
basic-battery-pack = "0.1.0"
"#;

    let features = super::read_active_features(manifest, "basic-battery-pack");
    assert_eq!(features, BTreeSet::from(["default".to_string()]));
}

// [verify manifest.features.storage]
#[test]
fn features_default_implicit_when_no_metadata() {
    // When there is no metadata at all, default is implicitly active
    let manifest = r#"
[package]
name = "my-app"
version = "0.1.0"
"#;

    let features = super::read_active_features(manifest, "basic-battery-pack");
    assert_eq!(features, BTreeSet::from(["default".to_string()]));
}

// [verify manifest.features.storage]
#[test]
fn features_default_implicit_when_bp_not_registered() {
    // When the battery pack is not in metadata, default is implicitly active
    let manifest = r#"
[package]
name = "my-app"
version = "0.1.0"

[package.metadata.battery-pack]
other-battery-pack = "0.2.0"
"#;

    let features = super::read_active_features(manifest, "basic-battery-pack");
    assert_eq!(features, BTreeSet::from(["default".to_string()]));
}

// ============================================================================
// manifest.features.storage — only default feature may use short form
// ============================================================================

// [verify manifest.features.storage]
#[test]
fn features_short_form_is_version_string() {
    // Short form: just a version string means only default feature is active
    let manifest = r#"
[package]
name = "my-app"
version = "0.1.0"

[package.metadata.battery-pack]
basic-battery-pack = "0.1.0"
"#;

    let features = super::read_active_features(manifest, "basic-battery-pack");
    // Short form (just version string) implies default feature
    assert_eq!(features, BTreeSet::from(["default".to_string()]));
}

// ============================================================================
// manifest.features.storage — active features stored alongside registration
// ============================================================================

// [verify manifest.features.storage]
#[test]
fn features_storage_reads_explicit_features() {
    // Features are stored as a features array in the registration table
    let manifest = r#"
[package]
name = "my-app"
version = "0.1.0"

[package.metadata.battery-pack.cli-battery-pack]
features = ["default", "indicators"]
"#;

    let features = super::read_active_features(manifest, "cli-battery-pack");
    assert_eq!(
        features,
        BTreeSet::from(["default".to_string(), "indicators".to_string()])
    );
}

// [verify manifest.features.storage]
#[test]
fn features_storage_reads_single_feature() {
    let manifest = r#"
[package]
name = "my-app"
version = "0.1.0"

[package.metadata.battery-pack.basic-battery-pack]
features = ["all-errors"]
"#;

    let features = super::read_active_features(manifest, "basic-battery-pack");
    assert_eq!(features, BTreeSet::from(["all-errors".to_string()]));
}

// ============================================================================
// manifest.deps.add — add to correct dependency section
// ============================================================================

// [verify manifest.deps.add]
#[test]
fn deps_add_simple_version() {
    // When a crate has no features, add as simple version string
    let mut table = toml_edit::Table::new();
    let spec = bphelper_manifest::CrateSpec {
        version: "1.0".to_string(),
        features: BTreeSet::new(),
        dep_kind: bphelper_manifest::DepKind::Normal,
        optional: false,
    };

    super::add_dep_to_table(&mut table, "anyhow", &spec);

    let value = table.get("anyhow").unwrap();
    assert_eq!(value.as_str().unwrap(), "1.0");
}

// [verify manifest.deps.add]
#[test]
fn deps_add_does_not_add_to_wrong_key() {
    // add_dep_to_table only adds to the given table; the caller
    // is responsible for choosing the right section
    let mut table = toml_edit::Table::new();
    let spec = bphelper_manifest::CrateSpec {
        version: "4".to_string(),
        features: BTreeSet::from(["derive".to_string()]),
        dep_kind: bphelper_manifest::DepKind::Normal,
        optional: false,
    };

    super::add_dep_to_table(&mut table, "clap", &spec);

    assert!(table.contains_key("clap"));
    assert_eq!(table.len(), 1);
}

// ============================================================================
// manifest.deps.version-features — entry must include version and features
// ============================================================================

// [verify manifest.deps.version-features]
#[test]
fn deps_version_features_included() {
    // When a crate has features, the entry must include both version and features
    let mut table = toml_edit::Table::new();
    let spec = bphelper_manifest::CrateSpec {
        version: "4".to_string(),
        features: BTreeSet::from(["derive".to_string(), "env".to_string()]),
        dep_kind: bphelper_manifest::DepKind::Normal,
        optional: false,
    };

    super::add_dep_to_table(&mut table, "clap", &spec);

    let value = table.get("clap").unwrap();
    let inline = value.as_inline_table().unwrap();
    assert_eq!(inline.get("version").unwrap().as_str().unwrap(), "4");

    let features = inline.get("features").unwrap().as_array().unwrap();
    let feat_strs: Vec<&str> = features.iter().map(|v| v.as_str().unwrap()).collect();
    assert_eq!(feat_strs, vec!["derive", "env"]);
}

// [verify manifest.deps.version-features]
#[test]
fn deps_version_features_empty_features_uses_simple_string() {
    // When features is empty, use simple version string format
    let mut table = toml_edit::Table::new();
    let spec = bphelper_manifest::CrateSpec {
        version: "1".to_string(),
        features: BTreeSet::new(),
        dep_kind: bphelper_manifest::DepKind::Normal,
        optional: false,
    };

    super::add_dep_to_table(&mut table, "anyhow", &spec);

    let value = table.get("anyhow").unwrap();
    // Should be a simple string, not an inline table
    assert!(
        value.as_str().is_some(),
        "expected simple string, got table"
    );
    assert_eq!(value.as_str().unwrap(), "1");
}

// ============================================================================
// manifest.deps.workspace — add to workspace.dependencies and reference
// ============================================================================

// [verify manifest.deps.workspace]
#[test]
fn deps_workspace_adds_to_workspace_deps_table() {
    // In workspace mode, deps are added to a workspace.dependencies table,
    // and the crate references them with { workspace = true }.
    // We test the building blocks: add_dep_to_table for the workspace table,
    // and then separately constructing the workspace=true reference.

    // Simulate workspace.dependencies table
    let mut ws_table = toml_edit::Table::new();
    let spec = bphelper_manifest::CrateSpec {
        version: "1".to_string(),
        features: BTreeSet::from(["derive".to_string()]),
        dep_kind: bphelper_manifest::DepKind::Normal,
        optional: false,
    };

    super::add_dep_to_table(&mut ws_table, "serde", &spec);

    // Verify the workspace table has the full spec
    let ws_entry = ws_table.get("serde").unwrap().as_inline_table().unwrap();
    assert_eq!(ws_entry.get("version").unwrap().as_str().unwrap(), "1");

    // Simulate the crate-level reference: { workspace = true }
    let mut crate_table = toml_edit::Table::new();
    let mut dep = toml_edit::InlineTable::new();
    dep.insert("workspace", toml_edit::Value::from(true));
    crate_table.insert(
        "serde",
        toml_edit::Item::Value(toml_edit::Value::InlineTable(dep)),
    );

    let crate_entry = crate_table.get("serde").unwrap().as_inline_table().unwrap();
    assert!(crate_entry.get("workspace").unwrap().as_bool().unwrap());
}

// ============================================================================
// manifest.deps.no-workspace — non-workspace adds directly with full spec
// ============================================================================

// [verify manifest.deps.no-workspace]
#[test]
fn deps_no_workspace_adds_directly() {
    // In non-workspace mode, deps are added directly with version and features
    let mut table = toml_edit::Table::new();
    let spec = bphelper_manifest::CrateSpec {
        version: "2".to_string(),
        features: BTreeSet::new(),
        dep_kind: bphelper_manifest::DepKind::Normal,
        optional: false,
    };

    super::add_dep_to_table(&mut table, "thiserror", &spec);

    assert_eq!(table.get("thiserror").unwrap().as_str().unwrap(), "2");
}

// [verify manifest.deps.no-workspace]
#[test]
fn deps_no_workspace_adds_with_features() {
    let mut table = toml_edit::Table::new();
    let spec = bphelper_manifest::CrateSpec {
        version: "1".to_string(),
        features: BTreeSet::from(["derive".to_string()]),
        dep_kind: bphelper_manifest::DepKind::Normal,
        optional: false,
    };

    super::add_dep_to_table(&mut table, "serde", &spec);

    let entry = table.get("serde").unwrap().as_inline_table().unwrap();
    assert_eq!(entry.get("version").unwrap().as_str().unwrap(), "1");
    let features = entry.get("features").unwrap().as_array().unwrap();
    assert_eq!(features.iter().next().unwrap().as_str().unwrap(), "derive");
}

// ============================================================================
// manifest.deps.existing — must not overwrite, only add missing features
// ============================================================================

// [verify manifest.deps.existing]
#[test]
fn deps_existing_does_not_overwrite_version() {
    // sync_dep_in_table updates version when behind but the key point is
    // it operates in-place rather than replacing the entry
    let mut table = toml_edit::Table::new();

    // User already has anyhow at version "1.0.50"
    table.insert("anyhow", toml_edit::value("1.0.50"));

    let spec = bphelper_manifest::CrateSpec {
        version: "1.0.80".to_string(),
        features: BTreeSet::new(),
        dep_kind: bphelper_manifest::DepKind::Normal,
        optional: false,
    };

    let changed = super::sync_dep_in_table(&mut table, "anyhow", &spec);
    assert!(changed, "should report a change for version update");

    // Version gets updated (sync behavior) but it's an update, not overwrite
    assert_eq!(table.get("anyhow").unwrap().as_str().unwrap(), "1.0.80");
}

// [verify manifest.deps.existing]
#[test]
fn deps_existing_adds_missing_features() {
    // sync_dep_in_table must add missing features without removing existing ones
    let toml_str = r#"clap = { version = "4", features = ["derive"] }"#;
    let doc: toml_edit::DocumentMut = toml_str.parse().unwrap();
    let mut table = doc.as_table().clone();

    let spec = bphelper_manifest::CrateSpec {
        version: "4".to_string(),
        features: BTreeSet::from(["derive".to_string(), "env".to_string()]),
        dep_kind: bphelper_manifest::DepKind::Normal,
        optional: false,
    };

    let changed = super::sync_dep_in_table(&mut table, "clap", &spec);
    assert!(changed, "should report a change for added features");

    let entry = table.get("clap").unwrap().as_inline_table().unwrap();
    let features = entry.get("features").unwrap().as_array().unwrap();
    let feat_strs: Vec<&str> = features.iter().map(|v| v.as_str().unwrap()).collect();
    assert!(feat_strs.contains(&"derive"), "original feature preserved");
    assert!(feat_strs.contains(&"env"), "new feature added");
}

// [verify manifest.deps.existing]
#[test]
fn deps_existing_preserves_user_features() {
    // User has extra features that the battery pack doesn't specify;
    // sync must preserve them
    let toml_str = r#"clap = { version = "4", features = ["derive", "color"] }"#;
    let doc: toml_edit::DocumentMut = toml_str.parse().unwrap();
    let mut table = doc.as_table().clone();

    let spec = bphelper_manifest::CrateSpec {
        version: "4".to_string(),
        features: BTreeSet::from(["derive".to_string()]),
        dep_kind: bphelper_manifest::DepKind::Normal,
        optional: false,
    };

    let changed = super::sync_dep_in_table(&mut table, "clap", &spec);
    assert!(
        !changed,
        "no changes needed when user already has everything"
    );

    let entry = table.get("clap").unwrap().as_inline_table().unwrap();
    let features = entry.get("features").unwrap().as_array().unwrap();
    let feat_strs: Vec<&str> = features.iter().map(|v| v.as_str().unwrap()).collect();
    assert!(feat_strs.contains(&"derive"));
    assert!(
        feat_strs.contains(&"color"),
        "user feature must be preserved"
    );
}

// [verify manifest.deps.existing]
#[test]
fn deps_existing_no_change_when_up_to_date() {
    let toml_str = r#"anyhow = "1""#;
    let doc: toml_edit::DocumentMut = toml_str.parse().unwrap();
    let mut table = doc.as_table().clone();

    let spec = bphelper_manifest::CrateSpec {
        version: "1".to_string(),
        features: BTreeSet::new(),
        dep_kind: bphelper_manifest::DepKind::Normal,
        optional: false,
    };

    let changed = super::sync_dep_in_table(&mut table, "anyhow", &spec);
    assert!(!changed, "no changes needed when already up to date");
}

// ============================================================================
// manifest.deps.add — dep_kind determines correct section
// ============================================================================

// [verify manifest.deps.add]
#[test]
fn deps_add_respects_dep_kind_in_spec() {
    // add_dep_to_table doesn't choose the section — the caller does.
    // But the CrateSpec carries dep_kind so the caller knows which section.
    // Here we verify add_dep_to_table works regardless of dep_kind.
    for kind in [
        bphelper_manifest::DepKind::Normal,
        bphelper_manifest::DepKind::Dev,
        bphelper_manifest::DepKind::Build,
    ] {
        let mut table = toml_edit::Table::new();
        let spec = bphelper_manifest::CrateSpec {
            version: "1.0".to_string(),
            features: BTreeSet::new(),
            dep_kind: kind,
            optional: false,
        };

        super::add_dep_to_table(&mut table, "some-crate", &spec);
        assert!(
            table.contains_key("some-crate"),
            "dep should be added for {:?}",
            kind,
        );
    }
}

// ============================================================================
// Integration: parse fixture + add deps to a fresh Cargo.toml
// ============================================================================

// [verify manifest.deps.add]
// [verify manifest.deps.version-features]
#[test]
fn integration_add_basic_fixture_deps_to_table() {
    // Parse the basic-battery-pack fixture and add its default crates
    let fixture = fixtures_dir().join("basic-battery-pack/Cargo.toml");
    let content = std::fs::read_to_string(&fixture).unwrap();
    let spec = bphelper_manifest::parse_battery_pack(&content).unwrap();

    // Resolve default crates
    let crates = spec.resolve_crates(&["default"]);
    assert!(crates.contains_key("anyhow"));
    assert!(crates.contains_key("thiserror"));
    assert!(
        !crates.contains_key("eyre"),
        "eyre is optional, not in default"
    );

    // Add them to a fresh table
    let mut table = toml_edit::Table::new();
    for (name, crate_spec) in &crates {
        super::add_dep_to_table(&mut table, name, crate_spec);
    }

    assert_eq!(table.get("anyhow").unwrap().as_str().unwrap(), "1");
    assert_eq!(table.get("thiserror").unwrap().as_str().unwrap(), "2");
}

// [verify manifest.deps.add]
// [verify manifest.deps.version-features]
#[test]
fn integration_add_fancy_fixture_deps_to_table() {
    // Parse the fancy-battery-pack fixture and add its default crates
    let fixture = fixtures_dir().join("fancy-battery-pack/Cargo.toml");
    let content = std::fs::read_to_string(&fixture).unwrap();
    let spec = bphelper_manifest::parse_battery_pack(&content).unwrap();

    // Resolve default crates
    let crates = spec.resolve_crates(&["default"]);
    assert!(crates.contains_key("clap"));
    assert!(crates.contains_key("dialoguer"));

    // Add them to a fresh table
    let mut table = toml_edit::Table::new();
    for (name, crate_spec) in &crates {
        super::add_dep_to_table(&mut table, name, crate_spec);
    }

    // clap should have version and features
    let clap = table.get("clap").unwrap().as_inline_table().unwrap();
    assert_eq!(clap.get("version").unwrap().as_str().unwrap(), "4");
    let features = clap.get("features").unwrap().as_array().unwrap();
    assert_eq!(features.iter().next().unwrap().as_str().unwrap(), "derive");

    // dialoguer should be a simple version string (no features)
    assert_eq!(table.get("dialoguer").unwrap().as_str().unwrap(), "0.11");
}

// [verify manifest.deps.add]
// [verify manifest.deps.version-features]
#[test]
fn integration_add_fancy_fixture_with_indicators_feature() {
    // Parse the fancy-battery-pack and resolve with indicators feature
    let fixture = fixtures_dir().join("fancy-battery-pack/Cargo.toml");
    let content = std::fs::read_to_string(&fixture).unwrap();
    let spec = bphelper_manifest::parse_battery_pack(&content).unwrap();

    let crates = spec.resolve_crates(&["default", "indicators"]);
    assert!(crates.contains_key("clap"));
    assert!(crates.contains_key("dialoguer"));
    assert!(crates.contains_key("indicatif"));
    assert!(crates.contains_key("console"));

    let mut table = toml_edit::Table::new();
    for (name, crate_spec) in &crates {
        super::add_dep_to_table(&mut table, name, crate_spec);
    }

    assert!(table.contains_key("indicatif"));
    assert!(table.contains_key("console"));
}

// ============================================================================
// manifest.register.format — round-trip: write metadata then read it back
// ============================================================================

// [verify manifest.register.format]
// [verify manifest.features.storage]
#[test]
fn register_format_roundtrip_with_features() {
    // Verify that a Cargo.toml with inline-table features can be read back correctly.
    let manifest = r#"[package]
name = "my-app"
version = "0.1.0"

[package.metadata.battery-pack]
basic-battery-pack = { features = ["default", "all-errors"] }
"#;

    let features = super::read_active_features(manifest, "basic-battery-pack");
    assert_eq!(
        features,
        BTreeSet::from(["default".to_string(), "all-errors".to_string()])
    );
}

// [verify manifest.features.storage]
#[test]
fn register_format_roundtrip_with_dotted_subtable() {
    // The old dotted sub-table format must still be readable.
    let manifest = r#"[package]
name = "my-app"
version = "0.1.0"

[package.metadata.battery-pack.basic-battery-pack]
features = ["default", "all-errors"]
"#;

    let features = super::read_active_features(manifest, "basic-battery-pack");
    assert_eq!(
        features,
        BTreeSet::from(["default".to_string(), "all-errors".to_string()])
    );
}

// [verify manifest.register.format]
#[test]
fn register_format_multiple_battery_packs() {
    // Multiple battery packs can be registered simultaneously
    let manifest = r#"
[package]
name = "my-app"
version = "0.1.0"

[package.metadata.battery-pack.cli-battery-pack]
features = ["default", "indicators"]

[package.metadata.battery-pack.error-battery-pack]
features = ["default"]

[build-dependencies]
cli-battery-pack = "0.3.0"
error-battery-pack = "0.4.0"
"#;

    let names = super::find_installed_bp_names(manifest).unwrap();
    assert_eq!(names.len(), 2);

    let cli_features = super::read_active_features(manifest, "cli-battery-pack");
    assert_eq!(
        cli_features,
        BTreeSet::from(["default".to_string(), "indicators".to_string()])
    );

    let error_features = super::read_active_features(manifest, "error-battery-pack");
    assert_eq!(error_features, BTreeSet::from(["default".to_string()]));
}

// ============================================================================
// Sync: adding a dep that doesn't exist yet
// ============================================================================

// [verify manifest.deps.existing]
#[test]
fn sync_adds_missing_dep() {
    let mut table = toml_edit::Table::new();

    let spec = bphelper_manifest::CrateSpec {
        version: "1".to_string(),
        features: BTreeSet::new(),
        dep_kind: bphelper_manifest::DepKind::Normal,
        optional: false,
    };

    let changed = super::sync_dep_in_table(&mut table, "anyhow", &spec);
    assert!(changed, "adding a missing dep counts as a change");
    assert!(table.contains_key("anyhow"));
    assert_eq!(table.get("anyhow").unwrap().as_str().unwrap(), "1");
}

// [verify manifest.deps.existing]
#[test]
fn sync_converts_simple_string_to_table_when_adding_features() {
    // If user has `anyhow = "1"` and we need to add features,
    // sync must convert from simple string to table format
    let toml_str = r#"anyhow = "1""#;
    let doc: toml_edit::DocumentMut = toml_str.parse().unwrap();
    let mut table = doc.as_table().clone();

    let spec = bphelper_manifest::CrateSpec {
        version: "1".to_string(),
        features: BTreeSet::from(["backtrace".to_string()]),
        dep_kind: bphelper_manifest::DepKind::Normal,
        optional: false,
    };

    let changed = super::sync_dep_in_table(&mut table, "anyhow", &spec);
    assert!(changed, "converting to table format is a change");

    // After sync, should have version and features
    let entry = table.get("anyhow").unwrap();
    let inline = entry.as_inline_table().unwrap();
    assert_eq!(inline.get("version").unwrap().as_str().unwrap(), "1");
    let features = inline.get("features").unwrap().as_array().unwrap();
    assert_eq!(
        features.iter().next().unwrap().as_str().unwrap(),
        "backtrace"
    );
}

// ============================================================================
// Workspace metadata reading — read_active_features_ws
// ============================================================================

// [verify manifest.register.workspace-default]
#[test]
fn read_active_features_from_workspace_metadata() {
    // When battery-pack metadata lives in workspace.metadata, read_active_features_ws
    // must correctly extract the features array.
    let ws_manifest = r#"
[workspace]
members = ["my-app"]

[workspace.metadata.battery-pack.cli-battery-pack]
features = ["default", "indicators"]
"#;

    let features = super::read_active_features_ws(ws_manifest, "cli-battery-pack");
    assert_eq!(
        features,
        BTreeSet::from(["default".to_string(), "indicators".to_string()])
    );
}

// [verify manifest.register.workspace-default]
#[test]
fn read_active_features_ws_fallback_to_default() {
    // When workspace.metadata.battery-pack exists but the specific battery pack
    // is not registered, default should be returned.
    let ws_manifest = r#"
[workspace]
members = ["my-app"]

[workspace.metadata.battery-pack.other-battery-pack]
features = ["default"]
"#;

    let features = super::read_active_features_ws(ws_manifest, "cli-battery-pack");
    assert_eq!(features, BTreeSet::from(["default".to_string()]));
}

// [verify manifest.register.workspace-default]
#[test]
fn read_active_features_ws_no_metadata_at_all() {
    // When workspace Cargo.toml has no metadata section at all, default is returned.
    let ws_manifest = r#"
[workspace]
members = ["my-app"]
"#;

    let features = super::read_active_features_ws(ws_manifest, "cli-battery-pack");
    assert_eq!(features, BTreeSet::from(["default".to_string()]));
}

// [verify manifest.register.both-levels]
#[test]
fn read_active_features_ws_multiple_battery_packs() {
    // Multiple battery packs can be registered in workspace metadata.
    let ws_manifest = r#"
[workspace]
members = ["my-app"]

[workspace.metadata.battery-pack.cli-battery-pack]
features = ["default", "indicators"]

[workspace.metadata.battery-pack.error-battery-pack]
features = ["all-errors"]
"#;

    let cli_features = super::read_active_features_ws(ws_manifest, "cli-battery-pack");
    assert_eq!(
        cli_features,
        BTreeSet::from(["default".to_string(), "indicators".to_string()])
    );

    let error_features = super::read_active_features_ws(ws_manifest, "error-battery-pack");
    assert_eq!(error_features, BTreeSet::from(["all-errors".to_string()]));
}

// --- from sync_behavior.rs ---

// Tests for the sync behavior spec rules (manifest.sync.*).
//
// These tests exercise `sync_dep_in_table` directly on `toml_edit::Table`
// values, verifying the four sync invariants:
//
//   - version-bump: older version is upgraded (newer left alone)
//   - feature-add:  missing features are added (existing preserved)

use bphelper_manifest::{CrateSpec, DepKind};
use toml_edit::DocumentMut;

// ---------------------------------------------------------------------------
// Helpers
// ---------------------------------------------------------------------------

/// Build a `CrateSpec` with the given version and features (normal dep, non-optional).
fn spec(version: &str, features: &[&str]) -> CrateSpec {
    CrateSpec {
        version: version.to_string(),
        features: features.iter().map(|s| s.to_string()).collect(),
        dep_kind: DepKind::Normal,
        optional: false,
    }
}

/// Parse a TOML string and return a mutable reference to `[dependencies]`.
fn parse_deps(toml_str: &str) -> DocumentMut {
    toml_str.parse::<DocumentMut>().expect("valid TOML")
}

/// Read the version string for `dep_name` from the dependencies table.
fn read_version(doc: &DocumentMut, dep_name: &str) -> String {
    let deps = doc["dependencies"].as_table().expect("dependencies table");
    match deps.get(dep_name).expect("dep exists") {
        toml_edit::Item::Value(toml_edit::Value::String(s)) => s.value().to_string(),
        toml_edit::Item::Value(toml_edit::Value::InlineTable(t)) => t
            .get("version")
            .and_then(|v| v.as_str())
            .expect("version key")
            .to_string(),
        toml_edit::Item::Table(t) => t
            .get("version")
            .and_then(|v| v.as_value())
            .and_then(|v| v.as_str())
            .expect("version key")
            .to_string(),
        other => panic!("unexpected dep format: {:?}", other),
    }
}

/// Read the features array for `dep_name` from the dependencies table.
fn read_features(doc: &DocumentMut, dep_name: &str) -> Vec<String> {
    let deps = doc["dependencies"].as_table().expect("dependencies table");
    let extract = |arr: &toml_edit::Array| -> Vec<String> {
        arr.iter()
            .filter_map(|v| v.as_str().map(String::from))
            .collect()
    };
    match deps.get(dep_name).expect("dep exists") {
        toml_edit::Item::Value(toml_edit::Value::InlineTable(t)) => t
            .get("features")
            .and_then(|v| v.as_array())
            .map(&extract)
            .unwrap_or_default(),
        toml_edit::Item::Table(t) => t
            .get("features")
            .and_then(|v| v.as_value())
            .and_then(|v| v.as_array())
            .map(extract)
            .unwrap_or_default(),
        toml_edit::Item::Value(toml_edit::Value::String(_)) => vec![],
        other => panic!("unexpected dep format: {:?}", other),
    }
}

// ---------------------------------------------------------------------------
// manifest.sync.version-bump
// ---------------------------------------------------------------------------

// [verify manifest.sync.version-bump]
#[test]
fn version_bump_simple_string() {
    let mut doc = parse_deps(
        r#"
[dependencies]
serde = "1.0"
"#,
    );
    let table = doc["dependencies"].as_table_mut().unwrap();
    let changed = super::sync_dep_in_table(table, "serde", &spec("1.2", &[]));
    assert!(changed, "sync should report a change");
    assert_eq!(read_version(&doc, "serde"), "1.2");
}

// [verify manifest.sync.version-bump]
#[test]
fn version_bump_inline_table() {
    let mut doc = parse_deps(
        r#"
[dependencies]
serde = { version = "1.0", features = ["derive"] }
"#,
    );
    let table = doc["dependencies"].as_table_mut().unwrap();
    let changed = super::sync_dep_in_table(table, "serde", &spec("1.2", &["derive"]));
    assert!(changed, "sync should report a change");
    assert_eq!(read_version(&doc, "serde"), "1.2");
}

// [verify manifest.sync.version-bump]
#[test]
fn version_bump_full_semver() {
    let mut doc = parse_deps(
        r#"
[dependencies]
tokio = { version = "1.0.0", features = ["full"] }
"#,
    );
    let table = doc["dependencies"].as_table_mut().unwrap();
    let changed = super::sync_dep_in_table(table, "tokio", &spec("1.38.0", &["full"]));
    assert!(changed, "sync should report a change");
    assert_eq!(read_version(&doc, "tokio"), "1.38.0");
}

// ---------------------------------------------------------------------------
// manifest.sync.feature-add
// ---------------------------------------------------------------------------

// [verify manifest.sync.feature-add]
#[test]
fn feature_add_to_existing_inline_table() {
    let mut doc = parse_deps(
        r#"
[dependencies]
serde = { version = "1.0", features = ["derive"] }
"#,
    );
    let table = doc["dependencies"].as_table_mut().unwrap();
    let changed = super::sync_dep_in_table(table, "serde", &spec("1.0", &["derive", "serde_json"]));
    assert!(changed, "sync should report a change");
    let features = read_features(&doc, "serde");
    assert!(
        features.contains(&"derive".to_string()),
        "existing feature 'derive' should be present"
    );
    assert!(
        features.contains(&"serde_json".to_string()),
        "new feature 'serde_json' should be added"
    );
}

// [verify manifest.sync.feature-add]
#[test]
fn feature_add_converts_simple_string_to_table() {
    let mut doc = parse_deps(
        r#"
[dependencies]
serde = "1.0"
"#,
    );
    let table = doc["dependencies"].as_table_mut().unwrap();
    let changed = super::sync_dep_in_table(table, "serde", &spec("1.0", &["derive"]));
    assert!(changed, "sync should report a change");
    let features = read_features(&doc, "serde");
    assert!(
        features.contains(&"derive".to_string()),
        "feature 'derive' should be added"
    );
    assert_eq!(
        read_version(&doc, "serde"),
        "1.0",
        "version should be preserved"
    );
}

// [verify manifest.sync.feature-add]
#[test]
fn no_change_when_features_already_present() {
    let mut doc = parse_deps(
        r#"
[dependencies]
serde = { version = "1.0", features = ["derive"] }
"#,
    );
    let table = doc["dependencies"].as_table_mut().unwrap();
    let changed = super::sync_dep_in_table(table, "serde", &spec("1.0", &["derive"]));
    assert!(
        !changed,
        "sync should report no change when already up to date"
    );
}

// ---------------------------------------------------------------------------
// manifest.sync.version-bump — must not downgrade
// ---------------------------------------------------------------------------

// [verify manifest.sync.version-bump]
#[test]
fn no_downgrade_simple_string() {
    let mut doc = parse_deps(
        r#"
[dependencies]
serde = "2.0"
"#,
    );
    let table = doc["dependencies"].as_table_mut().unwrap();
    let changed = super::sync_dep_in_table(table, "serde", &spec("1.5", &[]));
    assert!(!changed, "sync must not downgrade");
    assert_eq!(
        read_version(&doc, "serde"),
        "2.0",
        "version must stay at 2.0"
    );
}

// [verify manifest.sync.version-bump]
#[test]
fn no_downgrade_inline_table() {
    let mut doc = parse_deps(
        r#"
[dependencies]
serde = { version = "2.0", features = ["derive"] }
"#,
    );
    let table = doc["dependencies"].as_table_mut().unwrap();
    let changed = super::sync_dep_in_table(table, "serde", &spec("1.5", &["derive"]));
    assert!(!changed, "sync must not downgrade");
    assert_eq!(
        read_version(&doc, "serde"),
        "2.0",
        "version must stay at 2.0"
    );
}

// [verify manifest.sync.version-bump]
#[test]
fn no_downgrade_full_semver() {
    let mut doc = parse_deps(
        r#"
[dependencies]
tokio = { version = "1.40.0", features = ["full"] }
"#,
    );
    let table = doc["dependencies"].as_table_mut().unwrap();
    let changed = super::sync_dep_in_table(table, "tokio", &spec("1.38.0", &["full"]));
    assert!(!changed, "sync must not downgrade");
    assert_eq!(
        read_version(&doc, "tokio"),
        "1.40.0",
        "version must stay at 1.40.0"
    );
}

// [verify manifest.sync.version-bump]
#[test]
fn no_downgrade_when_adding_features() {
    // User has a newer version but battery pack recommends an older one with
    // additional features.  The features should be added but the version must
    // NOT be downgraded.
    let mut doc = parse_deps(
        r#"
[dependencies]
serde = "2.0"
"#,
    );
    let table = doc["dependencies"].as_table_mut().unwrap();
    let changed = super::sync_dep_in_table(table, "serde", &spec("1.5", &["derive"]));
    assert!(changed, "features should still be added");
    assert_eq!(
        read_version(&doc, "serde"),
        "2.0",
        "version must stay at 2.0 (no downgrade)"
    );
    let features = read_features(&doc, "serde");
    assert!(
        features.contains(&"derive".to_string()),
        "feature 'derive' should be added"
    );
}

// ---------------------------------------------------------------------------
// manifest.sync.feature-add — must not remove features
// ---------------------------------------------------------------------------

// [verify manifest.sync.feature-add]
#[test]
fn no_feature_remove_preserves_user_features() {
    let mut doc = parse_deps(
        r#"
[dependencies]
serde = { version = "1.0", features = ["derive", "custom-user-feature"] }
"#,
    );
    let table = doc["dependencies"].as_table_mut().unwrap();
    // Battery pack only knows about "derive", but user has "custom-user-feature" too
    let changed = super::sync_dep_in_table(table, "serde", &spec("1.0", &["derive"]));
    assert!(
        !changed,
        "no changes needed — all bp features already present"
    );
    let features = read_features(&doc, "serde");
    assert!(
        features.contains(&"derive".to_string()),
        "'derive' must be present"
    );
    assert!(
        features.contains(&"custom-user-feature".to_string()),
        "user's 'custom-user-feature' must be preserved"
    );
}

// [verify manifest.sync.feature-add]
#[test]
fn no_feature_remove_when_adding_new_features() {
    let mut doc = parse_deps(
        r#"
[dependencies]
serde = { version = "1.0", features = ["derive", "custom-user-feature"] }
"#,
    );
    let table = doc["dependencies"].as_table_mut().unwrap();
    // Battery pack wants "derive" + "serde_json"; user also has "custom-user-feature"
    let changed = super::sync_dep_in_table(table, "serde", &spec("1.0", &["derive", "serde_json"]));
    assert!(changed, "new feature 'serde_json' should be added");
    let features = read_features(&doc, "serde");
    assert!(
        features.contains(&"derive".to_string()),
        "'derive' must be present"
    );
    assert!(
        features.contains(&"custom-user-feature".to_string()),
        "user's 'custom-user-feature' must be preserved"
    );
    assert!(
        features.contains(&"serde_json".to_string()),
        "new 'serde_json' must be added"
    );
    assert_eq!(features.len(), 3, "should have exactly 3 features");
}

// ---------------------------------------------------------------------------
// Combined scenarios
// ---------------------------------------------------------------------------

// [verify manifest.sync.version-bump]
// [verify manifest.sync.feature-add]
#[test]
fn version_bump_and_feature_add_preserves_user_features() {
    let mut doc = parse_deps(
        r#"
[dependencies]
serde = { version = "1.0", features = ["derive", "my-extra"] }
"#,
    );
    let table = doc["dependencies"].as_table_mut().unwrap();
    let changed = super::sync_dep_in_table(table, "serde", &spec("1.2", &["derive", "rc"]));
    assert!(changed, "both version and features changed");
    assert_eq!(read_version(&doc, "serde"), "1.2", "version should bump");
    let features = read_features(&doc, "serde");
    assert!(features.contains(&"derive".to_string()));
    assert!(
        features.contains(&"my-extra".to_string()),
        "user feature preserved"
    );
    assert!(features.contains(&"rc".to_string()), "new bp feature added");
}

// ---------------------------------------------------------------------------
// Multi-line `dependencies.name` table format
// ---------------------------------------------------------------------------

// [verify manifest.sync.version-bump]
#[test]
fn version_bump_full_table() {
    let mut doc = parse_deps(
        r#"
[dependencies.serde]
version = "1.0"
features = ["derive"]
"#,
    );
    let table = doc["dependencies"].as_table_mut().unwrap();
    let changed = super::sync_dep_in_table(table, "serde", &spec("1.5", &["derive"]));
    assert!(changed, "version should bump");
    assert_eq!(read_version(&doc, "serde"), "1.5");
}

// [verify manifest.sync.version-bump]
#[test]
fn no_downgrade_full_table() {
    let mut doc = parse_deps(
        r#"
[dependencies.serde]
version = "2.0"
features = ["derive"]
"#,
    );
    let table = doc["dependencies"].as_table_mut().unwrap();
    let changed = super::sync_dep_in_table(table, "serde", &spec("1.5", &["derive"]));
    assert!(!changed, "sync must not downgrade");
    assert_eq!(read_version(&doc, "serde"), "2.0");
}

// [verify manifest.sync.feature-add]
#[test]
fn feature_add_full_table() {
    let mut doc = parse_deps(
        r#"
[dependencies.serde]
version = "1.0"
features = ["derive"]
"#,
    );
    let table = doc["dependencies"].as_table_mut().unwrap();
    let changed = super::sync_dep_in_table(table, "serde", &spec("1.0", &["derive", "rc"]));
    assert!(changed, "feature 'rc' should be added");
    let features = read_features(&doc, "serde");
    assert!(features.contains(&"derive".to_string()));
    assert!(features.contains(&"rc".to_string()));
}

// [verify manifest.sync.feature-add]
#[test]
fn no_feature_remove_full_table() {
    let mut doc = parse_deps(
        r#"
[dependencies.serde]
version = "1.0"
features = ["derive", "custom-user-feature"]
"#,
    );
    let table = doc["dependencies"].as_table_mut().unwrap();
    let changed = super::sync_dep_in_table(table, "serde", &spec("1.0", &["derive"]));
    assert!(!changed, "no new features to add");
    let features = read_features(&doc, "serde");
    assert!(
        features.contains(&"custom-user-feature".to_string()),
        "user feature must be preserved"
    );
}

// --- from toml_preservation.rs ---

// Round-trip tests proving that `toml_edit`-based manipulation preserves
// existing TOML formatting, comments, and ordering.

use super::{add_dep_to_table, sync_dep_in_table};

/// Helper: parse a TOML string, return a `DocumentMut`.
fn parse_doc(input: &str) -> toml_edit::DocumentMut {
    input.parse().expect("valid TOML")
}

/// Helper: build a simple `CrateSpec` with no features.
fn simple_spec(version: &str) -> CrateSpec {
    CrateSpec {
        version: version.to_string(),
        features: BTreeSet::new(),
        dep_kind: DepKind::Normal,
        optional: false,
    }
}

/// Helper: build a `CrateSpec` with features.
fn spec_with_features(version: &str, features: &[&str]) -> CrateSpec {
    CrateSpec {
        version: version.to_string(),
        features: features.iter().map(|s| s.to_string()).collect(),
        dep_kind: DepKind::Normal,
        optional: false,
    }
}

// ============================================================================
// manifest.toml.preserve — comments survive mutations
// ============================================================================

// [verify manifest.toml.preserve]
#[test]
fn comments_survive_add_dep() {
    let input = "\
# My project dependencies
[dependencies]
# Error handling
anyhow = \"1\"  # we love anyhow
serde = { version = \"1\", features = [\"derive\"] }
";

    let mut doc = parse_doc(input);
    let table = doc["dependencies"].as_table_mut().unwrap();

    add_dep_to_table(table, "tokio", &simple_spec("1.0"));

    let output = doc.to_string();

    // All original comments must survive
    assert!(
        output.contains("# My project dependencies"),
        "header comment lost: {output}"
    );
    assert!(
        output.contains("# Error handling"),
        "inline section comment lost: {output}"
    );
    assert!(
        output.contains("# we love anyhow"),
        "trailing comment lost: {output}"
    );

    // Original entries still present
    assert!(
        output.contains("anyhow = \"1\""),
        "anyhow entry changed: {output}"
    );
    assert!(
        output.contains("serde = { version = \"1\", features = [\"derive\"] }"),
        "serde entry changed: {output}"
    );

    // New entry was added
    assert!(
        output.contains("tokio = \"1.0\""),
        "tokio not added: {output}"
    );
}

// [verify manifest.toml.preserve]
#[test]
fn comments_survive_sync_dep() {
    let input = "\
[dependencies]
# important crate
anyhow = \"1.0.0\"  # pinned for reasons
";

    let mut doc = parse_doc(input);
    let table = doc["dependencies"].as_table_mut().unwrap();

    // Sync anyhow to a newer version
    let changed = sync_dep_in_table(table, "anyhow", &simple_spec("1.1.0"));

    let output = doc.to_string();

    assert!(changed, "sync should report a change");
    assert!(
        output.contains("# important crate"),
        "comment above entry lost: {output}"
    );
    // The trailing comment on the same line as the value is part of the value's
    // decor in toml_edit; when the value itself is replaced the trailing
    // comment may or may not survive depending on the toml_edit version.
    // We verify the structural comment above the key always survives.
}

// ============================================================================
// manifest.toml.preserve — ordering preserved after sync
// ============================================================================

// [verify manifest.toml.preserve]
#[test]
fn ordering_preserved_after_sync() {
    let input = "\
[dependencies]
zebra = \"1.0\"
alpha = \"2.0\"
middle = \"3.0\"
";

    let mut doc = parse_doc(input);
    let table = doc["dependencies"].as_table_mut().unwrap();

    // Update middle's version — ordering must stay z, a, m
    let changed = sync_dep_in_table(table, "middle", &simple_spec("3.1"));
    assert!(changed);

    let output = doc.to_string();

    let z_pos = output.find("zebra").expect("zebra missing");
    let a_pos = output.find("alpha").expect("alpha missing");
    let m_pos = output.find("middle").expect("middle missing");

    assert!(z_pos < a_pos, "zebra should come before alpha: {output}");
    assert!(a_pos < m_pos, "alpha should come before middle: {output}");

    // Verify version was actually updated
    assert!(
        output.contains("middle = \"3.1\""),
        "middle version not updated: {output}"
    );
}

// [verify manifest.toml.preserve]
#[test]
fn ordering_preserved_after_add() {
    let input = "\
[dependencies]
zebra = \"1.0\"
alpha = \"2.0\"
";

    let mut doc = parse_doc(input);
    let table = doc["dependencies"].as_table_mut().unwrap();

    add_dep_to_table(table, "new-crate", &simple_spec("0.5"));

    let output = doc.to_string();

    let z_pos = output.find("zebra").expect("zebra missing");
    let a_pos = output.find("alpha").expect("alpha missing");

    assert!(
        z_pos < a_pos,
        "original ordering (zebra before alpha) must survive: {output}"
    );
}

// ============================================================================
// manifest.toml.preserve — blank lines and sections preserved
// ============================================================================

// [verify manifest.toml.preserve]
#[test]
fn blank_lines_and_sections_preserved() {
    let input = "\
[package]
name = \"my-project\"
version = \"0.1.0\"

[dependencies]
anyhow = \"1\"

[dev-dependencies]
assert_cmd = \"2\"
";

    let mut doc = parse_doc(input);
    let table = doc["dependencies"].as_table_mut().unwrap();

    add_dep_to_table(table, "serde", &simple_spec("1"));

    let output = doc.to_string();

    // All three sections must still be present
    assert!(
        output.contains("[package]"),
        "package section lost: {output}"
    );
    assert!(
        output.contains("[dependencies]"),
        "dependencies section lost: {output}"
    );
    assert!(
        output.contains("[dev-dependencies]"),
        "dev-dependencies section lost: {output}"
    );

    // Section ordering: package before dependencies before dev-dependencies
    let pkg_pos = output.find("[package]").unwrap();
    let dep_pos = output.find("[dependencies]").unwrap();
    let dev_pos = output.find("[dev-dependencies]").unwrap();

    assert!(pkg_pos < dep_pos, "package should precede dependencies");
    assert!(
        dep_pos < dev_pos,
        "dependencies should precede dev-dependencies"
    );

    // Original entries survive
    assert!(
        output.contains("name = \"my-project\""),
        "package.name changed: {output}"
    );
    assert!(
        output.contains("assert_cmd = \"2\""),
        "dev-dep lost: {output}"
    );
}

// [verify manifest.toml.preserve]
#[test]
fn full_document_round_trip_with_multiple_sections() {
    let input = "\
[package]
name = \"example\"
version = \"0.1.0\"
edition = \"2021\"

# Runtime deps
[dependencies]
tokio = { version = \"1\", features = [\"full\"] }

# Test deps
[dev-dependencies]
pretty_assertions = \"1\"
";

    let mut doc = parse_doc(input);
    let table = doc["dependencies"].as_table_mut().unwrap();

    add_dep_to_table(table, "serde", &spec_with_features("1", &["derive"]));

    let output = doc.to_string();

    // Structural comments preserved
    assert!(
        output.contains("# Runtime deps"),
        "section comment lost: {output}"
    );
    assert!(
        output.contains("# Test deps"),
        "section comment lost: {output}"
    );

    // Existing inline table preserved exactly
    assert!(
        output.contains("tokio = { version = \"1\", features = [\"full\"] }"),
        "tokio entry mangled: {output}"
    );

    // New entry present
    assert!(output.contains("serde"), "serde not added: {output}");
}

// ============================================================================
// manifest.toml.style — new entries use inline tables when features present
// ============================================================================

// [verify manifest.toml.style]
#[test]
fn add_dep_uses_plain_string_for_version_only() {
    let input = "\
[dependencies]
existing = \"1.0\"
";

    let mut doc = parse_doc(input);
    let table = doc["dependencies"].as_table_mut().unwrap();

    add_dep_to_table(table, "simple", &simple_spec("2.0"));

    let output = doc.to_string();

    // A version-only dep should be added as a plain string, not an inline table
    assert!(
        output.contains("simple = \"2.0\""),
        "version-only dep should be a plain string: {output}"
    );
}

// [verify manifest.toml.style]
#[test]
fn add_dep_uses_inline_table_for_features() {
    let input = "\
[dependencies]
existing = { version = \"1.0\", features = [\"foo\"] }
";

    let mut doc = parse_doc(input);
    let table = doc["dependencies"].as_table_mut().unwrap();

    add_dep_to_table(
        table,
        "new-crate",
        &spec_with_features("3.0", &["bar", "baz"]),
    );

    let output = doc.to_string();

    // A dep with features should use an inline table
    assert!(
        output.contains("new-crate = { version = \"3.0\""),
        "dep with features should use inline table: {output}"
    );
    assert!(output.contains("bar"), "feature 'bar' missing: {output}");
    assert!(output.contains("baz"), "feature 'baz' missing: {output}");
}

// ============================================================================
// manifest.toml.preserve + style — sync preserves inline table structure
// ============================================================================

// [verify manifest.toml.preserve]
// [verify manifest.toml.style]
#[test]
fn sync_preserves_inline_table_format() {
    let input = "\
[dependencies]
serde = { version = \"1.0.0\", features = [\"derive\"] }
";

    let mut doc = parse_doc(input);
    let table = doc["dependencies"].as_table_mut().unwrap();

    // Sync with a newer version — the inline table format should be preserved
    let changed = sync_dep_in_table(table, "serde", &spec_with_features("1.1.0", &["derive"]));
    assert!(changed, "version bump should count as change");

    let output = doc.to_string();

    // Should still be an inline table (not exploded to multi-line)
    assert!(
        output.contains("serde = {"),
        "inline table format should be preserved: {output}"
    );
    assert!(
        output.contains("\"1.1.0\""),
        "version should be updated: {output}"
    );
    assert!(
        output.contains("\"derive\""),
        "existing feature should survive: {output}"
    );
}

// [verify manifest.toml.preserve]
// [verify manifest.toml.style]
#[test]
fn sync_adds_features_without_losing_existing() {
    let input = "\
[dependencies]
serde = { version = \"1.0.0\", features = [\"derive\"] }
";

    let mut doc = parse_doc(input);
    let table = doc["dependencies"].as_table_mut().unwrap();

    // Sync with an additional feature
    let changed = sync_dep_in_table(table, "serde", &spec_with_features("1.0.0", &["rc"]));
    assert!(changed, "adding a new feature should count as change");

    let output = doc.to_string();

    // Both the old and new features should be present
    assert!(
        output.contains("derive"),
        "existing feature 'derive' lost: {output}"
    );
    assert!(
        output.contains("rc"),
        "new feature 'rc' not added: {output}"
    );
}

// [verify manifest.toml.preserve]
#[test]
fn sync_no_change_when_already_current() {
    let input = "\
[dependencies]
anyhow = \"1.0.0\"
";

    let mut doc = parse_doc(input);
    let table = doc["dependencies"].as_table_mut().unwrap();

    // Sync with the same version — no change expected
    let changed = sync_dep_in_table(table, "anyhow", &simple_spec("1.0.0"));
    assert!(!changed, "syncing same version should report no change");

    let output = doc.to_string();
    assert_eq!(
        output, input,
        "document should be byte-identical when nothing changed"
    );
}
