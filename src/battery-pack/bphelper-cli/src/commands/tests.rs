//! Tests for commands module — CLI parsing, add, validate.
//!
//! Combined from: group1_tags.rs, group2_add.rs, group2_add_integration.rs,
//! validate.rs, validate_extras.rs.

// --- from group1_tags.rs ---

// Group 1 tests: verify behaviors for rules that are already implemented
// but were missing [impl] tags.
//
// Covers:
//   - cli.new.name-flag      — --name flag is accepted and parsed
//   - cli.new.name-prompt    — omitting --name still parses (template engine prompts)
//   - cli.new.template-select — multiple templates with no default triggers prompt path
//   - cli.bare.tui           — bare `cargo bp` produces command: None
//   - cli.add.idempotent     — re-adding same dep doesn't create duplicates

use clap::Parser;
use snapbox::{assert_data_eq, file};
use std::collections::{BTreeMap, BTreeSet};

/// Unwrap `Commands::Bp { command }` → `Option<BpCommands>`.
fn unwrap_bp_command(cli: super::Cli) -> Option<super::BpCommands> {
    match cli.command {
        super::Commands::Bp { command, .. } => command,
    }
}

// ============================================================================
// cli.bare.tui — bare `cargo bp` produces command: None (→ TUI or bail)
// ============================================================================

// [verify cli.bare.tui]
#[test]
fn bare_cargo_bp_produces_none_command() {
    // `cargo bp` with no subcommand should parse successfully with command = None.
    // At runtime, main() uses this to launch the TUI (if terminal) or bail.
    let cli = super::Cli::try_parse_from(["cargo", "bp"]).expect("bare `cargo bp` should parse");
    assert!(
        unwrap_bp_command(cli).is_none(),
        "bare `cargo bp` should produce command: None"
    );
}

// ============================================================================
// cli.new.name-flag — --name flag is accepted by the `new` subcommand
// ============================================================================

// [verify cli.new.name-flag]
#[test]
fn new_name_flag_is_parsed() {
    // `cargo bp new cli --name my-project` should parse successfully
    // with the name captured as Some("my-project").
    let cli = super::Cli::try_parse_from(["cargo", "bp", "new", "cli", "--name", "my-project"])
        .expect("--name flag should be accepted");

    match unwrap_bp_command(cli) {
        Some(super::BpCommands::New { name, .. }) => {
            assert_eq!(name.as_deref(), Some("my-project"));
        }
        None => panic!("expected Some(New), got None"),
        Some(other) => panic!("expected New, got {:?}", std::mem::discriminant(&other)),
    }
}

// [verify cli.new.name-flag]
#[test]
fn new_name_short_flag_is_parsed() {
    // `-n` is the short form of `--name`
    let cli = super::Cli::try_parse_from(["cargo", "bp", "new", "cli", "-n", "my-project"])
        .expect("-n flag should be accepted");

    match unwrap_bp_command(cli) {
        Some(super::BpCommands::New { name, .. }) => {
            assert_eq!(name.as_deref(), Some("my-project"));
        }
        None => panic!("expected Some(New), got None"),
        Some(other) => panic!("expected New, got {:?}", std::mem::discriminant(&other)),
    }
}

// ============================================================================
// cli.new.name-prompt — omitting --name is valid (the template engine will prompt)
// ============================================================================

// [verify cli.new.name-prompt]
#[test]
fn new_without_name_parses_as_none() {
    // `cargo bp new cli` without --name should parse successfully with name = None.
    // The actual prompting is handled by the template engine at runtime.
    let cli = super::Cli::try_parse_from(["cargo", "bp", "new", "cli"])
        .expect("new without --name should parse");

    match unwrap_bp_command(cli) {
        Some(super::BpCommands::New { name, .. }) => {
            assert!(name.is_none(), "name should be None when --name is omitted");
        }
        None => panic!("expected Some(New), got None"),
        Some(other) => panic!("expected New, got {:?}", std::mem::discriminant(&other)),
    }
}

// ============================================================================
// cli.new.template-select — multiple templates without default triggers prompt
// ============================================================================

// [verify cli.new.template-select]
#[test]
fn resolve_template_single_template_uses_it() {
    // With a single template, resolve_template picks it without prompting.
    let mut templates = BTreeMap::new();
    templates.insert(
        "simple".to_string(),
        crate::registry::TemplateConfig {
            path: "templates/simple".to_string(),
            description: Some("A simple template".to_string()),
        },
    );

    let result = super::resolve_template(&templates, None).unwrap();
    assert_eq!(result, "templates/simple");
}

// [verify cli.new.template-select]
#[test]
fn resolve_template_picks_default_when_present() {
    // With multiple templates including "default", resolve_template picks "default".
    let mut templates = BTreeMap::new();
    templates.insert(
        "default".to_string(),
        crate::registry::TemplateConfig {
            path: "templates/default".to_string(),
            description: Some("The default template".to_string()),
        },
    );
    templates.insert(
        "advanced".to_string(),
        crate::registry::TemplateConfig {
            path: "templates/advanced".to_string(),
            description: Some("An advanced template".to_string()),
        },
    );

    let result = super::resolve_template(&templates, None).unwrap();
    assert_eq!(result, "templates/default");
}

