//! Format-aware file merging for `cargo bp add -t`.
//!
//! When a template is applied to an existing project, files that already exist
//! need special handling. This module dispatches to the right merge strategy
//! based on file type:
//!
//! - `Cargo.toml`: TOML-aware merge (deps get version upgrade + feature union,
//!   other sections inserted if absent)
//! - `*.yml` / `*.yaml`: YAML-aware merge (top-level map keys merged additively)
//! - Everything else: write if new, prompt if exists

use anyhow::{Context, Result};
use std::path::{Path, PathBuf};

use crate::template_engine::RenderedFile;

// ============================================================================
// Merge strategy dispatch
// ============================================================================

/// How to handle a file that already exists in the target project.
enum MergeStrategy {
    /// TOML-aware merge (Cargo.toml files).
    Toml,
    /// YAML-aware merge (workflow files, etc.).
    Yaml,
    /// Plain file: skip or overwrite.
    Plain,
}

/// Determine the merge strategy for a file based on its path.
fn strategy_for(path: &str) -> MergeStrategy {
    let filename = path.rsplit('/').next().unwrap_or(path);
    if filename.ends_with(".toml") {
        MergeStrategy::Toml
    } else if filename.ends_with(".yml") || filename.ends_with(".yaml") {
        MergeStrategy::Yaml
    } else {
        MergeStrategy::Plain
    }
}

// ============================================================================
// Apply rendered files
// ============================================================================

/// Result of applying a single rendered file.
#[derive(Debug)]
pub(crate) enum FileResult {
    /// File was written (new file, no conflict).
    Created(String),
    /// File was merged (structured merge for TOML/YAML).
    Merged(String),
    /// File was skipped (user chose to skip, or non-interactive default).
    Skipped(String),
    /// File was overwritten (user chose to overwrite).
    Overwritten(String),
    /// File was unchanged (merge produced identical content).
    Unchanged(String),
}

/// Options for applying rendered template files to an existing project.
pub(crate) struct ApplyOpts {
    /// Root directory of the target project.
    pub(crate) project_dir: PathBuf,
    /// Whether to force overwrite plain file conflicts.
    pub(crate) overwrite: bool,
    /// Whether interactive prompts are allowed.
    pub(crate) interactive: bool,
}

/// Batch decision set by "accept all" or "skip all" during interactive prompts.
/// This is shared across both TOML/YAML and other-file prompts: if the user
/// picks "overwrite all" on a plain file, subsequent TOML/YAML merges are also
/// auto-accepted (and vice versa). This is intentional: the user has signaled
/// they want to stop being prompted.
#[derive(Clone, Copy)]
enum BatchDecision {
    /// No batch decision yet; prompt per file.
    None,
    /// Accept/overwrite all remaining conflicts.
    AcceptAll,
    /// Skip all remaining conflicts.
    SkipAll,
}

/// Apply rendered template files to an existing project directory.
///
/// Returns a list of results describing what happened to each file.
pub(crate) fn apply_rendered_files(
    files: &[RenderedFile],
    opts: &ApplyOpts,
) -> Result<Vec<FileResult>> {
    let mut results = Vec::new();
    let mut batch = BatchDecision::None;

    for file in files {
        let dest = opts.project_dir.join(&file.path);
        let result = if !dest.exists() {
            // New file: always write it.
            write_new_file(&dest, &file.content)?;
            FileResult::Created(file.path.clone())
        } else {
            // Existing file: dispatch by strategy.
            // Handle binary/non-UTF-8 files as plain file conflicts.
            let existing = match std::fs::read_to_string(&dest) {
                Ok(content) => content,
                Err(e) if e.kind() == std::io::ErrorKind::InvalidData => {
                    let ctx = ConflictContext {
                        dest: &dest,
                        rel_path: &file.path,
                        existing: "",
                        new_content: &file.content,
                        opts,
                    };
                    results.push(resolve_plain_conflict(&ctx, &mut batch)?);
                    continue;
                }
                Err(e) => {
                    return Err(e).with_context(|| format!("failed to read {}", dest.display()));
                }
            };

            match strategy_for(&file.path) {
                MergeStrategy::Toml => {
                    let merged = merge_toml(&existing, &file.content)?;
                    let ctx = ConflictContext {
                        dest: &dest,
                        rel_path: &file.path,
                        existing: &existing,
                        new_content: &merged,
                        opts,
                    };
                    resolve_structured_merge(&ctx, &mut batch)?
                }
                MergeStrategy::Yaml => {
                    let merged = merge_yaml(&existing, &file.content)?;
                    let ctx = ConflictContext {
                        dest: &dest,
                        rel_path: &file.path,
                        existing: &existing,
                        new_content: &merged,
                        opts,
                    };
                    resolve_structured_merge(&ctx, &mut batch)?
                }
                MergeStrategy::Plain => {
                    let ctx = ConflictContext {
                        dest: &dest,
                        rel_path: &file.path,
                        existing: &existing,
                        new_content: &file.content,
                        opts,
                    };
                    resolve_plain_conflict(&ctx, &mut batch)?
                }
            }
        };
        results.push(result);
    }

    Ok(results)
}

