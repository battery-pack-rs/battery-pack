# TUI Behavior

This section specifies the behavior of the interactive terminal interface
launched by `cargo bp` (no arguments).

## Main menu

r[tui.main.always-available]
The TUI MUST be launchable from any directory, whether or not
a Rust project is present.

r[tui.main.sections]
The TUI main screen MUST display the following sections:
- Installed battery packs (for managing current dependencies)
- Browse (for discovering and adding new battery packs)
- New project (for creating projects from templates)

r[tui.main.no-project]
When not inside a Rust project, the installed battery packs section
MUST be visually disabled (greyed out) with a message indicating
no project was found. Browse and New project MUST remain functional.

r[tui.main.context-detection]
The TUI MUST detect the current project context by searching for
a Cargo.toml in the current directory and walking up to find
a workspace root.

## Installed packs view

r[tui.installed.list-packs]
The installed packs view MUST list all battery packs registered
in the project's metadata, showing their names and versions.

r[tui.installed.list-crates]
For each installed battery pack, the TUI MUST display its
curated crates (excluding hidden dependencies), grouped by feature.

r[tui.installed.toggle-crate]
The user MUST be able to toggle individual crates on and off.
Toggling a crate on adds it to the user's dependencies;
toggling it off removes it, unless the crate is required by
another enabled feature (see `tui.installed.features`).

r[tui.installed.dep-kind]
The user MUST be able to change a crate's dependency kind
(runtime, dev, build) from the TUI. The default is determined
by the battery pack's Cargo.toml.

r[tui.installed.show-state]
Each crate MUST be displayed with its current state:
enabled/disabled, dependency kind, and version.

r[tui.installed.features]
Battery pack features MUST be displayed as toggleable groups.
Enabling a feature enables all its crates; disabling it
disables crates that aren't required by another enabled feature.

r[tui.installed.hidden]
Hidden dependencies MUST NOT appear in the installed packs view.

## Browse view

r[tui.browse.search]
The browse view MUST allow searching crates.io for battery packs
by name.

r[tui.browse.list]
Search results MUST display the battery pack name, version,
and description.

r[tui.browse.detail]
Selecting a battery pack in browse MUST show its details:
curated crates (excluding hidden dependencies), features,
templates, and examples.

r[tui.browse.add]
The user MUST be able to add a battery pack from the browse view.
When adding, the TUI MUST show a selection screen with
default crates pre-checked (based on the `default` feature),
excluding hidden dependencies.

r[tui.browse.hidden]
Hidden dependencies MUST NOT appear when browsing a battery pack's
contents.

## New project view

r[tui.new.template-list]
The new project view MUST list available templates from
installed battery packs and allow browsing templates from
battery packs on crates.io.

r[tui.new.create]
Selecting a template MUST prompt for a project name and directory,
then create the project using the built-in template engine.

## Network operations

r[tui.network.non-blocking]
Network operations (fetching battery pack lists, downloading specs)
MUST NOT block the TUI. The interface MUST remain responsive
with a loading indicator while network requests are in progress.

r[tui.network.error]
Network errors MUST be displayed to the user without crashing
the TUI. The user MUST be able to retry or continue using
other features.

## Navigation

r[tui.nav.keyboard]
The TUI MUST support keyboard navigation: arrow keys or j/k for
movement, Enter for selection, Space for toggling, Esc or q for
back/quit, Tab for switching between sections.

r[tui.nav.exit]
When the user confirms and exits the TUI (e.g., Enter on the
apply prompt), all pending changes (added/removed crates,
changed dependency kinds) MUST be applied to the project's
Cargo.toml files. Exits via cancel (see `tui.nav.cancel`)
MUST NOT apply changes.

r[tui.nav.cancel]
The user MUST be able to cancel without applying changes
(e.g., Ctrl+C or a dedicated cancel action).
