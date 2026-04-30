#![doc = include_str!(concat!(env!("OUT_DIR"), "/docs.md"))]

#[cfg(test)]
mod tests {
    use ::battery_pack::testing::PreviewBuilder;
    use snapbox::{assert_data_eq, file};

    #[test]
    fn validate() {
        ::battery_pack::testing::validate(env!("CARGO_MANIFEST_DIR")).unwrap();
    }

    fn snapshot(template: &str, defines: &[(&str, &str)]) -> String {
        let mut builder = PreviewBuilder::new(env!("CARGO_MANIFEST_DIR"))
            .template(format!("templates/{template}"))
            .define("ci_platform", "github")
            .define("repo_owner", "test-owner");
        for (k, v) in defines {
            builder = builder.define(*k, *v);
        }
        let files = builder.preview().unwrap();
        let mut out = String::new();
        for file in &files {
            out.push_str(&format!(
                "── {} ──\n{}\n",
                file.path,
                file.content.trim_end()
            ));
        }
        out
    }

    #[test]
    fn none_platform_strips_github_files() {
        let files = PreviewBuilder::new(env!("CARGO_MANIFEST_DIR"))
            .template("templates/full")
            .define("ci_platform", "none")
            .define("repo_owner", "test-owner")
            .define("all", "true")
            .preview()
            .unwrap();
        assert!(
            !files.iter().any(|f| f.path.contains(".github/")),
            "ci_platform=none should strip all .github/ files"
        );
    }

    // -- Merged snapshot tests --
    // Each test renders a template and snapshots ALL rendered files.
    // SHAs, MSRV, and version comments are masked with [..] in snapshot files.
    //
    // To update after template changes:
    //   SNAPSHOTS=overwrite cargo test -p ci-battery-pack -- snapshot_
    // Then re-apply masks with:
    //   sed -i 's/@[0-9a-f]\{40\}/@[..]/g; s/# v[0-9]*\.[0-9]*\.[0-9]*/# v[..]/g; s/rust-version = "[^"]*"/rust-version = "[..]"/g' battery-packs/ci-battery-pack/src/snapshots/*.txt

    #[test]
    fn snapshot_minimalist() {
        assert_data_eq!(snapshot("full", &[]), file!["snapshots/minimalist.txt"]);
    }

    #[test]
    fn snapshot_maximalist() {
        assert_data_eq!(
            snapshot("full", &[("all", "true")]),
            file!["snapshots/maximalist.txt"]
        );
    }

    #[test]
    fn snapshot_standalone_benchmarks() {
        assert_data_eq!(
            snapshot("benchmarks", &[]),
            file!["snapshots/standalone_benchmarks.txt"]
        );
    }

    #[test]
    fn snapshot_standalone_fuzzing() {
        assert_data_eq!(
            snapshot("fuzzing", &[]),
            file!["snapshots/standalone_fuzzing.txt"]
        );
    }

    #[test]
    fn snapshot_standalone_stress_test() {
        assert_data_eq!(
            snapshot("stress-test", &[]),
            file!["snapshots/standalone_stress_test.txt"]
        );
    }

    #[test]
    fn snapshot_standalone_mdbook() {
        assert_data_eq!(
            snapshot("mdbook", &[]),
            file!["snapshots/standalone_mdbook.txt"]
        );
    }

    #[test]
    fn snapshot_standalone_spellcheck() {
        assert_data_eq!(
            snapshot("spellcheck", &[]),
            file!["snapshots/standalone_spellcheck.txt"]
        );
    }

    #[test]
    fn snapshot_standalone_xtask() {
        assert_data_eq!(
            snapshot("xtask", &[]),
            file!["snapshots/standalone_xtask.txt"]
        );
    }

    #[test]
    fn snapshot_standalone_binary_release() {
        assert_data_eq!(
            snapshot("binary-release", &[]),
            file!["snapshots/standalone_binary_release.txt"]
        );
    }

    #[test]
    fn snapshot_standalone_trusted_publishing() {
        assert_data_eq!(
            snapshot("trusted-publishing", &[]),
            file!["snapshots/standalone_trusted_publishing.txt"]
        );
    }

    #[test]
    fn snapshot_standalone_mutation_testing() {
        assert_data_eq!(
            snapshot("mutation-testing", &[]),
            file!["snapshots/standalone_mutation_testing.txt"]
        );
    }

    #[test]
    fn snapshot_standalone_clippy_sarif() {
        assert_data_eq!(
            snapshot("clippy-sarif", &[]),
            file!["snapshots/standalone_clippy_sarif.txt"]
        );
    }
}
