use std::{
    fs,
    path::{Path, PathBuf},
};

use tempfile::{TempDir, tempdir};

use crate::{BatteryPackSpec, Error, parse_battery_pack_from_path};

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

        let workspace = format!("[workspace]\nresolver = \"2\"\nmembers = [\n{member_toml}\n]\n");

        fs::write(self.root.join("Cargo.toml"), workspace).unwrap();

        &self.root
    }
}

pub(crate) fn parse_test(manifest_str: &str) -> Result<BatteryPackSpec, Error> {
    let mut fx = WorkspaceFixture::new();
    let pack_dir = fx.add_pack("test-pack", manifest_str);
    fx.finalize();
    // parse_battery_pack_from_path returns raw spec data — name and feature-ref
    // validation are deferred to `.validate()` so tests can target either layer.
    parse_battery_pack_from_path(&pack_dir.join("Cargo.toml"))
}
