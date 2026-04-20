use snapbox::{assert_data_eq, file, str};

use super::*;

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

fn parse_fixture(name: &str) -> BatteryPackSpec {
    let path = fixtures_dir().join(name).join("Cargo.toml");
    let manifest = std::fs::read_to_string(&path)
        .unwrap_or_else(|e| panic!("reading {}: {e}", path.display()));
    bphelper_manifest::parse_battery_pack(&manifest).unwrap()
}

fn mock_descriptions() -> BTreeMap<String, String> {
    BTreeMap::from([
        (
            "anyhow".into(),
            "Flexible concrete Error type built on std::error::Error".into(),
        ),
        ("thiserror".into(), "derive(Error)".into()),
        (
            "eyre".into(),
            "Flexible concrete Error type for easy idiomatic error handling".into(),
        ),
        (
            "clap".into(),
            "A simple to use, efficient, and full-featured Command Line Argument Parser".into(),
        ),
        (
            "dialoguer".into(),
            "A command line prompting library".into(),
        ),
        (
            "indicatif".into(),
            "A progress bar and CLI reporting library".into(),
        ),
        (
            "console".into(),
            "A terminal and console abstraction for Rust".into(),
        ),
        ("serde".into(), "A serialization framework".into()),
        (
            "serde_json".into(),
            "A JSON serialization file format".into(),
        ),
        ("cc".into(), "A build-time C compiler detection".into()),
        ("assert_cmd".into(), "Easy command testing".into()),
        ("predicates".into(), "Boolean predicate combinators".into()),
    ])
}

// ================================================================
// build_context() tests
// ================================================================

#[test]
// [verify docgen.vars.crates]
// [verify docgen.vars.package]
fn test_context_basic() {
    let spec = parse_fixture("basic-battery-pack");
    let descriptions = mock_descriptions();
    let ctx = build_context(&spec, &descriptions, "# Hello");

    // Package metadata
    assert_eq!(ctx.package.name, "basic-battery-pack");
    assert_eq!(ctx.package.version, "0.1.0");
    assert_eq!(ctx.package.description, "A simple test battery pack");
    assert_eq!(ctx.package.repository, "");

    // All 3 crates should be visible (no hidden config)
    let names: Vec<&str> = ctx.crates.iter().map(|c| c.name.as_str()).collect();
    assert_eq!(names, ["anyhow", "eyre", "thiserror"]);

    // Check a specific crate entry
    let anyhow = ctx.crates.iter().find(|c| c.name == "anyhow").unwrap();
    assert_eq!(anyhow.version, "1");
    assert_eq!(
        anyhow.description,
        "Flexible concrete Error type built on std::error::Error"
    );
    assert_eq!(anyhow.dep_kind, "dependencies");
}

#[test]
// [verify docgen.hidden.excluded]
fn test_context_hidden_excluded() {
    let spec = parse_fixture("fancy-battery-pack");
    let descriptions = mock_descriptions();
    let ctx = build_context(&spec, &descriptions, "");

    let names: Vec<&str> = ctx.crates.iter().map(|c| c.name.as_str()).collect();
    assert_data_eq!(
        names.join(", "),
        str!["assert_cmd, clap, console, dialoguer, indicatif, predicates"]
    );
}

#[test]
// [verify docgen.vars.features]
fn test_context_features() {
    let spec = parse_fixture("fancy-battery-pack");
    let descriptions = mock_descriptions();
    let ctx = build_context(&spec, &descriptions, "");

    let feature_names: Vec<&str> = ctx.features.iter().map(|f| f.name.as_str()).collect();
    assert_data_eq!(feature_names.join(", "), str!["default, fancy, indicators"]);

    let indicators = ctx
        .features
        .iter()
        .find(|f| f.name == "indicators")
        .unwrap();
    assert_eq!(indicators.crates, vec!["console", "indicatif"]);
}

