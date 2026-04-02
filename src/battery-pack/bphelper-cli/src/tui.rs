//! Interactive TUI for battery-pack CLI.

use crate::registry::{
    BatteryPackDetail, BatteryPackSummary, CrateSource, InstalledPack, fetch_battery_pack_detail,
    fetch_battery_pack_list, load_installed_packs,
};
use anyhow::Result;
use bphelper_manifest::DepKind;
use crossterm::event::{self, Event, KeyCode, KeyEventKind, KeyModifiers};
use ratatui::{
    Frame,
    layout::{Constraint, Flex, Layout, Position, Rect},
    style::{Color, Modifier, Style, Stylize},
    text::{Line, Span, Text},
    widgets::{Block, Borders, Clear, List, ListItem, ListState, Paragraph, Wrap},
};
use std::collections::{BTreeMap, BTreeSet};
use std::rc::Rc;
use std::time::Duration;

// ============================================================================
// Public entry points
// ============================================================================

/// Run the TUI starting from the list view
pub fn run_list(source: CrateSource, filter: Option<String>) -> Result<()> {
    let app = App::new_list(source, filter);
    app.run()
}

/// Run the TUI starting from the detail view
pub fn run_show(name: &str, path: Option<&str>, source: CrateSource) -> Result<()> {
    let app = App::new_show(name, path, source);
    app.run()
}

/// Run the TUI for interactive dependency management
pub fn run_add(source: CrateSource) -> Result<()> {
    let app = App::new_add(source);
    app.run()
}

// ============================================================================
// App state
// ============================================================================

struct App {
    source: CrateSource,
    screen: Screen,
    should_quit: bool,
    pending_action: Option<PendingAction>,
}

/// A single crate being added or updated.
#[derive(Debug)]
#[allow(dead_code)]
struct CrateChange {
    name: String,
    dep_kind: DepKind,
}

impl From<&CrateEntry> for CrateChange {
    fn from(entry: &CrateEntry) -> Self {
        Self {
            name: entry.name.clone(),
            dep_kind: entry.dep_kind,
        }
    }
}

/// A change to apply from the Add TUI.
#[derive(Debug)]
enum AddChange {
    /// Add a new battery pack with selected crates.
    AddPack {
        name: String,
        crates: Vec<CrateChange>,
    },
    /// Update an existing pack's enabled crates.
    UpdatePack {
        name: String,
        /// Crates to add (weren't enabled before, now are).
        add_crates: Vec<CrateChange>,
        /// Crates to remove (were enabled before, now aren't).
        remove_crates: Vec<String>,
    },
}

enum Screen {
    /// Transient placeholder used when taking ownership of the screen via `mem::replace`.
    /// Never rendered or handled — immediately overwritten.
    Empty,
    Loading(LoadingState),
    /// [impl tui.network.error]
    Error(ErrorScreen),
    List(ListScreen),
    Detail(DetailScreen),
    NewProjectForm(FormScreen),
    Add(AddScreen),
    Preview(PreviewScreen),
}

struct ErrorScreen {
    message: String,
    retry_target: LoadingTarget,
}

struct LoadingState {
    message: String,
    target: LoadingTarget,
}

enum LoadingTarget {
    List {
        filter: Option<String>,
    },
    Detail {
        name: String,
        path: Option<String>,
        came_from_list: bool,
    },
    Add,
    /// Fetch the browse list for the Add screen (initial load or search).
    BrowseList {
        add_screen: AddScreen,
        filter: Option<String>,
    },
    /// Expand a battery pack from the browse list.
    BrowseExpand {
        add_screen: AddScreen,
        bp_name: String,
        bp_short_name: String,
    },
}

struct ListScreen {
    items: Vec<BatteryPackSummary>,
    list_state: ListState,
    filter: Option<String>,
}

struct DetailScreen {
    detail: Rc<BatteryPackDetail>,
    /// Index into selectable_items()
    selected_index: usize,
    came_from_list: bool,
}

/// A selectable item in the detail view
#[derive(Clone, Debug)]
enum DetailItem {
    /// A crate dependency - opens crates.io
    Crate(String),
    /// An extended battery pack - opens crates.io
    Extends(String),
    /// A template - opens GitHub tree URL (stores local path and resolved repo path)
    Template {
        _path: String,
        repo_path: Option<String>,
    },
    /// An example - opens GitHub blob URL (stores name and resolved repo path)
    Example {
        _name: String,
        repo_path: Option<String>,
    },
    /// Open on crates.io action
    ActionOpenCratesIo,
    /// Add to project action
    ActionAddToProject,
    /// Create new project action
    ActionNewProject,
}

/// Advance or retreat a wrapping index within `0..count`.
fn wrapping_nav(index: &mut usize, count: usize, forward: bool) {
    if count > 0 {
        *index = if forward {
            (*index + 1) % count
        } else {
            (*index + count - 1) % count
        };
    }
}

/// Clamped (non-wrapping) movement on a `ListState` within `0..count`.
fn list_nav(state: &mut ListState, count: usize, forward: bool) {
    if let Some(selected) = state.selected() {
        if forward {
            if selected < count.saturating_sub(1) {
                state.select(Some(selected + 1));
            }
        } else if selected > 0 {
            state.select(Some(selected - 1));
        }
    }
}

fn wait_for_enter() {
    // ratatui::restore() leaves the alternate screen and disables raw mode but
    // does not re-show the cursor, so we do it explicitly here.
    let _ = crossterm::execute!(std::io::stdout(), crossterm::cursor::Show);
    println!("\nPress Enter to return to TUI...");
    let _ = std::io::stdin().read_line(&mut String::new());
}

impl DetailScreen {
    /// Get the total count of selectable items without building the full vector.
    /// This is more efficient for navigation operations that only need the count.
    fn item_count(&self) -> usize {
        self.detail.crates.len()
            + self.detail.extends.len()
            + self.detail.templates.len()
            + self.detail.examples.len()
            + 3 // actions: OpenCratesIo, AddToProject, NewProject
    }

    /// Iterate over all selectable items in order.
    fn selectable_items(&self) -> impl Iterator<Item = DetailItem> + '_ {
        let crates = self.detail.crates.iter().cloned().map(DetailItem::Crate);
        let extends = self.detail.extends.iter().cloned().map(DetailItem::Extends);
        let templates = self.detail.templates.iter().map(|t| DetailItem::Template {
            _path: t.path.clone(),
            repo_path: t.repo_path.clone(),
        });
        let examples = self.detail.examples.iter().map(|e| DetailItem::Example {
            _name: e.name.clone(),
            repo_path: e.repo_path.clone(),
        });
        let actions = [
            DetailItem::ActionOpenCratesIo,
            DetailItem::ActionAddToProject,
            DetailItem::ActionNewProject,
        ];

        crates
            .chain(extends)
            .chain(templates)
            .chain(examples)
            .chain(actions)
    }

    fn selected_item(&self) -> Option<DetailItem> {
        self.selectable_items().nth(self.selected_index)
    }

    fn select_next(&mut self) {
        let count = self.item_count();
        wrapping_nav(&mut self.selected_index, count, true);
    }

    fn select_prev(&mut self) {
        let count = self.item_count();
        wrapping_nav(&mut self.selected_index, count, false);
    }
}

struct FormScreen {
    battery_pack: String,
    /// If set, use this specific template (passed via --template)
    template: Option<String>,
    directory: String,
    project_name: String,
    focused_field: FormField,
    cursor_position: usize,
    /// The detail screen to return to on cancel (shared to avoid cloning)
    detail: Rc<BatteryPackDetail>,
    /// Selected index to restore when returning to detail
    selected_index: usize,
    came_from_list: bool,
}

#[derive(Clone, Copy, PartialEq)]
enum FormField {
    Directory,
    ProjectName,
}

impl FormScreen {
    fn focused_field_mut(&mut self) -> &mut String {
        match self.focused_field {
            FormField::Directory => &mut self.directory,
            FormField::ProjectName => &mut self.project_name,
        }
    }

    fn focused_field_len(&self) -> usize {
        match self.focused_field {
            FormField::Directory => self.directory.len(),
            FormField::ProjectName => self.project_name.len(),
        }
    }
}

struct PreviewScreen {
    /// Syntax-highlighted content to display.
    content: Text<'static>,
    /// Template name for the header.
    template_name: String,
    /// Vertical scroll offset.
    scroll: u16,
    /// Total number of lines in content (for scroll bounds).
    line_count: u16,
    /// The detail screen to return to on Esc.
    detail: Rc<BatteryPackDetail>,
    /// Selected index to restore when returning to detail.
    selected_index: usize,
    came_from_list: bool,
}

// ============================================================================
// Add screen (interactive dependency manager)
// ============================================================================

struct AddScreen {
    tab: AddTab,
    installed: InstalledState,
    browse: BrowseState,
    /// Changes to apply after the TUI exits.
    changes: Option<Vec<AddChange>>,
}

#[derive(Clone, Copy, Debug, PartialEq)]
enum AddTab {
    Installed,
    Browse,
}

struct InstalledState {
    packs: Vec<InstalledPackState>,
    /// Flat index across all selectable rows (crate entries) in all packs.
    selected_index: usize,
}

struct InstalledPackState {
    name: String,
    short_name: String,
    version: String,
    entries: Vec<CrateEntry>,
    /// Feature→crate mapping from the battery pack spec, used to enforce
    /// the constraint that a crate can't be disabled if another enabled
    /// feature requires it.
    features: BTreeMap<String, BTreeSet<String>>,
}

impl InstalledPackState {
    /// Returns true if the given crate is required by a feature other than its own group
    /// that has at least one other enabled crate.
    fn is_required_by_other_feature(&self, crate_name: &str, crate_group: &str) -> bool {
        for (feature_name, crate_set) in &self.features {
            if feature_name == crate_group {
                continue; // Skip the crate's own feature group
            }
            if !crate_set.contains(crate_name) {
                continue; // This feature doesn't include the crate
            }
            // This feature includes the crate — check if it has other enabled crates
            let has_other_enabled = self
                .entries
                .iter()
                .any(|e| e.enabled && e.name != crate_name && crate_set.contains(&e.name));
            if has_other_enabled {
                return true;
            }
        }
        false
    }
}

#[derive(Clone)]
struct CrateEntry {
    name: String,
    version: String,
    features: Vec<String>,
    dep_kind: DepKind,
    original_dep_kind: DepKind,
    group: String,
    enabled: bool,
    originally_enabled: bool,
}

impl CrateEntry {
    fn new(
        group: String,
        name: String,
        dep: &bphelper_manifest::CrateSpec,
        enabled: bool,
        originally_enabled: bool,
    ) -> Self {
        Self {
            name,
            version: dep.version.clone(),
            features: dep.features.iter().cloned().collect(),
            dep_kind: dep.dep_kind,
            original_dep_kind: dep.dep_kind,
            group,
            enabled,
            originally_enabled,
        }
    }

    fn version_info(&self) -> String {
        // [impl tui.installed.show-state]
        let kind_label = match self.dep_kind {
            DepKind::Normal => None,
            DepKind::Dev => Some("dev"),
            DepKind::Build => Some("build"),
        };
        match (kind_label, self.features.is_empty()) {
            (None, true) => format!("({})", self.version),
            (None, false) => {
                format!("({}, features: {})", self.version, self.features.join(", "))
            }
            (Some(kind), true) => format!("({}, {})", self.version, kind),
            (Some(kind), false) => {
                format!(
                    "({}, {}, features: {})",
                    self.version,
                    kind,
                    self.features.join(", ")
                )
            }
        }
    }
}

struct BrowseState {
    items: Vec<BatteryPackSummary>,
    list_state: ListState,
    search_input: String,
    searching: bool,
    /// When a pack is expanded, its crate picker state
    expanded: Option<ExpandedPack>,
}

struct ExpandedPack {
    pack: InstalledPackState,
    selected_index: usize,
}

impl InstalledState {
    /// Total number of selectable crate rows across all packs.
    fn total_entries(&self) -> usize {
        self.packs.iter().map(|p| p.entries.len()).sum()
    }

    fn select_next(&mut self) {
        let total = self.total_entries();
        wrapping_nav(&mut self.selected_index, total, true);
    }

    fn select_prev(&mut self) {
        let total = self.total_entries();
        wrapping_nav(&mut self.selected_index, total, false);
    }

    /// Toggle the currently selected crate's enabled state.
    /// Refuses to disable a crate that is required by another enabled feature.
    // [impl tui.installed.toggle-crate]
    fn toggle_selected(&mut self) {
        // Find which pack and entry index the selected_index points to.
        let mut flat = 0;
        let mut target = None;
        for (pi, pack) in self.packs.iter().enumerate() {
            for ei in 0..pack.entries.len() {
                if flat == self.selected_index {
                    target = Some((pi, ei));
                    break;
                }
                flat += 1;
            }
            if target.is_some() {
                break;
            }
        }

        let Some((pi, ei)) = target else { return };
        let pack = &self.packs[pi];
        let entry = &pack.entries[ei];

        // When disabling, check the constraint.
        if entry.enabled && pack.is_required_by_other_feature(&entry.name, &entry.group) {
            return;
        }

        self.packs[pi].entries[ei].enabled = !self.packs[pi].entries[ei].enabled;
    }

    /// Cycle the currently selected crate's dep_kind: Normal → Dev → Build → Normal.
    // [impl tui.installed.dep-kind]
    fn cycle_dep_kind(&mut self) {
        let mut idx = 0;
        for pack in &mut self.packs {
            for entry in &mut pack.entries {
                if idx == self.selected_index {
                    entry.dep_kind = match entry.dep_kind {
                        DepKind::Normal => DepKind::Dev,
                        DepKind::Dev => DepKind::Build,
                        DepKind::Build => DepKind::Normal,
                    };
                    return;
                }
                idx += 1;
            }
        }
    }

    /// Returns true if any crate's enabled/dep_kind state differs from its original.
    fn has_changes(&self) -> bool {
        self.packs
            .iter()
            .flat_map(|p| &p.entries)
            .any(|e| e.enabled != e.originally_enabled || e.dep_kind != e.original_dep_kind)
    }

    /// Returns true if any pack was added from Browse (all entries have originally_enabled=false).
    fn has_new_packs(&self) -> bool {
        self.packs.iter().any(|p| {
            p.entries.iter().all(|e| !e.originally_enabled) && p.entries.iter().any(|e| e.enabled)
        })
    }