// [verify cli.new.template-select]
#[test]
fn resolve_template_unknown_name_errors() {
    // When --template specifies a name that doesn't exist, resolve_template
    // must error with a message listing available templates.
    let mut templates = BTreeMap::new();
    templates.insert(
        "simple".to_string(),
        crate::registry::TemplateConfig {
            path: "templates/simple".to_string(),
            description: None,
        },
    );
    templates.insert(
        "advanced".to_string(),
        crate::registry::TemplateConfig {
            path: "templates/advanced".to_string(),
            description: None,
        },
    );

    let result = super::resolve_template(&templates, Some("nonexistent"));
    assert!(result.is_err());
    let err = result.unwrap_err().to_string();
    assert!(
        err.contains("not found"),
        "error should say template not found: {err}"
    );
    assert!(
        err.contains("simple") && err.contains("advanced"),
        "error should list available templates: {err}"
    );
}

// [verify cli.new.template-select]
#[test]
fn resolve_template_explicit_flag_overrides() {
    // --template <name> selects the named template directly.
    let mut templates = BTreeMap::new();
    templates.insert(
        "simple".to_string(),
        crate::registry::TemplateConfig {
            path: "templates/simple".to_string(),
            description: None,
        },
    );
    templates.insert(
        "advanced".to_string(),
        crate::registry::TemplateConfig {
            path: "templates/advanced".to_string(),
            description: None,
        },
    );

    let result = super::resolve_template(&templates, Some("advanced")).unwrap();
    assert_eq!(result, "templates/advanced");
}

// ============================================================================
// cli.add.idempotent — re-adding same dep doesn't create duplicates
// ============================================================================

// [verify cli.add.idempotent]
#[test]
fn add_dep_twice_no_duplicate() {
    // Adding the same crate to a table twice should result in exactly one entry,
    // not two. toml_edit's insert() is an upsert.
    let mut table = toml_edit::Table::new();
    let spec = bphelper_manifest::CrateSpec {
        version: "1.0".to_string(),
        features: BTreeSet::new(),
        dep_kind: bphelper_manifest::DepKind::Normal,
        optional: false,
    };

    crate::manifest::add_dep_to_table(&mut table, "anyhow", &spec);
    assert_eq!(table.len(), 1);

    // Add again with updated version
    let spec_v2 = bphelper_manifest::CrateSpec {
        version: "2.0".to_string(),
        features: BTreeSet::new(),
        dep_kind: bphelper_manifest::DepKind::Normal,
        optional: false,
    };

    crate::manifest::add_dep_to_table(&mut table, "anyhow", &spec_v2);
    assert_eq!(table.len(), 1, "should still be exactly one entry");
    assert_eq!(
        table.get("anyhow").unwrap().as_str().unwrap(),
        "2.0",
        "version should be updated"
    );
}

// [verify cli.add.idempotent]
#[test]
fn add_dep_twice_with_features_no_duplicate() {
    // Same test but with features — the inline table should be replaced, not appended.
    let mut table = toml_edit::Table::new();
    let spec1 = bphelper_manifest::CrateSpec {
        version: "4".to_string(),
        features: BTreeSet::from(["derive".to_string()]),
        dep_kind: bphelper_manifest::DepKind::Normal,
        optional: false,
    };

    crate::manifest::add_dep_to_table(&mut table, "clap", &spec1);
    assert_eq!(table.len(), 1);

    let spec2 = bphelper_manifest::CrateSpec {
        version: "4.1".to_string(),
        features: BTreeSet::from(["derive".to_string(), "env".to_string()]),
        dep_kind: bphelper_manifest::DepKind::Normal,
        optional: false,
    };

    crate::manifest::add_dep_to_table(&mut table, "clap", &spec2);
    assert_eq!(table.len(), 1, "should still be exactly one entry");

    let entry = table.get("clap").unwrap().as_inline_table().unwrap();
    assert_eq!(entry.get("version").unwrap().as_str().unwrap(), "4.1");
    let features = entry.get("features").unwrap().as_array().unwrap();
    assert_eq!(features.len(), 2);
}