/// Write a new file, creating parent directories as needed.
fn write_new_file(dest: &Path, content: &str) -> Result<()> {
    if let Some(parent) = dest.parent() {
        std::fs::create_dir_all(parent)
            .with_context(|| format!("failed to create {}", parent.display()))?;
    }
    std::fs::write(dest, content).with_context(|| format!("failed to write {}", dest.display()))?;
    Ok(())
}

/// Shared context for conflict resolution (plain and structured).
struct ConflictContext<'a> {
    dest: &'a Path,
    rel_path: &'a str,
    existing: &'a str,
    new_content: &'a str,
    opts: &'a ApplyOpts,
}

/// Handle a plain file conflict: prompt, overwrite, or skip.
fn resolve_plain_conflict(
    ctx: &ConflictContext<'_>,
    batch: &mut BatchDecision,
) -> Result<FileResult> {
    if ctx.opts.overwrite || matches!(batch, BatchDecision::AcceptAll) {
        std::fs::write(ctx.dest, ctx.new_content)
            .with_context(|| format!("failed to write {}", ctx.dest.display()))?;
        return Ok(FileResult::Overwritten(ctx.rel_path.to_string()));
    }

    if !ctx.opts.interactive || matches!(batch, BatchDecision::SkipAll) {
        return Ok(FileResult::Skipped(ctx.rel_path.to_string()));
    }

    // Interactive prompt with single-key shortcuts.
    loop {
        eprintln!(
            "{} already exists: [s]kip [o]verwrite [d]iff [S]kip all [O]verwrite all",
            ctx.rel_path
        );
        match read_single_key()? {
            's' => return Ok(FileResult::Skipped(ctx.rel_path.to_string())),
            'o' => {
                std::fs::write(ctx.dest, ctx.new_content)
                    .with_context(|| format!("failed to write {}", ctx.dest.display()))?;
                return Ok(FileResult::Overwritten(ctx.rel_path.to_string()));
            }
            'd' => {
                let diff = unified_diff(ctx.existing, ctx.new_content, ctx.rel_path);
                eprintln!("{diff}");
            }
            'S' => {
                *batch = BatchDecision::SkipAll;
                return Ok(FileResult::Skipped(ctx.rel_path.to_string()));
            }
            'O' => {
                *batch = BatchDecision::AcceptAll;
                std::fs::write(ctx.dest, ctx.new_content)
                    .with_context(|| format!("failed to write {}", ctx.dest.display()))?;
                return Ok(FileResult::Overwritten(ctx.rel_path.to_string()));
            }
            _ => {} // Unrecognized key, re-prompt.
        }
    }
}

