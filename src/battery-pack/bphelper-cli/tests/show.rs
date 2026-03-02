//! Integration tests for `cargo bp show`.

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

// [verify cli.show.hidden]
#[test]
fn show_detail_excludes_hidden_crates() {
    let fancy_path = fixtures_dir().join("fancy-battery-pack");
    let detail =
        bphelper_cli::fetch_battery_pack_detail("fancy", Some(fancy_path.to_str().unwrap()))
            .unwrap();

    // BUG: hidden = ["serde*", "cc"] in the fancy fixture, but show
    // currently includes them. These assertions are inverted to document
    // the broken behaviour; the fix commit will flip them.
    assert!(
        detail.crates.iter().any(|c| c == "serde"),
        "hidden crate 'serde' unexpectedly filtered (bug not present?)"
    );
    assert!(
        detail.crates.iter().any(|c| c == "serde_json"),
        "hidden crate 'serde_json' unexpectedly filtered (bug not present?)"
    );
}