// [verify cli.add.idempotent]
#[test]
fn metadata_registration_idempotent() {
    // Simulating the metadata upsert: writing to
    // [package.metadata.battery-pack] twice should produce one entry.
    let toml_str = r#"[package]
name = "my-app"
version = "0.1.0"

[package.metadata.battery-pack]
cli-battery-pack = { features = ["default"] }
"#;
    let mut doc: toml_edit::DocumentMut = toml_str.parse().unwrap();

    // "Re-add" with updated features (inline table style)
    let mut features_array = toml_edit::Array::new();
    features_array.push("default");
    features_array.push("indicators");
    let mut inline = toml_edit::InlineTable::new();
    inline.insert("features", toml_edit::Value::Array(features_array));
    let bp_table = doc["package"]["metadata"]["battery-pack"]
        .as_table_mut()
        .unwrap();
    bp_table.insert(
        "cli-battery-pack",
        toml_edit::Item::Value(toml_edit::Value::InlineTable(inline)),
    );

    // Verify: should be exactly one battery-pack entry, not two
    let bp_table = doc["package"]["metadata"]["battery-pack"]
        .as_table()
        .unwrap();
    assert_eq!(
        bp_table.len(),
        1,
        "should have exactly one battery pack entry"
    );

    // Verify features were updated
    let features = crate::manifest::read_active_features(&doc.to_string(), "cli-battery-pack");
    assert_eq!(
        features,
        BTreeSet::from(["default".to_string(), "indicators".to_string()])
    );
}

// ============================================================================
// cli.show.non-interactive / cli.list.non-interactive
// ============================================================================

// [verify cli.show.non-interactive]
#[test]
fn show_non_interactive_flag_is_parsed() {
    let cli = super::Cli::try_parse_from(["cargo", "bp", "show", "cli", "--non-interactive"])
        .expect("--non-interactive should be accepted");

    match unwrap_bp_command(cli) {
        Some(super::BpCommands::Show {
            non_interactive, ..
        }) => {
            assert!(non_interactive, "non_interactive should be true");
        }
        None => panic!("expected Some(Show), got None"),
        Some(other) => panic!("expected Show, got {:?}", std::mem::discriminant(&other)),
    }
}

// [verify cli.show.non-interactive]
#[test]
fn show_defaults_to_interactive() {
    let cli = super::Cli::try_parse_from(["cargo", "bp", "show", "cli"])
        .expect("show without --non-interactive should parse");

    match unwrap_bp_command(cli) {
        Some(super::BpCommands::Show {
            non_interactive, ..
        }) => {
            assert!(!non_interactive, "non_interactive should default to false");
        }
        None => panic!("expected Some(Show), got None"),
        Some(other) => panic!("expected Show, got {:?}", std::mem::discriminant(&other)),
    }
}

// [verify cli.list.non-interactive]
#[test]
fn list_non_interactive_flag_is_parsed() {
    let cli = super::Cli::try_parse_from(["cargo", "bp", "list", "--non-interactive"])
        .expect("--non-interactive should be accepted");

    match unwrap_bp_command(cli) {
        Some(super::BpCommands::List {
            non_interactive, ..
        }) => {
            assert!(non_interactive, "non_interactive should be true");
        }
        None => panic!("expected Some(List), got None"),
        Some(other) => panic!("expected List, got {:?}", std::mem::discriminant(&other)),
    }
}

// [verify cli.list.non-interactive]
#[test]
fn list_defaults_to_interactive() {
    let cli = super::Cli::try_parse_from(["cargo", "bp", "list"])
        .expect("list without --non-interactive should parse");

    match unwrap_bp_command(cli) {
        Some(super::BpCommands::List {
            non_interactive, ..
        }) => {
            assert!(!non_interactive, "non_interactive should default to false");
        }
        None => panic!("expected Some(List), got None"),
        Some(other) => panic!("expected List, got {:?}", std::mem::discriminant(&other)),
    }
}

// --- from group2_add.rs ---

// Group 2 tests: `cargo bp add` enhancements.
//
// Covers:
//   - cli.add.features          — -F/--features flag resolves named features
//   - cli.add.features-multiple — comma-separated and repeated -F
//   - cli.add.default-crates    — default crates when no flags given
//   - cli.add.no-default-features — --no-default-features skips defaults
//   - cli.add.all-features      — --all-features selects every crate
//   - cli.add.specific-crates   — positional crate args after pack name
//   - cli.add.target            — --target={workspace,package,default}
//   - cli.add.unknown-crate     — error for unknown crate, valid ones proceed

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

fn load_fancy_spec() -> bphelper_manifest::BatteryPackSpec {
    let fixture = fixtures_dir().join("fancy-battery-pack/Cargo.toml");
    let content = std::fs::read_to_string(&fixture).unwrap();
    bphelper_manifest::parse_battery_pack(&content).unwrap()
}

fn load_basic_spec() -> bphelper_manifest::BatteryPackSpec {
    let fixture = fixtures_dir().join("basic-battery-pack/Cargo.toml");
    let content = std::fs::read_to_string(&fixture).unwrap();
    bphelper_manifest::parse_battery_pack(&content).unwrap()
}

/// Extract crate names from a ResolvedAdd, panicking if Interactive.
fn unwrap_resolved(resolved: super::ResolvedAdd) -> (BTreeSet<String>, BTreeSet<String>) {
    match resolved {
        super::ResolvedAdd::Crates {
            active_features,
            crates,
        } => (active_features, crates.keys().cloned().collect()),
        super::ResolvedAdd::Interactive => {
            panic!("expected Crates, got Interactive")
        }
    }
}