/// Handle a structured file merge (TOML/YAML): show diff, prompt accept/skip/edit.
///
/// `ctx.new_content` is the already-merged result. In non-interactive mode,
/// the merge is applied automatically (structured merges are additive and
/// non-destructive).
fn resolve_structured_merge(
    ctx: &ConflictContext<'_>,
    batch: &mut BatchDecision,
) -> Result<FileResult> {
    // No changes needed.
    if ctx.new_content == ctx.existing {
        return Ok(FileResult::Unchanged(ctx.rel_path.to_string()));
    }

    let diff = unified_diff(ctx.existing, ctx.new_content, ctx.rel_path);

    // Non-interactive or batch accept: apply automatically.
    if !ctx.opts.interactive || matches!(batch, BatchDecision::AcceptAll) {
        if !diff.is_empty() {
            eprintln!("merging {}:\n{}", ctx.rel_path, diff);
        }
        std::fs::write(ctx.dest, ctx.new_content)
            .with_context(|| format!("failed to write {}", ctx.dest.display()))?;
        return Ok(FileResult::Merged(ctx.rel_path.to_string()));
    }

    if matches!(batch, BatchDecision::SkipAll) {
        return Ok(FileResult::Skipped(ctx.rel_path.to_string()));
    }

    // Interactive: show diff and prompt with batch options.
    if !diff.is_empty() {
        eprintln!("merging {}:\n{}", ctx.rel_path, diff);
    }

    let mut content_to_write = ctx.new_content.to_string();
    loop {
        eprintln!(
            "Merge {}? [a]ccept [s]kip [e]dit [A]ccept all [S]kip all",
            ctx.rel_path
        );
        match read_single_key()? {
            'a' => {
                std::fs::write(ctx.dest, &content_to_write)
                    .with_context(|| format!("failed to write {}", ctx.dest.display()))?;
                return Ok(FileResult::Merged(ctx.rel_path.to_string()));
            }
            's' => return Ok(FileResult::Skipped(ctx.rel_path.to_string())),
            'e' => {
                content_to_write = open_in_editor(&content_to_write, ctx.rel_path)?;
                let updated_diff = unified_diff(ctx.existing, &content_to_write, ctx.rel_path);
                if updated_diff.is_empty() {
                    eprintln!("(no changes after editing)");
                } else {
                    eprintln!("{updated_diff}");
                }
            }
            'A' => {
                *batch = BatchDecision::AcceptAll;
                std::fs::write(ctx.dest, &content_to_write)
                    .with_context(|| format!("failed to write {}", ctx.dest.display()))?;
                return Ok(FileResult::Merged(ctx.rel_path.to_string()));
            }
            'S' => {
                *batch = BatchDecision::SkipAll;
                return Ok(FileResult::Skipped(ctx.rel_path.to_string()));
            }
            _ => {} // Unrecognized key, re-prompt.
        }
    }
}

/// Read a single keypress from the terminal (no Enter required).
fn read_single_key() -> Result<char> {
    let term = console::Term::stderr();
    let key = term.read_char().context("failed to read key")?;
    Ok(key)
}

/// Open content in $EDITOR and return the edited result.
fn open_in_editor(content: &str, filename: &str) -> Result<String> {
    // Determine the file extension for editor syntax highlighting.
    let extension = filename.rsplit('.').next().unwrap_or("txt");
    let mut tmp = tempfile::Builder::new()
        .suffix(&format!(".{extension}"))
        .tempfile()
        .context("failed to create temp file for editor")?;
    std::io::Write::write_all(&mut tmp, content.as_bytes()).context("failed to write temp file")?;
    let tmp_path = tmp.into_temp_path();

    let editor = std::env::var("VISUAL")
        .or_else(|_| std::env::var("EDITOR"))
        .unwrap_or_else(|_| "vi".to_string());

    // Handle editors with arguments (e.g., EDITOR="code --wait").
    let mut parts = editor.split_whitespace();
    let program = parts.next().unwrap_or("vi");
    let mut cmd = std::process::Command::new(program);
    cmd.args(parts);
    cmd.arg(&*tmp_path);

    let status = cmd
        .status()
        .with_context(|| format!("failed to launch editor '{editor}'"))?;

    if !status.success() {
        anyhow::bail!("editor exited with non-zero status");
    }

    std::fs::read_to_string(&*tmp_path).context("failed to read edited file")
}

// ============================================================================
// TOML merge
// ============================================================================

/// Merge a template Cargo.toml into an existing one.
///
/// - Dependencies: uses `sync_dep_in_table()` (upgrades version if behind,
///   unions features, never removes).
/// - Other sections/keys: inserted if absent, left alone if present.
/// - Preserves existing formatting via `toml_edit`.
pub(crate) fn merge_toml(existing: &str, template: &str) -> Result<String> {
    let mut doc: toml_edit::DocumentMut = existing
        .parse()
        .context("failed to parse existing Cargo.toml")?;
    let template_doc: toml_edit::DocumentMut = template
        .parse()
        .context("failed to parse template Cargo.toml")?;

    // Dependency sections get special merge treatment.
    let dep_sections = ["dependencies", "dev-dependencies", "build-dependencies"];

    for section in &dep_sections {
        if let Some(template_table) = template_doc.get(section).and_then(|t| t.as_table()) {
            // Ensure the section exists in the target.
            doc[section].or_insert(toml_edit::Item::Table(toml_edit::Table::new()));
            let target_table = doc[section].as_table_mut().expect("just inserted");

            for (dep_name, dep_value) in template_table.iter() {
                // Parse the template dep into a CrateSpec for sync_dep_in_table.
                let spec = parse_dep_as_spec(dep_value, section);
                crate::manifest::sync_dep_in_table(target_table, dep_name, &spec);
            }
        }
    }

    // Non-dependency sections: merge recursively (insert missing keys,
    // recurse into sub-tables, leave existing values alone).
    for (key, value) in template_doc.iter() {
        if dep_sections.contains(&key) {
            continue;
        }
        if doc.get(key).is_none() {
            doc.insert(key, value.clone());
        } else if let (Some(target_table), Some(source_table)) =
            (doc[key].as_table_mut(), value.as_table())
        {
            merge_toml_tables(target_table, source_table);
        }
    }

    Ok(doc.to_string())
}

