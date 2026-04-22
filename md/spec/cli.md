# CLI Behavior

This section specifies the behavior of each `cargo bp` subcommand.

## Crate sources

r[cli.source.flag]
`cargo bp --crate-source <path>` MUST use a local workspace as
the battery pack source, replacing crates.io. The `<path>` MUST
point to a directory containing a `Cargo.toml` with `[workspace]`.

r[cli.source.discover]
When a crate source is specified, `cargo bp` MUST scan the
workspace members for crates whose names end in `-battery-pack`
and make them available as battery packs.

r[cli.source.replace]
When `--crate-source` is specified, it MUST fully replace
crates.io. No network requests to crates.io are made.

r[cli.source.multiple]
The `--crate-source` flag MAY be specified multiple times to add
multiple local workspaces.

r[cli.source.subcommands]
The `--crate-source` flag MUST be accepted by all subcommands that
resolve battery packs: `add`, `new`, `show`, `list`, `status`,
and `sync`.

r[cli.source.scope]
The `--crate-source` flag is a per-invocation option that
replaces the default crates.io source with local directories.
It does not persist across invocations.

## Path flag

r[cli.path.flag]
`cargo bp --path <path>` MUST read a battery pack from the
given directory. Unlike `--crate-source`, which adds a searchable
workspace, `--path` identifies a single battery pack directory
directly.

r[cli.path.subcommands]
The `--path` flag MUST be accepted by all subcommands that
operate on a specific battery pack: `add`, `new`, `show`,
`check`, `validate`, `status`, and `sync`.

r[cli.path.no-resolve]
When `--path` is provided, name resolution is not needed.
The battery pack is read directly from the given directory.

## Non-interactive mode

r[cli.non-interactive.flag]
`cargo bp --non-interactive` (or `-N`) MUST suppress interactive
prompts and TUI mode. This is a global flag accepted by all
subcommands.

r[cli.non-interactive.env]
Setting `CARGO_BP_NON_INTERACTIVE=true` MUST have the same effect
as passing `--non-interactive`. The flag and env var are combined
with OR logic.

r[cli.non-interactive.tty]
When stdout is not a TTY, `cargo bp` MUST behave as if
`--non-interactive` were passed.

## Name resolution

r[cli.name.resolve]
When a battery pack name is given without the `-battery-pack` suffix,
the CLI MUST resolve it by appending `-battery-pack`.
For example, `cli` resolves to `cli-battery-pack`.

r[cli.name.exact]
If the user provides a full crate name ending in `-battery-pack`,
it MUST be used as-is without further modification.

## `cargo bp` (no arguments)

r[cli.bare.tui]
Running `cargo bp` with no subcommand and no flags MUST print
the available subcommands and exit. This is the default clap
behavior when a required subcommand is missing.

r[cli.bare.help]
Running `cargo bp --help` MUST print CLI help text and exit.

## `cargo bp add`

r[cli.add.register]
`cargo bp add <pack>` MUST register the battery pack in the project's
metadata and add the default crates to the appropriate dependency sections.

r[cli.add.default-crates]
When no `-F`/`--features`, `--no-default-features`, or `--all-features`
flags are given, `cargo bp add <pack>` MUST add the crates from the
battery pack's `default` feature (or all non-optional crates if no
`default` feature exists).

r[cli.add.features]
`cargo bp add <pack> -F <name>` (or `--features <name>`) MUST add
all crates from the named feature. Unless `--no-default-features`
is specified, the default crates are also included.

r[cli.add.features-multiple]
Multiple features MAY be specified as a comma-separated list
(`-F indicators,fancy`) or by repeating the flag (`-F indicators -F fancy`).

r[cli.add.no-default-features]
`cargo bp add <pack> --no-default-features` MUST add no crates
by itself. Combined with `-F`, it adds only the named feature's
crates.

r[cli.add.all-features]
`cargo bp add <pack> --all-features` MUST add every crate the battery pack
offers, regardless of features or optional status.

r[cli.add.specific-crates]
`cargo bp add <pack> <crate> [<crate>...]` MUST add only the
named crates from the battery pack, ignoring defaults and features.

r[cli.add.dep-kind]
Each crate MUST be added with the dependency kind matching its
section in the battery pack's Cargo.toml (regular, dev, or build),
unless the user overrides it.

r[cli.add.target]
`cargo bp add <pack> --target <level>` controls where the battery
pack registration is stored. The `<level>` MUST be one of:
- `workspace` — register in `[workspace.metadata.battery-pack]`
- `package` — register in `[package.metadata.battery-pack]`
- `default` — use workspace if a workspace root exists, otherwise package

If `--target` is not specified, the default behavior MUST be used.

r[cli.add.unknown-crate]
When specific crates are named (`cargo bp add <pack> <crate>...`)
and a named crate does not exist in the battery pack, `cargo bp`
MUST report an error for that crate. Other valid crates in the
same command MUST still be processed.

