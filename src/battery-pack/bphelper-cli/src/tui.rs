//! Interactive TUI for battery-pack CLI.

use crate::{
    BatteryPackDetail, BatteryPackSummary, fetch_battery_pack_detail, fetch_battery_pack_list,
};
use anyhow::Result;
use crossterm::event::{self, Event, KeyCode, KeyEventKind};
use ratatui::{
    Frame,
    layout::{Constraint, Flex, Layout, Position, Rect},
    style::{Color, Modifier, Style, Stylize},
    text::{Line, Span},
    widgets::{Block, Borders, Clear, List, ListItem, ListState, Paragraph, Wrap},
};
use std::time::Duration;

// ============================================================================
// Public entry points
// ============================================================================

/// Run the TUI starting from the list view
pub fn run_list(filter: Option<String>) -> Result<()> {
    let app = App::new_list(filter);
    app.run()
}

/// Run the TUI starting from the detail view
pub fn run_show(name: &str, path: Option<&str>) -> Result<()> {
    let app = App::new_show(name, path);
    app.run()
}

// ============================================================================
// App state
// ============================================================================

struct App {
    screen: Screen,
    should_quit: bool,
    pending_action: Option<PendingAction>,
}

enum Screen {
    Loading(LoadingState),
    List(ListScreen),
    Detail(DetailScreen),
    NewProjectForm(FormScreen),
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
}

struct ListScreen {
    items: Vec<BatteryPackSummary>,
    list_state: ListState,
    filter: Option<String>,
}

struct DetailScreen {
    detail: BatteryPackDetail,
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
    Template { _path: String, repo_path: Option<String> },
    /// An example - opens GitHub blob URL (stores name and resolved repo path)
    Example { _name: String, repo_path: Option<String> },
    /// Open on crates.io action
    ActionOpenCratesIo,
    /// Add to project action
    ActionAddToProject,
    /// Create new project action
    ActionNewProject,
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

    /// Build a list of all selectable items in order
    fn selectable_items(&self) -> Vec<DetailItem> {
        let mut items = Vec::with_capacity(self.item_count());

        // Crates
        for crate_name in &self.detail.crates {
            items.push(DetailItem::Crate(crate_name.clone()));
        }

        // Extends (other battery packs)
        for extends in &self.detail.extends {
            items.push(DetailItem::Extends(extends.clone()));
        }

        // Templates
        for tmpl in &self.detail.templates {
            items.push(DetailItem::Template {
                _path: tmpl.path.clone(),
                repo_path: tmpl.repo_path.clone(),
            });
        }

        // Examples
        for example in &self.detail.examples {
            items.push(DetailItem::Example {
                _name: example.name.clone(),
                repo_path: example.repo_path.clone(),
            });
        }

        // Actions (always present)
        items.push(DetailItem::ActionOpenCratesIo);
        items.push(DetailItem::ActionAddToProject);
        items.push(DetailItem::ActionNewProject);

        items
    }

    fn selected_item(&self) -> Option<DetailItem> {
        self.selectable_items().get(self.selected_index).cloned()
    }

    fn select_next(&mut self) {
        let count = self.item_count();
        if count > 0 {
            self.selected_index = (self.selected_index + 1) % count;
        }
    }