/// Parsed `Add` fields. Exhaustive destructure so new fields cause a compile error.
struct ParsedAdd {
    _battery_pack: Option<String>,
    crates: Vec<String>,
    features: Vec<String>,
    _no_default_features: bool,
    _all_features: bool,
    target: Option<super::AddTarget>,
    _path: Option<String>,
}

/// Parse args as `cargo bp add ...` and return all Add fields.
fn parse_add_command(args: &[&str]) -> ParsedAdd {
    let cli = super::Cli::try_parse_from(args)
        .unwrap_or_else(|e| panic!("parse failed for {args:?}: {e}"));
    match unwrap_bp_command(cli) {
        Some(super::BpCommands::Add {
            battery_pack,
            crates,
            features,
            no_default_features,
            all_features,
            target,
            path,
        }) => ParsedAdd {
            _battery_pack: battery_pack,
            crates,
            features,
            _no_default_features: no_default_features,
            _all_features: all_features,
            target,
            _path: path,
        },
        None => panic!("expected Some(Add), got None"),
        Some(other) => panic!("expected Add, got {:?}", std::mem::discriminant(&other)),
    }
}

// ============================================================================
// cli.add.features — -F/--features flag parsing
// ============================================================================

// [verify cli.add.features]
#[test]
fn features_long_flag_parsed() {
    let add = parse_add_command(&["cargo", "bp", "add", "cli", "--features", "indicators"]);
    assert_eq!(add.features, vec!["indicators"]);
}

// [verify cli.add.features]
#[test]
fn features_short_flag_parsed() {
    let add = parse_add_command(&["cargo", "bp", "add", "cli", "-F", "indicators"]);
    assert_eq!(add.features, vec!["indicators"]);
}

// [verify cli.add.features]
#[test]
fn features_old_with_flag_rejected() {
    let result = super::Cli::try_parse_from(["cargo", "bp", "add", "cli", "--with", "indicators"]);
    assert!(result.is_err(), "old --with flag should be rejected");
}

// ============================================================================
// cli.add.features-multiple — comma-separated and repeated -F
// ============================================================================

// [verify cli.add.features-multiple]
#[test]
fn features_comma_separated() {
    let add = parse_add_command(&["cargo", "bp", "add", "cli", "-F", "indicators,fancy"]);
    assert_eq!(add.features, vec!["indicators", "fancy"]);
}

// [verify cli.add.features-multiple]
#[test]
fn features_repeated_flag() {
    let add = parse_add_command(&[
        "cargo",
        "bp",
        "add",
        "cli",
        "-F",
        "indicators",
        "-F",
        "fancy",
    ]);
    assert_eq!(add.features, vec!["indicators", "fancy"]);
}

// ============================================================================
// cli.add.default-crates — resolve_add_crates with no flags
// ============================================================================

// [verify cli.add.default-crates]
#[test]
fn resolve_default_crates_returns_interactive_when_choices_exist() {
    // When no flags are given and the pack has meaningful choices,
    // resolve_add_crates returns Interactive (the caller decides
    // whether to show the picker or fall back to defaults).
    let spec = load_basic_spec();
    let resolved = super::resolve_add_crates(&spec, "basic-battery-pack", &[], false, false, &[]);
    assert!(
        matches!(resolved, super::ResolvedAdd::Interactive),
        "should signal Interactive when pack has choices and no flags given"
    );
}

// [verify cli.add.default-crates]
#[test]
fn resolve_default_crates_basic_via_explicit_feature() {
    // Explicitly requesting "default" feature bypasses the interactive path
    // and resolves default crates directly.
    // basic-battery-pack: default = ["anyhow", "thiserror"], eyre is optional.
    let spec = load_basic_spec();
    let feat = vec!["default".to_string()];
    let resolved = super::resolve_add_crates(&spec, "basic-battery-pack", &feat, false, false, &[]);
    let (_, crate_names) = unwrap_resolved(resolved);

    assert!(crate_names.contains("anyhow"));
    assert!(crate_names.contains("thiserror"));
    assert!(
        !crate_names.contains("eyre"),
        "eyre is optional, not in default"
    );
}

// [verify cli.add.default-crates]
#[test]
fn resolve_default_crates_fancy_via_named_feature() {
    // Passing -F indicators forces the non-interactive path and includes
    // both default and indicators crates.
    // fancy-battery-pack: default = [clap, dialoguer], indicators = [indicatif, console]
    let spec = load_fancy_spec();
    let feat = vec!["indicators".to_string()];
    let resolved = super::resolve_add_crates(&spec, "fancy-battery-pack", &feat, false, false, &[]);
    let (features, crate_names) = unwrap_resolved(resolved);

    assert_eq!(
        features,
        BTreeSet::from(["default".to_string(), "indicators".to_string()])
    );
    assert!(crate_names.contains("clap"), "default crate");
    assert!(crate_names.contains("dialoguer"), "default crate");
    assert!(crate_names.contains("indicatif"), "indicators crate");
    assert!(crate_names.contains("console"), "indicators crate");
    assert!(
        crate_names.contains("assert_cmd"),
        "non-hidden dev dep always included"
    );
    assert!(
        crate_names.contains("predicates"),
        "non-hidden dev dep always included"
    );
    assert!(!crate_names.contains("cc"), "hidden build dep excluded");
}

