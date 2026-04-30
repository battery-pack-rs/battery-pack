//! Test that the `with_template` authoring template produces a battery pack
//! whose inner template can itself generate a valid project.

use assert_cmd::Command;
use snapbox::{assert_data_eq, file, str};
use std::path::Path;

fn cargo_bp() -> Command {
    Command::new(assert_cmd::cargo::cargo_bin!("cargo-bp"))
}

fn battery_pack_root() -> std::path::PathBuf {
    Path::new(env!("CARGO_MANIFEST_DIR"))
        .parent()
        .unwrap()
        .join("battery-pack")
}

/// Patch crates-io deps so the generated battery pack resolves against local
/// workspace packages instead of published versions. This is similar to how our
/// `validate` helper works.
fn write_patches(bp_dir: &Path) {
    let cargo_dir = bp_dir.join(".cargo");
    std::fs::create_dir_all(&cargo_dir).unwrap();

    let crate_root = battery_pack_root();
    let patch = format!(
        "[patch.crates-io]\nbattery-pack = {{ path = \"{}\" }}\n",
        crate_root.display()
    );
    std::fs::write(cargo_dir.join("config.toml"), patch).unwrap();
}

#[test]
fn with_template_two_level_generation() {
    let tmp = tempfile::tempdir().unwrap();

    // Step 1: generate a battery pack from with_template
    cargo_bp()
        .args([
            "bp",
            "new",
            "battery-pack",
            "--name",
            "http",
            "--path",
            &battery_pack_root().to_string_lossy(),
            "--template",
            "with_template",
            "-d",
            "description=HTTP utilities",
        ])
        .current_dir(tmp.path())
        .assert()
        .success();

    let bp_dir = tmp.path().join("http-battery-pack");
    assert!(bp_dir.join("Cargo.toml").exists());
    assert!(bp_dir.join("templates/default/bp-template.toml").exists());
    assert!(bp_dir.join("templates/default/_Cargo.toml").exists());
    assert!(bp_dir.join("templates/default/src/main.rs").exists());

    // Verify outer battery pack content
    let bp_cargo = std::fs::read_to_string(bp_dir.join("Cargo.toml")).unwrap();
    assert!(bp_cargo.contains("name = \"http-battery-pack\""));
    assert!(bp_cargo.contains("description = \"HTTP utilities\""));
    assert_data_eq!(bp_cargo, file![_]);

    let bp_readme = std::fs::read_to_string(bp_dir.join("README.md")).unwrap();
    assert!(bp_readme.contains("# http-battery-pack"));
    assert!(bp_readme.contains("HTTP utilities"));
    assert_data_eq!(bp_readme, file![_]);

    let bp_lib = std::fs::read_to_string(bp_dir.join("src/lib.rs")).unwrap();
    assert!(bp_lib.contains("include_str!"));
    assert_data_eq!(bp_lib, file![_]);

    // Verify inner template has literal MiniJinja syntax (not rendered)
    let inner_cargo =
        std::fs::read_to_string(bp_dir.join("templates/default/_Cargo.toml")).unwrap();
    assert!(inner_cargo.contains("{{ project_name }}"));
    assert!(inner_cargo.contains("http-battery-pack"));
    assert_data_eq!(
        inner_cargo,
        str![[r#"
[package]
name = "{{ project_name }}"
version = "0.1.0"
edition = "2024"
description = "{{ project_description }}"
license = "MIT OR Apache-2.0"

[dependencies]
# you can declare dependencies normally, or reference
# versions managed by your battery pack:
# clap.bp-managed = true

[package.metadata.battery-pack]
http-battery-pack = { features = ["default"] }
"#]]
    );

    // Step 2: patch deps so validate can resolve against local workspace
    write_patches(&bp_dir);

    // Step 3: validate the generated battery pack (generates from inner
    // template, runs cargo check + cargo test on the result)
    cargo_bp()
        .args(["bp", "validate", "--path", &bp_dir.to_string_lossy()])
        .assert()
        .success();
}