    fn select_prev(&mut self) {
        let count = self.item_count();
        if count > 0 {
            self.selected_index = (self.selected_index + count - 1) % count;
        }
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
    /// The detail screen to return to on cancel
    detail: BatteryPackDetail,
    /// Selected index to restore when returning to detail
    selected_index: usize,
    came_from_list: bool,
}

#[derive(Clone, Copy, PartialEq)]
enum FormField {
    Directory,
    ProjectName,
}

enum PendingAction {
    /// Open a URL in the browser (generic)
    OpenUrl {
        url: String,
    },
    /// Open crates.io for the battery pack
    OpenCratesIo {
        crate_name: String,
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
    fn new_list(filter: Option<String>) -> Self {
        Self {
            screen: Screen::Loading(LoadingState {
                message: "Loading battery packs...".to_string(),
                target: LoadingTarget::List { filter },
            }),
            should_quit: false,
            pending_action: None,
        }
    }

    fn new_show(name: &str, path: Option<&str>) -> Self {
        Self {
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
        let mut terminal = ratatui::init();

        // Process initial loading state
        self.process_loading()?;

        loop {
            terminal.draw(|frame| self.render(frame))?;

            // Execute pending actions (exit TUI, run command, re-enter)
            if let Some(action) = self.pending_action.take() {
                ratatui::restore();
                self.execute_action(&action)?;
                terminal = ratatui::init();
                continue;
            }

            if event::poll(Duration::from_millis(100))? {
                if let Event::Key(key) = event::read()? {
                    // Windows compatibility: only handle Press events
                    if key.kind == KeyEventKind::Press {
                        self.handle_key(key.code);
                    }
                }
            }

            if self.should_quit {
                break;
            }
        }

        ratatui::restore();
        Ok(())
    }

    fn process_loading(&mut self) -> Result<()> {
        if let Screen::Loading(state) = &self.screen {
            match &state.target {
                LoadingTarget::List { filter } => {
                    let items = fetch_battery_pack_list(filter.as_deref())?;
                    let mut list_state = ListState::default();
                    if !items.is_empty() {
                        list_state.select(Some(0));
                    }
                    self.screen = Screen::List(ListScreen {
                        items,
                        list_state,
                        filter: filter.clone(),
                    });
                }
                LoadingTarget::Detail {
                    name,
                    path,
                    came_from_list,
                } => {
                    let detail = fetch_battery_pack_detail(name, path.as_deref())?;
                    // Start selection at first action (after crates/extends/templates/examples)
                    let initial_index = detail.crates.len()
                        + detail.extends.len()
                        + detail.templates.len()
                        + detail.examples.len();
                    self.screen = Screen::Detail(DetailScreen {
                        detail,
                        selected_index: initial_index,
                        came_from_list: *came_from_list,
                    });
                }
            }
        }
        Ok(())
    }

    fn execute_action(&self, action: &PendingAction) -> Result<()> {
        match action {
            PendingAction::OpenUrl { url } => {
                if let Err(e) = open::that(url) {
                    println!("Failed to open browser: {}", e);
                    println!("URL: {}", url);
                    println!("\nPress Enter to return to TUI...");
                    let _ = std::io::stdin().read_line(&mut String::new());
                }
                // No "press enter" for successful open - just return immediately
            }
            PendingAction::OpenCratesIo { crate_name } => {
                let url = format!("https://crates.io/crates/{}", crate_name);
                if let Err(e) = open::that(&url) {
                    println!("Failed to open browser: {}", e);
                    println!("URL: {}", url);
                    println!("\nPress Enter to return to TUI...");
                    let _ = std::io::stdin().read_line(&mut String::new());
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
                println!("\nPress Enter to return to TUI...");
                let _ = std::io::stdin().read_line(&mut String::new());
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
                println!("\nPress Enter to return to TUI...");
                let _ = std::io::stdin().read_line(&mut String::new());
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
            DetailNewProject(BatteryPackDetail, Option<String>, usize, bool),
            DetailBack(bool),
            FormToggleField,
            FormSubmit(
                String,
                Option<String>,
                String,
                String,
                BatteryPackDetail,
                usize,
                bool,
            ),
            FormCancel(BatteryPackDetail, usize, bool),
            FormChar(char),
            FormBackspace,
            FormDelete,
            FormLeft,
            FormRight,
            FormHome,
            FormEnd,
        }

        let action = match &self.screen {
            Screen::Loading(_) => Action::None,
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
                            DetailItem::Template { _path: _, repo_path } => Action::OpenTemplate {
                                repository: state.detail.repository.clone(),
                                repo_path,
                            },
                            DetailItem::Example { _name: _, repo_path } => Action::OpenExample {
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
                                    state.detail.clone(),
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
                    if let Some(DetailItem::Template { _path, repo_path: _ }) = state.selected_item() {
                        // Find the template name from the path
                        let template_name = state
                            .detail
                            .templates
                            .iter()
                            .find(|t| t.path == _path)
                            .map(|t| t.name.clone());
                        Action::DetailNewProject(
                            state.detail.clone(),
                            template_name,
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
                            state.detail.clone(),
                            state.selected_index,
                            state.came_from_list,
                        )
                    } else {
                        Action::None
                    }
                }
                KeyCode::Esc => Action::FormCancel(
                    state.detail.clone(),
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
        };

        // Now apply the action with full mutable access
        match action {
            Action::None => {}
            Action::Quit => self.should_quit = true,
            Action::ListUp => {
                if let Screen::List(state) = &mut self.screen {
                    if let Some(selected) = state.list_state.selected() {
                        if selected > 0 {
                            state.list_state.select(Some(selected - 1));
                        }
                    }
                }
            }
            Action::ListDown => {
                if let Screen::List(state) = &mut self.screen {
                    if let Some(selected) = state.list_state.selected() {
                        if selected < state.items.len().saturating_sub(1) {
                            state.list_state.select(Some(selected + 1));
                        }
                    }
                }
            }
            Action::ListSelect(selected) => {
                if let Screen::List(state) = &self.screen {
                    if let Some(bp) = state.items.get(selected) {
                        self.screen = Screen::Loading(LoadingState {
                            message: format!("Loading {}...", bp.short_name),
                            target: LoadingTarget::Detail {
                                name: bp.name.clone(),
                                path: None,
                                came_from_list: true,
                            },
                        });
                        let _ = self.process_loading();
                    }
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
            Action::OpenTemplate { repository, repo_path } => {
                let url = match repo_path {
                    Some(path) => build_github_url(repository.as_deref(), &path),
                    None => repository.unwrap_or_else(|| "https://crates.io".to_string()),
                };
                self.pending_action = Some(PendingAction::OpenUrl { url });
            }
            Action::OpenExample { repository, repo_path } => {
                let url = match repo_path {
                    Some(path) => build_github_blob_url(repository.as_deref(), &path),
                    None => repository.unwrap_or_else(|| "https://crates.io".to_string()),
                };
                self.pending_action = Some(PendingAction::OpenUrl { url });
            }
            Action::DetailOpenCratesIo(crate_name) => {
                self.pending_action = Some(PendingAction::OpenCratesIo { crate_name });
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
                    let _ = self.process_loading();
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
                    state.cursor_position = match state.focused_field {
                        FormField::Directory => state.directory.len(),
                        FormField::ProjectName => state.project_name.len(),
                    };
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
                    let field = match state.focused_field {
                        FormField::Directory => &mut state.directory,
                        FormField::ProjectName => &mut state.project_name,
                    };
                    field.insert(state.cursor_position, c);
                    state.cursor_position += 1;
                }
            }
            Action::FormBackspace => {
                if let Screen::NewProjectForm(state) = &mut self.screen {
                    if state.cursor_position > 0 {
                        let field = match state.focused_field {
                            FormField::Directory => &mut state.directory,
                            FormField::ProjectName => &mut state.project_name,
                        };
                        field.remove(state.cursor_position - 1);
                        state.cursor_position -= 1;
                    }
                }
            }
            Action::FormDelete => {
                if let Screen::NewProjectForm(state) = &mut self.screen {
                    let field = match state.focused_field {
                        FormField::Directory => &mut state.directory,
                        FormField::ProjectName => &mut state.project_name,
                    };
                    if state.cursor_position < field.len() {
                        field.remove(state.cursor_position);
                    }
                }
            }
            Action::FormLeft => {
                if let Screen::NewProjectForm(state) = &mut self.screen {
                    state.cursor_position = state.cursor_position.saturating_sub(1);
                }
            }
            Action::FormRight => {
                if let Screen::NewProjectForm(state) = &mut self.screen {
                    let field_len = match state.focused_field {
                        FormField::Directory => state.directory.len(),
                        FormField::ProjectName => state.project_name.len(),
                    };
                    if state.cursor_position < field_len {
                        state.cursor_position += 1;
                    }
                }
            }
            Action::FormHome => {
                if let Screen::NewProjectForm(state) = &mut self.screen {
                    state.cursor_position = 0;
                }
            }
            Action::FormEnd => {
                if let Screen::NewProjectForm(state) = &mut self.screen {
                    state.cursor_position = match state.focused_field {
                        FormField::Directory => state.directory.len(),
                        FormField::ProjectName => state.project_name.len(),
                    };
                }
            }
        }
    }

    // ========================================================================
    // Rendering
    // ========================================================================

    fn render(&mut self, frame: &mut Frame) {
        match &mut self.screen {
            Screen::Loading(state) => render_loading(frame, state),
            Screen::List(state) => render_list(frame, state),
            Screen::Detail(state) => render_detail(frame, state),
            Screen::NewProjectForm(state) => render_form(frame, state),
        }
    }
}

// ============================================================================
// Screen renderers
// ============================================================================

fn render_loading(frame: &mut Frame, state: &LoadingState) {
    let area = frame.area();
    let text = Paragraph::new(state.message.as_str())
        .style(Style::default().fg(Color::Cyan))
        .centered();

    let vertical = Layout::vertical([Constraint::Length(1)]).flex(Flex::Center);
    let [center] = vertical.areas(area);
    frame.render_widget(text, center);
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
    let items: Vec<ListItem> = state
        .items
        .iter()
        .map(|bp| {
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
        })
        .collect();

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
            .style(Style::default().fg(Color::DarkGray))
            .centered(),
        footer,
    );
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
    let selectable_items = state.selectable_items();

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

    if !detail.crates.is_empty() {
        lines.push(Line::styled("Crates:", Style::default().bold()));
        for crate_name in &detail.crates {
            let selected = state.selected_index == item_index;
            let style = if selected {
                Style::default().fg(Color::Black).bg(Color::Cyan).bold()
            } else {
                Style::default()
            };
            let prefix = if selected { "> " } else { "  " };
            lines.push(Line::styled(format!("{}{}", prefix, crate_name), style));
            item_index += 1;
        }
        lines.push(Line::from(""));
    }

    if !detail.extends.is_empty() {
        lines.push(Line::styled("Extends:", Style::default().bold()));
        for bp in &detail.extends {
            let selected = state.selected_index == item_index;
            let style = if selected {
                Style::default().fg(Color::Black).bg(Color::Cyan).bold()
            } else {
                Style::default().fg(Color::Yellow)
            };
            let prefix = if selected { "> " } else { "  " };
            lines.push(Line::styled(format!("{}{}", prefix, bp), style));
            item_index += 1;
        }
        lines.push(Line::from(""));
    }

    if !detail.templates.is_empty() {
        lines.push(Line::styled("Templates:", Style::default().bold()));
        for tmpl in &detail.templates {
            let selected = state.selected_index == item_index;
            let text = match &tmpl.description {
                Some(desc) => format!("{} - {}", tmpl.name, desc),
                None => tmpl.name.clone(),
            };
            let style = if selected {
                Style::default().fg(Color::Black).bg(Color::Cyan).bold()
            } else {
                Style::default().fg(Color::Cyan)
            };
            let prefix = if selected { "> " } else { "  " };
            lines.push(Line::styled(format!("{}{}", prefix, text), style));
            item_index += 1;
        }
        lines.push(Line::from(""));
    }

    if !detail.examples.is_empty() {
        lines.push(Line::styled("Examples:", Style::default().bold()));
        for example in &detail.examples {
            let selected = state.selected_index == item_index;
            let text = match &example.description {
                Some(desc) => format!("{} - {}", example.name, desc),
                None => example.name.clone(),
            };
            let style = if selected {
                Style::default().fg(Color::Black).bg(Color::Cyan).bold()
            } else {
                Style::default().fg(Color::Magenta)
            };
            let prefix = if selected { "> " } else { "  " };
            lines.push(Line::styled(format!("{}{}", prefix, text), style));
            item_index += 1;
        }
        lines.push(Line::from(""));
    }

    // Actions section
    lines.push(Line::styled("Actions:", Style::default().bold()));

    let action_labels = [
        "Open on crates.io",
        "Add to project",
        "Create new project from template",
    ];

    for label in action_labels {
        let selected = state.selected_index == item_index;
        let style = if selected {
            Style::default().fg(Color::Black).bg(Color::Cyan).bold()
        } else {
            Style::default()
        };
        let prefix = if selected { "> " } else { "  " };
        lines.push(Line::styled(format!("{}{}", prefix, label), style));
        item_index += 1;
    }

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
            "↑↓/jk Navigate | Enter Open | n New project | {}",
            back_hint
        )
    } else {
        format!("↑↓/jk Navigate | Enter Open/Select | {}", back_hint)
    };
    frame.render_widget(
        Paragraph::new(footer_text)
            .style(Style::default().fg(Color::DarkGray))
            .centered(),
        footer,
    );
}

fn render_form(frame: &mut Frame, state: &FormScreen) {
    // First render detail view dimmed underneath
    let dimmed_detail = DetailScreen {
        detail: state.detail.clone(),
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

    // Directory field
    frame.render_widget(
        Paragraph::new("Directory:").style(Style::default().bold()),
        dir_label,
    );

    let dir_style = if state.focused_field == FormField::Directory {
        Style::default().fg(Color::Yellow)
    } else {
        Style::default().fg(Color::DarkGray)
    };
    frame.render_widget(
        Paragraph::new(state.directory.as_str()).block(
            Block::default()
                .borders(Borders::ALL)
                .border_style(dir_style),
        ),
        dir_input,
    );

    // Project name field
    frame.render_widget(
        Paragraph::new("Project Name:").style(Style::default().bold()),
        name_label,
    );

    let name_style = if state.focused_field == FormField::ProjectName {
        Style::default().fg(Color::Yellow)
    } else {
        Style::default().fg(Color::DarkGray)
    };
    frame.render_widget(
        Paragraph::new(state.project_name.as_str()).block(
            Block::default()
                .borders(Borders::ALL)
                .border_style(name_style),
        ),
        name_input,
    );

    // Hint
    frame.render_widget(
        Paragraph::new("Tab Switch | Enter Create | Esc Cancel")
            .style(Style::default().fg(Color::DarkGray))
            .centered(),
        hint,
    );

    // Show cursor in active field
    let (cursor_area, cursor_x) = match state.focused_field {
        FormField::Directory => (dir_input, state.cursor_position.min(state.directory.len())),
        FormField::ProjectName => (
            name_input,
            state.cursor_position.min(state.project_name.len()),
        ),
    };
    // +1 for border
    frame.set_cursor_position(Position::new(
        cursor_area.x + 1 + cursor_x as u16,
        cursor_area.y + 1,
    ));
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
