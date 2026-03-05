//! Group 1 tests: verify behaviors for rules that are already implemented
//! but were missing [impl] tags.
//!
//! Covers:
//!   - cli.new.name-flag      — --name flag is accepted and parsed
//!   - cli.new.name-prompt    — omitting --name still parses (template engine prompts)
//!   - cli.new.template-select — multiple templates with no default triggers prompt path
//!   - cli.bare.tui           — bare `cargo bp` produces command: None
//!   - cli.add.idempotent     — re-adding same dep doesn't create duplicates

use clap::Parser;
use std::collections::{BTreeMap, BTreeSet};

/// Unwrap `Commands::Bp { command }` → `Option<BpCommands>`.
fn unwrap_bp_command(cli: bphelper_cli::Cli) -> Option<bphelper_cli::BpCommands> {
    match cli.command {
        bphelper_cli::Commands::Bp { command, .. } => command,
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
    let cli =
        bphelper_cli::Cli::try_parse_from(["cargo", "bp"]).expect("bare `cargo bp` should parse");
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
    let cli =
        bphelper_cli::Cli::try_parse_from(["cargo", "bp", "new", "cli", "--name", "my-project"])
            .expect("--name flag should be accepted");

    match unwrap_bp_command(cli) {
        Some(bphelper_cli::BpCommands::New { name, .. }) => {
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
    let cli = bphelper_cli::Cli::try_parse_from(["cargo", "bp", "new", "cli", "-n", "my-project"])
        .expect("-n flag should be accepted");

    match unwrap_bp_command(cli) {
        Some(bphelper_cli::BpCommands::New { name, .. }) => {
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
    let cli = bphelper_cli::Cli::try_parse_from(["cargo", "bp", "new", "cli"])
        .expect("new without --name should parse");

    match unwrap_bp_command(cli) {
        Some(bphelper_cli::BpCommands::New { name, .. }) => {
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
        bphelper_cli::TemplateConfig {
            path: "templates/simple".to_string(),
            description: Some("A simple template".to_string()),
        },
    );

    let result = bphelper_cli::resolve_template(&templates, None).unwrap();
    assert_eq!(result, "templates/simple");
}

// [verify cli.new.template-select]
#[test]
fn resolve_template_picks_default_when_present() {
    // With multiple templates including "default", resolve_template picks "default".
    let mut templates = BTreeMap::new();
    templates.insert(
        "default".to_string(),
        bphelper_cli::TemplateConfig {
            path: "templates/default".to_string(),
            description: Some("The default template".to_string()),
        },
    );
    templates.insert(
        "advanced".to_string(),
        bphelper_cli::TemplateConfig {
            path: "templates/advanced".to_string(),
            description: Some("An advanced template".to_string()),
        },
    );

    let result = bphelper_cli::resolve_template(&templates, None).unwrap();
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
        bphelper_cli::TemplateConfig {
            path: "templates/simple".to_string(),
            description: None,
        },
    );
    templates.insert(
        "advanced".to_string(),
        bphelper_cli::TemplateConfig {
            path: "templates/advanced".to_string(),
            description: None,
        },
    );

    let result = bphelper_cli::resolve_template(&templates, Some("nonexistent"));
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
        bphelper_cli::TemplateConfig {
            path: "templates/simple".to_string(),
            description: None,
        },
    );
    templates.insert(
        "advanced".to_string(),
        bphelper_cli::TemplateConfig {
            path: "templates/advanced".to_string(),
            description: None,
        },
    );

    let result = bphelper_cli::resolve_template(&templates, Some("advanced")).unwrap();
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

    bphelper_cli::add_dep_to_table(&mut table, "anyhow", &spec);
    assert_eq!(table.len(), 1);

    // Add again with updated version
    let spec_v2 = bphelper_manifest::CrateSpec {
        version: "2.0".to_string(),
        features: BTreeSet::new(),
        dep_kind: bphelper_manifest::DepKind::Normal,
        optional: false,
    };

    bphelper_cli::add_dep_to_table(&mut table, "anyhow", &spec_v2);
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

    bphelper_cli::add_dep_to_table(&mut table, "clap", &spec1);
    assert_eq!(table.len(), 1);

    let spec2 = bphelper_manifest::CrateSpec {
        version: "4.1".to_string(),
        features: BTreeSet::from(["derive".to_string(), "env".to_string()]),
        dep_kind: bphelper_manifest::DepKind::Normal,
        optional: false,
    };

    bphelper_cli::add_dep_to_table(&mut table, "clap", &spec2);
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
    // [package.metadata.battery-pack.<name>] twice should produce one entry.
    let toml_str = r#"[package]
name = "my-app"
version = "0.1.0"

[package.metadata.battery-pack.cli-battery-pack]
features = ["default"]
"#;
    let mut doc: toml_edit::DocumentMut = toml_str.parse().unwrap();

    // "Re-add" with updated features
    let bp_meta = &mut doc["package"]["metadata"]["battery-pack"]["cli-battery-pack"];
    let mut features_array = toml_edit::Array::new();
    features_array.push("default");
    features_array.push("indicators");
    *bp_meta = toml_edit::Item::Table(toml_edit::Table::new());
    bp_meta["features"] = toml_edit::value(features_array);

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
    let features = bphelper_cli::read_active_features(&doc.to_string(), "cli-battery-pack");
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
    let cli =
        bphelper_cli::Cli::try_parse_from(["cargo", "bp", "show", "cli", "--non-interactive"])
            .expect("--non-interactive should be accepted");

    match unwrap_bp_command(cli) {
        Some(bphelper_cli::BpCommands::Show {
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
    let cli = bphelper_cli::Cli::try_parse_from(["cargo", "bp", "show", "cli"])
        .expect("show without --non-interactive should parse");

    match unwrap_bp_command(cli) {
        Some(bphelper_cli::BpCommands::Show {
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
    let cli = bphelper_cli::Cli::try_parse_from(["cargo", "bp", "list", "--non-interactive"])
        .expect("--non-interactive should be accepted");

    match unwrap_bp_command(cli) {
        Some(bphelper_cli::BpCommands::List {
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
    let cli = bphelper_cli::Cli::try_parse_from(["cargo", "bp", "list"])
        .expect("list without --non-interactive should parse");

    match unwrap_bp_command(cli) {
        Some(bphelper_cli::BpCommands::List {
            non_interactive, ..
        }) => {
            assert!(!non_interactive, "non_interactive should default to false");
        }
        None => panic!("expected Some(List), got None"),
        Some(other) => panic!("expected List, got {:?}", std::mem::discriminant(&other)),
    }
}