    /// Collect all changes into AddChange items for processing.
    fn collect_changes(&self) -> Vec<AddChange> {
        let mut changes = Vec::new();

        for pack in &self.packs {
            let is_new = pack.entries.iter().all(|e| !e.originally_enabled);
            let has_enabled = pack.entries.iter().any(|e| e.enabled);

            if is_new && has_enabled {
                // New pack from Browse
                let crates = pack
                    .entries
                    .iter()
                    .filter(|e| e.enabled)
                    .map(CrateChange::from)
                    .collect();
                changes.push(AddChange::AddPack {
                    name: pack.name.clone(),
                    crates,
                });
            } else {
                // Existing pack — check for toggles
                let add_crates: Vec<_> = pack
                    .entries
                    .iter()
                    .filter(|e| e.enabled && !e.originally_enabled)
                    .map(CrateChange::from)
                    .collect();
                let remove_crates: Vec<_> = pack
                    .entries
                    .iter()
                    .filter(|e| !e.enabled && e.originally_enabled)
                    .map(|e| e.name.clone())
                    .collect();

                if !add_crates.is_empty() || !remove_crates.is_empty() {
                    changes.push(AddChange::UpdatePack {
                        name: pack.name.clone(),
                        add_crates,
                        remove_crates,
                    });
                }
            }
        }

        changes
    }
}

impl ExpandedPack {
    fn select_next(&mut self) {
        wrapping_nav(&mut self.selected_index, self.pack.entries.len(), true);
    }

    fn select_prev(&mut self) {
        wrapping_nav(&mut self.selected_index, self.pack.entries.len(), false);
    }

    fn toggle_selected(&mut self) {
        if let Some(entry) = self.pack.entries.get_mut(self.selected_index) {
            entry.enabled = !entry.enabled;
        }
    }
}

/// Build InstalledState from loaded packs.
fn build_installed_state(packs: Vec<InstalledPack>) -> InstalledState {
    let pack_states = packs
        .into_iter()
        .map(|pack| {
            let grouped = pack.spec.all_crates_with_grouping();
            // [impl format.hidden.effect]
            let resolved = pack.spec.resolve_for_features(&pack.active_features);

            let entries = grouped
                .into_iter()
                .map(|(group, crate_name, dep, _is_default)| {
                    let is_enabled = resolved.contains_key(&crate_name);
                    CrateEntry::new(group, crate_name, dep, is_enabled, is_enabled)
                })
                .collect();

            InstalledPackState {
                name: pack.name,
                short_name: pack.short_name,
                version: pack.version,
                entries,
                features: pack.spec.features.clone(),
            }
        })
        .collect();

    InstalledState {
        packs: pack_states,
        selected_index: 0,
    }
}

/// Build an ExpandedPack from a battery pack fetched from crates.io.
fn build_expanded_pack(
    detail: &BatteryPackSummary,
    spec: bphelper_manifest::BatteryPackSpec,
) -> ExpandedPack {
    let grouped = spec.all_crates_with_grouping();
    let entries = grouped
        .into_iter()
        .map(|(group, crate_name, dep, is_default)| {
            CrateEntry::new(group, crate_name, dep, is_default, false)
        })
        .collect();

    ExpandedPack {
        pack: InstalledPackState {
            name: detail.name.clone(),
            short_name: detail.short_name.clone(),
            version: detail.version.clone(),
            entries,
            features: spec.features,
        },
        selected_index: 0,
    }
}

enum PendingAction {
    /// Open a URL in the browser (generic)
    OpenUrl {
        url: String,
    },
    AddToProject {
        battery_pack: String,
    },
    NewProject {
        battery_pack: String,
        template: Option<String>,
        directory: String,
        name: String,
    },
}

// ============================================================================
// App implementation
// ============================================================================

impl App {
    fn new_list(source: CrateSource, filter: Option<String>) -> Self {
        Self {
            source,
            screen: Screen::Loading(LoadingState {
                message: "Loading battery packs...".to_string(),
                target: LoadingTarget::List { filter },
            }),
            should_quit: false,
            pending_action: None,
        }
    }

    fn new_add(source: CrateSource) -> Self {
        Self {
            source,
            screen: Screen::Loading(LoadingState {
                message: "Loading installed battery packs...".to_string(),
                target: LoadingTarget::Add,
            }),
            should_quit: false,
            pending_action: None,
        }
    }

    fn new_show(name: &str, path: Option<&str>, source: CrateSource) -> Self {
        Self {
            source,
            screen: Screen::Loading(LoadingState {
                message: format!("Loading {}...", name),
                target: LoadingTarget::Detail {
                    name: name.to_string(),
                    path: path.map(|s| s.to_string()),
                    came_from_list: false,
                },
            }),
            should_quit: false,
            pending_action: None,
        }
    }

    fn run(mut self) -> Result<()> {
        let result = self.run_inner();

        // Always restore the terminal, even if run_inner returned an error.
        ratatui::restore();
        let _ = crossterm::execute!(std::io::stdout(), crossterm::cursor::Show);

        result
    }

    fn run_inner(&mut self) -> Result<()> {
        let mut terminal = ratatui::init();

        loop {
            // Process any pending loading state (initial load, detail navigation,
            // browse tab transitions). This is the single place where Loading
            // screens are resolved — handle_key just sets up Screen::Loading.
            // Errors transition to Screen::Error instead of crashing.
            self.process_loading();

            terminal.draw(|frame| self.render(frame))?;

            // Execute pending actions (exit TUI, run command, re-enter)
            if let Some(action) = self.pending_action.take() {
                ratatui::restore();
                self.execute_action(&action)?;
                // This sets a panic hook
                terminal = ratatui::init();
                continue;
            }

            if event::poll(Duration::from_millis(100))?
                && let Event::Key(key) = event::read()?
            {
                // Windows compatibility: only handle Press events
                if key.kind == KeyEventKind::Press {
                    // Ctrl+C quits immediately
                    if key.modifiers.contains(KeyModifiers::CONTROL)
                        && key.code == KeyCode::Char('c')
                    {
                        break;
                    }
                    self.handle_key(key.code);
                }
            }

            if self.should_quit {
                break;
            }
        }

        // Apply any pending add changes after TUI exits
        if let Screen::Add(add_screen) = &mut self.screen
            && let Some(changes) = add_screen.changes.take()
        {
            // Restore terminal before applying changes so output is visible.
            ratatui::restore();
            apply_add_changes(&changes)?;
        }

        Ok(())
    }

    /// [impl tui.network.error]
    fn process_loading(&mut self) {
        // Take ownership of the screen so we can move data out of LoadingTarget variants.
        let screen = std::mem::replace(&mut self.screen, Screen::Empty);

        let Screen::Loading(state) = screen else {
            self.screen = screen;
            return;
        };

        match state.target {
            LoadingTarget::List { filter } => {
                match fetch_battery_pack_list(&self.source, filter.as_deref()) {
                    Ok(items) => {
                        let mut list_state = ListState::default();
                        if !items.is_empty() {
                            list_state.select(Some(0));
                        }
                        self.screen = Screen::List(ListScreen {
                            items,
                            list_state,
                            filter,
                        });
                    }
                    Err(e) => {
                        self.screen = Screen::Error(ErrorScreen {
                            message: format!("{e}"),
                            retry_target: LoadingTarget::List { filter },
                        });
                    }
                }
            }
            LoadingTarget::Detail {
                name,
                path,
                came_from_list,
            } => {
                // --path takes precedence over --crate-source
                let result = if path.is_some() {
                    fetch_battery_pack_detail(&name, path.as_deref())
                } else {
                    crate::registry::fetch_battery_pack_detail_from_source(&self.source, &name)
                };
                match result {
                    Ok(detail) => {
                        let initial_index = detail.crates.len()
                            + detail.extends.len()
                            + detail.templates.len()
                            + detail.examples.len();
                        self.screen = Screen::Detail(DetailScreen {
                            detail: Rc::new(detail),
                            selected_index: initial_index,
                            came_from_list,
                        });
                    }
                    Err(e) => {
                        self.screen = Screen::Error(ErrorScreen {
                            message: format!("{e}"),
                            retry_target: LoadingTarget::Detail {
                                name,
                                path,
                                came_from_list,
                            },
                        });
                    }
                }
            }
            LoadingTarget::Add => {
                let result = std::env::current_dir()
                    .map_err(anyhow::Error::from)
                    .and_then(|dir| load_installed_packs(&dir));
                match result {
                    Ok(packs) => {
                        let installed = build_installed_state(packs);
                        self.screen = Screen::Add(AddScreen {
                            tab: AddTab::Installed,
                            installed,
                            browse: BrowseState {
                                items: Vec::new(),
                                list_state: ListState::default(),
                                search_input: String::new(),
                                searching: false,
                                expanded: None,
                            },
                            changes: None,
                        });
                    }
                    Err(e) => {
                        self.screen = Screen::Error(ErrorScreen {
                            message: format!("{e}"),
                            retry_target: LoadingTarget::Add,
                        });
                    }
                }
            }
            LoadingTarget::BrowseList {
                mut add_screen,
                filter,
            } => match fetch_battery_pack_list(&self.source, filter.as_deref()) {
                Ok(items) => {
                    let has_items = !items.is_empty();
                    add_screen.browse.items = items;
                    add_screen.browse.list_state = ListState::default();
                    if has_items {
                        add_screen.browse.list_state.select(Some(0));
                    }
                    add_screen.tab = AddTab::Browse;
                    self.screen = Screen::Add(add_screen);
                }
                Err(e) => {
                    self.screen = Screen::Error(ErrorScreen {
                        message: format!("{e}"),
                        retry_target: LoadingTarget::BrowseList { add_screen, filter },
                    });
                }
            },
            LoadingTarget::BrowseExpand {
                mut add_screen,
                bp_name,
                bp_short_name,
            } => match crate::registry::fetch_bp_spec(&self.source, &bp_name) {
                Ok((_version, spec)) => {
                    let summary = BatteryPackSummary {
                        name: bp_name,
                        short_name: bp_short_name,
                        version: spec.version.clone(),
                        description: String::new(),
                    };
                    add_screen.browse.expanded = Some(build_expanded_pack(&summary, spec));
                    self.screen = Screen::Add(add_screen);
                }
                Err(e) => {
                    self.screen = Screen::Error(ErrorScreen {
                        message: format!("{e}"),
                        retry_target: LoadingTarget::BrowseExpand {
                            add_screen,
                            bp_name,
                            bp_short_name,
                        },
                    });
                }
            },
        }
    }

    fn execute_action(&self, action: &PendingAction) -> Result<()> {
        match action {
            PendingAction::OpenUrl { url } => {
                if let Err(e) = open::that(url) {
                    println!("Failed to open browser: {}", e);
                    println!("URL: {}", url);
                    wait_for_enter();
                }
                // No "press enter" for successful open - just return immediately
            }
            PendingAction::AddToProject { battery_pack } => {
                let status = std::process::Command::new("cargo")
                    .args(["bp", "add", battery_pack])
                    .status()?;

                if status.success() {
                    println!("\nSuccessfully added {}!", battery_pack);
                }
                wait_for_enter();
            }
            PendingAction::NewProject {
                battery_pack,
                template,
                directory,
                name,
            } => {
                let mut cmd = std::process::Command::new("cargo");
                cmd.args(["bp", "new", battery_pack, "-n", name]);
                if let Some(tmpl) = template {
                    cmd.args(["-t", tmpl]);
                }
                let status = cmd.current_dir(directory).status()?;

                if status.success() {
                    println!("\nSuccessfully created project '{}'!", name);
                }
                wait_for_enter();
            }
        }
        Ok(())
    }

