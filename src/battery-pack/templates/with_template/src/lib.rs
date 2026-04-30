#![doc = include_str!(concat!(env!("OUT_DIR"), "/docs.md"))]

#[cfg(test)]
mod tests {
    use ::battery_pack::testing::PreviewBuilder;
    use snapbox::{assert_data_eq, file};

    #[test]
    fn validate() {
        ::battery_pack::testing::validate(env!("CARGO_MANIFEST_DIR")).unwrap();
    }

    /// If a snapshot test fails, the diff is printed. To accept changes:
    ///   SNAPSHOTS=overwrite cargo test
    #[test]
    fn snapshot_default_template() {
        let files = PreviewBuilder::new(env!("CARGO_MANIFEST_DIR"))
            .template("templates/default")
            .project_name("test-project")
            .preview()
            .unwrap();
        let mut out = String::new();
        for file in &files {
            out.push_str(&format!(
                "── {} ──\n{}\n\n",
                file.path,
                file.content
            ));
        }
        assert_data_eq!(out, file!["snapshots/default_template.txt"]);
    }
}