#[test]
// [verify docgen.vars.readme]
fn test_context_readme() {
    let spec = parse_fixture("basic-battery-pack");
    let descriptions = mock_descriptions();
    let readme = "# My Battery Pack\n\nThis is great.";
    let ctx = build_context(&spec, &descriptions, readme);

    assert_eq!(ctx.readme, readme);
}

#[test]
// [verify docgen.vars.crates]
fn test_context_dep_kinds() {
    let spec = parse_fixture("fancy-battery-pack");
    let descriptions = mock_descriptions();
    let ctx = build_context(&spec, &descriptions, "");

    // clap is in [dependencies] (optional)
    let clap = ctx.crates.iter().find(|c| c.name == "clap").unwrap();
    assert_eq!(clap.dep_kind, "dependencies");

    // assert_cmd is in [dev-dependencies]
    let assert_cmd = ctx.crates.iter().find(|c| c.name == "assert_cmd").unwrap();
    assert_eq!(assert_cmd.dep_kind, "dev-dependencies");

    // cc is hidden (build-dep), so not present — no build-dep to test directly
    // but we can verify the hidden ones are truly excluded
    assert!(ctx.crates.iter().all(|c| c.name != "cc"));
}

// ================================================================
// render_docs() tests
// ================================================================

/// Helper to build a minimal context for render tests.
fn simple_context() -> DocsContext {
    DocsContext {
        crates: vec![
            CrateEntry {
                name: "anyhow".into(),
                version: "1".into(),
                description: "Flexible concrete Error type".into(),
                features: vec![],
                dep_kind: "dependencies".into(),
            },
            CrateEntry {
                name: "thiserror".into(),
                version: "2".into(),
                description: "derive(Error)".into(),
                features: vec!["std".into()],
                dep_kind: "dependencies".into(),
            },
        ],
        features: vec![FeatureEntry {
            name: "default".into(),
            crates: vec!["anyhow".into(), "thiserror".into()],
        }],
        readme: "# My Pack\n\nA great battery pack.".into(),
        package: PackageInfo {
            name: "test-battery-pack".into(),
            version: "0.1.0".into(),
            description: "A test pack".into(),
            repository: "https://github.com/example/test".into(),
        },
    }
}

