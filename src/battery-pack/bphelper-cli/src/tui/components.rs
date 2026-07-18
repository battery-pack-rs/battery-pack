use super::*;
use crate::registry::BatteryPackSummary;
use crossterm::event::{self, Event, KeyCode, KeyEventKind};
use ratatui::{
    Frame,
    layout::{Constraint, Flex, Layout, Position, Rect},
    style::{Color, Modifier, Style},
    text::{Line, Span, Text},
    widgets::{Block, Borders, Clear, List, ListItem, Paragraph, Wrap},
};
use std::rc::Rc;
use std::time::Duration;

// ============================================================================
// Screen renderers
// ============================================================================

/// Build a `ListItem` for a battery pack summary row (shared by list and browse views).
pub(crate) fn bp_summary_list_item(bp: &BatteryPackSummary) -> ListItem<'_> {
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

pub(crate) fn render_loading(frame: &mut Frame, state: &LoadingState) {
    let area = frame.area();
    let text = Paragraph::new(state.message.as_str())
        .style(Style::default().fg(Color::Cyan))
        .centered();

    let vertical = Layout::vertical([Constraint::Length(1)]).flex(Flex::Center);
    let [center] = vertical.areas(area);
    frame.render_widget(text, center);
}

/// [impl tui.network.error]
pub(crate) fn render_error(frame: &mut Frame, state: &ErrorScreen) {
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

pub(crate) fn render_list(frame: &mut Frame, state: &mut ListScreen) {
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
pub(crate) fn render_selectable_section<'a, T>(
    lines: &mut Vec<Line<'a>>,
    item_index: &mut usize,
    selected_index: usize,
    label: &'a str,
    items: &[T],
    normal_color: Option<Color>,
    format_item: impl Fn(&T) -> String,
) -> Option<usize> {
    if items.is_empty() {
        return None;
    }

    let mut selected_line = None;
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
        if selected {
            selected_line = Some(lines.len());
        }
        lines.push(Line::styled(
            format!("{}{}", prefix, format_item(item)),
            style,
        ));
        *item_index += 1;
    }
    lines.push(Line::from(""));
    selected_line
}

