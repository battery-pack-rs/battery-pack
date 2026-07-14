use anyhow::{Context, Result, bail};
use serde_json::Value;
use std::collections::HashMap;
use std::io::{self, Read};
use std::path::{Path, PathBuf};
use std::process::Command;

/// Metadata extracted from a battery pack's Cargo.toml.
struct PackInfo {
    /// Crate name (e.g., "cli-battery-pack").
    name: String,
    /// Short name without "-battery-pack" suffix (e.g., "cli").
    short_name: String,
    /// Package description from Cargo.toml.
    description: String,
}

fn main() -> Result<()> {
    let args: Vec<String> = std::env::args().collect();

    // mdbook calls preprocessors with "supports <renderer>" to check compatibility.
    if args.len() >= 3 && args[1] == "supports" {
        return Ok(());
    }

    // mdbook sends [context, book] as a JSON array on stdin.
    // We must return just the book object on stdout.
    let mut input = String::new();
    io::stdin()
        .read_to_string(&mut input)
        .context("reading stdin")?;


    let parsed: Value = serde_json::from_str(&input).context("parsing mdbook JSON")?;
    let arr = parsed.as_array().context("expected [context, book] array")?;
    if arr.len() < 2 {
        bail!("expected [context, book] array with 2 elements");
    }

    let context = &arr[0];
    let mut book = arr[1].clone();

    // Determine workspace root from the mdbook context.
    let book_root = context
        .get("root")
        .and_then(|r| r.as_str())
        .map(PathBuf::from)
        .unwrap_or_else(|| PathBuf::from("."));

    // The workspace root is one level up from the book root (md/ is the src dir).
    let workspace_root = book_root.clone();

    // Discover all battery packs in the workspace.
    let packs = discover_battery_packs(&workspace_root);

    // Resolve out_dir paths for battery pack crates via `cargo check`.
    let out_dirs = resolve_out_dirs(&book, &packs, &workspace_root)?;

    // Walk the book items and expand directives.
    // mdbook uses "items" (not "sections") as the top-level key.
    if let Some(items) = book.get_mut("items") {
        expand_sections(items, &out_dirs, &packs)?;
    }

    // Write just the book object to stdout.
    serde_json::to_writer(io::stdout(), &book).context("writing output")?;

    Ok(())
}

/// Discover battery packs by scanning `battery-packs/` and
/// `opinionated-battery-packs/` directories.
fn discover_battery_packs(workspace_root: &Path) -> Vec<PackInfo> {
    let mut packs = Vec::new();

    let dirs_to_scan = ["battery-packs", "opinionated-battery-packs"];
    for dir in &dirs_to_scan {
        let search_dir = workspace_root.join(dir);
        if !search_dir.is_dir() {
            continue;
        }
        let entries = match std::fs::read_dir(&search_dir) {
            Ok(e) => e,
            Err(_) => continue,
        };
        for entry in entries.flatten() {
            let cargo_toml = entry.path().join("Cargo.toml");
            if !cargo_toml.exists() {
                continue;
            }
            if let Some(info) = parse_pack_info(&cargo_toml) {
                packs.push(info);
            }
        }
    }

    packs.sort_by(|a, b| a.short_name.cmp(&b.short_name));
    packs
}

/// Parse minimal metadata from a battery pack's Cargo.toml.
fn parse_pack_info(cargo_toml: &Path) -> Option<PackInfo> {
    let contents = std::fs::read_to_string(cargo_toml).ok()?;
    let doc: toml::Table = contents.parse().ok()?;

    let package = doc.get("package")?.as_table()?;
    let name = package.get("name")?.as_str()?.to_string();

    if !name.ends_with("-battery-pack") {
        return None;
    }

    let short_name = name.strip_suffix("-battery-pack").unwrap().to_string();
    let description = package
        .get("description")
        .and_then(|d| d.as_str())
        .unwrap_or("")
        .to_string();

    Some(PackInfo {
        name,
        short_name,
        description,
    })
}

/// Resolve OUT_DIR paths for all battery packs referenced by directives
/// plus all discovered packs.
fn resolve_out_dirs(
    book: &Value,
    packs: &[PackInfo],
    workspace_root: &Path,
) -> Result<HashMap<String, PathBuf>> {
    // Collect explicitly referenced packages from directives.
    let mut packages: Vec<String> = Vec::new();
    collect_package_names(book, &mut packages);

    // Also include all discovered packs.
    for pack in packs {
        packages.push(pack.name.clone());
    }

    packages.sort();
    packages.dedup();

    if packages.is_empty() {
        return Ok(HashMap::new());
    }

    // Run a single `cargo check` with all packages to populate build script outputs.
    let mut out_dirs = HashMap::new();
    for pkg in &packages {
        match get_out_dir(pkg, workspace_root) {
            Ok(dir) => {
                out_dirs.insert(pkg.clone(), dir);
            }
            Err(e) => {
                eprintln!("mdbook-battery-pack: warning: could not resolve out_dir for {pkg}: {e}");
            }
        }
    }

    Ok(out_dirs)
}