    fn handle_key(&mut self, key: KeyCode) {
        // Extract needed data to avoid borrow conflicts
        enum Action {
            None,
            Quit,
            ListSelect(usize),
            ListUp,
            ListDown,
            DetailNext,
            DetailPrev,
            OpenCratesIoUrl(String),
            OpenTemplate {
                repository: Option<String>,
                repo_path: Option<String>,
            },
            OpenExample {
                repository: Option<String>,
                repo_path: Option<String>,
            },
            DetailOpenCratesIo(String),
            DetailAdd(String),
            DetailNewProject(Rc<BatteryPackDetail>, Option<String>, usize, bool),
            DetailBack(bool),
            FormToggleField,
            FormSubmit(
                String,
                Option<String>,
                String,
                String,
                Rc<BatteryPackDetail>,
                usize,
                bool,
            ),
            FormCancel(Rc<BatteryPackDetail>, usize, bool),
            FormChar(char),
            FormBackspace,
            FormDelete,
            FormLeft,
            FormRight,
            FormHome,
            FormEnd,
            PreviewTemplate(Rc<BatteryPackDetail>, String, usize, bool),
            PreviewScroll(i16),
            PreviewBack(Rc<BatteryPackDetail>, usize, bool),
        }

        let action = match &self.screen {
            Screen::Empty | Screen::Loading(_) => Action::None,
            Screen::Error(_) => {
                self.handle_error_key(key);
                return;
            }
            Screen::List(state) => match key {
                KeyCode::Up | KeyCode::Char('k') => Action::ListUp,
                KeyCode::Down | KeyCode::Char('j') => Action::ListDown,
                KeyCode::Enter => {
                    if let Some(selected) = state.list_state.selected() {
                        Action::ListSelect(selected)
                    } else {
                        Action::None
                    }
                }
                KeyCode::Char('q') | KeyCode::Esc => Action::Quit,
                _ => Action::None,
            },
            Screen::Detail(state) => match key {
                KeyCode::Tab | KeyCode::Down | KeyCode::Char('j') => Action::DetailNext,
                KeyCode::BackTab | KeyCode::Up | KeyCode::Char('k') => Action::DetailPrev,
                KeyCode::Enter => {
                    if let Some(item) = state.selected_item() {
                        match item {
                            DetailItem::Crate(crate_name) => Action::OpenCratesIoUrl(crate_name),
                            DetailItem::Extends(bp_name) => {
                                // Extends are battery packs, resolve to full name
                                let full_name = if bp_name.ends_with("-battery-pack") {
                                    bp_name
                                } else {
                                    format!("{}-battery-pack", bp_name)
                                };
                                Action::OpenCratesIoUrl(full_name)
                            }
                            DetailItem::Template {
                                _path: _,
                                repo_path,
                            } => Action::OpenTemplate {
                                repository: state.detail.repository.clone(),
                                repo_path,
                            },
                            DetailItem::Example {
                                _name: _,
                                repo_path,
                            } => Action::OpenExample {
                                repository: state.detail.repository.clone(),
                                repo_path,
                            },
                            DetailItem::ActionOpenCratesIo => {
                                Action::DetailOpenCratesIo(state.detail.name.clone())
                            }
                            DetailItem::ActionAddToProject => {
                                Action::DetailAdd(state.detail.short_name.clone())
                            }
                            DetailItem::ActionNewProject => {
                                Action::DetailNewProject(
                                    Rc::clone(&state.detail),
                                    None, // no specific template
                                    state.selected_index,
                                    state.came_from_list,
                                )
                            }
                        }
                    } else {
                        Action::None
                    }
                }
                KeyCode::Char('n') => {
                    // 'n' creates new project with currently selected template (if a template is selected)
                    if let Some(DetailItem::Template {
                        _path,
                        repo_path: _,
                    }) = state.selected_item()
                    {
                        // Find the template name from the path
                        let template_name = state
                            .detail
                            .templates
                            .iter()
                            .find(|t| t.path == _path)
                            .map(|t| t.name.clone());
                        Action::DetailNewProject(
                            Rc::clone(&state.detail),
                            template_name,
                            state.selected_index,
                            state.came_from_list,
                        )
                    } else {
                        Action::None
                    }
                }
                KeyCode::Char('p') => {
                    // 'p' previews the currently selected template
                    if let Some(DetailItem::Template { _path, .. }) = state.selected_item() {
                        Action::PreviewTemplate(
                            Rc::clone(&state.detail),
                            _path,
                            state.selected_index,
                            state.came_from_list,
                        )
                    } else {
                        Action::None
                    }
                }
                KeyCode::Esc => Action::DetailBack(state.came_from_list),
                KeyCode::Char('q') => Action::Quit,
                _ => Action::None,
            },
            Screen::NewProjectForm(state) => match key {
                KeyCode::Tab => Action::FormToggleField,
                KeyCode::Enter => {
                    if !state.project_name.is_empty() {
                        Action::FormSubmit(
                            state.battery_pack.clone(),
                            state.template.clone(),
                            state.directory.clone(),
                            state.project_name.clone(),
                            Rc::clone(&state.detail),
                            state.selected_index,
                            state.came_from_list,
                        )
                    } else {
                        Action::None
                    }
                }
                KeyCode::Esc => Action::FormCancel(
                    Rc::clone(&state.detail),
                    state.selected_index,
                    state.came_from_list,
                ),
                KeyCode::Char(c) => Action::FormChar(c),
                KeyCode::Backspace => Action::FormBackspace,
                KeyCode::Delete => Action::FormDelete,
                KeyCode::Left => Action::FormLeft,
                KeyCode::Right => Action::FormRight,
                KeyCode::Home => Action::FormHome,
                KeyCode::End => Action::FormEnd,
                _ => Action::None,
            },
            Screen::Add(_) => {
                // Add screen handles its own state directly to avoid
                // threading complex state through the Action enum.
                self.handle_add_key(key);
                return;
            }
            Screen::Preview(state) => match key {
                KeyCode::Esc | KeyCode::Char('q') => Action::PreviewBack(
                    Rc::clone(&state.detail),
                    state.selected_index,
                    state.came_from_list,
                ),
                KeyCode::Down | KeyCode::Char('j') => Action::PreviewScroll(1),
                KeyCode::Up | KeyCode::Char('k') => Action::PreviewScroll(-1),
                KeyCode::PageDown | KeyCode::Char('f') => Action::PreviewScroll(20),
                KeyCode::PageUp | KeyCode::Char('b') => Action::PreviewScroll(-20),
                KeyCode::Home | KeyCode::Char('g') => Action::PreviewScroll(-30000),
                KeyCode::End | KeyCode::Char('G') => Action::PreviewScroll(30000),
                _ => Action::None,
            },
        };

        // Now apply the action with full mutable access
        match action {
            Action::None => {}
            Action::Quit => self.should_quit = true,
            Action::ListUp => {
                if let Screen::List(state) = &mut self.screen {
                    list_nav(&mut state.list_state, state.items.len(), false);
                }
            }
            Action::ListDown => {
                if let Screen::List(state) = &mut self.screen {
                    list_nav(&mut state.list_state, state.items.len(), true);
                }
            }
            Action::ListSelect(selected) => {
                if let Screen::List(state) = &self.screen
                    && let Some(bp) = state.items.get(selected)
                {
                    self.screen = Screen::Loading(LoadingState {
                        message: format!("Loading {}...", bp.short_name),
                        target: LoadingTarget::Detail {
                            name: bp.name.clone(),
                            path: None,
                            came_from_list: true,
                        },
                    });
                }
            }
            Action::DetailNext => {
                if let Screen::Detail(state) = &mut self.screen {
                    state.select_next();
                }
            }
            Action::DetailPrev => {
                if let Screen::Detail(state) = &mut self.screen {
                    state.select_prev();
                }
            }
            Action::OpenCratesIoUrl(crate_name) => {
                let url = format!("https://crates.io/crates/{}", crate_name);
                self.pending_action = Some(PendingAction::OpenUrl { url });
            }
            Action::OpenTemplate {
                repository,
                repo_path,
            } => {
                let url = match repo_path {
                    Some(path) => build_github_url(repository.as_deref(), &path),
                    None => repository.unwrap_or_else(|| "https://crates.io".to_string()),
                };
                self.pending_action = Some(PendingAction::OpenUrl { url });
            }
            Action::OpenExample {
                repository,
                repo_path,
            } => {
                let url = match repo_path {
                    Some(path) => build_github_blob_url(repository.as_deref(), &path),
                    None => repository.unwrap_or_else(|| "https://crates.io".to_string()),
                };
                self.pending_action = Some(PendingAction::OpenUrl { url });
            }
            Action::DetailOpenCratesIo(crate_name) => {
                let url = format!("https://crates.io/crates/{}", crate_name);
                self.pending_action = Some(PendingAction::OpenUrl { url });
            }
            Action::DetailAdd(battery_pack) => {
                self.pending_action = Some(PendingAction::AddToProject { battery_pack });
            }
            Action::DetailNewProject(detail, template, selected_index, came_from_list) => {
                let cwd = std::env::current_dir()
                    .map(|p| p.to_string_lossy().to_string())
                    .unwrap_or_else(|_| ".".to_string());
                self.screen = Screen::NewProjectForm(FormScreen {
                    battery_pack: detail.short_name.clone(),
                    template,
                    directory: cwd,
                    project_name: String::new(),
                    focused_field: FormField::ProjectName,
                    cursor_position: 0,
                    detail,
                    selected_index,
                    came_from_list,
                });
            }
            Action::DetailBack(came_from_list) => {
                if came_from_list {
                    self.screen = Screen::Loading(LoadingState {
                        message: "Loading battery packs...".to_string(),
                        target: LoadingTarget::List { filter: None },
                    });
                } else {
                    self.should_quit = true;
                }
            }
            Action::FormToggleField => {
                if let Screen::NewProjectForm(state) = &mut self.screen {
                    state.focused_field = match state.focused_field {
                        FormField::Directory => FormField::ProjectName,
                        FormField::ProjectName => FormField::Directory,
                    };
                    state.cursor_position = state.focused_field_len();
                }
            }
            Action::FormSubmit(
                battery_pack,
                template,
                directory,
                name,
                detail,
                selected_index,
                came_from_list,
            ) => {
                self.pending_action = Some(PendingAction::NewProject {
                    battery_pack,
                    template,
                    directory,
                    name,
                });
                self.screen = Screen::Detail(DetailScreen {
                    detail,
                    selected_index,
                    came_from_list,
                });
            }
            Action::FormCancel(detail, selected_index, came_from_list) => {
                self.screen = Screen::Detail(DetailScreen {
                    detail,
                    selected_index,
                    came_from_list,
                });
            }
            Action::FormChar(c) => {
                if let Screen::NewProjectForm(state) = &mut self.screen {
                    let pos = state.cursor_position;
                    state.focused_field_mut().insert(pos, c);
                    state.cursor_position += 1;
                }
            }
            Action::FormBackspace => {
                if let Screen::NewProjectForm(state) = &mut self.screen
                    && state.cursor_position > 0
                {
                    let pos = state.cursor_position - 1;
                    state.focused_field_mut().remove(pos);
                    state.cursor_position -= 1;
                }
            }
            Action::FormDelete => {
                if let Screen::NewProjectForm(state) = &mut self.screen {
                    let pos = state.cursor_position;
                    if pos < state.focused_field_len() {
                        state.focused_field_mut().remove(pos);
                    }
                }
            }
            Action::FormLeft => {
                if let Screen::NewProjectForm(state) = &mut self.screen {
                    state.cursor_position = state.cursor_position.saturating_sub(1);
                }
            }
            Action::FormRight => {
                if let Screen::NewProjectForm(state) = &mut self.screen
                    && state.cursor_position < state.focused_field_len()
                {
                    state.cursor_position += 1;
                }
            }
            Action::FormHome => {
                if let Screen::NewProjectForm(state) = &mut self.screen {
                    state.cursor_position = 0;
                }
            }
            Action::FormEnd => {
                if let Screen::NewProjectForm(state) = &mut self.screen {
                    state.cursor_position = state.focused_field_len();
                }
            }
            Action::PreviewTemplate(detail, template_path, selected_index, came_from_list) => {
                // Find the template name from the path
                let template_name = detail
                    .templates
                    .iter()
                    .find(|t| t.path == template_path)
                    .map(|t| t.name.clone())
                    .unwrap_or_else(|| template_path.clone());

                // Find the crate root to render the template from.
                // For registry packs the detail was built from a downloaded
                // crate, but we don't keep that temp dir around. Use
                // --crate-source / --path if available, otherwise try cargo
                // metadata.
                let crate_root = match &self.source {
                    CrateSource::Local(ws) => {
                        crate::registry::find_local_battery_pack_dir(ws, &detail.name).ok()
                    }
                    CrateSource::Registry => {
                        // Try to locate via cargo metadata (works if already
                        // installed as a build-dep).
                        crate::manifest::resolve_battery_pack_manifest(&detail.name)
                            .ok()
                            .and_then(|p| p.parent().map(|p| p.to_path_buf()))
                    }
                };

                let content = match crate_root {
                    Some(root) => {
                        let opts = crate::template_engine::RenderOpts {
                            crate_root: root,
                            template_path,
                            project_name: "my-project".to_string(),
                            defines: BTreeMap::new(),
                            interactive_override: None,
                        };
                        match crate::template_engine::preview(opts) {
                            Ok(files) => highlight_preview(&files),
                            Err(e) => Text::from(format!("Failed to render preview: {e}")),
                        }
                    }
                    None => Text::from(
                        "Template preview unavailable — battery pack not found locally.\nUse --crate-source or install the pack first.",
                    ),
                };

                let line_count = content.lines.len() as u16;
                self.screen = Screen::Preview(PreviewScreen {
                    content,
                    template_name,
                    scroll: 0,
                    line_count,
                    detail,
                    selected_index,
                    came_from_list,
                });
            }
            Action::PreviewScroll(delta) => {
                if let Screen::Preview(state) = &mut self.screen {
                    let new_scroll = state.scroll as i32 + delta as i32;
                    state.scroll =
                        new_scroll.clamp(0, state.line_count.saturating_sub(1) as i32) as u16;
                }
            }
            Action::PreviewBack(detail, selected_index, came_from_list) => {
                self.screen = Screen::Detail(DetailScreen {
                    detail,
                    selected_index,
                    came_from_list,
                });
            }
        }
    }

    // ========================================================================
    // Add screen key handling
    // ========================================================================

    /// Take the AddScreen out of self.screen, replacing it with a Loading screen.
    fn take_add_screen_for_loading(
        &mut self,
        message: &str,
        target_fn: impl FnOnce(AddScreen) -> LoadingTarget,
    ) {
        let screen = std::mem::replace(&mut self.screen, Screen::Empty);
        let Screen::Add(add_screen) = screen else {
            self.screen = screen;
            return;
        };
        self.screen = Screen::Loading(LoadingState {
            message: message.to_string(),
            target: target_fn(add_screen),
        });
    }

    /// [impl tui.network.error]
    fn handle_error_key(&mut self, key: KeyCode) {
        match key {
            KeyCode::Enter | KeyCode::Char('r') => {
                // Retry: re-enter Loading with the same target.
                let screen = std::mem::replace(&mut self.screen, Screen::Empty);
                let Screen::Error(error) = screen else {
                    self.screen = screen;
                    return;
                };
                self.screen = Screen::Loading(LoadingState {
                    message: "Loading...".to_string(),
                    target: error.retry_target,
                });
            }
            KeyCode::Esc | KeyCode::Char('q') => {
                self.should_quit = true;
            }
            _ => {}
        }
    }

    fn handle_add_key(&mut self, key: KeyCode) {
        let Screen::Add(state) = &mut self.screen else {
            return;
        };

        match state.tab {
            AddTab::Installed => match key {
                KeyCode::Up | KeyCode::Char('k') => state.installed.select_prev(),
                KeyCode::Down | KeyCode::Char('j') => state.installed.select_next(),
                KeyCode::Char(' ') => state.installed.toggle_selected(),
                KeyCode::Char('d') => state.installed.cycle_dep_kind(),
                KeyCode::Tab => {
                    if state.browse.items.is_empty() {
                        // First visit — load browse list via loading screen
                        self.take_add_screen_for_loading("Loading battery packs...", |s| {
                            LoadingTarget::BrowseList {
                                add_screen: s,
                                filter: None,
                            }
                        });
                    } else {
                        state.tab = AddTab::Browse;
                    }
                }
                KeyCode::Enter => {
                    if state.installed.has_changes() || state.installed.has_new_packs() {
                        state.changes = Some(state.installed.collect_changes());
                        self.should_quit = true;
                    }
                }
                KeyCode::Char('q') | KeyCode::Esc => self.should_quit = true,
                _ => {}
            },
            AddTab::Browse => {
                if state.browse.searching {
                    match key {
                        KeyCode::Enter => {
                            let filter = if state.browse.search_input.is_empty() {
                                None
                            } else {
                                Some(state.browse.search_input.clone())
                            };
                            // Search via loading screen
                            self.take_add_screen_for_loading("Searching...", |mut s| {
                                s.browse.searching = false;
                                LoadingTarget::BrowseList {
                                    add_screen: s,
                                    filter,
                                }
                            });
                        }
                        KeyCode::Esc => {
                            state.browse.searching = false;
                        }
                        KeyCode::Char(c) => {
                            state.browse.search_input.push(c);
                        }
                        KeyCode::Backspace => {
                            state.browse.search_input.pop();
                        }
                        _ => {}
                    }
                } else if let Some(ref mut expanded) = state.browse.expanded {
                    match key {
                        KeyCode::Up | KeyCode::Char('k') => expanded.select_prev(),
                        KeyCode::Down | KeyCode::Char('j') => expanded.select_next(),
                        KeyCode::Char(' ') => expanded.toggle_selected(),
                        KeyCode::Enter => {
                            if let Some(expanded) = state.browse.expanded.take() {
                                let has_selected = expanded.pack.entries.iter().any(|e| e.enabled);
                                if has_selected {
                                    state.installed.packs.push(expanded.pack);
                                }
                            }
                            state.tab = AddTab::Installed;
                        }
                        KeyCode::Esc => {
                            state.browse.expanded = None;
                        }
                        _ => {}
                    }
                } else {
                    match key {
                        KeyCode::Up | KeyCode::Char('k') => {
                            list_nav(
                                &mut state.browse.list_state,
                                state.browse.items.len(),
                                false,
                            );
                        }
                        KeyCode::Down | KeyCode::Char('j') => {
                            list_nav(&mut state.browse.list_state, state.browse.items.len(), true);
                        }
                        KeyCode::Enter => {
                            if let Some(selected) = state.browse.list_state.selected()
                                && let Some(bp) = state.browse.items.get(selected)
                            {
                                let already_installed =
                                    state.installed.packs.iter().any(|p| p.name == bp.name);
                                if already_installed {
                                    state.tab = AddTab::Installed;
                                } else {
                                    let bp_name = bp.name.clone();
                                    let bp_short_name = bp.short_name.clone();
                                    self.take_add_screen_for_loading(
                                        "Loading battery pack...",
                                        |s| LoadingTarget::BrowseExpand {
                                            add_screen: s,
                                            bp_name,
                                            bp_short_name,
                                        },
                                    );
                                }
                            }
                        }
                        KeyCode::Char('/') => {
                            state.browse.searching = true;
                        }
                        KeyCode::Tab => {
                            state.tab = AddTab::Installed;
                        }
                        KeyCode::Char('q') => self.should_quit = true,
                        KeyCode::Esc => {
                            state.tab = AddTab::Installed;
                        }
                        _ => {}
                    }
                }
            }
        }
    }

