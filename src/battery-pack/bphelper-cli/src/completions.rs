use bphelper_manifest::BatteryPackSpec;
use clap_complete::CompletionCandidate;
use std::collections::BTreeSet;
use std::ffi::OsStr;
use std::fs;
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
    let Ok(dir) = std::env::current_dir() else {
        return names;
    };

    if let Some(installed) = crate::manifest::find_user_manifest(&dir)
        .ok()
        .and_then(|manifest_path| fs::read_to_string(manifest_path).ok())
        .and_then(|content| crate::manifest::find_installed_bp_names(&content).ok())
    {
        for name in installed {
            names.push(CompletionCandidate::new(name));
        }
    }

    names
}

pub fn registry_and_local_packs(_current: &OsStr) -> Vec<CompletionCandidate> {
    let mut names = BTreeSet::new();

    if let Ok(dir) = std::env::current_dir() {
        let installed = crate::manifest::find_user_manifest(&dir)
            .ok()
            .and_then(|path| fs::read_to_string(path).ok())
            .and_then(|content| crate::manifest::find_installed_bp_names(&content).ok())
            .unwrap_or_default();
        names.extend(installed);
    }

    let cache_file = get_cache_dir().join("registry_packs.json");
    if let Some(packs) = fs::read_to_string(cache_file)
        .ok()
        .and_then(|content| serde_json::from_str::<Vec<String>>(&content).ok())
    {
        for pack in packs {
            if let Some(short) = pack.strip_suffix("-battery-pack") {
                names.insert(short.to_string());
            }
            names.insert(pack);
        }
    } else if let Ok(exe) = std::env::current_exe() {
        // Spawn cache update gracefully
        let _ = std::process::Command::new(exe)
            .arg("bp")
            .arg("update-cache")
            .stdout(std::process::Stdio::null())
            .stderr(std::process::Stdio::null())
            .spawn();
    }

    names.into_iter().map(CompletionCandidate::new).collect()
}

fn get_cached_spec(pack_name: &str) -> Option<BatteryPackSpec> {
    let spec_file = get_cache_dir().join(format!("{}_spec.toml", pack_name));
    let content = fs::read_to_string(spec_file).ok()?;
    bphelper_manifest::parse_battery_pack(&content).ok()
}

fn collect_keys<F, I>(f: F) -> Vec<CompletionCandidate>
where
    F: FnOnce(&BatteryPackSpec) -> I,
    I: IntoIterator<Item = String>,
{
    find_context_battery_pack()
        .and_then(|pack| get_cached_spec(&pack))
        .map(|spec| {
            spec.features
                .into_keys()
                .map(CompletionCandidate::new)
                .collect()
        })
        .unwrap_or_default()
}

pub fn templates(_current: &OsStr) -> Vec<CompletionCandidate> {
    collect_keys(|spec| spec.templates.clone().into_keys())
}

pub fn pack_features(_current: &OsStr) -> Vec<CompletionCandidate> {
    collect_keys(|spec| spec.features.keys().cloned().collect::<Vec<_>>())
}

pub fn pack_crates(_current: &OsStr) -> Vec<CompletionCandidate> {
    collect_keys(|spec| spec.crates.keys().cloned().collect::<Vec<_>>())
}
