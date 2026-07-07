//! Rendering logic for the picker widget.

use crate::PickerAction;
use crate::SelectionMode;
use crate::state::{Entry, PickerState};
use ratatui::macros::span;
use ratatui::style::Stylize;
use ratatui::widgets::Padding;
use ratatui::{
    Frame,
    layout::{Constraint, Layout},
    style::{Color, Modifier, Style},
    text::{Line, Span},
    widgets::{Block, Borders, Paragraph},
};

/// Render the picker into the given frame.
///
/// This updates `state.visible_height` from the actual terminal geometry and
/// calls `ensure_cursor_visible` so scroll is correct even after a resize.
pub fn render_picker(
    frame: &mut Frame,
    title: &str,
    state: &mut PickerState,
    actions: &[PickerAction<'_>],
) {
    let area = frame.area();

    // A banner line appears above the list when there's a warning to surface:
    // a rejected-confirm error, or a pre-existing radio conflict (>1 selected).
    let banner = state.confirm_error.clone().or_else(|| {
        state
            .radio_conflict_title()
            .map(|title| format!("Multiple selections in \"{title}\" — pick one to resolve"))
    });
    let banner_height: u16 = if banner.is_some() { 1 } else { 0 };

    let [banner_area, main_area, footer_area] = Layout::vertical([
        Constraint::Length(banner_height),
        Constraint::Fill(1),
        Constraint::Length(1),
    ])
    .areas(area);

    // Render the warning banner if present.
    if let Some(message) = &banner {
        frame.render_widget(
            Paragraph::new(format!(" ⚠ {message}"))
                .style(Style::default().fg(Color::Black).bg(Color::Yellow).bold()),
            banner_area,
        );
    }

    // Update visible height from actual terminal geometry.
    state.visible_height = main_area.height.saturating_sub(2) as usize;
    state.ensure_cursor_visible();

    // Build the content lines. The current section's mode is tracked while
    // walking entries so items know whether to render as radio or checkbox.
    let current_entry_idx = state.current_entry_idx();
    let mut lines: Vec<Line> = Vec::new();
    let mut section_mode = SelectionMode::Checkbox;

    for (i, entry) in state.entries.iter().enumerate() {
        match entry {
            Entry::Header {
                title,
                mode,
                collapsed,
            } => {
                section_mode = *mode;
                if i > 0 {
                    lines.push(Line::from(""));
                }
                let chevron = if *collapsed { "▶ " } else { "▼ " };
                let hint = if *mode == SelectionMode::Radio {
                    " (pick at most one)"
                } else {
                    ""
                };
                lines.push(Line::from(Span::styled(
                    format!("{chevron}{title}{hint}"),
                    Style::default().add_modifier(Modifier::BOLD),
                )));
            }
            Entry::Item {
                label,
                checked,
                description,
            } => {
                // Collapsed items contribute no rendered line (mirrors entry_to_line).
                if !state.visible[i] {
                    continue;
                }
                let is_cursor = i == current_entry_idx;
                let symbol = match section_mode {
                    SelectionMode::Checkbox if *checked => "[x]",
                    SelectionMode::Checkbox => "[ ]",
                    SelectionMode::Radio if *checked => "●",
                    SelectionMode::Radio => "○",
                };
                let prefix = if is_cursor { "> " } else { "  " };
                let desc = match description {
                    Some(d) => format!("    {d}"),
                    None => String::new(),
                };
                let text = format!("{prefix}{symbol} {label}{desc}");

                let style = if is_cursor {
                    Style::default()
                        .fg(Color::Black)
                        .bg(Color::Cyan)
                        .add_modifier(Modifier::BOLD)
                } else {
                    Style::default()
                };
                lines.push(Line::styled(text, style));
            }
        }
    }

    let content = Paragraph::new(lines)
        .block(
            Block::default()
                .borders(Borders::TOP)
                .padding(Padding::horizontal(1))
                .border_style(Style::new().dim()),
        )
        .scroll((state.scroll as u16, 0));
    frame.render_widget(content, main_area);

    // Footer — built-in keys + caller-defined action labels.
    let [footer_left, footer_right] = Layout::horizontal([
        Constraint::Length(title.len() as u16 + 2),
        Constraint::Fill(1),
    ])
    .areas(footer_area);
    let left_footer = span!(" {title} ").on_green().black().bold();
    frame.render_widget(left_footer, footer_left);
    let mut footer_parts = vec![
        " ↑↓/jk Navigate".to_string(),
        "Space Toggle".to_string(),
        "←/→ Collapse/expand".to_string(),
        "a Toggle section".to_string(),
    ];
    for action in actions {
        footer_parts.push(format!("{} {}", action.key, action.label));
    }
    footer_parts.push("Enter Confirm".to_string());
    footer_parts.push("Esc Cancel".to_string());

    frame.render_widget(
        Paragraph::new(footer_parts.join(" | ")).style(Style::default().white().on_dark_gray()),
        footer_right,
    );
}
