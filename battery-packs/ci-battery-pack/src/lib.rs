#![doc = include_str!(concat!(env!("OUT_DIR"), "/docs.md"))]

#[cfg(test)]
mod tests {
    use ::battery_pack::testing::PreviewBuilder;
    use snapbox::{Assert, Redactions, file};

    /// Custom assert that unconditionally maps `[EXE]` to `.exe` so snapshots
    /// containing literal `.exe` (e.g. in GitHub Actions workflow templates)
    /// pass on all platforms.
    fn assert_snapshot(actual: impl snapbox::IntoData, expected: impl snapbox::IntoData) {
        let mut redactions = Redactions::new();
        redactions.insert("[EXE]", ".exe").unwrap();
        Assert::new()
            .action_env("SNAPSHOTS")
            .redact_with(redactions)
            .eq(actual, expected);
    }

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
        assert_snapshot(snapshot("full", &[]), file!["snapshots/minimalist.txt"]);
    }

    #[test]
    fn snapshot_maximalist() {
        assert_snapshot(
            snapshot("full", &[("all", "true")]),
            file!["snapshots/maximalist.txt"],
        );
    }

    #[test]
    fn snapshot_standalone_benchmarks() {
        assert_snapshot(
            snapshot("benchmarks", &[]),
            file!["snapshots/standalone_benchmarks.txt"],
        );
    }

    #[test]
    fn snapshot_standalone_fuzzing() {
        assert_snapshot(
            snapshot("fuzzing", &[]),
            file!["snapshots/standalone_fuzzing.txt"],
        );
    }

    #[test]
    fn snapshot_standalone_stress_test() {
        assert_snapshot(
            snapshot("stress-test", &[]),
            file!["snapshots/standalone_stress_test.txt"],
        );
    }

    #[test]
    fn snapshot_standalone_mdbook() {
        assert_snapshot(
            snapshot("mdbook", &[]),
            file!["snapshots/standalone_mdbook.txt"],
        );
    }

    #[test]
    fn snapshot_standalone_spellcheck() {
        assert_snapshot(
            snapshot("spellcheck", &[]),
            file!["snapshots/standalone_spellcheck.txt"],
        );
    }

    #[test]
    fn snapshot_standalone_xtask() {
        assert_snapshot(
            snapshot("xtask", &[]),
            file!["snapshots/standalone_xtask.txt"],
        );
    }

    #[test]
    fn snapshot_standalone_binary_release() {
        assert_snapshot(
            snapshot("binary-release", &[]),
            file!["snapshots/standalone_binary_release.txt"],
        );
    }

    #[test]
    fn snapshot_standalone_trusted_publishing() {
        assert_snapshot(
            snapshot("trusted-publishing", &[]),
            file!["snapshots/standalone_trusted_publishing.txt"],
        );
    }

    #[test]
    fn snapshot_standalone_mutation_testing() {
        assert_snapshot(
            snapshot("mutation-testing", &[]),
            file!["snapshots/standalone_mutation_testing.txt"],
        );
    }

    #[test]
    fn snapshot_standalone_clippy_sarif() {
        assert_snapshot(
            snapshot("clippy-sarif", &[]),
            file!["snapshots/standalone_clippy_sarif.txt"],
        );
    }
}