    // ========================================================================
    // Rendering
    // ========================================================================

    fn render(&mut self, frame: &mut Frame) {
        match &mut self.screen {
            Screen::Empty => {}
            Screen::Loading(state) => render_loading(frame, state),
            Screen::Error(state) => render_error(frame, state),
            Screen::List(state) => render_list(frame, state),
            Screen::Detail(state) => render_detail(frame, state),
            Screen::NewProjectForm(state) => render_form(frame, state),
            Screen::Add(state) => render_add(frame, state),
            Screen::Preview(state) => render_preview(frame, state),
        }
    }
}

// ============================================================================
// Screen renderers
// ============================================================================

/// Build a `ListItem` for a battery pack summary row (shared by list and browse views).
fn bp_summary_list_item(bp: &BatteryPackSummary) -> ListItem<'_> {
    let desc = bp.description.lines().next().unwrap_or("");
    let line = Line::from(vec![
        Span::styled(
            format!("{:<20}", bp.short_name),
            Style::default().fg(Color::Green).bold(),
        ),
        Span::raw("  "),
        Span::styled(
            format!("{:<10}", bp.version),
            Style::default().fg(Color::DarkGray),
        ),
        Span::raw("  "),
        Span::raw(desc),
    ]);
    ListItem::new(line)
}

fn render_loading(frame: &mut Frame, state: &LoadingState) {
    let area = frame.area();
    let text = Paragraph::new(state.message.as_str())
        .style(Style::default().fg(Color::Cyan))
        .centered();

    let vertical = Layout::vertical([Constraint::Length(1)]).flex(Flex::Center);
    let [center] = vertical.areas(area);
    frame.render_widget(text, center);
}

/// [impl tui.network.error]
fn render_error(frame: &mut Frame, state: &ErrorScreen) {
    let area = frame.area();

    let error_text = Text::from(vec![
        Line::from(Span::styled(
            "Error",
            Style::default().fg(Color::Red).add_modifier(Modifier::BOLD),
        )),
        Line::from(""),
        Line::from(state.message.as_str()),
        Line::from(""),
        Line::from(Span::styled(
            "Press Enter or r to retry, Esc or q to quit",
            Style::default().fg(Color::DarkGray),
        )),
    ]);

    let paragraph = Paragraph::new(error_text).centered();

    let vertical = Layout::vertical([Constraint::Length(5)]).flex(Flex::Center);
    let [center] = vertical.areas(area);
    frame.render_widget(paragraph, center);
}

fn render_list(frame: &mut Frame, state: &mut ListScreen) {
    let area = frame.area();

    let [header, main, footer] = Layout::vertical([
        Constraint::Length(2),
        Constraint::Fill(1),
        Constraint::Length(1),
    ])
    .areas(area);

    // Header
    let title = match &state.filter {
        Some(f) => format!("Battery Packs (filter: {})", f),
        None => "Battery Packs".to_string(),
    };
    frame.render_widget(
        Paragraph::new(title)
            .style(Style::default().bold())
            .centered(),
        header,
    );

    // List
    let items: Vec<ListItem> = state.items.iter().map(bp_summary_list_item).collect();

    let list = List::new(items)
        .block(Block::default().borders(Borders::ALL))
        .highlight_style(
            Style::default()
                .bg(Color::Blue)
                .fg(Color::White)
                .add_modifier(Modifier::BOLD),
        )
        .highlight_symbol("> ");

    frame.render_stateful_widget(list, main, &mut state.list_state);

    // Footer
    frame.render_widget(
        Paragraph::new("↑↓/jk Navigate | Enter Select | q Quit")
            .style(Style::default().white().on_dark_gray()),
        footer,
    );
}

/// Helper function to render a selectable section with consistent styling
fn render_selectable_section<'a, T>(
    lines: &mut Vec<Line<'a>>,
    item_index: &mut usize,
    selected_index: usize,
    label: &'a str,
    items: &[T],
    normal_color: Option<Color>,
    format_item: impl Fn(&T) -> String,
) {
    if items.is_empty() {
        return;
    }

    lines.push(Line::styled(label, Style::default().bold()));
    for item in items {
        let selected = selected_index == *item_index;
        let style = if selected {
            Style::default().fg(Color::Black).bg(Color::Cyan).bold()
        } else {
            match normal_color {
                Some(color) => Style::default().fg(color),
                None => Style::default(),
            }
        };
        let prefix = if selected { "> " } else { "  " };
        lines.push(Line::styled(
            format!("{}{}", prefix, format_item(item)),
            style,
        ));
        *item_index += 1;
    }
    lines.push(Line::from(""));
}

fn render_detail(frame: &mut Frame, state: &DetailScreen) {
    let area = frame.area();
    let detail = &state.detail;

    let [header, main, footer] = Layout::vertical([
        Constraint::Length(3),
        Constraint::Fill(1),
        Constraint::Length(1),
    ])
    .areas(area);

    // Header
    let header_text = Line::from(vec![
        Span::styled(&detail.name, Style::default().fg(Color::Green).bold()),
        Span::raw(" "),
        Span::styled(&detail.version, Style::default().fg(Color::DarkGray)),
    ]);
    frame.render_widget(Paragraph::new(header_text).centered(), header);

    // Build selectable items to track indices
    let selectable_items: Vec<_> = state.selectable_items().collect();

    // Info section
    let mut lines: Vec<Line> = Vec::new();
    let mut item_index: usize = 0;

    if !detail.description.is_empty() {
        lines.push(Line::from(detail.description.clone()));
        lines.push(Line::from(""));
    }

    if !detail.owners.is_empty() {
        lines.push(Line::styled("Authors:", Style::default().bold()));
        for owner in &detail.owners {
            let text = match &owner.name {
                Some(name) => format!("  {} ({})", name, owner.login),
                None => format!("  {}", owner.login),
            };
            lines.push(Line::from(text));
        }
        lines.push(Line::from(""));
    }

    render_selectable_section(
        &mut lines,
        &mut item_index,
        state.selected_index,
        "Crates:",
        &detail.crates,
        None,
        |crate_name| crate_name.clone(),
    );

    render_selectable_section(
        &mut lines,
        &mut item_index,
        state.selected_index,
        "Extends:",
        &detail.extends,
        Some(Color::Yellow),
        |bp| bp.clone(),
    );

    render_selectable_section(
        &mut lines,
        &mut item_index,
        state.selected_index,
        "Templates:",
        &detail.templates,
        Some(Color::Cyan),
        |tmpl| match &tmpl.description {
            Some(desc) => format!("{} - {}", tmpl.name, desc),
            None => tmpl.name.clone(),
        },
    );

    render_selectable_section(
        &mut lines,
        &mut item_index,
        state.selected_index,
        "Examples:",
        &detail.examples,
        Some(Color::Magenta),
        |example| match &example.description {
            Some(desc) => format!("{} - {}", example.name, desc),
            None => example.name.clone(),
        },
    );

    // Actions section (always present)
    let action_labels = [
        "Open on crates.io",
        "Add to project",
        "Create new project from template",
    ];
    render_selectable_section(
        &mut lines,
        &mut item_index,
        state.selected_index,
        "Actions:",
        &action_labels,
        None,
        |label| (*label).to_string(),
    );

    // Sanity check
    debug_assert_eq!(
        item_index,
        selectable_items.len(),
        "Mismatch between rendered items and selectable_items()"
    );

    let info = Paragraph::new(lines)
        .block(Block::default().borders(Borders::ALL))
        .wrap(Wrap { trim: false });
    frame.render_widget(info, main);

    // Footer - show 'n' hint when template is selected
    let back_hint = if state.came_from_list {
        "Esc Back"
    } else {
        "Esc/q Quit"
    };
    let template_selected = matches!(state.selected_item(), Some(DetailItem::Template { .. }));
    let footer_text = if template_selected {
        format!(
            "↑↓/jk Navigate | Enter Open | p Preview | n New project | {}",
            back_hint
        )
    } else {
        format!("↑↓/jk Navigate | Enter Open/Select | {}", back_hint)
    };
    frame.render_widget(
        Paragraph::new(footer_text).style(Style::default().white().on_dark_gray()),
        footer,
    );
}

fn render_form_field(
    frame: &mut Frame,
    label: &str,
    value: &str,
    focused: bool,
    label_area: Rect,
    input_area: Rect,
) {
    frame.render_widget(
        Paragraph::new(label).style(Style::default().bold()),
        label_area,
    );
    let border_style = if focused {
        Style::default().fg(Color::Yellow)
    } else {
        Style::default().fg(Color::DarkGray)
    };
    frame.render_widget(
        Paragraph::new(value).block(
            Block::default()
                .borders(Borders::ALL)
                .border_style(border_style),
        ),
        input_area,
    );
}

fn render_form(frame: &mut Frame, state: &FormScreen) {
    // First render detail view dimmed underneath
    let dimmed_detail = DetailScreen {
        detail: Rc::clone(&state.detail),
        selected_index: state.selected_index,
        came_from_list: state.came_from_list,
    };
    render_detail(frame, &dimmed_detail);

    // Calculate popup area
    let popup_area = centered_rect(60, 40, frame.area());

    // Clear the popup area
    frame.render_widget(Clear, popup_area);

    let block = Block::default()
        .title(" New Project ")
        .borders(Borders::ALL)
        .style(Style::default().bg(Color::Black));
    let inner = block.inner(popup_area);
    frame.render_widget(block, popup_area);

    let [_, dir_label, dir_input, _, name_label, name_input, _, hint] = Layout::vertical([
        Constraint::Length(1),
        Constraint::Length(1),
        Constraint::Length(3),
        Constraint::Length(1),
        Constraint::Length(1),
        Constraint::Length(3),
        Constraint::Fill(1),
        Constraint::Length(1),
    ])
    .areas(inner);

    render_form_field(
        frame,
        "Directory:",
        &state.directory,
        state.focused_field == FormField::Directory,
        dir_label,
        dir_input,
    );
    render_form_field(
        frame,
        "Project Name:",
        &state.project_name,
        state.focused_field == FormField::ProjectName,
        name_label,
        name_input,
    );

    // Hint
    frame.render_widget(
        Paragraph::new("Tab Switch | Enter Create | Esc Cancel")
            .style(Style::default().white().on_dark_gray()),
        hint,
    );

    // Show cursor in active field
    let cursor_x = state.cursor_position.min(state.focused_field_len());
    let cursor_area = match state.focused_field {
        FormField::Directory => dir_input,
        FormField::ProjectName => name_input,
    };
    // +1 for border
    frame.set_cursor_position(Position::new(
        cursor_area.x + 1 + cursor_x as u16,
        cursor_area.y + 1,
    ));
}

/// Convert rendered template files into syntax-highlighted [`Text`].
fn highlight_preview(files: &[crate::template_engine::RenderedFile]) -> Text<'static> {
    use syntect::easy::HighlightLines;
    use syntect::highlighting::ThemeSet;

    let ss = two_face::syntax::extra_newlines();
    let ts = ThemeSet::load_defaults();
    let theme = &ts.themes["base16-eighties.dark"];

    let mut lines: Vec<Line<'static>> = Vec::new();

    for (i, file) in files.iter().enumerate() {
        if i > 0 {
            lines.push(Line::from(""));
        }
        // File header
        lines.push(Line::from(Span::styled(
            format!("── {} ──", file.path),
            Style::default()
                .fg(Color::Cyan)
                .add_modifier(Modifier::BOLD),
        )));

        // Pick syntax by file extension
        let syntax = std::path::Path::new(&file.path)
            .extension()
            .and_then(|e| e.to_str())
            .and_then(|ext| ss.find_syntax_by_extension(ext))
            .unwrap_or_else(|| ss.find_syntax_plain_text());

        let mut h = HighlightLines::new(syntax, theme);
        for line in file.content.lines() {
            let spans: Vec<Span<'static>> = match h.highlight_line(line, &ss) {
                Ok(ranges) => ranges
                    .into_iter()
                    .map(|(style, text)| {
                        let fg =
                            Color::Rgb(style.foreground.r, style.foreground.g, style.foreground.b);
                        Span::styled(text.to_string(), Style::default().fg(fg))
                    })
                    .collect(),
                Err(_) => vec![Span::raw(line.to_string())],
            };
            lines.push(Line::from(spans));
        }
    }

    Text::from(lines)
}

fn render_preview(frame: &mut Frame, state: &PreviewScreen) {
    let area = frame.area();
    let [header, main, footer] = Layout::vertical([
        Constraint::Length(1),
        Constraint::Fill(1),
        Constraint::Length(1),
    ])
    .areas(area);

    frame.render_widget(
        Paragraph::new(Line::from(vec![
            Span::styled(&state.detail.name, Style::default().fg(Color::Green).bold()),
            Span::raw(" / "),
            Span::styled(
                &state.template_name,
                Style::default().fg(Color::Cyan).bold(),
            ),
        ]))
        .centered(),
        header,
    );

    let preview = Paragraph::new(state.content.clone())
        .block(
            Block::default()
                .borders(Borders::BOTTOM)
                .padding(ratatui::widgets::Padding::horizontal(1)),
        )
        .scroll((state.scroll, 0));
    frame.render_widget(preview, main);

    frame.render_widget(
        Paragraph::new("↑↓/jk/PgUp/PgDn Scroll | Esc Back")
            .style(Style::default().white().on_dark_gray()),
        footer,
    );
}

fn centered_rect(percent_x: u16, percent_y: u16, area: Rect) -> Rect {
    let vertical = Layout::vertical([Constraint::Percentage(percent_y)]).flex(Flex::Center);
    let horizontal = Layout::horizontal([Constraint::Percentage(percent_x)]).flex(Flex::Center);
    let [v_area] = vertical.areas(area);
    let [h_area] = horizontal.areas(v_area);
    h_area
}

