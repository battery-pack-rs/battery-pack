use std::{
    fs,
    path::{Path, PathBuf},
};

use tempfile::{TempDir, tempdir};

use crate::{BatteryPackSpec, Error, discover_battery_packs};

pub(crate) struct WorkspaceFixture {
    _temp: TempDir,
    root: PathBuf,
    members: Vec<String>,
}

impl WorkspaceFixture {
    pub fn new() -> Self {
        let temp = tempdir().expect("tempdir");
        let root = temp.path().to_path_buf();

        Self {
            _temp: temp,
            root,
            members: Vec::new(),
        }
    }

    pub fn add_pack(&mut self, dir: &str, manifest: &str) -> PathBuf {
        let member_dir = self.root.join(dir);
        fs::create_dir_all(member_dir.join("src")).unwrap();
        fs::write(member_dir.join("Cargo.toml"), manifest).unwrap();
        fs::write(member_dir.join("src").join("lib.rs"), " ").unwrap();
        self.members.push(dir.to_string());

        member_dir
    }

    pub fn finalize(&mut self) -> &Path {
        let member_toml = self
            .members
            .iter()
            .map(|member| format!("\"{member}\""))
            .collect::<Vec<_>>()
            .join(",\n");

        let workspace = format!("[workspace]\nresolver \"2\"\nmembers = [\n{member_toml}\n]\n");

        fs::write(self.root.join("Cargo.toml"), workspace).unwrap();

        &self.root
    }
}

pub(crate) fn parse_test(manifest_str: &str) -> Result<BatteryPackSpec, Error> {
    let mut fx = WorkspaceFixture::new();
    fx.add_pack("test-pack", manifest_str);

    let root = fx.finalize();
    let mut packs = discover_battery_packs(root)?;
    packs.pop().ok_or(Error::MissingField("test pack"))
}
