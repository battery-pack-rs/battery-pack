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

    /// Default template (axum, redis, jemalloc, dial9, stdout, benchmarks).
    #[test]
    fn validate() {
        ::battery_pack::testing::validate(env!("CARGO_MANIFEST_DIR")).unwrap();
    }

    #[test]
    fn snapshot_default() {
        assert_data_eq!(render(&[]), file!["snapshots/default.txt"]);
    }

    #[test]
    fn snapshot_http_circuit_breaker() {
        assert_data_eq!(
            render(&[("downstream", "http-service"), ("circuit_breaker", "true")]),
            file!["snapshots/http_circuit_breaker.txt"]
        );
    }

    #[test]
    fn snapshot_minimal() {
        assert_data_eq!(
            render(&[
                ("downstream", "none"),
                ("dial9", "false"),
                ("allocator", "mimalloc"),
                ("metrics_output", "disk"),
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

    #[test]
    fn validate_http_downstream_with_circuit_breaker() {
        validate_template_with(
            env!("CARGO_MANIFEST_DIR"),
            "service",
            &[("downstream", "http-service"), ("circuit_breaker", "true")],
        )
        .unwrap();
    }

    /// No downstream, no dial9, mimalloc, disk metrics, no benchmarks, and every Tower layer
    /// disabled, exercising the "off" path of each toggle in one container-free combination.
    #[test]
    fn validate_minimal() {
        validate_template_with(
            env!("CARGO_MANIFEST_DIR"),
            "service",
            &[
                ("downstream", "none"),
                ("dial9", "false"),
                ("allocator", "mimalloc"),
                ("metrics_output", "disk"),
                ("benchmarks", "false"),
                ("tower_timeout", "false"),
                ("tower_catch_panic", "false"),
                ("tower_on_early_drop", "false"),
            ],
        )
        .unwrap();
    }
}