r[cli.add.idempotent]
Adding a battery pack that is already registered MUST NOT create
duplicate entries. If the battery pack is already present,
`cargo bp add` MUST update its version and sync any new crates.

### Template merging

r[cli.add.template-flag]
`cargo bp add <pack> --template <name>` (or `-t <name>`) MUST
render the named template and merge the output into the current
project directory. This does not register the battery pack or
add its curated crates; it only applies the template files.

r[cli.add.template-project-name]
When merging a template, `cargo bp` MUST infer `project_name`
from the current `Cargo.toml` `[package].name`. If no
`Cargo.toml` exists or it has no `[package].name`, the current
directory name MUST be used as a fallback.

r[cli.add.template-define]
`cargo bp add <pack> -t <name> --define <key>=<value>` (or `-d`)
MUST set the named placeholder to the given value, skipping the
prompt for that placeholder. Multiple `-d` flags MAY be provided.

r[cli.add.template-merge-toml]
When a template produces a `.toml` file and the target file
already exists, `cargo bp` MUST merge using TOML-aware logic:
for `Cargo.toml`, dependencies are synced (version upgraded if
behind, features unioned, never removed); for all `.toml` files,
sections and keys are recursively merged (inserted if absent,
left alone if present). The user's existing formatting MUST
be preserved.

r[cli.add.template-merge-yaml]
When a template produces a `.yml` or `.yaml` file and the target
file already exists, `cargo bp` MUST merge using YAML-aware
logic: top-level mapping keys are merged additively. For known
GitHub Actions keys (`jobs`, `on`, `permissions`), child maps
are also merged additively. Existing keys are never removed.

r[cli.add.template-merge-plain]
When a template produces any other file and the target file
already exists, `cargo bp` MUST prompt the user to skip,
overwrite, or view a diff.

r[cli.add.template-overwrite]
`cargo bp add <pack> -t <name> --overwrite` MUST force overwrite
all plain file conflicts without prompting. Structured file
merges (TOML, YAML) MUST still use merge logic.

r[cli.add.template-non-interactive]
In non-interactive mode, conflicts with files that are not
`.toml`, `.yml`, or `.yaml` MUST be skipped unless `--overwrite`
is passed. TOML and YAML merges MUST still apply.

r[cli.add.template-hints]
If the template's `bp-template.toml` declares `[[hints]]`
entries, `cargo bp` MUST print them after the merge summary.

r[cli.add.template-git-dirty]
Before applying template files, `cargo bp` MUST check for
uncommitted git changes. In interactive mode, it MUST warn
and prompt for confirmation. In non-interactive mode, it MUST
refuse unless `--overwrite` is passed. If the directory is not
a git repository, the check MUST be skipped.

r[cli.add.template-batch]
When prompting for conflict resolution, `cargo bp` MUST offer
batch options. For TOML and YAML merge prompts: "accept all"
and "skip all". For other file prompts: "overwrite all" and
"skip all". Batch options apply to all remaining conflicts
without further prompting.

r[cli.add.template-edit]
When prompting for structured merge conflicts (TOML, YAML),
`cargo bp` MUST offer an "edit" option that opens the merged
result in `$VISUAL`, `$EDITOR`, or `vi` (in that order). After
editing, the updated diff MUST be shown and the user MUST be
returned to the accept/skip/edit prompt.

## `cargo bp new`

r[cli.new.template]
`cargo bp new <pack>` MUST create a new project from the battery
pack's template using the built-in template engine.

r[cli.new.name-flag]
`cargo bp new <pack> --name <name>` MUST pass the project name
to the template engine, skipping the name prompt.

r[cli.new.name-prompt]
If `--name` is not provided, the CLI MUST prompt the user for
a project name.

r[cli.new.template-select]
If the battery pack has multiple templates and `--template` is not
provided, the CLI MUST prompt the user to select one.

r[cli.new.template-flag]
`cargo bp new <pack> --template <name>` MUST use the specified template
without prompting.

r[cli.new.define-flag]
`cargo bp new <pack> --define <key>=<value>` (or `-d`) MUST set the
named placeholder to the given value, skipping the prompt for that
placeholder. Multiple `-d` flags MAY be provided.

r[cli.new.non-interactive]
In non-interactive mode, `cargo bp new` MUST fail with an error
if `--name` is not provided. Template placeholders without a
default or `--define` override MUST also cause an error.

## `cargo bp status`

r[cli.status.list]
`cargo bp status` MUST list all installed battery packs with their
registered versions.

r[cli.status.version-warn]
For each installed battery pack, `cargo bp status` MUST display
a warning for each dependency whose version is older than what
the battery pack recommends. Dependencies with equal or newer
versions MUST NOT produce a warning.

r[cli.status.no-project]
If run outside a Rust project, `cargo bp status` MUST report
that no project was found.