/// Recursively merge TOML tables: insert missing keys, recurse into sub-tables.
fn merge_toml_tables(target: &mut toml_edit::Table, source: &toml_edit::Table) {
    for (key, value) in source.iter() {
        if let Some(existing) = target.get(key) {
            // Both have this key: recurse if both are tables.
            if existing.is_table() && value.is_table() {
                let target_sub = target[key].as_table_mut().expect("checked is_table");
                let source_sub = value.as_table().expect("checked is_table");
                merge_toml_tables(target_sub, source_sub);
            }
            // Otherwise, existing value wins.
        } else {
            target.insert(key, value.clone());
        }
    }
}

/// Parse a toml_edit dependency value into a CrateSpec for sync_dep_in_table.
fn parse_dep_as_spec(value: &toml_edit::Item, section: &str) -> bphelper_manifest::CrateSpec {
    let dep_kind = match section {
        "dev-dependencies" => bphelper_manifest::DepKind::Dev,
        "build-dependencies" => bphelper_manifest::DepKind::Build,
        _ => bphelper_manifest::DepKind::Normal,
    };

    // Simple string version: `clap = "4"`
    if let Some(version) = value.as_str() {
        return bphelper_manifest::CrateSpec {
            version: version.to_string(),
            features: std::collections::BTreeSet::new(),
            dep_kind,
            optional: false,
        };
    }

    // Extract fields from either inline table or regular table.
    // Both have the same keys, but regular tables wrap values in Item.
    let get_str = |key| -> Option<&str> {
        value
            .as_inline_table()
            .and_then(|t| t.get(key)?.as_str())
            .or_else(|| value.as_table()?.get(key)?.as_str())
    };
    let get_bool = |key| -> Option<bool> {
        value
            .as_inline_table()
            .and_then(|t| t.get(key)?.as_bool())
            .or_else(|| value.as_table()?.get(key)?.as_value()?.as_bool())
    };
    let get_array = |key| -> Option<&toml_edit::Array> {
        value
            .as_inline_table()
            .and_then(|t| t.get(key)?.as_array())
            .or_else(|| value.as_table()?.get(key)?.as_value()?.as_array())
    };

    let version = get_str("version").unwrap_or("").to_string();
    let features = get_array("features")
        .map(|arr| {
            arr.iter()
                .filter_map(|v| v.as_str().map(String::from))
                .collect()
        })
        .unwrap_or_default();
    let optional = get_bool("optional").unwrap_or(false);

    bphelper_manifest::CrateSpec {
        version,
        features,
        dep_kind,
        optional,
    }
}

// ============================================================================
// YAML merge
// ============================================================================

/// Merge a template YAML file into an existing one.
///
/// Top-level mapping keys are merged additively: new keys from the template
/// are inserted, existing keys in the user's file are left alone.
/// For known GitHub Actions keys (`jobs`, `on`, `permissions`), child maps
/// are also merged additively.
///
/// Note: YAML round-tripping through the parser/emitter may reformat the file
/// (normalize indentation, remove comments, reorder keys). This is a known
/// limitation. When the merge changes content, a diff is shown so the user
/// can review the result.
pub(crate) fn merge_yaml(existing: &str, template: &str) -> Result<String> {
    use yaml_rust2::{Yaml, YamlEmitter, YamlLoader};

    let existing_docs =
        YamlLoader::load_from_str(existing).context("failed to parse existing YAML")?;
    let template_docs =
        YamlLoader::load_from_str(template).context("failed to parse template YAML")?;

    // Work with the first document in each file.
    let existing_doc = existing_docs.first().cloned().unwrap_or(Yaml::Null);
    let template_doc = template_docs.first().cloned().unwrap_or(Yaml::Null);

    let merged = merge_yaml_values(&existing_doc, &template_doc);

    let mut out = String::new();
    let mut emitter = YamlEmitter::new(&mut out);
    emitter
        .dump(&merged)
        .context("failed to emit merged YAML")?;

    // YamlEmitter prepends "---\n"; strip it to match typical workflow files.
    let out = out.strip_prefix("---\n").unwrap_or(&out).to_string();

    Ok(out)
}