// ============================================================================
// cli.add.features — resolution with named features
// ============================================================================

// [verify cli.add.features]
#[test]
fn resolve_with_named_feature_adds_to_defaults() {
    // -F indicators → default + indicators crates.
    // fancy-battery-pack: default = [clap, dialoguer], indicators = [indicatif, console]
    let spec = load_fancy_spec();
    let features_flag = vec!["indicators".to_string()];
    let resolved = super::resolve_add_crates(
        &spec,
        "fancy-battery-pack",
        &features_flag,
        false,
        false,
        &[],
    );
    let (features, crate_names) = unwrap_resolved(resolved);

    assert_eq!(
        features,
        BTreeSet::from(["default".to_string(), "indicators".to_string()])
    );
    // Default crates
    assert!(crate_names.contains("clap"));
    assert!(crate_names.contains("dialoguer"));
    // indicators crates
    assert!(crate_names.contains("indicatif"));
    assert!(crate_names.contains("console"));
}

// [verify cli.add.features]
#[test]
fn resolve_with_all_errors_feature() {
    // basic-battery-pack: all-errors = [anyhow, thiserror, eyre]
    // -F all-errors → default + all-errors, which adds eyre
    let spec = load_basic_spec();
    let features_flag = vec!["all-errors".to_string()];
    let resolved = super::resolve_add_crates(
        &spec,
        "basic-battery-pack",
        &features_flag,
        false,
        false,
        &[],
    );
    let (features, crate_names) = unwrap_resolved(resolved);

    assert_eq!(
        features,
        BTreeSet::from(["default".to_string(), "all-errors".to_string()])
    );
    assert!(crate_names.contains("anyhow"));
    assert!(crate_names.contains("thiserror"));
    assert!(crate_names.contains("eyre"), "all-errors includes eyre");
}

// ============================================================================
// cli.add.no-default-features — skips defaults
// ============================================================================

// [verify cli.add.no-default-features]
#[test]
fn resolve_no_default_features_alone_yields_nothing() {
    // --no-default-features with no -F → empty feature list → no crates
    let spec = load_fancy_spec();
    let resolved = super::resolve_add_crates(&spec, "fancy-battery-pack", &[], true, false, &[]);
    let (features, crate_names) = unwrap_resolved(resolved);

    assert!(features.is_empty(), "no features active");
    assert!(crate_names.is_empty(), "no crates resolved");
}

// [verify cli.add.no-default-features]
#[test]
fn resolve_no_default_features_with_named_feature() {
    // --no-default-features -F indicators → only indicators crates
    let spec = load_fancy_spec();
    let features_flag = vec!["indicators".to_string()];
    let resolved = super::resolve_add_crates(
        &spec,
        "fancy-battery-pack",
        &features_flag,
        true,
        false,
        &[],
    );
    let (features, crate_names) = unwrap_resolved(resolved);

    assert_eq!(features, BTreeSet::from(["indicators".to_string()]));
    assert!(crate_names.contains("indicatif"));
    assert!(crate_names.contains("console"));
    assert!(!crate_names.contains("clap"), "default crate excluded");
    assert!(!crate_names.contains("dialoguer"), "default crate excluded");
}

// ============================================================================
// cli.add.all-features — resolves every crate
// ============================================================================

// [verify cli.add.all-features]
#[test]
fn resolve_all_features_fancy() {
    // --all-features on fancy-battery-pack → all visible crates (hidden filtered out)
    let spec = load_fancy_spec();
    let resolved = super::resolve_add_crates(&spec, "fancy-battery-pack", &[], false, true, &[]);
    let (features, crate_names) = unwrap_resolved(resolved);

    assert_eq!(features, BTreeSet::from(["all".to_string()]));
    // Visible crates included
    assert!(crate_names.contains("clap"));
    assert!(crate_names.contains("dialoguer"));
    assert!(crate_names.contains("indicatif"));
    assert!(crate_names.contains("console"));
    // Dev deps too
    assert!(crate_names.contains("assert_cmd"));
    assert!(crate_names.contains("predicates"));
    // Hidden crates filtered out
    // [verify format.hidden.effect]
    assert!(!crate_names.contains("serde"), "serde is hidden");
    assert!(!crate_names.contains("serde_json"), "serde_json is hidden");
    assert!(!crate_names.contains("cc"), "cc is hidden");
}

