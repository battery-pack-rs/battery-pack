#![doc = include_str!(concat!(env!("OUT_DIR"), "/docs.md"))]

#[cfg(test)]
mod tests {
    use ::battery_pack::testing::{PreviewBuilder, PreviewFile, validate_template_with};
    use snapbox::{assert_data_eq, file};

    /// Renders every file of the `service` template with the given placeholders into one string
    /// suitable for a snapshot. To accept rendering changes: `SNAPSHOTS=overwrite cargo test`.
    fn render(defines: &[(&str, &str)]) -> String {
        let mut builder = PreviewBuilder::new(env!("CARGO_MANIFEST_DIR"))
            .template("templates/service")
            .project_name("test-service");
        for (key, value) in defines {
            builder = builder.define(*key, *value);
        }
        let files: Vec<PreviewFile> = builder.preview().unwrap();
        let mut out = String::new();
        for f in &files {
            out.push_str(&format!("── {} ──\n{}\n\n", f.path, f.content));
        }
        out
    }

    /// Default template (axum, in-memory-or-forward store, jemalloc, dial9, stdout, benchmarks).
    #[test]
    fn validate() {
        ::battery_pack::testing::validate(env!("CARGO_MANIFEST_DIR")).unwrap();
    }

    #[test]
    fn snapshot_default() {
        assert_data_eq!(render(&[]), file!["snapshots/default.txt"]);
    }

    #[test]
    fn snapshot_minimal() {
        assert_data_eq!(
            render(&[
                ("dial9", "false"),
                ("allocator", "mimalloc"),
                ("benchmarks", "false"),
                ("tower_timeout", "false"),
                ("tower_catch_panic", "false"),
                ("tower_on_early_drop", "false"),
            ]),
            file!["snapshots/minimal.txt"]
        );
    }

    // Feature combinations beyond the defaults. These share the `bp-validate` target cache with
    // `validate`, so dependencies compile once and each combo only rebuilds the generated crate.

    /// Exercises the optional inbound rate-limit layer (off by default).
    #[test]
    fn validate_rate_limited() {
        validate_template_with(
            env!("CARGO_MANIFEST_DIR"),
            "service",
            &[("rate_limit", "true")],
        )
        .unwrap();
    }

    /// No dial9, mimalloc, disk metrics, no benchmarks, and every Tower layer disabled, exercising
    /// the "off" path of each toggle in one combination.
    #[test]
    fn validate_minimal() {
        validate_template_with(
            env!("CARGO_MANIFEST_DIR"),
            "service",
            &[
                ("dial9", "false"),
                ("allocator", "mimalloc"),
                ("benchmarks", "false"),
                ("tower_timeout", "false"),
                ("tower_catch_panic", "false"),
                ("tower_on_early_drop", "false"),
            ],
        )
        .unwrap();
    }
}