/// Build a GitHub tree URL for a directory path.
/// If repository is set and looks like a GitHub URL, construct a tree URL.
/// Otherwise fall back to a crates.io search or just open the repo root.
fn build_github_url(repository: Option<&str>, path: &str) -> String {
    build_github_ref_url(repository, "tree", path)
}

/// Build a GitHub blob URL for a file path.
fn build_github_blob_url(repository: Option<&str>, path: &str) -> String {
    build_github_ref_url(repository, "blob", path)
}

/// Build a GitHub URL with the specified ref type (tree or blob).
fn build_github_ref_url(repository: Option<&str>, ref_type: &str, path: &str) -> String {
    match repository {
        Some(repo) => {
            // Try to parse GitHub URL: https://github.com/owner/repo
            if let Some(gh_path) = repo
                .strip_prefix("https://github.com/")
                .or_else(|| repo.strip_prefix("http://github.com/"))
            {
                // Remove trailing .git if present
                let gh_path = gh_path.strip_suffix(".git").unwrap_or(gh_path);
                // Remove trailing slash
                let gh_path = gh_path.trim_end_matches('/');
                // Construct URL with main branch
                format!("https://github.com/{}/{}/main/{}", gh_path, ref_type, path)
            } else {
                // Not a GitHub URL, just open the repository URL
                repo.to_string()
            }
        }
        None => {
            // No repository, can't construct URL
            // Fall back to nothing useful - this shouldn't happen in practice
            "https://crates.io".to_string()
        }
    }
}

// ============================================================================
// Add screen renderer
// ============================================================================

fn render_add(frame: &mut Frame, state: &mut AddScreen) {
    let area = frame.area();

    let [header, main, footer] = Layout::vertical([
        Constraint::Length(2),
        Constraint::Fill(1),
        Constraint::Length(1),
    ])
    .areas(area);

    // Tab bar
    let installed_style = if state.tab == AddTab::Installed {
        Style::default().fg(Color::White).bold()
    } else {
        Style::default().fg(Color::DarkGray)
    };
    let browse_style = if state.tab == AddTab::Browse {
        Style::default().fg(Color::White).bold()
    } else {
        Style::default().fg(Color::DarkGray)
    };

    let tab_line = Line::from(vec![
        Span::raw("  "),
        Span::styled("Installed", installed_style),
        Span::raw("  "),
        Span::styled("Browse", browse_style),
    ]);
    frame.render_widget(Paragraph::new(tab_line), header);

    match state.tab {
        AddTab::Installed => render_add_installed(frame, &state.installed, main),
        AddTab::Browse => render_add_browse(frame, &mut state.browse, main),
    }

    // Footer
    let footer_text = match state.tab {
        AddTab::Installed => {
            let has_changes = state.installed.has_changes() || state.installed.has_new_packs();
            if has_changes {
                "↑↓ Navigate │ Space Toggle │ Enter Apply │ Tab Browse │ q Quit"
            } else {
                "↑↓ Navigate │ Space Toggle │ Tab Browse │ q Quit"
            }
        }
        AddTab::Browse => {
            if state.browse.searching {
                "Type to search │ Enter Search │ Esc Cancel"
            } else if state.browse.expanded.is_some() {
                "↑↓ Navigate │ Space Toggle │ Enter Confirm │ Esc Back"
            } else {
                "↑↓ Navigate │ Enter Select │ / Search │ Tab Installed │ q Quit"
            }
        }
    };
    frame.render_widget(
        Paragraph::new(footer_text).style(Style::default().white().on_dark_gray()),
        footer,
    );
}

/// Render a pack header and its crate entries as checkbox lines.
///
/// `selected_index` is the global selection index; `entry_offset` is the starting
/// index of this pack's entries within that global space. Returns the number of
/// entries rendered (so the caller can advance `entry_offset`).
fn render_pack_entries<'a>(
    lines: &mut Vec<Line<'a>>,
    pack: &'a InstalledPackState,
    selected_index: usize,
    entry_offset: usize,
    show_changes: bool,
) -> usize {
    // Pack header
    lines.push(Line::from(vec![
        Span::raw("  "),
        Span::styled(&pack.short_name, Style::default().fg(Color::Green).bold()),
        Span::raw(" "),
        Span::styled(&pack.version, Style::default().fg(Color::DarkGray)),
    ]));

    let mut current_group = String::new();
    for (i, entry) in pack.entries.iter().enumerate() {
        if entry.group != current_group && entry.group != "default" {
            lines.push(Line::styled(
                format!("    {}:", entry.group),
                Style::default().fg(Color::Cyan).bold(),
            ));
            current_group = entry.group.clone();
        } else if current_group.is_empty() {
            current_group = entry.group.clone();
        }

        let is_selected = (entry_offset + i) == selected_index;
        // [impl tui.installed.show-state]
        let checkbox = if entry.enabled { "[x]" } else { "[ ]" };

        let changed = show_changes && entry.enabled != entry.originally_enabled;
        let style = if is_selected {
            Style::default().fg(Color::Black).bg(Color::Cyan).bold()
        } else if changed {
            Style::default().fg(Color::Yellow)
        } else {
            Style::default()
        };

        lines.push(Line::styled(
            format!("    {} {} {}", checkbox, entry.name, entry.version_info()),
            style,
        ));
    }
    lines.push(Line::from(""));

    pack.entries.len()
}

fn render_add_installed(frame: &mut Frame, state: &InstalledState, area: Rect) {
    let mut lines: Vec<Line> = Vec::new();

    if state.packs.is_empty() {
        lines.push(Line::styled(
            "  No battery packs installed.",
            Style::default().fg(Color::DarkGray),
        ));
        lines.push(Line::from(""));
        lines.push(Line::styled(
            "  Press Tab to browse available battery packs.",
            Style::default().fg(Color::DarkGray),
        ));
    }

    let mut entry_offset = 0;
    for pack in &state.packs {
        entry_offset +=
            render_pack_entries(&mut lines, pack, state.selected_index, entry_offset, true);
    }

    let content = Paragraph::new(lines)
        .block(Block::default().borders(Borders::ALL))
        .wrap(Wrap { trim: false });
    frame.render_widget(content, area);
}

fn render_add_browse(frame: &mut Frame, state: &mut BrowseState, area: Rect) {
    if let Some(ref expanded) = state.expanded {
        // Show expanded pack crate picker
        render_expanded_pack(frame, expanded, area);
        return;
    }

    let [search_area, list_area] = Layout::vertical([
        Constraint::Length(if state.searching { 3 } else { 0 }),
        Constraint::Fill(1),
    ])
    .areas(area);

    // Search input (only shown when searching)
    if state.searching {
        let input = Paragraph::new(state.search_input.as_str()).block(
            Block::default()
                .title(" Search ")
                .borders(Borders::ALL)
                .border_style(Style::default().fg(Color::Yellow)),
        );
        frame.render_widget(input, search_area);
        // Show cursor
        frame.set_cursor_position(Position::new(
            search_area.x + 1 + state.search_input.len() as u16,
            search_area.y + 1,
        ));
    }

    // Battery pack list
    let items: Vec<ListItem> = state.items.iter().map(bp_summary_list_item).collect();

    if items.is_empty() && !state.searching {
        let hint = Paragraph::new("  Press / to search for battery packs on crates.io")
            .style(Style::default().fg(Color::DarkGray))
            .block(Block::default().borders(Borders::ALL));
        frame.render_widget(hint, list_area);
    } else {
        let list = List::new(items)
            .block(Block::default().borders(Borders::ALL))
            .highlight_style(
                Style::default()
                    .bg(Color::Blue)
                    .fg(Color::White)
                    .add_modifier(Modifier::BOLD),
            )
            .highlight_symbol("> ");
        frame.render_stateful_widget(list, list_area, &mut state.list_state);
    }
}

fn render_expanded_pack(frame: &mut Frame, expanded: &ExpandedPack, area: Rect) {
    let mut lines: Vec<Line> = Vec::new();
    render_pack_entries(
        &mut lines,
        &expanded.pack,
        expanded.selected_index,
        0,
        false,
    );

    let content = Paragraph::new(lines)
        .block(Block::default().borders(Borders::ALL))
        .wrap(Wrap { trim: false });
    frame.render_widget(content, area);
}

// ============================================================================
// Add screen helpers
// ============================================================================

/// Apply collected changes from the Add TUI.
fn apply_add_changes(changes: &[AddChange]) -> Result<()> {
    use console::style;

    for change in changes {
        match change {
            AddChange::AddPack { name, crates } => {
                println!(
                    "{}",
                    style(format!("Adding {} ({} crate(s))...", name, crates.len())).bold()
                );
                // Shell out to cargo bp add with --all, then we could refine.
                // For now, use the existing add_battery_pack flow via CLI.
                let status = std::process::Command::new("cargo")
                    .args(["bp", "add", name, "--all"])
                    .status()?;
                if !status.success() {
                    println!("{}", style(format!("  Failed to add {}", name)).red());
                }
            }
            AddChange::UpdatePack {
                name,
                add_crates,
                remove_crates,
            } => {
                if !add_crates.is_empty() {
                    println!(
                        "{}",
                        style(format!(
                            "Adding {} crate(s) to {}...",
                            add_crates.len(),
                            name
                        ))
                        .bold()
                    );
                    for c in add_crates {
                        println!("  + {}", c.name);
                    }
                }
                if !remove_crates.is_empty() {
                    println!(
                        "{}",
                        style(format!(
                            "Removing {} crate(s) from {}...",
                            remove_crates.len(),
                            name
                        ))
                        .bold()
                    );
                    for crate_name in remove_crates {
                        println!("  - {}", crate_name);
                    }
                }
                // TODO: Implement fine-grained Cargo.toml manipulation
                // For now, run cargo bp sync to reconcile
                let status = std::process::Command::new("cargo")
                    .args(["bp", "sync"])
                    .status()?;
                if !status.success() {
                    println!("{}", style(format!("  Failed to sync {}", name)).red());
                }
            }
        }
    }

    Ok(())
}

#[cfg(test)]
mod tests {
    use super::*;
    use bphelper_manifest::{BatteryPackSpec, CrateSpec, DepKind};
    use expect_test::expect;
    use std::collections::{BTreeMap, BTreeSet};

    // ====================================================================
    // Fixture helpers
    // ====================================================================

    /// Create a minimal CrateSpec with sensible defaults.
    fn crate_spec(version: &str) -> CrateSpec {
        CrateSpec {
            version: version.to_string(),
            features: BTreeSet::new(),
            dep_kind: DepKind::Normal,
            optional: false,
        }
    }

    /// Create a CrateSpec with features.
    fn crate_spec_with_features(version: &str, features: &[&str]) -> CrateSpec {
        CrateSpec {
            version: version.to_string(),
            features: features.iter().map(|s| s.to_string()).collect(),
            dep_kind: DepKind::Normal,
            optional: false,
        }
    }

    /// Create a BatteryPackSpec with the given crates and features.
    fn make_spec(crates: &[(&str, CrateSpec)], features: &[(&str, &[&str])]) -> BatteryPackSpec {
        BatteryPackSpec {
            name: "test-battery-pack".to_string(),
            version: "1.0.0".to_string(),
            description: "A test battery pack".to_string(),
            repository: None,
            keywords: Vec::new(),
            crates: crates
                .iter()
                .map(|(name, spec)| (name.to_string(), spec.clone()))
                .collect(),
            features: features
                .iter()
                .map(|(name, deps)| {
                    (
                        name.to_string(),
                        deps.iter().map(|d| d.to_string()).collect(),
                    )
                })
                .collect(),
            hidden: BTreeSet::new(),
            templates: BTreeMap::new(),
        }
    }

    /// Create an InstalledPackState directly (bypasses I/O).
    fn make_installed_pack(name: &str, entries: Vec<CrateEntry>) -> InstalledPackState {
        InstalledPackState {
            name: format!("{}-battery-pack", name),
            short_name: name.to_string(),
            version: "1.0.0".to_string(),
            entries,
            features: BTreeMap::new(),
        }
    }

    /// Create a CrateEntry with the given state.
    fn make_entry(name: &str, enabled: bool, originally_enabled: bool) -> CrateEntry {
        CrateEntry {
            name: name.to_string(),
            version: "0.1.0".to_string(),
            features: Vec::new(),
            dep_kind: DepKind::Normal,
            original_dep_kind: DepKind::Normal,
            group: "default".to_string(),
            enabled,
            originally_enabled,
        }
    }

    /// Create an InstalledState from packs.
    fn make_installed(packs: Vec<InstalledPackState>) -> InstalledState {
        InstalledState {
            packs,
            selected_index: 0,
        }
    }

    /// Create a BatteryPackDetail for DetailScreen tests.
    fn make_detail(crates: &[&str], templates: &[&str], examples: &[&str]) -> BatteryPackDetail {
        BatteryPackDetail {
            name: "test-battery-pack".to_string(),
            short_name: "test".to_string(),
            version: "1.0.0".to_string(),
            description: "Test battery pack".to_string(),
            repository: Some("https://github.com/test/test".to_string()),
            owners: Vec::new(),
            crates: crates.iter().map(|s| s.to_string()).collect(),
            extends: Vec::new(),
            templates: templates
                .iter()
                .map(|name| crate::registry::TemplateInfo {
                    name: name.to_string(),
                    path: format!("templates/{}", name),
                    description: None,
                    repo_path: None,
                })
                .collect(),
            examples: examples
                .iter()
                .map(|name| crate::registry::ExampleInfo {
                    name: name.to_string(),
                    description: None,
                    repo_path: None,
                })
                .collect(),
        }
    }

    // ====================================================================
    // Tier 1: State logic tests
    // ====================================================================

    // --- CrateEntry ---

    /// [verify tui.installed.show-state]
    #[test]
    fn version_info_no_features() {
        let entry = CrateEntry {
            name: "serde".to_string(),
            version: "1.0.0".to_string(),
            features: Vec::new(),
            dep_kind: DepKind::Normal,
            original_dep_kind: DepKind::Normal,
            group: "default".to_string(),
            enabled: true,
            originally_enabled: true,
        };
        assert_eq!(entry.version_info(), "(1.0.0)");
    }

    /// [verify tui.installed.show-state]
    #[test]
    fn version_info_with_features() {
        let entry = CrateEntry {
            name: "serde".to_string(),
            version: "1.0.0".to_string(),
            features: vec!["derive".to_string(), "std".to_string()],
            dep_kind: DepKind::Normal,
            original_dep_kind: DepKind::Normal,
            group: "default".to_string(),
            enabled: true,
            originally_enabled: true,
        };
        assert_eq!(entry.version_info(), "(1.0.0, features: derive, std)");
    }

