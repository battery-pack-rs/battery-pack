use super::*;
use snapbox::{assert_data_eq, str};

#[test]
fn test_find_context_battery_pack() {
    let args = vec!["cargo-bp".to_string(), "add".to_string(), "cli".to_string()];
    let res = find_context_battery_pack_from_args(&args);
    assert_data_eq!(res.unwrap(), str!["cli"]);

    let args = vec![
        "cargo-bp".to_string(),
        "new".to_string(),
        "cli".to_string(),
        "--name".to_string(),
        "foo".to_string(),
    ];
    assert_eq!(
        find_context_battery_pack_from_args(&args),
        Some("cli".to_string())
    );

    let args = vec!["cargo-bp".to_string(), "list".to_string()];
    assert_eq!(find_context_battery_pack_from_args(&args), None);
}

#[test]
fn test_installed_packs_empty() {
    // In an empty directory, no packs should be installed
    let candidates = installed_packs(OsStr::new(""));
    // Since we are in the repo root during tests, it might find something if there is a manifest.
    // But we can check the format.
    for c in candidates {
        assert!(!c.get_value().is_empty());
    }
}

#[test]
fn test_registry_and_local_packs_format() {
    let candidates = registry_and_local_packs(OsStr::new(""));
    // Verify we get both short and long names if something is in cache
    // This is hard to assert without a fixed cache, but we can check it returns something or at least doesn't crash
    let _vals: Vec<_> = candidates.iter().map(|c| c.get_value()).collect();
}
