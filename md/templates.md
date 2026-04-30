# Templates

Battery pack templates scaffold files into your project. There are two modes: merging into an existing project (`cargo bp add -t`) and creating a new project from scratch (`cargo bp new`).

## Choosing and previewing templates

To see what templates a battery pack offers:

```bash
cargo bp show ci                  # lists templates in the detail view
cargo bp show ci -t spellcheck    # preview the rendered output
cargo bp show ci -t full -d fuzzing -d repo_owner=myorg  # preview with placeholder overrides
```

If a battery pack has multiple templates and you don't pass `-t`, you'll be prompted to pick one.

## Template variables

Templates can define variables (called placeholders) that are prompted interactively. Use `-d` to set them from the command line:

```bash
cargo bp add ci -t fuzzing -d ci_platform=github -d repo_owner=myorg
```

Bare `-d benchmarks` implies `=true` for boolean placeholders.

## Merging a template into an existing project

Some battery packs include small, single-purpose templates (spellcheck config, fuzzing scaffold, CI workflows) that you can merge into an existing project:

```bash
cargo bp add ci -t spellcheck
cargo bp add ci -t fuzzing -d ci_platform=github
cargo bp add ci -t trusted-publishing
```

New files are written directly. Existing files are handled based on type:

- **`.toml` files** are merged: new deps and sections are added, existing ones are left alone.
- **`.yml` / `.yaml` files** are merged: new top-level keys are added, existing ones are left alone.
- **Everything else** prompts you to skip, overwrite, or view a diff.

Each prompt has a single-key shortcut shown in brackets (e.g., `[a]ccept`, `[s]kip`). Uppercase variants (`[A]ccept all`, `[S]kip all`) apply to all remaining files.

For TOML and YAML merges, you can also open the result in `$EDITOR` before accepting.

### Flags

```bash
cargo bp add ci -t spellcheck --overwrite   # overwrite non-TOML/YAML files without prompting
cargo bp add ci -t spellcheck -N            # non-interactive: skip conflicts, auto-apply merges
cargo bp add ci -t spellcheck -N --overwrite # non-interactive + overwrite everything
```

TOML and YAML files are always merged, never overwritten, regardless of flags.

### Notes

- If your working tree has uncommitted changes, you'll be warned before proceeding. In `-N` mode, this is an error (since you can't be prompted to confirm) unless `--overwrite` is passed.
- The project name for template variables comes from your `Cargo.toml` `[package].name` (or the directory name as fallback).
- Some templates print follow-up instructions after the merge (e.g., "add `mod errors;` to your lib.rs").
- In the TUI, select a template in the detail view and press `u` to merge it.

## Creating a new project from a template

Templates can also scaffold an entirely new project:

```bash
cargo bp new cli
cargo bp new cli --template subcmds
cargo bp new cli --name my-app -d description="My CLI tool"
```

You'll be prompted for a project name (or pass `--name`). Template selection, previewing, and `-d` placeholders work the same as [merging](#merging-a-template-into-an-existing-project).

You can also create new projects from the TUI's "New project" tab.