/// Recursively merge two YAML values. Existing keys take precedence.
fn merge_yaml_values(existing: &yaml_rust2::Yaml, template: &yaml_rust2::Yaml) -> yaml_rust2::Yaml {
    use yaml_rust2::Yaml;

    match (existing, template) {
        (Yaml::Hash(existing_map), Yaml::Hash(template_map)) => {
            let mut merged = existing_map.clone();
            for (key, template_value) in template_map {
                if let Some(existing_value) = existing_map.get(key) {
                    // Both have this key: recurse for known deep-merge keys.
                    let key_str = key.as_str().unwrap_or("");
                    // Deep-merge known GitHub Actions keys so new jobs,
                    // triggers, and permissions are added without replacing
                    // existing ones. Other keys are treated as atomic.
                    if matches!(key_str, "jobs" | "on" | "permissions") {
                        merged.insert(
                            key.clone(),
                            merge_yaml_values(existing_value, template_value),
                        );
                    }
                    // For other keys, existing value wins (no change).
                } else {
                    // New key from template: insert it.
                    merged.insert(key.clone(), template_value.clone());
                }
            }
            Yaml::Hash(merged)
        }
        // Existing is empty/null: use the template value.
        (Yaml::Null | Yaml::BadValue, _) => template.clone(),
        // Non-mapping types: existing value wins.
        _ => existing.clone(),
    }
}

// ============================================================================
// Diff display
// ============================================================================

/// Produce a colored unified diff between two strings. Returns an empty
/// string when the inputs are identical (no hunks to display).
pub(crate) fn unified_diff(old: &str, new: &str, path: &str) -> String {
    use console::style;
    use similar::TextDiff;

    let diff = TextDiff::from_lines(old, new);
    let mut output = String::new();

    for hunk in diff
        .unified_diff()
        .header(&format!("a/{path}"), &format!("b/{path}"))
        .iter_hunks()
    {
        // Hunk header (e.g., @@ -1,3 +1,5 @@)
        output.push_str(&style(format!("{}", hunk.header())).cyan().to_string());
        output.push('\n');

        for change in hunk.iter_changes() {
            let sign = match change.tag() {
                similar::ChangeTag::Insert => "+",
                similar::ChangeTag::Delete => "-",
                similar::ChangeTag::Equal => " ",
            };
            let line = format!("{sign}{}", change.value());
            let styled = match change.tag() {
                similar::ChangeTag::Insert => style(line).green().to_string(),
                similar::ChangeTag::Delete => style(line).red().to_string(),
                similar::ChangeTag::Equal => line,
            };
            output.push_str(&styled);
            // Ensure trailing newline for lines that don't have one.
            if !change.value().ends_with('\n') {
                output.push('\n');
            }
        }
    }

    output
}

// ============================================================================
// Summary printing
// ============================================================================

/// Print a summary of what happened during the merge.
pub(crate) fn print_summary(results: &[FileResult]) {
    use console::style;

    let mut created = 0;
    let mut merged = 0;
    let mut skipped = 0;
    let mut overwritten = 0;

    for result in results {
        match result {
            FileResult::Created(path) => {
                eprintln!("  {} {}", style("create").green(), path);
                created += 1;
            }
            FileResult::Merged(path) => {
                eprintln!("  {} {}", style("merge").cyan(), path);
                merged += 1;
            }
            FileResult::Skipped(path) => {
                eprintln!("  {} {}", style("skip").yellow(), path);
                skipped += 1;
            }
            FileResult::Overwritten(path) => {
                eprintln!("  {} {}", style("overwrite").red(), path);
                overwritten += 1;
            }
            FileResult::Unchanged(path) => {
                eprintln!("  {} {}", style("unchanged").dim(), path);
            }
        }
    }

    eprintln!();
    let mut parts = Vec::new();
    if created > 0 {
        parts.push(format!("{created} created"));
    }
    if merged > 0 {
        parts.push(format!("{merged} merged"));
    }
    if skipped > 0 {
        parts.push(format!("{skipped} skipped"));
    }
    if overwritten > 0 {
        parts.push(format!("{overwritten} overwritten"));
    }
    if !parts.is_empty() {
        eprintln!("{}", parts.join(", "));
    }
}

#[cfg(test)]
mod tests;