## `cargo bp sync`

r[cli.sync.update-versions]
`cargo bp sync` MUST update dependency versions that are older
than what the installed battery packs recommend. Versions that
are equal to or newer than recommended MUST be left unchanged.

r[cli.sync.add-features]
`cargo bp sync` MUST add any Cargo features that the battery pack
specifies but are missing from the user's dependency entry.
Existing user-added features MUST be preserved.

r[cli.sync.add-crates]
`cargo bp sync` MUST add any crates that belong to the user's
active features but are missing from the user's dependencies.
Existing crates MUST NOT be removed.

## `cargo bp list`

r[cli.list.query]
`cargo bp list` MUST query crates.io for crates with the
`battery-pack` keyword.

r[cli.list.filter]
`cargo bp list <filter>` MUST filter results by name pattern.

r[cli.list.interactive]
If running in a TTY, `cargo bp list` SHOULD display results
in the interactive TUI.

r[cli.list.non-interactive]
In non-interactive mode, `cargo bp list` MUST print results as
plain text.

## `cargo bp check`

r[cli.check.purpose]
`cargo bp check` MUST validate that installed battery packs match
the project's current dependencies and warn about version drift.

r[cli.check.version-drift]
`cargo bp check` MUST compare the user's current dependency versions
against the versions recommended by installed battery packs and warn
when user versions are older than recommended versions.

r[cli.check.output]
`cargo bp check` MUST display the status of each installed battery pack
with clear indicators (✅ for up-to-date, ⚠️ for outdated versions).

r[cli.check.no-packs]
If no battery packs are installed, `cargo bp check` MUST display
"No battery packs installed." and exit successfully.

## `cargo bp validate`

r[cli.validate.purpose]
`cargo bp validate` MUST check whether a battery pack crate
conforms to the battery pack format specification (`format.*` rules).

r[cli.validate.default-path]
If `--path` is not provided, `cargo bp validate` MUST validate
the battery pack in the current directory.

r[cli.validate.checks]
`cargo bp validate` MUST check all applicable `format.*` rules,
including both data-level checks (from the parsed `Cargo.toml`)
and filesystem-level checks (on-disk structure).

r[cli.validate.severity]
Violations of MUST rules MUST be reported as errors.
Violations of SHOULD rules MUST be reported as warnings.

r[cli.validate.rule-id]
Each diagnostic MUST include the spec rule ID in its output
(e.g., `error[format.crate.name]: ...`).

r[cli.validate.clean]
When a battery pack passes all checks with no diagnostics,
`cargo bp validate` MUST print `<name> is valid` and exit
successfully.

r[cli.validate.warnings-only]
When a battery pack has warnings but no errors,
`cargo bp validate` MUST print `<name> is valid (<N> warning(s))`
and exit successfully.

r[cli.validate.errors]
When a battery pack has one or more errors, `cargo bp validate`
MUST exit with a non-zero status.

r[cli.validate.workspace-error]
If the target `Cargo.toml` is a workspace manifest (contains
`[workspace]` but no `[package]`), `cargo bp validate` MUST
report a clear error directing the user to run from a battery
pack crate directory or use `--path`.

r[cli.validate.no-package]
If the target `Cargo.toml` has no `[package]` section and is not
a workspace manifest, `cargo bp validate` MUST report a clear
error indicating the file is not a battery pack crate.

r[cli.validate.templates]
`cargo bp validate` MUST generate each declared template into a
temporary directory, then run `cargo check` and `cargo test` on
the result. If any template fails to compile or its tests fail,
validation MUST fail.

r[cli.validate.templates.patch]
When validating templates, `cargo bp validate` MUST patch
crates-io dependencies with local workspace packages so that
validation runs against the current source.

r[cli.validate.templates.cache]
Compiled artifacts from template validation SHOULD be cached in
`<target_dir>/bp-validate/` so that subsequent runs are faster.

r[cli.validate.templates.none]
If the battery pack declares no templates, template validation
MUST be skipped.

## `cargo bp show`

r[cli.show.details]
`cargo bp show <pack>` MUST display the battery pack's name, version,
description, curated crates, features, templates, and examples.

r[cli.show.hidden]
`cargo bp show` MUST NOT display hidden dependencies.

r[cli.show.interactive]
If running in a TTY, `cargo bp show` SHOULD display results
in the interactive TUI.

r[cli.show.non-interactive]
In non-interactive mode, `cargo bp show` MUST print results as
plain text.

r[cli.show.template-preview]
`cargo bp show <pack> --template <name>` MUST render the named
template and display the resulting files. In a TTY, the output
SHOULD be shown in the interactive TUI preview screen. With
`--non-interactive`, the rendered files MUST be printed to stdout.
Placeholders without a default MUST fall back to `<name>` so the
preview always succeeds. The project name MUST default to
`my-project`.
