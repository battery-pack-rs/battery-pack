//! Integration tests for `cargo bp show --template --non-interactive`.

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

#[test]
fn show_template_preview_prints_rendered_files() {
    let fixture = fixtures_dir().join("fancy-battery-pack");

    let output = cargo_bp()
        .args([
            "bp",
            "show",
            "fancy",
            "-t",
            "default",
            "--non-interactive",
            "--path",
            &fixture.to_string_lossy(),
        ])
        .output()
        .expect("failed to run cargo-bp");

    assert!(
        output.status.success(),
        "stderr: {}",
        String::from_utf8_lossy(&output.stderr)
    );

    let stdout = String::from_utf8_lossy(&output.stdout);
    assert_data_eq!(
        stdout.as_ref(),
        str![[r#"
── .github/workflows/ci.yml ──
name: CI
on: [push]
jobs:
  lint:
    runs-on: ubuntu-latest
    steps:
      - uses: actions/checkout@v4
      - run: cargo clippy

── Cargo.toml ──
[package]
name = "my-project"
version = "0.1.0"
edition = "2021"

[dependencies]
clap = { version = "4", features = ["derive"] }
dialoguer = "0.11"

── src/main.rs ──
fn main() {
    println!("Hello from default template!");
}


"#]]
    );
}

#[test]
fn show_template_preview_unknown_template_errors() {
    let fixture = fixtures_dir().join("fancy-battery-pack");

    let output = cargo_bp()
        .args([
            "bp",
            "show",
            "fancy",
            "-t",
            "nonexistent",
            "--non-interactive",
            "--path",
            &fixture.to_string_lossy(),
        ])
        .output()
        .expect("failed to run cargo-bp");

    assert!(!output.status.success());
    let stderr = String::from_utf8_lossy(&output.stderr);
    assert_data_eq!(
        stderr.as_ref(),
        str![[r#"
Error: Template 'nonexistent' not found. Available: default, full

"#]]
    );
}
