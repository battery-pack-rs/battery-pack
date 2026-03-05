#![doc = include_str!(concat!(env!("OUT_DIR"), "/docs.md"))]

/// Validate that the consumer's dependencies match this battery pack's specs.
///
/// Call from the consumer's `build.rs`:
/// ```rust,ignore
/// fn main() {
///     error_battery_pack::validate();
/// }
/// ```
pub fn validate() {
    battery_pack::validate(include_str!("../Cargo.toml"));
}

#[cfg(test)]
mod tests {
    #[test]
    fn validate_templates() {
        battery_pack::testing::validate_templates(env!("CARGO_MANIFEST_DIR")).unwrap();
    }
}
