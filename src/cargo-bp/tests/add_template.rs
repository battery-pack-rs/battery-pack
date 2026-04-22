//! Integration tests for `cargo bp add --template`.

use assert_cmd::Command;
use snapbox::{assert_data_eq, str};
use std::path::Path;

fn cargo_bp() -> Command {
    Command::new(assert_cmd::cargo::cargo_bin!("cargo-bp"))
}

fn fixtures_dir() -> std::path::PathBuf {
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

/// Create a minimal existing project in a temp directory.
fn create_existing_project(dir: &Path) {
    std::fs::write(
        dir.join("Cargo.toml"),
        "[package]\nname = \"my-app\"\nversion = \"0.1.0\"\nedition = \"2021\"\n\n[dependencies]\nserde = \"1\"\n",
    )
    .unwrap();
    std::fs::create_dir_all(dir.join("src")).unwrap();
    std::fs::write(dir.join("src/main.rs"), "fn main() {}\n").unwrap();
}

#[test]
fn add_template_merges_into_existing_project() {
    let tmp = tempfile::tempdir().unwrap();
    create_existing_project(tmp.path());

    let fixture = fixtures_dir().join("fancy-battery-pack");

    let output = cargo_bp()
        .args([
            "bp",
            "add",
            "fancy",
            "-t",
            "default",
            "--path",
            &fixture.to_string_lossy(),
            "-N",
        ])
        .current_dir(tmp.path())
        .output()
        .expect("failed to run cargo-bp");

    assert!(
        output.status.success(),
        "stderr: {}",
        String::from_utf8_lossy(&output.stderr)
    );

    // Verify Cargo.toml was merged.
    let cargo_toml = std::fs::read_to_string(tmp.path().join("Cargo.toml")).unwrap();
    assert!(
        cargo_toml.contains("serde"),
        "existing dep should be preserved"
    );
    assert!(cargo_toml.contains("clap"), "template dep should be added");
    assert!(
        cargo_toml.contains("dialoguer"),
        "template dep should be added"
    );
    assert!(
        cargo_toml.contains(r#"name = "my-app""#),
        "existing package name should be preserved"
    );

    // Snapshot the merge summary output.
    let stderr = String::from_utf8_lossy(&output.stderr);
    assert_data_eq!(
        stderr.as_ref(),
        str![[r#"
merging Cargo.toml:
@@ -5,3 +5,5 @@
 
 [dependencies]
 serde = "1"
+clap = { version = "4", features = ["derive"] }
+dialoguer = "0.11"

  create .github/workflows/ci.yml
  merge Cargo.toml
  skip src/main.rs

1 created, 1 merged, 1 skipped

"#]]
    );
}

#[test]
fn add_template_creates_new_files() {
    let tmp = tempfile::tempdir().unwrap();
    // Project with Cargo.toml but no src/main.rs.
    std::fs::write(
        tmp.path().join("Cargo.toml"),
        "[package]\nname = \"my-app\"\nversion = \"0.1.0\"\nedition = \"2021\"\n",
    )
    .unwrap();

    let fixture = fixtures_dir().join("fancy-battery-pack");

    let output = cargo_bp()
        .args([
            "bp",
            "add",
            "fancy",
            "-t",
            "default",
            "--path",
            &fixture.to_string_lossy(),
            "-N",
        ])
        .current_dir(tmp.path())
        .output()
        .expect("failed to run cargo-bp");

    assert!(
        output.status.success(),
        "stderr: {}",
        String::from_utf8_lossy(&output.stderr)
    );

    // src/main.rs should be created.
    assert!(tmp.path().join("src/main.rs").exists());

    let stderr = String::from_utf8_lossy(&output.stderr);
    assert_data_eq!(
        stderr.as_ref(),
        str![[r#"
merging Cargo.toml:
@@ -2,3 +2,7 @@
 name = "my-app"
 version = "0.1.0"
 edition = "2021"
+
+[dependencies]
+clap = { version = "4", features = ["derive"] }
+dialoguer = "0.11"

  create .github/workflows/ci.yml
  merge Cargo.toml
  create src/main.rs

2 created, 1 merged

"#]]
    );
}

#[test]
fn add_template_skips_existing_plain_non_interactive() {
    let tmp = tempfile::tempdir().unwrap();
    create_existing_project(tmp.path());

    let fixture = fixtures_dir().join("fancy-battery-pack");

    let output = cargo_bp()
        .args([
            "bp",
            "add",
            "fancy",
            "-t",
            "default",
            "--path",
            &fixture.to_string_lossy(),
            "-N",
        ])
        .current_dir(tmp.path())
        .output()
        .expect("failed to run cargo-bp");

    assert!(output.status.success());

    // src/main.rs should still have the original content (skipped).
    let main_rs = std::fs::read_to_string(tmp.path().join("src/main.rs")).unwrap();
    assert_eq!(main_rs, "fn main() {}\n");
}

#[test]
fn add_template_overwrite_replaces_plain_files() {
    let tmp = tempfile::tempdir().unwrap();
    create_existing_project(tmp.path());

    let fixture = fixtures_dir().join("fancy-battery-pack");

    let output = cargo_bp()
        .args([
            "bp",
            "add",
            "fancy",
            "-t",
            "default",
            "--path",
            &fixture.to_string_lossy(),
            "-N",
            "--overwrite",
        ])
        .current_dir(tmp.path())
        .output()
        .expect("failed to run cargo-bp");

    assert!(
        output.status.success(),
        "stderr: {}",
        String::from_utf8_lossy(&output.stderr)
    );

    // With --overwrite, src/main.rs should have the template content.
    let main_rs = std::fs::read_to_string(tmp.path().join("src/main.rs")).unwrap();
    assert!(main_rs.contains("Hello from default template"));

    // Snapshot: overwrite instead of skip.
    let stderr = String::from_utf8_lossy(&output.stderr);
    assert_data_eq!(
        stderr.as_ref(),
        str![[r#"
merging Cargo.toml:
@@ -5,3 +5,5 @@
 
 [dependencies]
 serde = "1"
+clap = { version = "4", features = ["derive"] }
+dialoguer = "0.11"

  create .github/workflows/ci.yml
  merge Cargo.toml
  overwrite src/main.rs

1 created, 1 merged, 1 overwritten

"#]]
    );
}

#[test]
fn add_template_unknown_template_errors() {
    let tmp = tempfile::tempdir().unwrap();
    create_existing_project(tmp.path());

    let fixture = fixtures_dir().join("fancy-battery-pack");

    let output = cargo_bp()
        .args([
            "bp",
            "add",
            "fancy",
            "-t",
            "nonexistent",
            "--path",
            &fixture.to_string_lossy(),
            "-N",
        ])
        .current_dir(tmp.path())
        .output()
        .expect("failed to run cargo-bp");

    assert!(!output.status.success());
    let stderr = String::from_utf8_lossy(&output.stderr);
    assert_data_eq!(
        stderr.as_ref(),
        str![[r#"
Error: Template 'nonexistent' not found. Available templates: default, full

"#]]
    );
}

#[test]
fn add_template_no_conflicts_all_created() {
    let tmp = tempfile::tempdir().unwrap();
    // Empty directory, no Cargo.toml at all.

    let fixture = fixtures_dir().join("fancy-battery-pack");

    let output = cargo_bp()
        .args([
            "bp",
            "add",
            "fancy",
            "-t",
            "default",
            "--path",
            &fixture.to_string_lossy(),
            "-N",
        ])
        .current_dir(tmp.path())
        .output()
        .expect("failed to run cargo-bp");

    assert!(
        output.status.success(),
        "stderr: {}",
        String::from_utf8_lossy(&output.stderr)
    );

    // Both files should be created fresh.
    assert!(tmp.path().join("Cargo.toml").exists());
    assert!(tmp.path().join("src/main.rs").exists());

    let stderr = String::from_utf8_lossy(&output.stderr);
    assert_data_eq!(
        stderr.as_ref(),
        snapbox::file!["snapshots/add_template_no_conflicts.txt"]
    );
}

#[test]
fn add_template_merges_yaml_additively() {
    let tmp = tempfile::tempdir().unwrap();
    create_existing_project(tmp.path());

    // Pre-create a workflow file with an existing job.
    let wf_dir = tmp.path().join(".github/workflows");
    std::fs::create_dir_all(&wf_dir).unwrap();
    std::fs::write(
        wf_dir.join("ci.yml"),
        "name: CI\non: [push]\njobs:\n  test:\n    runs-on: ubuntu-latest\n    steps:\n      - run: cargo test\n",
    )
    .unwrap();

    let fixture = fixtures_dir().join("fancy-battery-pack");

    let output = cargo_bp()
        .args([
            "bp",
            "add",
            "fancy",
            "-t",
            "default",
            "--path",
            &fixture.to_string_lossy(),
            "-N",
        ])
        .current_dir(tmp.path())
        .output()
        .expect("failed to run cargo-bp");

    assert!(
        output.status.success(),
        "stderr: {}",
        String::from_utf8_lossy(&output.stderr)
    );

    // Verify YAML was merged: existing job preserved, new job added.
    let ci_yml = std::fs::read_to_string(wf_dir.join("ci.yml")).unwrap();
    assert!(ci_yml.contains("test:"), "existing job should be preserved");
    assert!(ci_yml.contains("lint:"), "template job should be added");

    // Verify stderr mentions the YAML merge.
    let stderr = String::from_utf8_lossy(&output.stderr);
    assert!(stderr.contains("merge .github/workflows/ci.yml"));
}