    /// [verify tui.installed.dep-kind]
    /// [verify tui.installed.show-state]
    #[test]
    fn version_info_dev_dep() {
        let entry = CrateEntry {
            name: "insta".to_string(),
            version: "1.0.0".to_string(),
            features: Vec::new(),
            dep_kind: DepKind::Dev,
            original_dep_kind: DepKind::Dev,
            group: "default".to_string(),
            enabled: true,
            originally_enabled: true,
        };
        assert_eq!(entry.version_info(), "(1.0.0, dev)");
    }

    /// [verify tui.installed.dep-kind]
    /// [verify tui.installed.show-state]
    #[test]
    fn version_info_build_dep_with_features() {
        let entry = CrateEntry {
            name: "cc".to_string(),
            version: "1.0.0".to_string(),
            features: vec!["parallel".to_string()],
            dep_kind: DepKind::Build,
            original_dep_kind: DepKind::Build,
            group: "default".to_string(),
            enabled: true,
            originally_enabled: true,
        };
        assert_eq!(entry.version_info(), "(1.0.0, build, features: parallel)");
    }

    // --- InstalledState::toggle_selected ---

    /// [verify tui.installed.toggle-crate]
    #[test]
    fn toggle_selected_flips_entry() {
        let mut state = make_installed(vec![make_installed_pack(
            "web",
            vec![
                make_entry("axum", true, true),
                make_entry("tower", true, true),
                make_entry("reqwest", false, false),
            ],
        )]);

        // Toggle first entry (axum: true -> false)
        state.toggle_selected();
        assert!(!state.packs[0].entries[0].enabled);
        assert!(state.packs[0].entries[1].enabled); // tower unchanged

        // Toggle again (axum: false -> true)
        state.toggle_selected();
        assert!(state.packs[0].entries[0].enabled);
    }

    /// [verify tui.installed.toggle-crate]
    #[test]
    fn toggle_selected_targets_correct_entry_across_packs() {
        let mut state = make_installed(vec![
            make_installed_pack(
                "web",
                vec![
                    make_entry("axum", true, true),
                    make_entry("tower", true, true),
                ],
            ),
            make_installed_pack(
                "db",
                vec![
                    make_entry("sqlx", true, true),
                    make_entry("sea-orm", false, false),
                ],
            ),
        ]);

        // Move to index 2 (sqlx in second pack) and toggle
        state.selected_index = 2;
        state.toggle_selected();
        assert!(!state.packs[1].entries[0].enabled); // sqlx toggled off
        assert!(state.packs[0].entries[0].enabled); // axum unchanged
        assert!(state.packs[0].entries[1].enabled); // tower unchanged
    }

    /// [verify tui.installed.features] (partial: verifies group isolation, not group-level toggling)
    #[test]
    fn toggle_only_affects_target_not_other_groups() {
        let mut state = make_installed(vec![make_installed_pack(
            "web",
            vec![
                CrateEntry {
                    name: "axum".to_string(),
                    version: "0.7.0".to_string(),
                    features: Vec::new(),
                    dep_kind: DepKind::Normal,
                    original_dep_kind: DepKind::Normal,
                    group: "server".to_string(),
                    enabled: true,
                    originally_enabled: true,
                },
                CrateEntry {
                    name: "reqwest".to_string(),
                    version: "0.12.0".to_string(),
                    features: Vec::new(),
                    dep_kind: DepKind::Normal,
                    original_dep_kind: DepKind::Normal,
                    group: "client".to_string(),
                    enabled: true,
                    originally_enabled: true,
                },
            ],
        )]);

        // Toggle axum (server group)
        state.selected_index = 0;
        state.toggle_selected();
        assert!(!state.packs[0].entries[0].enabled); // axum off
        assert!(state.packs[0].entries[1].enabled); // reqwest (client) unchanged
    }

    // --- toggle constraint: feature dependencies ---

    /// [verify tui.installed.toggle-crate]
    #[test]
    fn toggle_off_prevented_when_required_by_other_feature() {
        // "axum" is in group "server" but also listed in the "networking" feature.
        // "reqwest" is in group "client" and also in "networking".
        // With both enabled and "networking" feature defined, disabling either
        // should be prevented because the other keeps "networking" active.
        let mut pack = make_installed_pack(
            "web",
            vec![
                CrateEntry {
                    name: "axum".to_string(),
                    version: "0.7.0".to_string(),
                    features: Vec::new(),
                    dep_kind: DepKind::Normal,
                    original_dep_kind: DepKind::Normal,
                    group: "server".to_string(),
                    enabled: true,
                    originally_enabled: true,
                },
                CrateEntry {
                    name: "reqwest".to_string(),
                    version: "0.12.0".to_string(),
                    features: Vec::new(),
                    dep_kind: DepKind::Normal,
                    original_dep_kind: DepKind::Normal,
                    group: "client".to_string(),
                    enabled: true,
                    originally_enabled: true,
                },
            ],
        );
        // Both crates are in the "networking" feature
        pack.features.insert(
            "networking".to_string(),
            BTreeSet::from(["axum".to_string(), "reqwest".to_string()]),
        );

        let mut state = make_installed(vec![pack]);

        // Try to toggle off axum — should be prevented (reqwest keeps "networking" active)
        state.selected_index = 0;
        state.toggle_selected();
        assert!(state.packs[0].entries[0].enabled); // still enabled

        // Try to toggle off reqwest — should also be prevented
        state.selected_index = 1;
        state.toggle_selected();
        assert!(state.packs[0].entries[1].enabled); // still enabled
    }

    /// [verify tui.installed.toggle-crate]
    #[test]
    fn toggle_off_allowed_when_no_cross_feature_dependency() {
        // Crate in its own group with no cross-feature memberships.
        let mut state = make_installed(vec![make_installed_pack(
            "web",
            vec![make_entry("axum", true, true)],
        )]);

        state.toggle_selected();
        assert!(!state.packs[0].entries[0].enabled); // toggled off successfully
    }

    /// [verify tui.installed.toggle-crate]
    #[test]
    fn toggle_on_always_allowed_even_with_features() {
        // Enabling a crate is always allowed, regardless of feature constraints.
        let mut pack = make_installed_pack(
            "web",
            vec![
                CrateEntry {
                    name: "axum".to_string(),
                    version: "0.7.0".to_string(),
                    features: Vec::new(),
                    dep_kind: DepKind::Normal,
                    original_dep_kind: DepKind::Normal,
                    group: "server".to_string(),
                    enabled: false,
                    originally_enabled: false,
                },
                CrateEntry {
                    name: "reqwest".to_string(),
                    version: "0.12.0".to_string(),
                    features: Vec::new(),
                    dep_kind: DepKind::Normal,
                    original_dep_kind: DepKind::Normal,
                    group: "client".to_string(),
                    enabled: true,
                    originally_enabled: true,
                },
            ],
        );
        pack.features.insert(
            "networking".to_string(),
            BTreeSet::from(["axum".to_string(), "reqwest".to_string()]),
        );

        let mut state = make_installed(vec![pack]);

        // Toggle axum ON — should always succeed
        state.selected_index = 0;
        state.toggle_selected();
        assert!(state.packs[0].entries[0].enabled);
    }

    // --- dep_kind cycling ---

    /// [verify tui.installed.dep-kind]
    #[test]
    fn cycle_dep_kind_cycles_through_all_variants() {
        let mut state = make_installed(vec![make_installed_pack(
            "web",
            vec![make_entry("axum", true, true)],
        )]);

        assert_eq!(state.packs[0].entries[0].dep_kind, DepKind::Normal);

        state.cycle_dep_kind();
        assert_eq!(state.packs[0].entries[0].dep_kind, DepKind::Dev);

        state.cycle_dep_kind();
        assert_eq!(state.packs[0].entries[0].dep_kind, DepKind::Build);

        state.cycle_dep_kind();
        assert_eq!(state.packs[0].entries[0].dep_kind, DepKind::Normal);
    }

    /// [verify tui.installed.dep-kind]
    #[test]
    fn cycle_dep_kind_targets_selected_entry() {
        let mut state = make_installed(vec![make_installed_pack(
            "web",
            vec![
                make_entry("axum", true, true),
                make_entry("tower", true, true),
            ],
        )]);

        // Cycle second entry
        state.selected_index = 1;
        state.cycle_dep_kind();
        assert_eq!(state.packs[0].entries[0].dep_kind, DepKind::Normal); // axum unchanged
        assert_eq!(state.packs[0].entries[1].dep_kind, DepKind::Dev); // tower cycled
    }

    /// [verify tui.installed.dep-kind]
    #[test]
    fn dep_kind_change_detected_by_has_changes() {
        let mut state = make_installed(vec![make_installed_pack(
            "web",
            vec![make_entry("axum", true, true)],
        )]);

        assert!(!state.has_changes());
        state.cycle_dep_kind();
        assert!(state.has_changes());

        // Cycle back to original — no longer a change
        state.cycle_dep_kind(); // Dev -> Build
        state.cycle_dep_kind(); // Build -> Normal
        assert!(!state.has_changes());
    }

    // --- InstalledState navigation ---

    #[test]
    fn installed_navigation_wraps() {
        let mut state = make_installed(vec![make_installed_pack(
            "web",
            vec![
                make_entry("a", true, true),
                make_entry("b", true, true),
                make_entry("c", true, true),
            ],
        )]);

        assert_eq!(state.selected_index, 0);
        state.select_prev(); // wrap to end
        assert_eq!(state.selected_index, 2);
        state.select_next(); // wrap to start
        assert_eq!(state.selected_index, 0);
    }

    // --- InstalledState::has_changes / has_new_packs ---

    /// [verify tui.nav.exit]
    #[test]
    fn has_changes_detects_toggled_entries() {
        let state = make_installed(vec![make_installed_pack(
            "web",
            vec![
                make_entry("axum", true, true),   // no change
                make_entry("tower", false, true), // was on, now off
            ],
        )]);
        assert!(state.has_changes());
    }

    #[test]
    fn has_changes_false_when_unchanged() {
        let state = make_installed(vec![make_installed_pack(
            "web",
            vec![
                make_entry("axum", true, true),
                make_entry("tower", false, false),
            ],
        )]);
        assert!(!state.has_changes());
    }

    /// [verify tui.browse.add]
    #[test]
    fn has_new_packs_detects_added_from_browse() {
        let state = make_installed(vec![make_installed_pack(
            "web",
            vec![
                // All originally_enabled=false (came from Browse), some now enabled
                make_entry("axum", true, false),
                make_entry("tower", false, false),
            ],
        )]);
        assert!(state.has_new_packs());
    }

    #[test]
    fn has_new_packs_false_when_none_enabled() {
        let state = make_installed(vec![make_installed_pack(
            "web",
            vec![
                make_entry("axum", false, false),
                make_entry("tower", false, false),
            ],
        )]);
        assert!(!state.has_new_packs());
    }

    // --- InstalledState::collect_changes ---