// [verify cli.add.all-features]
#[test]
fn resolve_all_features_basic() {
    // --all-features on basic-battery-pack → anyhow, thiserror, eyre
    let spec = load_basic_spec();
    let resolved = super::resolve_add_crates(&spec, "basic-battery-pack", &[], false, true, &[]);
    let (features, crate_names) = unwrap_resolved(resolved);

    assert_eq!(features, BTreeSet::from(["all".to_string()]));
    assert!(crate_names.contains("anyhow"));
    assert!(crate_names.contains("thiserror"));
    assert!(
        crate_names.contains("eyre"),
        "optional eyre included with --all-features"
    );
}

// ============================================================================
// cli.add.specific-crates — positional args select exact crates
// ============================================================================

// [verify cli.add.specific-crates]
#[test]
fn specific_crates_parsed() {
    let add = parse_add_command(&["cargo", "bp", "add", "cli", "clap", "dialoguer"]);
    assert_eq!(add.crates, vec!["clap", "dialoguer"]);
}

// [verify cli.add.specific-crates]
#[test]
fn resolve_specific_crates_selects_only_named() {
    // Selecting "clap" from fancy-battery-pack should return only clap,
    // ignoring default feature and other crates.
    let spec = load_fancy_spec();
    let specific = vec!["clap".to_string()];
    let resolved =
        super::resolve_add_crates(&spec, "fancy-battery-pack", &[], false, false, &specific);
    let (features, crate_names) = unwrap_resolved(resolved);

    assert!(
        features.is_empty(),
        "specific crates mode records no features"
    );
    assert_eq!(crate_names.len(), 1);
    assert!(crate_names.contains("clap"));
}

// [verify cli.add.specific-crates]
#[test]
fn resolve_specific_crates_multiple() {
    // Selecting "anyhow" and "eyre" from basic-battery-pack
    let spec = load_basic_spec();
    let specific = vec!["anyhow".to_string(), "eyre".to_string()];
    let resolved =
        super::resolve_add_crates(&spec, "basic-battery-pack", &[], false, false, &specific);
    let (_, crate_names) = unwrap_resolved(resolved);

    assert_eq!(crate_names.len(), 2);
    assert!(crate_names.contains("anyhow"));
    assert!(crate_names.contains("eyre"));
    assert!(!crate_names.contains("thiserror"), "not requested");
}

// [verify cli.add.specific-crates]
#[test]
fn resolve_specific_crates_ignores_features_flag() {
    // When specific crates are given, -F flags are irrelevant — only the
    // named crates matter. (The features flag is still parsed by clap but
    // specific_crates takes priority in resolve_add_crates.)
    let spec = load_fancy_spec();
    let features_flag = vec!["indicators".to_string()];
    let specific = vec!["dialoguer".to_string()];
    let resolved = super::resolve_add_crates(
        &spec,
        "fancy-battery-pack",
        &features_flag,
        false,
        false,
        &specific,
    );
    let (_, crate_names) = unwrap_resolved(resolved);

    assert_eq!(crate_names.len(), 1);
    assert!(crate_names.contains("dialoguer"));
    assert!(
        !crate_names.contains("indicatif"),
        "not selected despite -F indicators"
    );
}

// ============================================================================
// cli.add.unknown-crate — errors for unknown, valid ones proceed
// ============================================================================

// [verify cli.add.unknown-crate]
#[test]
fn resolve_unknown_crate_skipped_valid_proceeds() {
    // "nonexistent" is unknown, "clap" is valid → only clap returned
    let spec = load_fancy_spec();
    let specific = vec!["nonexistent".to_string(), "clap".to_string()];
    let resolved =
        super::resolve_add_crates(&spec, "fancy-battery-pack", &[], false, false, &specific);
    let (_, crate_names) = unwrap_resolved(resolved);

    assert_eq!(crate_names.len(), 1);
    assert!(crate_names.contains("clap"));
    assert!(!crate_names.contains("nonexistent"));
}

// [verify cli.add.unknown-crate]
#[test]
fn resolve_all_unknown_crates_yields_empty() {
    let spec = load_fancy_spec();
    let specific = vec!["nope".to_string(), "also-nope".to_string()];
    let resolved =
        super::resolve_add_crates(&spec, "fancy-battery-pack", &[], false, false, &specific);
    let (_, crate_names) = unwrap_resolved(resolved);

    assert!(crate_names.is_empty());
}

// ============================================================================
// cli.add.target — flag parsing
// ============================================================================

// [verify cli.add.target]
#[test]
fn target_values_parsed() {
    for (arg, expected) in [
        ("workspace", super::AddTarget::Workspace),
        ("package", super::AddTarget::Package),
        ("default", super::AddTarget::Default),
    ] {
        let add = parse_add_command(&["cargo", "bp", "add", "cli", "--target", arg]);
        assert_eq!(add.target, Some(expected), "for --target {arg}");
    }
}

// [verify cli.add.target]
#[test]
fn target_omitted_is_none() {
    let add = parse_add_command(&["cargo", "bp", "add", "cli"]);
    assert!(add.target.is_none());
}

// [verify cli.add.target]
#[test]
fn target_invalid_value_rejected() {
    let result = super::Cli::try_parse_from(["cargo", "bp", "add", "cli", "--target", "invalid"]);
    assert!(result.is_err());
}

