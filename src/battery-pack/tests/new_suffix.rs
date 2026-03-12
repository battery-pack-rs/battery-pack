//! Tests for `cargo bp new` auto-appending `-battery-pack` to project names.

use assert_cmd::Command;

fn cargo_bp() -> Command {
    Command::new(assert_cmd::cargo::cargo_bin!("cargo-bp"))
}

fn crate_root() -> std::path::PathBuf {
    std::path::Path::new(env!("CARGO_MANIFEST_DIR")).to_path_buf()
}

/// Run `cargo bp new battery-pack --name <name> --path <template_root>` in a temp dir
/// and return the generated package name from Cargo.toml.
fn generate(name: &str) -> (String, String) {
    generate_with_pack("battery-pack", name)
}

fn generate_with_pack(pack: &str, name: &str) -> (String, String) {
    let tmp = tempfile::tempdir().unwrap();

    cargo_bp()
        .args([
            "bp",
            "new",
            pack,
            "--name",
            name,
            "--path",
            &crate_root().to_string_lossy(),
        ])
        .current_dir(tmp.path())
        .assert()
        .success();

    let entry = std::fs::read_dir(tmp.path())
        .unwrap()
        .next()
        .unwrap()
        .unwrap();
    let dir_name = entry.file_name().to_string_lossy().into_owned();
    let manifest = std::fs::read_to_string(entry.path().join("Cargo.toml")).unwrap();
    let parsed: toml::Value = toml::from_str(&manifest).unwrap();
    let pkg_name = parsed["package"]["name"].as_str().unwrap().to_string();
    (dir_name, pkg_name)
}

#[test]
fn new_appends_battery_pack_suffix() {
    let (dir_name, pkg_name) = generate("kafka");
    assert_eq!(dir_name, "kafka-battery-pack");
    assert_eq!(pkg_name, "kafka-battery-pack");
}

#[test]
fn new_preserves_existing_suffix() {
    let (dir_name, pkg_name) = generate("kafka-battery-pack");
    assert_eq!(dir_name, "kafka-battery-pack");
    assert_eq!(pkg_name, "kafka-battery-pack");
}

#[test]
fn new_non_battery_pack_preserves_name() {
    // When the battery pack is not "battery-pack", the project name should
    // be used as-is without appending -battery-pack.
    let (dir_name, pkg_name) = generate_with_pack("other-pack", "my-app");
    assert_eq!(dir_name, "my-app");
    assert_eq!(pkg_name, "my-app");
}