/// Recursively collect package names from `{{#battery-pack <name>}}` directives.
fn collect_package_names(value: &Value, names: &mut Vec<String>) {
    match value {
        Value::String(s) => {
            for name in extract_directive_names(s) {
                names.push(name);
            }
        }
        Value::Array(arr) => {
            for item in arr {
                collect_package_names(item, names);
            }
        }
        Value::Object(map) => {
            for v in map.values() {
                collect_package_names(v, names);
            }
        }
        _ => {}
    }
}

/// Run `cargo check -p <pkg> --message-format=json` and extract the OUT_DIR.
fn get_out_dir(pkg: &str, workspace_root: &Path) -> Result<PathBuf> {
    let output = Command::new("cargo")
        .args(["check", "-p", pkg, "--message-format=json"])
        .current_dir(workspace_root)
        .output()
        .with_context(|| format!("running cargo check for {pkg}"))?;

    if !output.status.success() {
        let stderr = String::from_utf8_lossy(&output.stderr);
        bail!("cargo check failed for {pkg}:\n{stderr}");
    }

    let stdout = String::from_utf8_lossy(&output.stdout);
    for line in stdout.lines() {
        let msg: Value = match serde_json::from_str(line) {
            Ok(v) => v,
            Err(_) => continue,
        };

        if msg.get("reason").and_then(|r| r.as_str()) != Some("build-script-executed") {
            continue;
        }

        let msg_pkg_id = msg
            .get("package_id")
            .and_then(|p| p.as_str())
            .unwrap_or("");
        if !msg_pkg_id.contains(pkg) {
            continue;
        }

        if let Some(out_dir) = msg.get("out_dir").and_then(|d| d.as_str()) {
            return Ok(PathBuf::from(out_dir));
        }
    }

    bail!("no build-script-executed message found for {pkg}")
}

/// Expand directives in all book items.
fn expand_sections(
    items: &mut Value,
    out_dirs: &HashMap<String, PathBuf>,
    packs: &[PackInfo],
) -> Result<()> {
    match items {
        Value::Array(arr) => {
            // First: inject sub-chapters where {{#battery-pack-table}} appears.
            // This must happen before content expansion removes the directive.
            inject_pack_chapters(arr, out_dirs, packs)?;

            // Then expand content in all chapters (including newly injected ones).
            for item in arr.iter_mut() {
                expand_sections(item, out_dirs, packs)?;
            }
        }
        Value::Object(map) => {
            if let Some(Value::String(s)) = map.get_mut("content") {
                *s = expand_content(s, out_dirs, packs)?;
            }
            for v in map.values_mut() {
                expand_sections(v, out_dirs, packs)?;
            }
        }
        _ => {}
    }
    Ok(())
}

/// Find the chapter containing `{{#battery-pack-table}}` and inject
/// sub-chapters for each discovered battery pack.
fn inject_pack_chapters(
    sections: &mut [Value],
    out_dirs: &HashMap<String, PathBuf>,
    packs: &[PackInfo],
) -> Result<()> {
    for item in sections.iter_mut() {
        let injected = try_inject_in_chapter(item, out_dirs, packs)?;
        if injected {
            return Ok(());
        }
    }
    Ok(())
}