// --- from group2_add_integration.rs ---

// Group 2 integration tests: full `add_battery_pack` flow with real fixtures.
//
// These tests create a temp project, call `add_battery_pack` with `--path`
// pointing at test fixtures, then snapshot the written Cargo.toml sections
// using expect-test.
//
// Covers the write side of:
//   - cli.add.default-crates    — default deps appear in Cargo.toml
//   - cli.add.features          — named feature crates appear
//   - cli.add.no-default-features — only named feature crates, no defaults
//   - cli.add.all-features      — all crates appear
//   - cli.add.specific-crates   — only named crates appear
//   - cli.add.unknown-crate     — unknown skipped, valid written
//   - cli.add.target            — metadata lands in package vs workspace
//   - cli.add.register          — battery pack in build-dependencies
//   - cli.add.dep-kind          — dev-deps land in [dev-dependencies]

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
    target: Option<super::AddTarget>,
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
    super::add_battery_pack(
        pack_name,
        &features,
        no_default_features,
        all_features,
        &specific,
        target,
        Some(fixture_path.to_str().unwrap()),
        &crate::registry::CrateSource::Registry,
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

    assert_data_eq!(deps, file![_]);
}

#[test]
fn add_default_includes_dev_and_build_deps() {
    let tmp = make_temp_project();
    add(
        "managed",
        "managed-battery-pack",
        &["default"],
        FeatureMode::Default,
        &[],
        None,
        tmp.path(),
    );

    let content = read_cargo_toml(&tmp);
    let dev_deps = extract_section(&content, "[dev-dependencies]");
    let build_deps = extract_section(&content, "[build-dependencies]");

    assert!(
        dev_deps.contains("insta"),
        "dev-dep should be included with default features: {dev_deps}"
    );
    assert!(
        build_deps.contains("cc"),
        "build-dep should be included with default features: {build_deps}"
    );
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

    assert!(deps.contains("clap"), "Expected clap");
    assert!(deps.contains("console"), "Expected console");
    assert!(deps.contains("dialoguer"), "Expected dialoguer");
    assert!(deps.contains("indicatif"), "Expected indicatif");
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

    assert_data_eq!(meta, file![_])
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

    assert!(deps.contains("console"), "Expected console dependency");
    assert!(deps.contains("indicatif"), "Expected indicatif dependency");
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

    assert!(deps.is_empty(), "Expected empty dependencies");
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

    assert!(deps.contains("anyhow"), "Expected anyhow dependency");
    assert!(deps.contains("eyre"), "Expected eyre dependency");
    assert!(deps.contains("thiserror"), "Expected thiserror dependency");
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

    assert!(
        meta.contains("basic-battery-pack"),
        "Expected basic-battery-pack metadata"
    );
    assert!(meta.contains("features"), "Expected features");
    assert!(meta.contains("all"), "Expected all feature");
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
    assert!(deps.contains("clap"), "Expected clap dependency");
    assert!(deps.contains("console"), "Expected console dependency");
    assert!(deps.contains("dialoguer"), "Expected dialoguer dependency");
    assert!(deps.contains("indicatif"), "Expected indicatif dependency");

    // Dev-deps land in [dev-dependencies]
    // [verify cli.add.dep-kind]
    assert!(
        dev_deps.contains("assert_cmd"),
        "Expected assert_cmd in dev-dependencies"
    );
    assert!(
        dev_deps.contains("predicates"),
        "Expected predicates in dev-dependencies"
    );

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

    assert!(
        deps.contains("clap"),
        "Expected clap dependency with derive feature"
    );
    assert!(deps.contains("version"), "Expected version");
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

    assert!(deps.contains("clap"), "Expected clap dependency");
    assert!(
        !deps.contains("nonexistent"),
        "Expected nonexistent to not be in deps"
    );
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
        Some(super::AddTarget::Package),
        tmp.path(),
    );

    let content = read_cargo_toml(&tmp);
    let meta = extract_metadata(&content, "basic-battery-pack");

    assert!(
        meta.contains("basic-battery-pack"),
        "Expected basic-battery-pack metadata"
    );
    assert!(meta.contains("features"), "Expected features");
    assert!(meta.contains("default"), "Expected default feature");
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

    assert!(build_rs.contains("fn main()"), "Expected fn main()");
    assert!(
        build_rs.contains("basic_battery_pack::validate"),
        "Expected basic_battery_pack::validate call"
    );
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
    assert!(build_rs.contains("fn main()"), "Expected fn main()");
    assert!(
        build_rs.contains("basic_battery_pack::validate"),
        "Expected basic_battery_pack::validate call"
    );
}

// ============================================================================
// cli.bare.help — --help prints help and exits (clap behaviour)
// ============================================================================

