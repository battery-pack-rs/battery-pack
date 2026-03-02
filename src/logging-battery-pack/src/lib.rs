const SELF_MANIFEST: &str = include_str!(concat!(env!("CARGO_MANIFEST_DIR"), "/Cargo.toml"));

/// Validate that the calling crate's dependencies match this battery pack's specs.
///
/// Call this from your build.rs:
/// ```rust,ignore
/// fn main() {
///     logging_battery_pack::validate();
/// }
/// ```
pub fn validate() {
    battery_pack::validate(SELF_MANIFEST);
}
