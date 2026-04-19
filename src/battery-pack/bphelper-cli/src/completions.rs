use clap_complete::CompletionCandidate;
use std::ffi::OsStr;
use std::path::PathBuf;

pub(crate) fn get_cache_dir() -> PathBuf {
    if let Ok(home) = std::env::var("CARGO_HOME") {
        PathBuf::from(home).join("bp-cache")
    } else if let Ok(home) = std::env::var("HOME") {
        PathBuf::from(home).join(".cargo").join("bp-cache")
    } else {
        std::env::temp_dir().join("cargo-bp")
    }
}

fn find_context_battery_pack() -> Option<String> {
    let args: Vec<String> = std::env::args().collect();
    let cmds = ["new", "add", "show", "rm", "info", "edit"];
    let mut found_cmd = false;
    for arg in args.into_iter().skip(1) {
        if arg.starts_with('-') {
            continue;
        }
        if !found_cmd && cmds.contains(&arg.as_str()) {
            found_cmd = true;
            continue;
        }
        if found_cmd {
            return Some(arg);
        }
    }
    None
}

pub fn installed_packs(_current: &OsStr) -> Vec<CompletionCandidate> {
    let mut names = vec![];
    if let Ok(dir) = std::env::current_dir() {
        if let Ok(manifest_path) = crate::manifest::find_user_manifest(&dir) {
            if let Ok(content) = std::fs::read_to_string(&manifest_path) {
                if let Ok(installed) = crate::manifest::find_installed_bp_names(&content) {
                    for name in installed {
                        names.push(CompletionCandidate::new(name));
                    }
                }
            }
        }
    }
    names
}

pub fn registry_and_local_packs(current: &OsStr) -> Vec<CompletionCandidate> {
    let mut names = installed_packs(current);
    
    let cache_file = get_cache_dir().join("registry_packs.json");
    if let Ok(content) = std::fs::read_to_string(&cache_file) {
        if let Ok(packs) = serde_json::from_str::<Vec<String>>(&content) {
            for pack in packs {
                names.push(CompletionCandidate::new(pack));
            }
        }
    } else if let Ok(exe) = std::env::current_exe() {
        // Spawn cache update gracefully
        let _ = std::process::Command::new(exe)
            .arg("update-cache")
            .stdout(std::process::Stdio::null())
            .stderr(std::process::Stdio::null())
            .spawn();
    }
    
    names
}

fn get_cached_spec(pack_name: &str) -> Option<bphelper_manifest::BatteryPackSpec> {
    let spec_file = get_cache_dir().join(format!("{}_spec.toml", pack_name));
    if let Ok(content) = std::fs::read_to_string(&spec_file) {
        bphelper_manifest::parse_battery_pack(&content).ok()
    } else {
        None
    }
}

pub fn templates(_current: &OsStr) -> Vec<CompletionCandidate> {
    let mut names = vec![];
    if let Some(pack) = find_context_battery_pack() {
        if let Some(spec) = get_cached_spec(&pack) {
            for (name, _) in spec.templates {
                names.push(CompletionCandidate::new(name));
            }
        }
    }
    names
}

pub fn pack_features(_current: &OsStr) -> Vec<CompletionCandidate> {
    let mut names = vec![];
    if let Some(pack) = find_context_battery_pack() {
        if let Some(spec) = get_cached_spec(&pack) {
            for name in spec.features.keys() {
                names.push(CompletionCandidate::new(name));
            }
        }
    }
    names
}

pub fn pack_crates(_current: &OsStr) -> Vec<CompletionCandidate> {
    let mut names = vec![];
    if let Some(pack) = find_context_battery_pack() {
        if let Some(spec) = get_cached_spec(&pack) {
            for name in spec.crates.keys() {
                names.push(CompletionCandidate::new(name));
            }
        }
    }
    names
}
