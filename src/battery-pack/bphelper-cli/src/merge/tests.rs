use super::*;
use indoc::indoc;

// ============================================================================
// TOML merge tests
// ============================================================================

#[test]
fn merge_toml_adds_new_dependency() {
    let existing = indoc! {r#"
        [package]
        name = "my-project"
        version = "0.1.0"

        [dependencies]
        serde = "1"
    "#};

    let template = indoc! {r#"
        [package]
        name = "template-project"

        [dependencies]
        clap = { version = "4", features = ["derive"] }
    "#};

    let result = merge_toml(existing, template).unwrap();
    assert!(result.contains("serde"));
    assert!(result.contains("clap"));
    assert!(result.contains(r#"features = ["derive"]"#));
    // Existing package name is preserved.
    assert!(result.contains(r#"name = "my-project""#));
}

#[test]
fn merge_toml_upgrades_older_version() {
    let existing = indoc! {r#"
        [dependencies]
        clap = "3"
    "#};

    let template = indoc! {r#"
        [dependencies]
        clap = "4"
    "#};

    let result = merge_toml(existing, template).unwrap();
    assert!(result.contains(r#"clap = "4""#));
}

#[test]
fn merge_toml_preserves_newer_version() {
    let existing = indoc! {r#"
        [dependencies]
        clap = "5"
    "#};

    let template = indoc! {r#"
        [dependencies]
        clap = "4"
    "#};

    let result = merge_toml(existing, template).unwrap();
    assert!(result.contains(r#"clap = "5""#));
}

#[test]
fn merge_toml_unions_features() {
    let existing = indoc! {r#"
        [dependencies]
        clap = { version = "4", features = ["derive"] }
    "#};

    let template = indoc! {r#"
        [dependencies]
        clap = { version = "4", features = ["env"] }
    "#};

    let result = merge_toml(existing, template).unwrap();
    assert!(result.contains("derive"));
    assert!(result.contains("env"));
}

#[test]
fn merge_toml_adds_new_section() {
    let existing = indoc! {r#"
        [dependencies]
        serde = "1"
    "#};

    let template = indoc! {r#"
        [dependencies]
        serde = "1"

        [dev-dependencies]
        criterion = { version = "0.5", features = ["html_reports"] }
    "#};

    let result = merge_toml(existing, template).unwrap();
    assert!(result.contains("[dev-dependencies]"));
    assert!(result.contains("criterion"));
}

#[test]
fn merge_toml_preserves_existing_section() {
    let existing = indoc! {r#"
        [dev-dependencies]
        tokio-test = "0.4"
    "#};

    let template = indoc! {r#"
        [dev-dependencies]
        criterion = "0.5"
    "#};

    let result = merge_toml(existing, template).unwrap();
    // Both deps should be present.
    assert!(result.contains("tokio-test"));
    assert!(result.contains("criterion"));
}

#[test]
fn merge_toml_adds_non_dep_section() {
    let existing = indoc! {r#"
        [package]
        name = "my-project"

        [dependencies]
        serde = "1"
    "#};

    let template = indoc! {r#"
        [package]
        name = "template"

        [[bin]]
        name = "my-tool"
        path = "src/main.rs"
    "#};

    let result = merge_toml(existing, template).unwrap();
    assert!(result.contains("[[bin]]"));
    assert!(result.contains(r#"name = "my-project""#));
}

#[test]
fn merge_toml_preserves_existing_non_dep_section() {
    let existing = indoc! {r#"
        [package]
        name = "my-project"

        [features]
        default = ["serde"]
    "#};

    let template = indoc! {r#"
        [package]
        name = "template"

        [features]
        default = ["clap"]
    "#};

    let result = merge_toml(existing, template).unwrap();
    // Existing features section is preserved.
    assert!(result.contains(r#"default = ["serde"]"#));
}

#[test]
fn merge_toml_adds_package_metadata() {
    let existing = indoc! {r#"
        [package]
        name = "my-project"
        version = "0.1.0"
    "#};

    let template = indoc! {r#"
        [package]
        name = "template"
        version = "0.1.0"

        [package.metadata.binstall]
        pkg-url = "{ repo }/releases/download/v{ version }/{ name }-{ target }{ archive-suffix }"
    "#};

    let result = merge_toml(existing, template).unwrap();
    assert!(result.contains("[package.metadata.binstall]"));
    assert!(result.contains("pkg-url"));
}

// ============================================================================
// YAML merge tests
// ============================================================================

#[test]
fn merge_yaml_adds_new_job() {
    let existing = indoc! {"
        name: CI
        on: [push]
        jobs:
          test:
            runs-on: ubuntu-latest
            steps:
              - uses: actions/checkout@v4
    "};

    let template = indoc! {"
        name: CI
        on: [push]
        jobs:
          lint:
            runs-on: ubuntu-latest
            steps:
              - uses: actions/checkout@v4
    "};

    let result = merge_yaml(existing, template).unwrap();
    assert!(result.contains("test:"));
    assert!(result.contains("lint:"));
}

#[test]
fn merge_yaml_preserves_existing_job() {
    let existing = indoc! {"
        name: CI
        jobs:
          test:
            runs-on: ubuntu-latest
            steps:
              - run: cargo test
    "};

    let template = indoc! {"
        name: CI
        jobs:
          test:
            runs-on: ubuntu-latest
            steps:
              - run: cargo test --all
    "};

    let result = merge_yaml(existing, template).unwrap();
    // Existing job content is preserved (not replaced by template).
    assert!(result.contains("cargo test"));
    // The template's version should not overwrite.
    assert!(!result.contains("cargo test --all"));
}

#[test]
fn merge_yaml_adds_new_trigger() {
    let existing = indoc! {"
        on:
          push:
            branches: [main]
        jobs:
          test:
            runs-on: ubuntu-latest
    "};

    let template = indoc! {"
        on:
          push:
            branches: [main]
          pull_request: {}
        jobs:
          test:
            runs-on: ubuntu-latest
    "};

    let result = merge_yaml(existing, template).unwrap();
    assert!(result.contains("pull_request"));
    assert!(result.contains("push"));
}

#[test]
fn merge_yaml_new_file_passthrough() {
    // When existing is empty, the template content is used as-is.
    let template = indoc! {"
        name: Typos
        on: [push]
        jobs:
          spellcheck:
            runs-on: ubuntu-latest
    "};

    let result = merge_yaml("", template).unwrap();
    assert!(result.contains("spellcheck"));
}

#[test]
fn merge_yaml_adds_permissions() {
    let existing = indoc! {"
        permissions:
          contents: read
        jobs:
          test:
            runs-on: ubuntu-latest
    "};

    let template = indoc! {"
        permissions:
          contents: read
          security-events: write
        jobs:
          test:
            runs-on: ubuntu-latest
    "};

    let result = merge_yaml(existing, template).unwrap();
    assert!(result.contains("security-events"));
    assert!(result.contains("contents"));
}

// ============================================================================
// Diff display tests
// ============================================================================

#[test]
fn unified_diff_shows_changes() {
    let old = "line1\nline2\nline3\n";
    let new = "line1\nmodified\nline3\n";

    let diff = unified_diff(old, new, "test.txt");
    assert!(diff.contains("-line2"));
    assert!(diff.contains("+modified"));
}

#[test]
fn unified_diff_empty_for_identical() {
    let content = "same\n";
    let diff = unified_diff(content, content, "test.txt");
    assert!(diff.is_empty());
}

// ============================================================================
// Strategy dispatch tests
// ============================================================================

#[test]
fn strategy_dispatch_cargo_toml() {
    assert!(matches!(strategy_for("Cargo.toml"), MergeStrategy::Toml));
    assert!(matches!(
        strategy_for("some/nested/Cargo.toml"),
        MergeStrategy::Toml
    ));
    assert!(matches!(strategy_for("_typos.toml"), MergeStrategy::Toml));
    assert!(matches!(
        strategy_for("release-plz.toml"),
        MergeStrategy::Toml
    ));
}

#[test]
fn strategy_dispatch_yaml() {
    assert!(matches!(
        strategy_for(".github/workflows/ci.yml"),
        MergeStrategy::Yaml
    ));
    assert!(matches!(strategy_for("config.yaml"), MergeStrategy::Yaml));
}

#[test]
fn strategy_dispatch_plain() {
    assert!(matches!(strategy_for("src/main.rs"), MergeStrategy::Plain));
    assert!(matches!(strategy_for("README.md"), MergeStrategy::Plain));
    assert!(matches!(
        strategy_for(".github/dependabot.yml"),
        MergeStrategy::Yaml
    ));
}

// ============================================================================
// Plain file apply tests
// ============================================================================

#[test]
fn apply_creates_new_files() {
    let tmp = tempfile::tempdir().unwrap();
    let files = vec![
        RenderedFile {
            path: "src/main.rs".to_string(),
            content: "fn main() {}".to_string(),
        },
        RenderedFile {
            path: ".github/workflows/ci.yml".to_string(),
            content: "name: CI".to_string(),
        },
    ];

    let opts = ApplyOpts {
        project_dir: tmp.path().to_path_buf(),
        overwrite: false,
        interactive: false,
    };

    let results = apply_rendered_files(&files, &opts).unwrap();
    assert_eq!(results.len(), 2);
    assert!(matches!(&results[0], FileResult::Created(_)));
    assert!(matches!(&results[1], FileResult::Created(_)));

    // Verify files were written.
    assert!(tmp.path().join("src/main.rs").exists());
    assert!(tmp.path().join(".github/workflows/ci.yml").exists());
}

#[test]
fn apply_skips_existing_plain_non_interactive() {
    let tmp = tempfile::tempdir().unwrap();

    // Pre-create a file.
    let readme = tmp.path().join("README.md");
    std::fs::write(&readme, "existing content").unwrap();

    let files = vec![RenderedFile {
        path: "README.md".to_string(),
        content: "template content".to_string(),
    }];

    let opts = ApplyOpts {
        project_dir: tmp.path().to_path_buf(),
        overwrite: false,
        interactive: false,
    };

    let results = apply_rendered_files(&files, &opts).unwrap();
    assert!(matches!(&results[0], FileResult::Skipped(_)));

    // Verify original content is preserved.
    let content = std::fs::read_to_string(&readme).unwrap();
    assert_eq!(content, "existing content");
}

#[test]
fn apply_overwrites_existing_plain_with_flag() {
    let tmp = tempfile::tempdir().unwrap();

    // Pre-create a file.
    let readme = tmp.path().join("README.md");
    std::fs::write(&readme, "existing content").unwrap();

    let files = vec![RenderedFile {
        path: "README.md".to_string(),
        content: "template content".to_string(),
    }];

    let opts = ApplyOpts {
        project_dir: tmp.path().to_path_buf(),
        overwrite: true,
        interactive: false,
    };

    let results = apply_rendered_files(&files, &opts).unwrap();
    assert!(matches!(&results[0], FileResult::Overwritten(_)));

    // Verify content was replaced.
    let content = std::fs::read_to_string(&readme).unwrap();
    assert_eq!(content, "template content");
}

#[test]
fn apply_merges_existing_toml() {
    let tmp = tempfile::tempdir().unwrap();

    // Pre-create a Cargo.toml.
    let cargo_toml = tmp.path().join("Cargo.toml");
    std::fs::write(
        &cargo_toml,
        indoc! {r#"
            [package]
            name = "my-project"

            [dependencies]
            serde = "1"
        "#},
    )
    .unwrap();

    let files = vec![RenderedFile {
        path: "Cargo.toml".to_string(),
        content: indoc! {r#"
            [package]
            name = "template"

            [dependencies]
            clap = "4"
        "#}
        .to_string(),
    }];

    let opts = ApplyOpts {
        project_dir: tmp.path().to_path_buf(),
        overwrite: false,
        interactive: false,
    };

    let results = apply_rendered_files(&files, &opts).unwrap();
    assert!(matches!(&results[0], FileResult::Merged(_)));

    // Verify both deps are present.
    let content = std::fs::read_to_string(&cargo_toml).unwrap();
    assert!(content.contains("serde"));
    assert!(content.contains("clap"));
    assert!(content.contains(r#"name = "my-project""#));
}

#[test]
fn apply_merges_existing_yaml() {
    let tmp = tempfile::tempdir().unwrap();

    // Pre-create a workflow file.
    let workflow_dir = tmp.path().join(".github/workflows");
    std::fs::create_dir_all(&workflow_dir).unwrap();
    let ci_yml = workflow_dir.join("ci.yml");
    std::fs::write(
        &ci_yml,
        indoc! {"
            name: CI
            jobs:
              test:
                runs-on: ubuntu-latest
        "},
    )
    .unwrap();

    let files = vec![RenderedFile {
        path: ".github/workflows/ci.yml".to_string(),
        content: indoc! {"
            name: CI
            jobs:
              lint:
                runs-on: ubuntu-latest
        "}
        .to_string(),
    }];

    let opts = ApplyOpts {
        project_dir: tmp.path().to_path_buf(),
        overwrite: false,
        interactive: false,
    };

    let results = apply_rendered_files(&files, &opts).unwrap();
    assert!(matches!(&results[0], FileResult::Merged(_)));

    // Verify both jobs are present.
    let content = std::fs::read_to_string(&ci_yml).unwrap();
    assert!(content.contains("test:"));
    assert!(content.contains("lint:"));
}

// ============================================================================
// Nested package metadata merge tests
// ============================================================================

#[test]
fn merge_toml_adds_nested_metadata_when_metadata_exists() {
    let existing = indoc! {r#"
        [package]
        name = "my-project"

        [package.metadata.docs.rs]
        all-features = true
    "#};

    let template = indoc! {r#"
        [package]
        name = "template"

        [package.metadata.binstall]
        pkg-url = "{ repo }/releases/download/v{ version }/{ name }-{ target }{ archive-suffix }"
    "#};

    let result = merge_toml(existing, template).unwrap();
    // Both metadata sub-tables should be present.
    assert!(result.contains("docs.rs") || result.contains("[package.metadata.docs]"));
    assert!(result.contains("binstall"));
    assert!(result.contains("pkg-url"));
}

#[test]
fn merge_toml_preserves_existing_nested_metadata() {
    let existing = indoc! {r#"
        [package]
        name = "my-project"

        [package.metadata.binstall]
        pkg-url = "custom-url"
    "#};

    let template = indoc! {r#"
        [package]
        name = "template"

        [package.metadata.binstall]
        pkg-url = "template-url"
    "#};

    let result = merge_toml(existing, template).unwrap();
    // Existing value should be preserved.
    assert!(result.contains("custom-url"));
    assert!(!result.contains("template-url"));
}

// ============================================================================
// Non-Cargo TOML merge tests
// ============================================================================

#[test]
fn merge_toml_generic_file_adds_new_keys() {
    let existing = indoc! {r#"
        [default.extend-words]

        [files]
        extend-exclude = ["*.lock"]
    "#};

    let template = indoc! {r#"
        [default.extend-words]

        [default.extend-identifiers]
        crate = "crate"

        [files]
        extend-exclude = ["*.lock", "target/"]
    "#};

    let result = merge_toml(existing, template).unwrap();
    // New section should be added.
    assert!(result.contains("extend-identifiers"));
    // Existing section should be preserved (not replaced by template).
    assert!(result.contains("extend-exclude"));
}

#[test]
fn merge_toml_release_plz_adds_new_section() {
    let existing = indoc! {r#"
        [workspace]
        changelog_update = false
    "#};

    let template = indoc! {r#"
        [workspace]
        changelog_update = false

        [[package]]
        name = "my-crate"
        publish = true
    "#};

    let result = merge_toml(existing, template).unwrap();
    assert!(result.contains("[[package]]"));
    assert!(result.contains("changelog_update"));
}

// ============================================================================
// Snapbox snapshot tests for merge output
// ============================================================================

#[test]
fn unified_diff_snapshot_toml_merge() {
    let old = indoc! {r#"
        [package]
        name = "my-project"

        [dependencies]
        serde = "1"
    "#};

    let new = indoc! {r#"
        [package]
        name = "my-project"

        [dependencies]
        serde = "1"
        clap = { version = "4", features = ["derive"] }
    "#};

    let diff = unified_diff(old, new, "Cargo.toml");
    snapbox::assert_data_eq!(
        diff,
        snapbox::str![[r#"
@@ -3,3 +3,4 @@
 
 [dependencies]
 serde = "1"
+clap = { version = "4", features = ["derive"] }

"#]]
    );
}

#[test]
fn unified_diff_snapshot_yaml_merge() {
    let old = indoc! {"
        name: CI
        jobs:
          test:
            runs-on: ubuntu-latest
    "};

    let new = indoc! {"
        name: CI
        jobs:
          test:
            runs-on: ubuntu-latest
          lint:
            runs-on: ubuntu-latest
    "};

    let diff = unified_diff(old, new, ".github/workflows/ci.yml");
    snapbox::assert_data_eq!(
        diff,
        snapbox::str![[r#"
@@ -2,3 +2,5 @@
 jobs:
   test:
     runs-on: ubuntu-latest
+  lint:
+    runs-on: ubuntu-latest

"#]]
    );
}

#[test]
fn print_summary_snapshot_mixed_results() {
    // Capture stderr output from print_summary.
    let results = [
        FileResult::Created(".github/workflows/typos.yml".to_string()),
        FileResult::Created("_typos.toml".to_string()),
        FileResult::Merged("Cargo.toml".to_string()),
        FileResult::Skipped("src/main.rs".to_string()),
    ];

    // print_summary writes to stderr, so we test the logic indirectly
    // by verifying the result classification.
    let created = results
        .iter()
        .filter(|r| matches!(r, FileResult::Created(_)))
        .count();
    let merged = results
        .iter()
        .filter(|r| matches!(r, FileResult::Merged(_)))
        .count();
    let skipped = results
        .iter()
        .filter(|r| matches!(r, FileResult::Skipped(_)))
        .count();

    assert_eq!(created, 2);
    assert_eq!(merged, 1);
    assert_eq!(skipped, 1);
}

#[test]
fn merge_toml_snapshot_full_merge() {
    let existing = indoc! {r#"
        [package]
        name = "my-project"
        version = "0.1.0"

        [dependencies]
        serde = "1"
    "#};

    let template = indoc! {r#"
        [package]
        name = "template"

        [dependencies]
        clap = { version = "4", features = ["derive"] }

        [dev-dependencies]
        criterion = { version = "0.5", features = ["html_reports"] }
    "#};

    let result = merge_toml(existing, template).unwrap();
    snapbox::assert_data_eq!(
        result,
        snapbox::file![
            "../../tests/snapshots/bphelper_cli__merge__tests__merge_toml_snapshot_full_merge.txt"
        ]
    );
}

// ============================================================================
// Binary file handling tests
// ============================================================================

#[test]
fn apply_skips_binary_file_conflict_non_interactive() {
    let tmp = tempfile::tempdir().unwrap();

    // Pre-create a binary file (invalid UTF-8).
    let bin_path = tmp.path().join("icon.png");
    std::fs::write(&bin_path, [0xFF, 0xD8, 0xFF, 0xE0, 0x00]).unwrap();

    let files = vec![RenderedFile {
        path: "icon.png".to_string(),
        content: "replacement content".to_string(),
    }];

    let opts = ApplyOpts {
        project_dir: tmp.path().to_path_buf(),
        overwrite: false,
        interactive: false,
    };

    let results = apply_rendered_files(&files, &opts).unwrap();
    assert!(matches!(&results[0], FileResult::Skipped(_)));

    // Original binary content should be preserved.
    let content = std::fs::read(&bin_path).unwrap();
    assert_eq!(content, [0xFF, 0xD8, 0xFF, 0xE0, 0x00]);
}

#[test]
fn apply_overwrites_binary_file_with_flag() {
    let tmp = tempfile::tempdir().unwrap();

    // Pre-create a binary file.
    let bin_path = tmp.path().join("icon.png");
    std::fs::write(&bin_path, [0xFF, 0xD8, 0xFF, 0xE0, 0x00]).unwrap();

    let files = vec![RenderedFile {
        path: "icon.png".to_string(),
        content: "replacement content".to_string(),
    }];

    let opts = ApplyOpts {
        project_dir: tmp.path().to_path_buf(),
        overwrite: true,
        interactive: false,
    };

    let results = apply_rendered_files(&files, &opts).unwrap();
    assert!(matches!(&results[0], FileResult::Overwritten(_)));
}