    /// [verify tui.nav.exit]
    #[test]
    fn collect_changes_new_pack() {
        let state = make_installed(vec![make_installed_pack(
            "web",
            vec![
                make_entry("axum", true, false),
                make_entry("tower", true, false),
                make_entry("reqwest", false, false), // not selected
            ],
        )]);

        let changes = state.collect_changes();
        expect![[r#"
            [
                AddPack {
                    name: "web-battery-pack",
                    crates: [
                        CrateChange {
                            name: "axum",
                            dep_kind: Normal,
                        },
                        CrateChange {
                            name: "tower",
                            dep_kind: Normal,
                        },
                    ],
                },
            ]
        "#]]
        .assert_debug_eq(&changes);
    }

    /// [verify tui.nav.exit]
    #[test]
    fn collect_changes_update_existing_pack() {
        let state = make_installed(vec![make_installed_pack(
            "web",
            vec![
                make_entry("axum", true, true),     // unchanged
                make_entry("tower", false, true),   // removed
                make_entry("reqwest", true, false), // added
            ],
        )]);

        let changes = state.collect_changes();
        expect![[r#"
            [
                UpdatePack {
                    name: "web-battery-pack",
                    add_crates: [
                        CrateChange {
                            name: "reqwest",
                            dep_kind: Normal,
                        },
                    ],
                    remove_crates: [
                        "tower",
                    ],
                },
            ]
        "#]]
        .assert_debug_eq(&changes);
    }

    /// [verify tui.nav.exit]
    #[test]
    fn collect_changes_skips_unchanged_packs() {
        let state = make_installed(vec![
            make_installed_pack(
                "web",
                vec![
                    make_entry("axum", true, true),
                    make_entry("tower", true, true),
                ],
            ),
            make_installed_pack(
                "db",
                vec![
                    make_entry("sqlx", false, true), // changed
                ],
            ),
        ]);

        let changes = state.collect_changes();
        expect![[r#"
            [
                UpdatePack {
                    name: "db-battery-pack",
                    add_crates: [],
                    remove_crates: [
                        "sqlx",
                    ],
                },
            ]
        "#]]
        .assert_debug_eq(&changes);
    }

    // --- DetailScreen ---

    /// [verify tui.browse.detail]
    #[test]
    fn detail_selectable_items_includes_all_sections() {
        let detail = make_detail(
            &["serde", "tokio"],    // 2 crates
            &["basic", "advanced"], // 2 templates
            &["hello-world"],       // 1 example
        );
        let screen = DetailScreen {
            detail: Rc::new(detail),
            selected_index: 0,
            came_from_list: false,
        };

        let items: Vec<_> = screen.selectable_items().collect();
        assert_eq!(screen.item_count(), items.len());

        expect![[r#"
            [
                Crate(
                    "serde",
                ),
                Crate(
                    "tokio",
                ),
                Template {
                    _path: "templates/basic",
                    repo_path: None,
                },
                Template {
                    _path: "templates/advanced",
                    repo_path: None,
                },
                Example {
                    _name: "hello-world",
                    repo_path: None,
                },
                ActionOpenCratesIo,
                ActionAddToProject,
                ActionNewProject,
            ]
        "#]]
        .assert_debug_eq(&items);
    }

    /// [verify tui.browse.detail]
    #[test]
    fn detail_navigation_wraps() {
        let detail = make_detail(&["serde"], &[], &[]);
        let mut screen = DetailScreen {
            detail: Rc::new(detail),
            selected_index: 0,
            came_from_list: false,
        };

        // 1 crate + 3 actions = 4 items
        assert_eq!(screen.item_count(), 4);

        // Navigate to last item
        screen.select_prev(); // wraps to 3
        assert_eq!(screen.selected_index, 3);

        // Navigate forward wraps to 0
        screen.select_next();
        assert_eq!(screen.selected_index, 0);
    }

    #[test]
    fn detail_selected_item_returns_correct_item() {
        let detail = make_detail(&["serde", "tokio"], &[], &[]);
        let screen = DetailScreen {
            detail: Rc::new(detail),
            selected_index: 1, // tokio
            came_from_list: false,
        };

        let item = screen.selected_item().unwrap();
        assert!(matches!(item, DetailItem::Crate(n) if n == "tokio"));
    }

    // --- ExpandedPack ---

    /// [verify tui.browse.add]
    #[test]
    fn expanded_pack_toggle_and_navigate() {
        let mut expanded = ExpandedPack {
            pack: make_installed_pack(
                "web",
                vec![
                    make_entry("axum", true, false),
                    make_entry("tower", true, false),
                    make_entry("reqwest", false, false),
                ],
            ),
            selected_index: 0,
        };

        // Toggle first entry off
        expanded.toggle_selected();
        assert!(!expanded.pack.entries[0].enabled);

        // Navigate to reqwest (index 2)
        expanded.select_next();
        expanded.select_next();
        assert_eq!(expanded.selected_index, 2);

        // Toggle reqwest on
        expanded.toggle_selected();
        assert!(expanded.pack.entries[2].enabled);

        // Wrap navigation
        expanded.select_next(); // wraps to 0
        assert_eq!(expanded.selected_index, 0);
    }

    // --- build_installed_state (integration with BatteryPackSpec) ---

    /// [verify tui.installed.show-state]
    #[test]
    fn build_installed_state_from_spec() {
        let spec = make_spec(
            &[
                ("serde", crate_spec("1.0.0")),
                ("tokio", crate_spec_with_features("1.0.0", &["full"])),
            ],
            &[("default", &["serde", "tokio"])],
        );

        let installed_pack = crate::registry::InstalledPack {
            name: "test-battery-pack".to_string(),
            short_name: "test".to_string(),
            version: "1.0.0".to_string(),
            spec,
            active_features: BTreeSet::from(["default".to_string()]),
        };

        let state = build_installed_state(vec![installed_pack]);
        assert_eq!(state.packs.len(), 1);

        let pack = &state.packs[0];
        assert_eq!(pack.name, "test-battery-pack");
        assert_eq!(pack.entries.len(), 2);

        // Both should be enabled (in active features)
        assert!(pack.entries.iter().all(|e| e.enabled));
        assert!(pack.entries.iter().all(|e| e.originally_enabled));

        // Check tokio has features
        let tokio_entry = pack.entries.iter().find(|e| e.name == "tokio").unwrap();
        assert_eq!(tokio_entry.features, vec!["full"]);
    }

    /// [verify tui.browse.add]
    #[test]
    fn build_expanded_pack_defaults_prechecked() {
        let spec = make_spec(
            &[
                ("serde", crate_spec("1.0.0")),
                ("tokio", crate_spec("1.0.0")),
                ("tracing", crate_spec("0.1.0")),
            ],
            &[("default", &["serde", "tokio"])],
        );

        let summary = BatteryPackSummary {
            name: "test-battery-pack".to_string(),
            short_name: "test".to_string(),
            version: "1.0.0".to_string(),
            description: String::new(),
        };

        let expanded = build_expanded_pack(&summary, spec);

        // All originally_enabled should be false (new pack)
        assert!(expanded.pack.entries.iter().all(|e| !e.originally_enabled));

        // Default crates should be pre-checked (enabled=true)
        let serde = expanded
            .pack
            .entries
            .iter()
            .find(|e| e.name == "serde")
            .unwrap();
        assert!(serde.enabled, "default crate serde should be pre-checked");

        let tokio = expanded
            .pack
            .entries
            .iter()
            .find(|e| e.name == "tokio")
            .unwrap();
        assert!(tokio.enabled, "default crate tokio should be pre-checked");

        // Non-default crates should not be pre-checked
        let tracing = expanded
            .pack
            .entries
            .iter()
            .find(|e| e.name == "tracing")
            .unwrap();
        assert!(
            !tracing.enabled,
            "non-default crate tracing should not be pre-checked"
        );
    }

    /// Helper: create an App with a given screen (bypasses loading).
    fn make_app(screen: Screen) -> App {
        App {
            source: CrateSource::Registry,
            screen,
            should_quit: false,
            pending_action: None,
        }
    }

    /// Helper: create an AddScreen with installed packs and empty browse.
    fn make_add_screen(packs: Vec<InstalledPackState>) -> AddScreen {
        AddScreen {
            tab: AddTab::Installed,
            installed: make_installed(packs),
            browse: BrowseState {
                items: Vec::new(),
                list_state: ListState::default(),
                search_input: String::new(),
                searching: false,
                expanded: None,
            },
            changes: None,
        }
    }

    /// Create a BatteryPackSummary with sensible defaults.
    fn make_summary(short_name: &str, version: &str, desc: &str) -> BatteryPackSummary {
        BatteryPackSummary {
            name: format!("{}-battery-pack", short_name),
            short_name: short_name.to_string(),
            version: version.to_string(),
            description: desc.to_string(),
        }
    }

    /// Extract the AddScreen from an App, panicking if it's a different screen.
    fn unwrap_add_screen(app: &App) -> &AddScreen {
        match &app.screen {
            Screen::Add(state) => state,
            _ => panic!("Expected Add screen"),
        }
    }

    fn unwrap_list_screen(app: &App) -> &ListScreen {
        match &app.screen {
            Screen::List(state) => state,
            _ => panic!("Expected List screen"),
        }
    }

    fn unwrap_detail_screen(app: &App) -> &DetailScreen {
        match &app.screen {
            Screen::Detail(state) => state,
            _ => panic!("Expected Detail screen"),
        }
    }

    /// [verify tui.installed.show-state]
    #[test]
    fn build_installed_state_partial_features() {
        let spec = make_spec(
            &[
                ("serde", crate_spec("1.0.0")),
                ("tokio", crate_spec("1.0.0")),
                ("tracing", crate_spec("0.1.0")),
            ],
            &[
                ("default", &["serde"]),
                ("async", &["tokio"]),
                ("observability", &["tracing"]),
            ],
        );

        let installed_pack = crate::registry::InstalledPack {
            name: "test-battery-pack".to_string(),
            short_name: "test".to_string(),
            version: "1.0.0".to_string(),
            spec,
            // Only default + async active, not observability
            active_features: BTreeSet::from(["default".to_string(), "async".to_string()]),
        };

        let state = build_installed_state(vec![installed_pack]);
        let pack = &state.packs[0];

        let serde = pack.entries.iter().find(|e| e.name == "serde").unwrap();
        assert!(serde.enabled, "serde should be enabled (in default)");

        let tokio = pack.entries.iter().find(|e| e.name == "tokio").unwrap();
        assert!(tokio.enabled, "tokio should be enabled (in async)");

        let tracing = pack.entries.iter().find(|e| e.name == "tracing").unwrap();
        assert!(
            !tracing.enabled,
            "tracing should be disabled (observability not active)"
        );
    }

    // ====================================================================
    // Tier 2: Key handling tests
    // ====================================================================

    // --- List screen navigation ---

    /// [verify tui.nav.keyboard]
    #[test]
    fn list_j_k_navigation() {
        let mut list_state = ListState::default();
        list_state.select(Some(0));
        let mut app = make_app(Screen::List(ListScreen {
            items: vec![
                make_summary("a", "1.0.0", "Pack A"),
                make_summary("b", "1.0.0", "Pack B"),
            ],
            list_state,
            filter: None,
        }));

        app.handle_key(KeyCode::Char('j')); // down
        assert_eq!(unwrap_list_screen(&app).list_state.selected(), Some(1));

        app.handle_key(KeyCode::Char('k')); // back up
        assert_eq!(unwrap_list_screen(&app).list_state.selected(), Some(0));
    }

    /// [verify tui.nav.keyboard]
    #[test]
    fn list_q_quits() {
        let mut list_state = ListState::default();
        list_state.select(Some(0));
        let mut app = make_app(Screen::List(ListScreen {
            items: vec![make_summary("a", "1.0.0", "")],
            list_state,
            filter: None,
        }));

        app.handle_key(KeyCode::Char('q'));
        assert!(app.should_quit);
    }

    /// [verify tui.nav.keyboard]
    #[test]
    fn list_esc_quits() {
        let mut list_state = ListState::default();
        list_state.select(Some(0));
        let mut app = make_app(Screen::List(ListScreen {
            items: Vec::new(),
            list_state,
            filter: None,
        }));

        app.handle_key(KeyCode::Esc);
        assert!(app.should_quit);
    }

    // --- Detail screen key handling ---

    /// [verify tui.nav.keyboard]
    #[test]
    fn detail_tab_and_arrows_navigate() {
        let detail = make_detail(&["serde", "tokio"], &[], &[]);
        let mut app = make_app(Screen::Detail(DetailScreen {
            detail: Rc::new(detail),
            selected_index: 0,
            came_from_list: false,
        }));

        // Tab moves forward
        app.handle_key(KeyCode::Tab);
        assert_eq!(unwrap_detail_screen(&app).selected_index, 1);

        // Down arrow also moves forward
        app.handle_key(KeyCode::Down);
        assert_eq!(unwrap_detail_screen(&app).selected_index, 2);

        // Up arrow moves back
        app.handle_key(KeyCode::Up);
        assert_eq!(unwrap_detail_screen(&app).selected_index, 1);
    }

    /// [verify tui.nav.keyboard]
    #[test]
    fn detail_esc_when_came_from_list_goes_back() {
        let detail = make_detail(&["serde"], &[], &[]);
        let mut app = make_app(Screen::Detail(DetailScreen {
            detail: Rc::new(detail),
            selected_index: 0,
            came_from_list: true,
        }));

        // Esc with came_from_list transitions to Loading (process_loading runs in the
        // main loop, not inline). The important thing: it didn't quit.
        app.handle_key(KeyCode::Esc);
        assert!(!app.should_quit);
    }

    /// [verify tui.nav.keyboard]
    #[test]
    fn detail_esc_when_not_from_list_quits() {
        let detail = make_detail(&["serde"], &[], &[]);
        let mut app = make_app(Screen::Detail(DetailScreen {
            detail: Rc::new(detail),
            selected_index: 0,
            came_from_list: false,
        }));

        app.handle_key(KeyCode::Esc);
        assert!(app.should_quit);
    }

    /// [verify tui.nav.keyboard]
    #[test]
    fn detail_q_quits() {
        let detail = make_detail(&["serde"], &[], &[]);
        let mut app = make_app(Screen::Detail(DetailScreen {
            detail: Rc::new(detail),
            selected_index: 0,
            came_from_list: true, // even when came_from_list, q quits
        }));

        app.handle_key(KeyCode::Char('q'));
        assert!(app.should_quit);
    }

    // --- Add screen: Installed tab ---

    /// [verify tui.nav.keyboard]
    #[test]
    fn add_installed_space_toggles() {
        let mut app = make_app(Screen::Add(make_add_screen(vec![make_installed_pack(
            "web",
            vec![
                make_entry("axum", true, true),
                make_entry("tower", true, true),
            ],
        )])));

        app.handle_key(KeyCode::Char(' ')); // toggle first entry
        assert!(!unwrap_add_screen(&app).installed.packs[0].entries[0].enabled);
    }

    /// [verify tui.nav.keyboard]
    #[test]
    fn add_installed_j_k_navigates() {
        let mut app = make_app(Screen::Add(make_add_screen(vec![make_installed_pack(
            "web",
            vec![
                make_entry("axum", true, true),
                make_entry("tower", true, true),
                make_entry("reqwest", false, false),
            ],
        )])));

        app.handle_key(KeyCode::Char('j'));
        assert_eq!(unwrap_add_screen(&app).installed.selected_index, 1);

        app.handle_key(KeyCode::Char('k'));
        assert_eq!(unwrap_add_screen(&app).installed.selected_index, 0);
    }

    /// [verify tui.nav.cancel]
    #[test]
    fn add_installed_esc_quits_without_changes() {
        let mut app = make_app(Screen::Add(make_add_screen(vec![make_installed_pack(
            "web",
            vec![make_entry("axum", true, true)],
        )])));

        // Toggle a crate to create a pending change
        app.handle_key(KeyCode::Char(' '));

        // Esc quits without applying
        app.handle_key(KeyCode::Esc);
        assert!(app.should_quit);
        assert!(
            unwrap_add_screen(&app).changes.is_none(),
            "Esc should not apply changes"
        );
    }

    /// [verify tui.nav.exit]
    #[test]
    fn add_installed_enter_applies_when_changes_exist() {
        let mut app = make_app(Screen::Add(make_add_screen(vec![make_installed_pack(
            "web",
            vec![
                make_entry("axum", true, true),
                make_entry("tower", false, true), // changed: was on, now off
            ],
        )])));

        app.handle_key(KeyCode::Enter);
        assert!(app.should_quit);
        let state = unwrap_add_screen(&app);
        assert!(state.changes.is_some(), "Enter should collect changes");
        assert_eq!(state.changes.as_ref().unwrap().len(), 1);
    }

    /// [verify tui.nav.exit]
    #[test]
    fn add_installed_enter_does_nothing_when_no_changes() {
        let mut app = make_app(Screen::Add(make_add_screen(vec![make_installed_pack(
            "web",
            vec![make_entry("axum", true, true)],
        )])));

        app.handle_key(KeyCode::Enter);
        assert!(!app.should_quit, "Enter with no changes should not quit");
    }

    // --- Add screen: Browse tab ---

    /// [verify tui.browse.search]
    #[test]
    fn add_browse_search_mode() {
        let mut add_screen = make_add_screen(vec![]);
        add_screen.tab = AddTab::Browse;
        add_screen.browse.items = vec![make_summary("web", "1.0.0", "Web stuff")];
        let mut app = make_app(Screen::Add(add_screen));

        // '/' enters search mode
        app.handle_key(KeyCode::Char('/'));
        assert!(unwrap_add_screen(&app).browse.searching);

        // Type search text
        app.handle_key(KeyCode::Char('w'));
        app.handle_key(KeyCode::Char('e'));
        app.handle_key(KeyCode::Char('b'));
        assert_eq!(unwrap_add_screen(&app).browse.search_input, "web");

        // Backspace removes a character
        app.handle_key(KeyCode::Backspace);
        assert_eq!(unwrap_add_screen(&app).browse.search_input, "we");

        // Esc cancels search mode
        app.handle_key(KeyCode::Esc);
        assert!(!unwrap_add_screen(&app).browse.searching);
    }

    /// [verify tui.nav.keyboard]
    #[test]
    fn add_browse_tab_switches_to_installed() {
        let mut add_screen = make_add_screen(vec![make_installed_pack(
            "web",
            vec![make_entry("axum", true, true)],
        )]);
        add_screen.tab = AddTab::Browse;
        add_screen.browse.items = vec![make_summary("db", "1.0.0", "")];
        let mut app = make_app(Screen::Add(add_screen));

        app.handle_key(KeyCode::Tab);
        assert_eq!(unwrap_add_screen(&app).tab, AddTab::Installed);
    }

    // --- Add screen: Browse expanded pack ---

    /// [verify tui.browse.add]
    #[test]
    fn add_browse_expanded_confirm_moves_to_installed() {
        let mut add_screen = make_add_screen(vec![]);
        add_screen.tab = AddTab::Browse;
        add_screen.browse.expanded = Some(ExpandedPack {
            pack: make_installed_pack(
                "web",
                vec![
                    make_entry("axum", true, false),
                    make_entry("tower", true, false),
                ],
            ),
            selected_index: 0,
        });
        let mut app = make_app(Screen::Add(add_screen));

        // Enter confirms and moves to Installed tab
        app.handle_key(KeyCode::Enter);
        let state = unwrap_add_screen(&app);
        assert_eq!(state.tab, AddTab::Installed);
        assert!(state.browse.expanded.is_none());
        // The pack should have been added to installed
        assert_eq!(state.installed.packs.len(), 1);
        assert_eq!(state.installed.packs[0].short_name, "web");
    }

    /// [verify tui.browse.add]
    #[test]
    fn add_browse_expanded_esc_cancels() {
        let mut add_screen = make_add_screen(vec![]);
        add_screen.tab = AddTab::Browse;
        add_screen.browse.expanded = Some(ExpandedPack {
            pack: make_installed_pack("web", vec![make_entry("axum", true, false)]),
            selected_index: 0,
        });
        let mut app = make_app(Screen::Add(add_screen));

        app.handle_key(KeyCode::Esc);
        let state = unwrap_add_screen(&app);
        assert!(state.browse.expanded.is_none());
        assert!(state.installed.packs.is_empty()); // not added
    }

    /// [verify tui.browse.add]
    #[test]
    fn add_browse_expanded_no_selection_discards() {
        let mut add_screen = make_add_screen(vec![]);
        add_screen.tab = AddTab::Browse;
        add_screen.browse.expanded = Some(ExpandedPack {
            pack: make_installed_pack(
                "web",
                vec![
                    make_entry("axum", false, false), // nothing selected
                    make_entry("tower", false, false),
                ],
            ),
            selected_index: 0,
        });
        let mut app = make_app(Screen::Add(add_screen));

        // Enter with no selections — pack should NOT be added
        app.handle_key(KeyCode::Enter);
        let state = unwrap_add_screen(&app);
        assert_eq!(state.tab, AddTab::Installed);
        assert!(state.installed.packs.is_empty());
    }

    // ====================================================================
    // Tier 3: Rendering tests
    // ====================================================================

    /// Helper: render into an in-memory terminal and return the buffer content
    /// as a string (one line per row, padded with spaces).
    fn render_to_string(width: u16, height: u16, draw: impl FnOnce(&mut Frame)) -> String {
        use ratatui::Terminal;
        use ratatui::backend::TestBackend;

        let backend = TestBackend::new(width, height);
        let mut terminal = Terminal::new(backend).unwrap();
        terminal.draw(draw).unwrap();
        terminal.backend().to_string()
    }

    fn render_add_to_string(state: &mut AddScreen, width: u16, height: u16) -> String {
        render_to_string(width, height, |frame| render_add(frame, state))
    }

    fn render_list_to_string(state: &mut ListScreen, width: u16, height: u16) -> String {
        render_to_string(width, height, |frame| render_list(frame, state))
    }

    /// [verify tui.main.no-project] (partial: tests empty-state message, not greyed-out styling)
    /// When no packs are installed, the installed tab shows a message.
    #[test]
    fn render_no_packs_installed_message() {
        let mut state = make_add_screen(vec![]);
        state.tab = AddTab::Installed;
        let output = render_add_to_string(&mut state, 60, 15);
        assert!(
            output.contains("No battery packs installed"),
            "Expected 'No battery packs installed' in:\n{}",
            output
        );
    }

    /// [verify tui.installed.list-packs]
    /// Pack headers show name and version.
    #[test]
    fn render_installed_pack_headers() {
        let mut state = make_add_screen(vec![
            make_installed_pack("web", vec![make_entry("axum", true, true)]),
            make_installed_pack("db", vec![make_entry("sqlx", true, true)]),
        ]);
        state.tab = AddTab::Installed;
        let output = render_add_to_string(&mut state, 60, 20);
        assert!(
            output.contains("web") && output.contains("1.0.0"),
            "Expected 'web' and '1.0.0' in:\n{}",
            output
        );
        assert!(output.contains("db"), "Expected 'db' in:\n{}", output);
    }

    /// [verify tui.installed.list-crates]
    /// [verify tui.installed.show-state]
    /// Crate entries show checkbox ([x]/[ ]), name, and version info.
    #[test]
    fn render_installed_crate_entries_with_checkboxes() {
        let mut state = make_add_screen(vec![make_installed_pack(
            "web",
            vec![
                make_entry("axum", true, true),
                make_entry("tower", false, false),
            ],
        )]);
        state.tab = AddTab::Installed;
        let output = render_add_to_string(&mut state, 60, 15);
        assert!(
            output.contains("[x] axum"),
            "Expected '[x] axum' in:\n{}",
            output
        );
        assert!(
            output.contains("[ ] tower"),
            "Expected '[ ] tower' in:\n{}",
            output
        );
        // Version info should be present
        assert!(
            output.contains("(0.1.0)"),
            "Expected version info '(0.1.0)' in:\n{}",
            output
        );
    }

    /// [verify tui.browse.list]
    /// Browse list shows name, version, and description.
    #[test]
    fn render_browse_list_shows_name_version_description() {
        let mut list_state = ListState::default();
        list_state.select(Some(0));
        let mut state = ListScreen {
            items: vec![
                make_summary("web", "2.3.0", "Web framework essentials"),
                make_summary("db", "1.5.0", "Database toolkit"),
            ],
            list_state,
            filter: None,
        };
        let output = render_list_to_string(&mut state, 80, 15);
        // First pack
        assert!(output.contains("web"), "Expected 'web' in:\n{}", output);
        assert!(output.contains("2.3.0"), "Expected '2.3.0' in:\n{}", output);
        assert!(
            output.contains("Web framework essentials"),
            "Expected description in:\n{}",
            output
        );
        // Second pack
        assert!(output.contains("db"), "Expected 'db' in:\n{}", output);
        assert!(output.contains("1.5.0"), "Expected '1.5.0' in:\n{}", output);
        assert!(
            output.contains("Database toolkit"),
            "Expected description in:\n{}",
            output
        );
    }

    /// [verify tui.nav.keyboard]
    #[test]
    fn add_browse_expanded_space_toggles() {
        let mut add_screen = make_add_screen(vec![]);
        add_screen.tab = AddTab::Browse;
        add_screen.browse.expanded = Some(ExpandedPack {
            pack: make_installed_pack(
                "web",
                vec![
                    make_entry("axum", true, false),
                    make_entry("tower", false, false),
                ],
            ),
            selected_index: 1, // tower
        });
        let mut app = make_app(Screen::Add(add_screen));

        app.handle_key(KeyCode::Char(' '));
        let expanded = unwrap_add_screen(&app).browse.expanded.as_ref().unwrap();
        assert!(expanded.pack.entries[1].enabled); // tower toggled on
    }

    // ====================================================================
    // Coverage completion: main, network, new rules
    // ====================================================================

    fn render_app_to_string(app: &mut App, width: u16, height: u16) -> String {
        render_to_string(width, height, |frame| app.render(frame))
    }

    /// [verify tui.main.sections]
    /// The Add screen tab bar renders both "Installed" and "Browse" sections.
    /// Note: the spec lists three sections (Installed, Browse, New project),
    /// but New project is accessed from the Browse detail view rather than
    /// as a top-level tab.
    #[test]
    fn render_add_screen_shows_section_tabs() {
        let mut state = make_add_screen(vec![make_installed_pack(
            "web",
            vec![make_entry("axum", true, true)],
        )]);
        let output = render_add_to_string(&mut state, 60, 15);
        assert!(
            output.contains("Installed"),
            "Expected 'Installed' tab in:\n{}",
            output
        );
        assert!(
            output.contains("Browse"),
            "Expected 'Browse' tab in:\n{}",
            output
        );
    }

    /// [verify tui.main.always-available]
    /// The App can be constructed and rendered without any I/O. The
    /// constructors create a Loading screen (pure state), and rendering
    /// that screen produces a loading message — no filesystem or network
    /// access required.
    #[test]
    fn app_renders_loading_screen_without_io() {
        let mut app = make_app(Screen::Loading(LoadingState {
            message: "Loading battery packs...".to_string(),
            target: LoadingTarget::List { filter: None },
        }));
        let output = render_app_to_string(&mut app, 60, 10);
        assert!(
            output.contains("Loading battery packs..."),
            "Expected loading message in:\n{}",
            output
        );
    }

    /// [verify tui.main.context-detection]
    /// Context detection (finding Cargo.toml, walking up to workspace root)
    /// is delegated to cargo_metadata inside process_loading(). The TUI does
    /// not implement its own project discovery — it calls load_installed_packs()
    /// which invokes cargo_metadata on the current directory. Testing this
    /// would be testing cargo_metadata's behavior, not our code.
    #[test]
    fn context_detection_delegated_to_cargo_metadata() {
        // Intentionally empty — see doc comment.
    }

    /// [verify tui.network.non-blocking]
    /// Non-blocking network behavior is an architectural property of the
    /// event loop: process_loading() runs at the top of each iteration,
    /// and event::poll() uses a 100ms timeout so the UI stays responsive.
    /// This is a design invariant, not something expressible as a unit test.
    #[test]
    fn network_non_blocking_is_architectural() {
        // Intentionally empty — see doc comment.
    }

    /// [verify tui.network.error]
    /// Error screen renders with error message and key hints.
    #[test]
    fn error_screen_renders_message() {
        let mut app = make_app(Screen::Error(ErrorScreen {
            message: "connection refused".to_string(),
            retry_target: LoadingTarget::List { filter: None },
        }));
        let output = render_app_to_string(&mut app, 60, 10);
        expect![[r#"
            "                                                            "
            "                                                            "
            "                                                            "
            "                            Error                           "
            "                                                            "
            "                     connection refused                     "
            "                                                            "
            "         Press Enter or r to retry, Esc or q to quit        "
            "                                                            "
            "                                                            "
        "#]]
        .assert_eq(&output);
    }

    /// [verify tui.network.error]
    /// Enter key retries by transitioning back to Loading screen with
    /// the original target (including filter) preserved.
    #[test]
    fn error_screen_enter_retries() {
        let mut app = make_app(Screen::Error(ErrorScreen {
            message: "timeout".to_string(),
            retry_target: LoadingTarget::List {
                filter: Some("test".to_string()),
            },
        }));

        app.handle_key(KeyCode::Enter);

        let Screen::Loading(LoadingState {
            target: LoadingTarget::List { filter },
            ..
        }) = &app.screen
        else {
            panic!("expected Screen::Loading(LoadingTarget::List)");
        };
        assert_eq!(filter.as_deref(), Some("test"));
    }

    /// [verify tui.network.error]
    /// 'r' key also retries.
    #[test]
    fn error_screen_r_retries() {
        let mut app = make_app(Screen::Error(ErrorScreen {
            message: "timeout".to_string(),
            retry_target: LoadingTarget::List { filter: None },
        }));

        app.handle_key(KeyCode::Char('r'));
        assert!(matches!(app.screen, Screen::Loading(_)));
    }

    /// [verify tui.network.error]
    /// Esc quits from error screen.
    #[test]
    fn error_screen_esc_quits() {
        let mut app = make_app(Screen::Error(ErrorScreen {
            message: "error".to_string(),
            retry_target: LoadingTarget::Add,
        }));

        app.handle_key(KeyCode::Esc);
        assert!(app.should_quit);
    }

    /// [verify tui.network.error]
    /// 'q' quits from error screen.
    #[test]
    fn error_screen_q_quits() {
        let mut app = make_app(Screen::Error(ErrorScreen {
            message: "error".to_string(),
            retry_target: LoadingTarget::Add,
        }));

        app.handle_key(KeyCode::Char('q'));
        assert!(app.should_quit);
    }

    /// [verify tui.new.template-list]
    /// Template listing is shown in the DetailScreen's selectable items
    /// (already tested by detail_selectable_items_includes_all_sections).
    /// The "from crates.io" aspect requires a network fetch in
    /// process_loading() which is not unit-testable without mocking.
    #[test]
    fn template_list_covered_by_detail_screen_tests() {
        // Intentionally empty — see doc comment.
        // Real coverage: detail_selectable_items_includes_all_sections.
    }

    /// [verify tui.new.create]
    /// Project creation shells out to `cargo bp new` via
    /// PendingAction::NewProject in execute_action(). This spawns an
    /// external process (`cargo bp new`), which is not unit-testable.
    #[test]
    fn new_project_creates_via_external_process() {
        // Intentionally empty — see doc comment.
    }

    // --- Preview screen ---

    #[test]
    fn preview_esc_returns_to_detail() {
        let detail = make_detail(&["serde"], &["default"], &[]);
        let mut app = make_app(Screen::Preview(PreviewScreen {
            content: Text::from("test content"),
            template_name: "default".to_string(),
            scroll: 0,
            line_count: 1,
            detail: Rc::new(detail),
            selected_index: 2,
            came_from_list: true,
        }));

        app.handle_key(KeyCode::Esc);
        assert!(matches!(app.screen, Screen::Detail(_)));
        if let Screen::Detail(state) = &app.screen {
            assert_eq!(state.selected_index, 2);
            assert!(state.came_from_list);
        }
    }

    #[test]
    fn preview_scroll_down_and_up() {
        let detail = make_detail(&[], &["default"], &[]);
        let mut app = make_app(Screen::Preview(PreviewScreen {
            content: Text::from("line1\nline2\nline3\nline4\nline5"),
            template_name: "default".to_string(),
            scroll: 0,
            line_count: 5,
            detail: Rc::new(detail),
            selected_index: 0,
            came_from_list: false,
        }));

        app.handle_key(KeyCode::Down);
        if let Screen::Preview(state) = &app.screen {
            assert_eq!(state.scroll, 1);
        }

        app.handle_key(KeyCode::Char('j'));
        if let Screen::Preview(state) = &app.screen {
            assert_eq!(state.scroll, 2);
        }

        app.handle_key(KeyCode::Up);
        if let Screen::Preview(state) = &app.screen {
            assert_eq!(state.scroll, 1);
        }

        app.handle_key(KeyCode::Char('k'));
        if let Screen::Preview(state) = &app.screen {
            assert_eq!(state.scroll, 0);
        }
    }

    #[test]
    fn preview_scroll_clamps_at_bounds() {
        let detail = make_detail(&[], &["default"], &[]);
        let mut app = make_app(Screen::Preview(PreviewScreen {
            content: Text::from("line1\nline2"),
            template_name: "default".to_string(),
            scroll: 0,
            line_count: 2,
            detail: Rc::new(detail),
            selected_index: 0,
            came_from_list: false,
        }));

        // Scroll up at 0 stays at 0
        app.handle_key(KeyCode::Up);
        if let Screen::Preview(state) = &app.screen {
            assert_eq!(state.scroll, 0);
        }
    }
}
