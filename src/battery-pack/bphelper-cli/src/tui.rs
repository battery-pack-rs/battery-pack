//! Interactive TUI for battery-pack CLI.

use crate::{fetch_battery_pack_detail, fetch_battery_pack_list, BatteryPackDetail, BatteryPackSummary};
use anyhow::Result;
use crossterm::event::{self, Event, KeyCode, KeyEventKind};
use ratatui::{
    layout::{Constraint, Flex, Layout, Position, Rect},
    style::{Color, Modifier, Style, Stylize},
    text::{Line, Span},
    widgets::{Block, Borders, Clear, List, ListItem, ListState, Paragraph, Wrap},
    Frame,
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
pub fn run_show(name: &str) -> Result<()> {
    let app = App::new_show(name);
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
    List { filter: Option<String> },
    Detail { name: String, came_from_list: bool },
}

struct ListScreen {
    items: Vec<BatteryPackSummary>,
    list_state: ListState,
    filter: Option<String>,
}

struct DetailScreen {
    detail: BatteryPackDetail,
    selected_action: ActionSelection,
    came_from_list: bool,
}

#[derive(Clone, Copy, PartialEq)]
enum ActionSelection {
    OpenCratesIo,
    AddToProject,
    NewProject,
}

impl ActionSelection {
    fn next(self) -> Self {
        match self {
            Self::OpenCratesIo => Self::AddToProject,
            Self::AddToProject => Self::NewProject,
            Self::NewProject => Self::OpenCratesIo,
        }
    }

    fn prev(self) -> Self {
        match self {
            Self::OpenCratesIo => Self::NewProject,
            Self::AddToProject => Self::OpenCratesIo,
            Self::NewProject => Self::AddToProject,
        }
    }
}

struct FormScreen {
    battery_pack: String,
    directory: String,
    project_name: String,
    focused_field: FormField,
    cursor_position: usize,
    /// The detail screen to return to on cancel
    detail: BatteryPackDetail,
    came_from_list: bool,
}

#[derive(Clone, Copy, PartialEq)]
enum FormField {
    Directory,
    ProjectName,
}

enum PendingAction {
    OpenCratesIo { crate_name: String },
    AddToProject { battery_pack: String },
    NewProject { battery_pack: String, directory: String, name: String },
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

    fn new_show(name: &str) -> Self {
        Self {
            screen: Screen::Loading(LoadingState {
                message: format!("Loading {}...", name),
                target: LoadingTarget::Detail {
                    name: name.to_string(),
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
                LoadingTarget::Detail { name, came_from_list } => {
                    let detail = fetch_battery_pack_detail(name)?;
                    self.screen = Screen::Detail(DetailScreen {
                        detail,
                        selected_action: ActionSelection::OpenCratesIo,
                        came_from_list: *came_from_list,
                                            });
                }
            }
        }
        Ok(())
    }

    fn execute_action(&self, action: &PendingAction) -> Result<()> {
        match action {
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
            PendingAction::NewProject { battery_pack, directory, name } => {
                let status = std::process::Command::new("cargo")
                    .args(["bp", "new", battery_pack, "-n", name])
                    .current_dir(directory)
                    .status()?;

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
            DetailNextAction,
            DetailPrevAction,
            DetailOpenCratesIo(String),
            DetailAdd(String),
            DetailNewProject(BatteryPackDetail, bool),
            DetailBack(bool),
            FormToggleField,
            FormSubmit(String, String, String, BatteryPackDetail, bool),
            FormCancel(BatteryPackDetail, bool),
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
                KeyCode::Tab | KeyCode::Down | KeyCode::Char('j') => Action::DetailNextAction,
                KeyCode::BackTab | KeyCode::Up | KeyCode::Char('k') => Action::DetailPrevAction,
                KeyCode::Enter => match state.selected_action {
                    ActionSelection::OpenCratesIo => {
                        Action::DetailOpenCratesIo(state.detail.name.clone())
                    }
                    ActionSelection::AddToProject => {
                        Action::DetailAdd(state.detail.short_name.clone())
                    }
                    ActionSelection::NewProject => {
                        Action::DetailNewProject(state.detail.clone(), state.came_from_list)
                    }
                },
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
                            state.directory.clone(),
                            state.project_name.clone(),
                            state.detail.clone(),
                            state.came_from_list,
                        )
                    } else {
                        Action::None
                    }
                }
                KeyCode::Esc => Action::FormCancel(state.detail.clone(), state.came_from_list),
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
                                came_from_list: true,
                            },
                        });
                        let _ = self.process_loading();
                    }
                }
            }
            Action::DetailNextAction => {
                if let Screen::Detail(state) = &mut self.screen {
                    state.selected_action = state.selected_action.next();
                }
            }
            Action::DetailPrevAction => {
                if let Screen::Detail(state) = &mut self.screen {
                    state.selected_action = state.selected_action.prev();
                }
            }
            Action::DetailOpenCratesIo(crate_name) => {
                self.pending_action = Some(PendingAction::OpenCratesIo { crate_name });
            }
            Action::DetailAdd(battery_pack) => {
                self.pending_action = Some(PendingAction::AddToProject { battery_pack });
            }
            Action::DetailNewProject(detail, came_from_list) => {
                let cwd = std::env::current_dir()
                    .map(|p| p.to_string_lossy().to_string())
                    .unwrap_or_else(|_| ".".to_string());
                self.screen = Screen::NewProjectForm(FormScreen {
                    battery_pack: detail.short_name.clone(),
                    directory: cwd,
                    project_name: String::new(),
                    focused_field: FormField::ProjectName,
                    cursor_position: 0,
                    detail,
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
            Action::FormSubmit(battery_pack, directory, name, detail, came_from_list) => {
                self.pending_action = Some(PendingAction::NewProject {
                    battery_pack,
                    directory,
                    name,
                });
                self.screen = Screen::Detail(DetailScreen {
                    detail,
                    selected_action: ActionSelection::NewProject,
                    came_from_list,
                                    });
            }
            Action::FormCancel(detail, came_from_list) => {
                self.screen = Screen::Detail(DetailScreen {
                    detail,
                    selected_action: ActionSelection::NewProject,
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

    // Info section
    let mut lines: Vec<Line> = Vec::new();

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
        for dep in &detail.crates {
            lines.push(Line::from(format!("  {}", dep)));
        }
        lines.push(Line::from(""));
    }

    if !detail.extends.is_empty() {
        lines.push(Line::styled("Extends:", Style::default().bold()));
        for dep in &detail.extends {
            lines.push(Line::from(format!("  {}", dep)));
        }
        lines.push(Line::from(""));
    }

    if !detail.templates.is_empty() {
        lines.push(Line::styled("Templates:", Style::default().bold()));
        for tmpl in &detail.templates {
            let text = match &tmpl.description {
                Some(desc) => format!("  {} - {}", tmpl.name, desc),
                None => format!("  {}", tmpl.name),
            };
            lines.push(Line::styled(text, Style::default().fg(Color::Cyan)));
        }
        lines.push(Line::from(""));
    }

    // Actions section (inline)
    lines.push(Line::styled("Actions:", Style::default().bold()));

    let actions = [
        (ActionSelection::OpenCratesIo, "Open on crates.io"),
        (ActionSelection::AddToProject, "Add to project"),
        (ActionSelection::NewProject, "Create new project from template"),
    ];

    for (action, label) in actions {
        let style = if state.selected_action == action {
            Style::default().fg(Color::Black).bg(Color::Cyan).bold()
        } else {
            Style::default()
        };
        let prefix = if state.selected_action == action { "> " } else { "  " };
        lines.push(Line::styled(format!("{}{}", prefix, label), style));
    }

    let info = Paragraph::new(lines)
        .block(Block::default().borders(Borders::ALL))
        .wrap(Wrap { trim: false });
    frame.render_widget(info, main);

    // Footer
    let back_hint = if state.came_from_list {
        "Esc Back"
    } else {
        "Esc/q Quit"
    };
    frame.render_widget(
        Paragraph::new(format!("↑↓/jk Navigate | Enter Select | {}", back_hint))
            .style(Style::default().fg(Color::DarkGray))
            .centered(),
        footer,
    );
}

fn render_form(frame: &mut Frame, state: &FormScreen) {
    // First render detail view dimmed underneath
    let mut dimmed_detail = DetailScreen {
        detail: state.detail.clone(),
        selected_action: ActionSelection::NewProject,
        came_from_list: state.came_from_list,
            };
    render_detail(frame, &mut dimmed_detail);

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
        Paragraph::new(state.directory.as_str())
            .block(Block::default().borders(Borders::ALL).border_style(dir_style)),
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
        Paragraph::new(state.project_name.as_str())
            .block(Block::default().borders(Borders::ALL).border_style(name_style)),
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
        FormField::ProjectName => (name_input, state.cursor_position.min(state.project_name.len())),
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