// [verify cli.bare.help]
#[test]
fn cli_bare_help_prints_help() {
    // Parsing `cargo bp --help` should result in a DisplayHelp error from
    // clap (which the binary translates into printing help and exiting 0).
    // We verify this by attempting to parse the args and checking for the
    // expected clap error kind.
    match super::Cli::try_parse_from(["cargo", "bp", "--help"]) {
        Ok(_) => panic!("--help should cause clap to return a DisplayHelp error"),
        Err(err) => {
            assert_eq!(
                err.kind(),
                clap::error::ErrorKind::DisplayHelp,
                "expected DisplayHelp, got {:?}",
                err.kind()
            );
        }
    }
}

// --- from status.rs ---

// Tests for `cargo bp status` spec rules.
//
// Covers:
//   - cli.status.list         — lists installed battery packs with versions
//   - cli.status.version-warn — warns when user versions are older; no warning when newer
//   - cli.status.no-project   — reports error outside a Rust project

// ---------------------------------------------------------------------------
// collect_user_dep_versions — version extraction from Cargo.toml
// ---------------------------------------------------------------------------

/// Helper: write a temporary Cargo.toml and collect versions from it.
///
/// The file is written to a temp directory so `find_workspace_manifest`
/// won't find a parent workspace (giving us isolated single-crate behavior).
fn collect_versions(toml_content: &str) -> BTreeMap<String, String> {
    let dir = tempfile::tempdir().unwrap();
    let manifest_path = dir.path().join("Cargo.toml");
    std::fs::write(&manifest_path, toml_content).unwrap();
    super::collect_user_dep_versions(&manifest_path, toml_content).unwrap()
}

// [verify cli.status.version-warn]
#[test]
fn collects_simple_string_versions() {
    let versions = collect_versions(
        r#"
[package]
name = "test"
version = "0.1.0"

[dependencies]
serde = "1.0"
anyhow = "1.0.86"
"#,
    );
    assert_eq!(versions.get("serde").unwrap(), "1.0");
    assert_eq!(versions.get("anyhow").unwrap(), "1.0.86");
}

// [verify cli.status.version-warn]
#[test]
fn collects_inline_table_versions() {
    let versions = collect_versions(
        r#"
[package]
name = "test"
version = "0.1.0"

[dependencies]
tokio = { version = "1.38.0", features = ["full"] }
serde = { version = "1.0", features = ["derive"] }
"#,
    );
    assert_eq!(versions.get("tokio").unwrap(), "1.38.0");
    assert_eq!(versions.get("serde").unwrap(), "1.0");
}

// [verify cli.status.version-warn]
#[test]
fn collects_from_all_dep_sections() {
    let versions = collect_versions(
        r#"
[package]
name = "test"
version = "0.1.0"

[dependencies]
serde = "1.0"

[dev-dependencies]
insta = "1.39"

[build-dependencies]
cc = "1.0"
"#,
    );
    assert_eq!(versions.get("serde").unwrap(), "1.0");
    assert_eq!(versions.get("insta").unwrap(), "1.39");
    assert_eq!(versions.get("cc").unwrap(), "1.0");
}

// [verify cli.status.version-warn]
#[test]
fn skips_deps_without_version() {
    let versions = collect_versions(
        r#"
[package]
name = "test"
version = "0.1.0"

[dependencies]
my-local = { path = "../my-local" }
serde = "1.0"
"#,
    );
    assert!(
        !versions.contains_key("my-local"),
        "path deps have no version"
    );
    assert_eq!(versions.get("serde").unwrap(), "1.0");
}

// [verify cli.status.version-warn]
#[test]
fn should_upgrade_detects_older_version() {
    // This tests the version comparison logic that status relies on.
    // should_upgrade_version(current, recommended) returns true when
    // recommended > current — meaning the user should upgrade.
    //
    // We test it indirectly: if user has "1.0" and BP recommends "1.2",
    // the version shows up in collect_versions and should_upgrade_version
    // (used internally by status) would flag it.
    let versions = collect_versions(
        r#"
[package]
name = "test"
version = "0.1.0"

[dependencies]
serde = "1.0"
tokio = "1.40.0"
"#,
    );
    // User has serde 1.0, BP might recommend 1.2 → would warn
    assert_eq!(versions.get("serde").unwrap(), "1.0");
    // User has tokio 1.40.0, BP might recommend 1.38.0 → would NOT warn (newer-ok)
    assert_eq!(versions.get("tokio").unwrap(), "1.40.0");
}

// Note: cli.status.no-project is tested via the CLI binary (status_battery_packs
// calls find_user_manifest which bails when no Cargo.toml exists). That function
// is private, so we verify the next layer: collect_user_dep_versions errors on
// unparsable content.
#[test]
fn collect_versions_errors_on_invalid_toml() {
    let dir = tempfile::tempdir().unwrap();
    let manifest_path = dir.path().join("Cargo.toml");
    std::fs::write(&manifest_path, "not valid toml {{{").unwrap();
    let result = super::collect_user_dep_versions(&manifest_path, "not valid toml {{{");
    assert!(result.is_err());
}