#[test]
// [verify docgen.helper.readme]
fn test_render_readme_helper() {
    let ctx = simple_context();
    let output = render_docs("{{readme}}", &ctx).unwrap();
    assert_data_eq!(
        output,
        str![[r#"
# My Pack

A great battery pack.
"#]]
    )
}

#[test]
// [verify docgen.helper.crate-table]
fn test_render_crate_table() {
    let ctx = simple_context();
    let output = render_docs("{{crate-table}}", &ctx).unwrap();
    assert_data_eq!(output, file![_])
}

#[test]
// [verify docgen.template.default]
fn test_render_default_template() {
    let ctx = simple_context();
    let output = render_docs("{{readme}}\n\n{{crate-table}}", &ctx).unwrap();
    assert_data_eq!(output, file![_])
}

#[test]
// [verify docgen.template.custom]
fn test_render_custom_template() {
    let ctx = simple_context();
    let template = r#"# {{package.name}} v{{package.version}}

{{package.description}}

## Crates

{{#each crates}}
- **{{name}}** ({{version}}): {{description}}
{{/each}}"#;
    let output = render_docs(template, &ctx).unwrap();

    assert_data_eq!(output, file![_])
}

#[test]
fn test_render_crate_table_empty() {
    let ctx = DocsContext {
        crates: vec![],
        features: vec![],
        readme: String::new(),
        package: PackageInfo {
            name: "empty-battery-pack".into(),
            version: "0.1.0".into(),
            description: "".into(),
            repository: "".into(),
        },
    };
    let output = render_docs("{{crate-table}}", &ctx).unwrap();
    assert_eq!(output, "");
}

#[test]
// [verify docgen.helper.crate-table]
fn test_render_crate_table_links() {
    let ctx = simple_context();
    let output = render_docs("{{crate-table}}", &ctx).unwrap();
    assert_data_eq!(output, file![_]);
}

#[test]
fn test_render_no_html_escaping() {
    let ctx = DocsContext {
        readme: "Use `Option<T>` and `Result<T, E>` & more".into(),
        ..simple_context()
    };
    let output = render_docs("{{readme}}", &ctx).unwrap();
    assert_data_eq!(output, str!["Use `Option<T>` and `Result<T, E>` & more"]);
}

// ================================================================
// Full pipeline tests (parse fixture → build context → render)
// ================================================================

#[test]
// [verify docgen.template.handlebars]
fn test_full_pipeline_basic() {
    let spec = parse_fixture("basic-battery-pack");
    let descriptions = mock_descriptions();
    let readme = "# basic-battery-pack\n\nError handling crates for Rust.";
    let ctx = build_context(&spec, &descriptions, readme);
    let output = render_docs("{{readme}}\n\n{{crate-table}}", &ctx).unwrap();

    assert_data_eq!(output, file![_]);
}

// ================================================================
// True-by-construction rule verification
// ================================================================

#[test]
// [verify docgen.build.lib-include]
fn test_template_lib_rs_includes_generated_docs() {
    // True by construction: the `cargo bp new` template emits a lib.rs
    // containing `#![doc = include_str!(concat!(env!("OUT_DIR"), "/docs.md"))]`.
    // This test verifies the template file contains the expected directive.
    let template_dir = std::path::Path::new(env!("CARGO_MANIFEST_DIR"))
        .parent()
        .unwrap()
        .join("templates/default/src/lib.rs");
    let content = std::fs::read_to_string(&template_dir).unwrap();
    assert_data_eq!(content, file![_]);
}

#[test]
// [verify docgen.helper.crate-table-update]
fn test_crate_table_update_is_automatic() {
    // Documentation-only test. The spec says updating bphelper must
    // automatically update table rendering for all battery packs.
    // This is true by construction: `{{crate-table}}` is a Handlebars
    // helper registered at runtime by `render_docs()` in this crate.
    // Battery packs call `generate_docs()` → `render_docs()`, so
    // updating this crate updates the helper for all consumers. No
    // per-battery-pack code generation is involved. This architectural
    // invariant cannot be meaningfully unit-tested — the test below
    // merely confirms the helper is registered (already covered by
    // test_render_crate_table).
    let ctx = simple_context();
    let output = render_docs("{{crate-table}}", &ctx).unwrap();
    assert_data_eq!(output, file![_]);
}

// ================================================================
// Full pipeline tests (parse fixture → build context → render)
// ================================================================

#[test]
// [verify docgen.template.handlebars]
// [verify docgen.hidden.excluded]
fn test_full_pipeline_fancy() {
    let spec = parse_fixture("fancy-battery-pack");
    let descriptions = mock_descriptions();
    let readme = "# fancy-battery-pack\n\nCLI tools.";
    let ctx = build_context(&spec, &descriptions, readme);
    let output = render_docs("{{readme}}\n\n{{crate-table}}", &ctx).unwrap();

    assert_data_eq!(output, file![_]);
}

// ================================================================
// I/O integration tests (generate_docs_from_dir)
// ================================================================

/// Set up a tempdir with a battery pack's Cargo.toml, template, and README.
fn setup_docgen_dir(manifest: &str, template: &str, readme: Option<&str>) -> tempfile::TempDir {
    let dir = tempfile::tempdir().unwrap();
    std::fs::write(dir.path().join("Cargo.toml"), manifest).unwrap();
    std::fs::write(dir.path().join("docs.handlebars.md"), template).unwrap();
    if let Some(readme) = readme {
        std::fs::write(dir.path().join("README.md"), readme).unwrap();
    }
    dir
}

#[test]
// [verify docgen.build.trigger]
fn test_generate_docs_writes_output_file() {
    let fixture = fixtures_dir().join("basic-battery-pack/Cargo.toml");
    let manifest = std::fs::read_to_string(&fixture).unwrap();
    let dir = setup_docgen_dir(&manifest, "{{readme}}\n\n{{crate-table}}", Some("# Hello"));

    let out_dir = tempfile::tempdir().unwrap();
    let descriptions = mock_descriptions();

    generate_docs_from_dir(
        dir.path().to_str().unwrap(),
        out_dir.path().to_str().unwrap(),
        &descriptions,
    )
    .unwrap();

    let output_path = out_dir.path().join("docs.md");
    assert!(output_path.exists(), "docs.md must be written to out_dir");

    let content = std::fs::read_to_string(&output_path).unwrap();
    assert_data_eq!(content, file![_]);
}

#[test]
// [verify docgen.build.template]
fn test_generate_docs_reads_template_from_manifest_dir() {
    let fixture = fixtures_dir().join("basic-battery-pack/Cargo.toml");
    let manifest = std::fs::read_to_string(&fixture).unwrap();

    // Custom template — only uses package name, no readme or crate table.
    let dir = setup_docgen_dir(
        &manifest,
        "# Docs for {{package.name}}",
        Some("ignored readme"),
    );

    let out_dir = tempfile::tempdir().unwrap();
    let descriptions = mock_descriptions();

    generate_docs_from_dir(
        dir.path().to_str().unwrap(),
        out_dir.path().to_str().unwrap(),
        &descriptions,
    )
    .unwrap();

    let content = std::fs::read_to_string(out_dir.path().join("docs.md")).unwrap();
    assert_eq!(content, "# Docs for basic-battery-pack");
}

#[test]
fn test_generate_docs_missing_template_errors() {
    let fixture = fixtures_dir().join("basic-battery-pack/Cargo.toml");
    let manifest = std::fs::read_to_string(&fixture).unwrap();

    // Set up dir with Cargo.toml but NO template file.
    let dir = tempfile::tempdir().unwrap();
    std::fs::write(dir.path().join("Cargo.toml"), &manifest).unwrap();

    let out_dir = tempfile::tempdir().unwrap();
    let descriptions = mock_descriptions();

    let result = generate_docs_from_dir(
        dir.path().to_str().unwrap(),
        out_dir.path().to_str().unwrap(),
        &descriptions,
    );

    assert!(result.is_err(), "missing template must produce an error");
    let err = result.unwrap_err().to_string();
    assert_data_eq!(
        err,
        str!["reading [..]/docs.handlebars.md: No such file or directory (os error 2)"]
    );
}

#[test]
fn test_generate_docs_readme_optional() {
    let fixture = fixtures_dir().join("basic-battery-pack/Cargo.toml");
    let manifest = std::fs::read_to_string(&fixture).unwrap();

    // No README — should still succeed with empty readme in context.
    let dir = setup_docgen_dir(&manifest, "readme=[{{readme}}]", None);

    let out_dir = tempfile::tempdir().unwrap();
    let descriptions = mock_descriptions();

    generate_docs_from_dir(
        dir.path().to_str().unwrap(),
        out_dir.path().to_str().unwrap(),
        &descriptions,
    )
    .unwrap();

    let content = std::fs::read_to_string(out_dir.path().join("docs.md")).unwrap();
    assert_eq!(
        content, "readme=[]",
        "missing README should produce empty string"
    );
}

#[test]
// [verify docgen.helper.crate-table-metadata]
fn test_fetch_crate_descriptions_returns_workspace_packages() {
    // This test calls the real cargo metadata against our workspace.
    // It verifies that fetch_crate_descriptions() returns descriptions
    // for packages that exist in the workspace.
    let descriptions = fetch_crate_descriptions().unwrap();

    // bphelper-build is in our workspace and has a description.
    assert!(
        descriptions.contains_key("bphelper-build"),
        "must include workspace package bphelper-build"
    );
    assert_data_eq!(
        &descriptions["bphelper-build"],
        str!["Build-time documentation generation for battery packs"]
    );
}
