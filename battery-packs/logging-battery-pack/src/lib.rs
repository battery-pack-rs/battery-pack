#![doc = include_str!(concat!(env!("OUT_DIR"), "/docs.md"))]

#[cfg(test)]
mod tests {
    #[test]
    fn validate_templates() {
        battery_pack::testing::validate_templates(env!("CARGO_MANIFEST_DIR")).unwrap();
    }
}