/// Try to inject sub-chapters into a chapter item. Returns true if injection happened.
fn try_inject_in_chapter(
    item: &mut Value,
    out_dirs: &HashMap<String, PathBuf>,
    packs: &[PackInfo],
) -> Result<bool> {
    // mdbook sections are either {"Chapter": {...}} or {"Separator": ...} or {"PartTitle": ...}
    let chapter = match item.get_mut("Chapter") {
        Some(ch) => ch,
        None => return Ok(false),
    };

    // Check if this chapter's content had the table directive.
    let has_table = chapter
        .get("content")
        .and_then(|c| c.as_str())
        .map(|c| c.contains("{{#battery-pack-table}}"))
        .unwrap_or(false);

    if has_table {
        // Extract immutable data before taking mutable borrows.
        let parent_path = chapter
            .get("path")
            .and_then(|p| p.as_str())
            .unwrap_or("battery-packs/README.md")
            .to_string();
        let parent_dir = Path::new(&parent_path)
            .parent()
            .unwrap_or(Path::new(""))
            .to_string_lossy()
            .to_string();
        let parent_number_arr = chapter
            .get("number")
            .and_then(|n| n.as_array())
            .cloned()
            .unwrap_or_default();

        // Build the sub-chapters to inject.
        let mut new_chapters = Vec::new();
        for (i, pack) in packs.iter().enumerate() {
            let content = get_pack_docs(out_dirs, &pack.name);
            let child_path = format!("{}/{}.md", parent_dir, pack.short_name);

            let mut child_number = parent_number_arr.clone();
            child_number.push(Value::Number(serde_json::Number::from(i + 1)));

            new_chapters.push(serde_json::json!({
                "Chapter": {
                    "name": pack.short_name,
                    "content": content,
                    "number": child_number,
                    "sub_items": [],
                    "path": child_path,
                    "source_path": child_path,
                    "parent_names": []
                }
            }));
        }

        // Get or create sub_items array, then append.
        if chapter.get("sub_items").is_none() {
            chapter
                .as_object_mut()
                .unwrap()
                .insert("sub_items".to_string(), Value::Array(Vec::new()));
        }
        let sub_items = chapter
            .get_mut("sub_items")
            .unwrap()
            .as_array_mut()
            .unwrap();
        sub_items.extend(new_chapters);

        return Ok(true);
    }

    // Recurse into sub_items.
    if let Some(sub_items) = chapter.get_mut("sub_items").and_then(|s| s.as_array_mut()) {
        for sub in sub_items.iter_mut() {
            if try_inject_in_chapter(sub, out_dirs, packs)? {
                return Ok(true);
            }
        }
    }

    Ok(false)
}

/// Get the generated docs for a battery pack, or a placeholder on failure.
fn get_pack_docs(out_dirs: &HashMap<String, PathBuf>, pkg_name: &str) -> String {
    if let Some(out_dir) = out_dirs.get(pkg_name) {
        let docs_path = out_dir.join("docs.md");
        match std::fs::read_to_string(&docs_path) {
            Ok(content) => content,
            Err(e) => format!(
                "> **Error**: could not read generated docs for `{pkg_name}`: {e}\n"
            ),
        }
    } else {
        format!(
            "> **Error**: could not resolve `{pkg_name}` — is the package in the workspace?\n"
        )
    }
}

/// Replace directives in a chapter's content.
fn expand_content(
    content: &str,
    out_dirs: &HashMap<String, PathBuf>,
    packs: &[PackInfo],
) -> Result<String> {
    let mut result = String::with_capacity(content.len());
    let mut remaining = content;

    while let Some(start) = remaining.find("{{#battery-pack") {
        result.push_str(&remaining[..start]);

        let after_open = &remaining[start..];
        let end = after_open
            .find("}}")
            .context("unclosed {{#battery-pack...}} directive")?;

        let directive = &after_open[2..end]; // strip leading "{{"
        remaining = &after_open[end + 2..]; // skip "}}"

        if directive.starts_with("#battery-pack-table") {
            // Generate the table from discovered packs.
            result.push_str(&generate_table(packs));
        } else if let Some(name) = directive.strip_prefix("#battery-pack ") {
            let name = name.trim();
            result.push_str(&get_pack_docs(out_dirs, name));
        }
    }

    result.push_str(remaining);
    Ok(result)
}

/// Generate a markdown table of all discovered battery packs.
fn generate_table(packs: &[PackInfo]) -> String {
    let mut table = String::new();
    table.push_str("| Pack | Description |\n");
    table.push_str("|------|-------------|\n");

    for pack in packs {
        table.push_str(&format!(
            "| [{}](./{}.md) | {} |\n",
            pack.short_name, pack.short_name, pack.description
        ));
    }

    table
}

/// Extract package names from `{{#battery-pack <name>}}` directives in a string.
fn extract_directive_names(s: &str) -> Vec<String> {
    let mut names = Vec::new();
    let mut remaining = s.as_bytes();
    let prefix = b"{{#battery-pack ";

    while let Some(pos) = remaining.windows(prefix.len()).position(|w| w == prefix) {
        let after = &remaining[pos + prefix.len()..];
        if let Some(end) = after.windows(2).position(|w| w == b"}}") {
            let name = std::str::from_utf8(&after[..end]).unwrap_or("").trim();
            if !name.is_empty() && !name.starts_with("table") {
                names.push(name.to_string());
            }
            remaining = &after[end + 2..];
        } else {
            break;
        }
    }

    names
}
