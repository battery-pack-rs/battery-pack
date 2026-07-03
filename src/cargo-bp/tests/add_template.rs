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

/// Regression test: `cargo bp add ci -t full` without `--path` downloads from
/// crates.io. The TempDir holding the extracted crate must stay alive until
/// rendering is complete. Previously, `ResolvedCrate` was dropped too early,
/// deleting the temp directory before the template could be read.
#[test]
fn add_template_registry_download_keeps_tempdir_alive() {
    let tmp = tempfile::tempdir().unwrap();
    create_existing_project(tmp.path());

    let output = cargo_bp()
        .args(["bp", "add", "ci", "-t", "spellcheck", "-N"])
        .current_dir(tmp.path())
        .output()
        .expect("failed to run cargo-bp");

    assert!(
        output.status.success(),
        "stderr: {}",
        String::from_utf8_lossy(&output.stderr)
    );
}

#[test]
fn add_template_records_in_state_and_shows_in_status() {
    let tmp = tempfile::tempdir().unwrap();

    let fixture = fixtures_dir().join("fancy-battery-pack");

    // Create a project that has fancy-battery-pack as a build-dep so status can find it.
    std::fs::write(
        tmp.path().join("Cargo.toml"),
        "[package]\nname = \"my-app\"\nversion = \"0.1.0\"\nedition = \"2021\"\n\n[dependencies]\nserde = \"1\"\n\n[build-dependencies]\nfancy-battery-pack = \"0.2.0\"\n",
    )
    .unwrap();
    std::fs::create_dir_all(tmp.path().join("src")).unwrap();
    std::fs::write(tmp.path().join("src/main.rs"), "fn main() {}\n").unwrap();

    // Apply a template.
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

    // Verify battery-pack.toml records the applied template.
    let state_content = std::fs::read_to_string(tmp.path().join("battery-pack.toml")).unwrap();
    assert!(
        state_content.contains("applied-templates"),
        "state file should contain applied-templates section:\n{state_content}"
    );
    assert!(
        state_content.contains(r#"applied-templates = ["default"]"#),
        "state file should record the 'default' template:\n{state_content}"
    );

    // Verify status --json includes the applied template.
    let status_output = cargo_bp()
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
        status_output.status.success(),
        "stderr: {}",
        String::from_utf8_lossy(&status_output.stderr)
    );

    let report: serde_json::Value =
        serde_json::from_slice(&status_output.stdout).expect("valid JSON from status");
    let pack = &report["packs"][0];
    assert_eq!(
        pack["applied_templates"],
        serde_json::json!(["default"]),
        "status JSON should include the applied template"
    );
}

#[test]
fn add_template_twice_does_not_duplicate_in_state() {
    let tmp = tempfile::tempdir().unwrap();
    create_existing_project(tmp.path());

    let fixture = fixtures_dir().join("fancy-battery-pack");

    // Apply the same template twice.
    for _ in 0..2 {
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
    }

    // Verify applied-templates list contains exactly one entry.
    let state_content = std::fs::read_to_string(tmp.path().join("battery-pack.toml")).unwrap();
    let count = state_content.matches("applied-templates").count();
    assert_eq!(
        count, 1,
        "applied-templates should appear once in state, found {count}:\n{state_content}"
    );
    assert!(
        state_content.contains(r#"applied-templates = ["default"]"#),
        "should contain exactly one entry:\n{state_content}"
    );
}

#[test]
fn new_from_template_records_in_state() {
    let tmp = tempfile::tempdir().unwrap();
    let fixture = fixtures_dir().join("fancy-battery-pack");

    // Generate a new project from the fancy battery pack's "default" template.
    let output = cargo_bp()
        .args([
            "bp",
            "new",
            "fancy",
            "--name",
            "my-app",
            "--path",
            &fixture.to_string_lossy(),
            "-t",
            "default",
        ])
        .current_dir(tmp.path())
        .output()
        .expect("failed to run cargo-bp");

    assert!(
        output.status.success(),
        "stderr: {}",
        String::from_utf8_lossy(&output.stderr)
    );

    // The generated project should have a battery-pack.toml recording the template.
    let project_dir = tmp.path().join("my-app");
    assert!(project_dir.join("Cargo.toml").exists());

    let state_path = project_dir.join("battery-pack.toml");
    assert!(
        state_path.exists(),
        "battery-pack.toml should be created in the new project"
    );

    let state_content = std::fs::read_to_string(&state_path).unwrap();
    assert!(
        state_content.contains(r#"applied-templates = ["default"]"#),
        "state file should record the template used:\n{state_content}"
    );
    assert!(
        state_content.contains(r#"name = "fancy""#),
        "state file should reference the battery pack:\n{state_content}"
    );
}