pub(crate) fn render_detail(frame: &mut Frame, state: &DetailScreen) {
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
    let mut selected_line: Option<usize> = None;

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

    selected_line = selected_line.or(render_selectable_section(
        &mut lines,
        &mut item_index,
        state.selected_index,
        "Crates:",
        &detail.crates,
        None,
        |crate_name| crate_name.clone(),
    ));

    // Features (non-selectable, informational)
    if !detail.features.is_empty() {
        lines.push(Line::styled("Features:", Style::default().bold()));
        for (feat_name, members) in &detail.features {
            lines.push(Line::from(format!(
                "  {} → {}",
                feat_name,
                members.join(", ")
            )));
        }
        lines.push(Line::from(""));
    }

    selected_line = selected_line.or(render_selectable_section(
        &mut lines,
        &mut item_index,
        state.selected_index,
        "Extends:",
        &detail.extends,
        Some(Color::Yellow),
        |bp| bp.clone(),
    ));

    selected_line = selected_line.or(render_selectable_section(
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
    ));

    selected_line = selected_line.or(render_selectable_section(
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
    ));

    // Actions section (always present)
    let add_label = if !state.in_project {
        "Add to project (not in a project)".to_string()
    } else if state.is_installed {
        "Add crates or features".to_string()
    } else {
        "Add to project".to_string()
    };
    let action_labels = [
        "Open on crates.io".to_string(),
        add_label,
        "Create new project from template".to_string(),
    ];
    selected_line = selected_line.or(render_selectable_section(
        &mut lines,
        &mut item_index,
        state.selected_index,
        "Actions:",
        &action_labels,
        None,
        |label| (*label).to_string(),
    ));

    // Sanity check
    debug_assert_eq!(
        item_index,
        selectable_items.len(),
        "Mismatch between rendered items and selectable_items()"
    );

    let visible_height = main.height.saturating_sub(2) as usize; // borders
    let scroll_offset = selected_line
        .map(|line| line.saturating_sub(visible_height.saturating_sub(1)))
        .unwrap_or(0);

    let info = Paragraph::new(lines)
        .block(Block::default().borders(Borders::ALL))
        .wrap(Wrap { trim: false })
        .scroll((scroll_offset as u16, 0));
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
            "↑↓/jk Navigate | Enter Open | p Preview | n New project | u Use in project | {}",
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

pub(crate) fn render_form_field(
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

pub(crate) fn render_form(frame: &mut Frame, state: &FormScreen) {
    // First render detail view dimmed underneath
    let dimmed_detail = DetailScreen {
        detail: Rc::clone(&state.detail),
        selected_index: state.selected_index,
        came_from_list: state.came_from_list,
        in_project: true, // doesn't matter for dimmed background
        is_installed: false,
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
pub(crate) fn highlight_preview(files: &[crate::template_engine::RenderedFile]) -> Text<'static> {
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

/// Show a scrollable syntax-highlighted preview. Blocks until the user presses Esc.
pub(crate) fn show_preview(
    terminal: &mut ratatui::DefaultTerminal,
    title: &str,
    content: Text<'static>,
) {
    let line_count = content.lines.len() as u16;
    let mut scroll: u16 = 0;

    loop {
        let _ = terminal.draw(|frame| {
            render_standalone_preview(frame, title, &content, scroll);
        });

        if event::poll(Duration::from_millis(100)).unwrap_or(false)
            && let Ok(Event::Key(key)) = event::read()
        {
            if key.kind != KeyEventKind::Press {
                continue;
            }
            match key.code {
                KeyCode::Esc | KeyCode::Char('q') => break,
                KeyCode::Down | KeyCode::Char('j') => {
                    scroll = scroll.saturating_add(1).min(line_count.saturating_sub(1));
                }
                KeyCode::Up | KeyCode::Char('k') => {
                    scroll = scroll.saturating_sub(1);
                }
                KeyCode::PageDown | KeyCode::Char('f') => {
                    scroll = scroll.saturating_add(20).min(line_count.saturating_sub(1));
                }
                KeyCode::PageUp | KeyCode::Char('b') => {
                    scroll = scroll.saturating_sub(20);
                }
                KeyCode::Home | KeyCode::Char('g') => {
                    scroll = 0;
                }
                KeyCode::End | KeyCode::Char('G') => {
                    scroll = line_count.saturating_sub(1);
                }
                _ => {}
            }
        }
    }
}

pub(crate) fn render_standalone_preview(
    frame: &mut Frame,
    title: &str,
    content: &Text<'_>,
    scroll: u16,
) {
    let area = frame.area();
    let [header, main, footer] = Layout::vertical([
        Constraint::Length(1),
        Constraint::Fill(1),
        Constraint::Length(1),
    ])
    .areas(area);

    frame.render_widget(
        Paragraph::new(Line::from(Span::styled(
            title,
            Style::default().fg(Color::Cyan).bold(),
        )))
        .centered(),
        header,
    );

    let preview = Paragraph::new(content.clone())
        .block(
            Block::default()
                .borders(Borders::BOTTOM)
                .padding(ratatui::widgets::Padding::horizontal(1)),
        )
        .scroll((scroll, 0));
    frame.render_widget(preview, main);

    frame.render_widget(
        Paragraph::new("↑↓/jk/PgUp/PgDn Scroll | Esc Back")
            .style(Style::default().white().on_dark_gray()),
        footer,
    );
}

pub(crate) fn render_preview(frame: &mut Frame, state: &PreviewScreen) {
    let area = frame.area();
    let [header, main, footer] = Layout::vertical([
        Constraint::Length(1),
        Constraint::Fill(1),
        Constraint::Length(1),
    ])
    .areas(area);

    frame.render_widget(
        Paragraph::new(Line::from(vec![
            Span::styled(
                &state.battery_pack_name,
                Style::default().fg(Color::Green).bold(),
            ),
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
