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
        normalize_snapshot_output(&out)
    }

    fn normalize_snapshot_output(snapshot: &str) -> String {
        snapshot
            .lines()
            .map(normalize_snapshot_line)
            .collect::<Vec<_>>()
            .join("\n")
            + "\n"
    }

    fn normalize_snapshot_line(line: &str) -> String {
        let line = mask_action_resolution_failure(line);
        let line = mask_action_sha(&line);
        let line = strip_action_pin_comment(&line);
        mask_rust_version(&line)
    }

    fn mask_action_resolution_failure(line: &str) -> String {
        let Some((prefix, _)) = line.split_once("@could-not-resolve-git-sha-for-") else {
            return line.to_owned();
        };
        format!("{prefix}@[..]")
    }

    fn mask_action_sha(line: &str) -> String {
        let mut out = String::with_capacity(line.len());
        let mut rest = line;
        while let Some(index) = rest.find('@') {
            out.push_str(&rest[..index + 1]);
            let candidate = &rest[index + 1..];
            if candidate.len() >= 40 && candidate[..40].bytes().all(|b| b.is_ascii_hexdigit()) {
                out.push_str("[..]");
                rest = &candidate[40..];
            } else {
                rest = candidate;
            }
        }
        out.push_str(rest);
        out
    }

    fn strip_action_pin_comment(line: &str) -> String {
        if line.contains("@[..]") {
            return line.split_once(" # ").unwrap_or((line, "")).0.to_owned();
        }
        line.to_owned()
    }

    fn mask_rust_version(line: &str) -> String {
        if line.starts_with("rust-version = \"") {
            return "rust-version = \"[..]\"".to_owned();
        }
        line.to_owned()
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

    #[test]
    fn audit_issue_publication_can_be_disabled() {
        let files = PreviewBuilder::new(env!("CARGO_MANIFEST_DIR"))
            .template("templates/full")
            .define("ci_platform", "github")
            .define("repo_owner", "test-owner")
            .define("audit_issue", "false")
            .preview()
            .unwrap();
        let audit = files
            .iter()
            .find(|f| f.path == ".github/workflows/audit.yml")
            .unwrap();

        assert!(!audit.content.contains("issues: write"));
        assert!(!audit.content.contains("checks: write"));
        assert!(!audit.content.contains("rustsec/audit-check"));
        assert!(audit.content.contains("cargo audit --deny warnings"));
    }

    #[test]
    fn standalone_security_scanning_issue_publication_can_be_disabled() {
        let files = PreviewBuilder::new(env!("CARGO_MANIFEST_DIR"))
            .template("templates/security-scanning")
            .define("ci_platform", "github")
            .define("post_issue", "false")
            .preview()
            .unwrap();
        let audit = files
            .iter()
            .find(|f| f.path == ".github/workflows/audit.yml")
            .unwrap();

        assert!(!audit.content.contains("issues: write"));
        assert!(!audit.content.contains("checks: write"));
        assert!(!audit.content.contains("rustsec/audit-check"));
        assert!(audit.content.contains("cargo audit --deny warnings"));
    }

    #[test]
    fn dependency_policy_can_be_disabled() {
        let files = PreviewBuilder::new(env!("CARGO_MANIFEST_DIR"))
            .template("templates/full")
            .define("ci_platform", "github")
            .define("repo_owner", "test-owner")
            .define("dependency_policy", "false")
            .preview()
            .unwrap();

        assert!(
            !files
                .iter()
                .any(|f| f.path == ".github/workflows/dependency-policy.yml")
        );
        assert!(!files.iter().any(|f| f.path == "deny.toml"));
    }

    // -- Merged snapshot tests --
    // Each test renders a template and snapshots ALL rendered files.
    // SHAs, MSRV, and version comments are masked with [..] before comparison.
    //
    // To update after template changes:
    //   SNAPSHOTS=overwrite cargo test -p ci-battery-pack -- snapshot_

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

    #[test]
    fn snapshot_standalone_dependency_policy() {
        assert_snapshot(
            snapshot("dependency-policy", &[]),
            file!["snapshots/standalone_dependency_policy.txt"],
        );
    }

    #[test]
    fn snapshot_standalone_security_scanning() {
        assert_snapshot(
            snapshot("security-scanning", &[]),
            file!["snapshots/standalone_security_scanning.txt"],
        );
    }
}
