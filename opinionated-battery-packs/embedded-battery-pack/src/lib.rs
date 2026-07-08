#![cfg_attr(not(test), no_std)]
#![doc = include_str!(concat!(env!("OUT_DIR"), "/docs.md"))]

#[cfg(test)]
mod tests {
    // Template validation (cargo check on the rendered template) is intentionally
    // skipped because embedded HAL crates require cross-compilation targets and
    // device-specific features that cannot be built on the host.
    #[test]
    fn pack_parses() {
        let spec = ::bphelper_manifest::parse_battery_pack_from_path(
            std::path::Path::new(env!("CARGO_MANIFEST_DIR"))
                .join("Cargo.toml")
                .as_ref(),
        )
        .unwrap();
        assert_eq!(spec.name, "embedded-battery-pack");
        assert!(!spec.templates.is_empty());
        assert!(!spec.categories.is_empty());
    }
}
