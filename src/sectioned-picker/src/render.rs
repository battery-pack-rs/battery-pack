//! Rendering logic for the picker widget.

use crate::PickerAction;
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

    let [main_area, footer_area] =
        Layout::vertical([Constraint::Fill(1), Constraint::Length(1)]).areas(area);

    // Update visible height from actual terminal geometry.
    state.visible_height = main_area.height.saturating_sub(2) as usize;
    state.ensure_cursor_visible();

    // Build the content lines.
    let current_entry_idx = state.current_entry_idx();
    let mut lines: Vec<Line> = Vec::new();

    for (i, entry) in state.entries.iter().enumerate() {
        match entry {
            Entry::Header(text) => {
                if i > 0 {
                    lines.push(Line::from(""));
                }
                lines.push(Line::from(Span::styled(
                    text.as_str(),
                    Style::default().add_modifier(Modifier::BOLD),
                )));
            }
            Entry::Item { label, checked } => {
                let is_cursor = i == current_entry_idx;
                let checkbox = if *checked { "[x]" } else { "[ ]" };
                let prefix = if is_cursor { "> " } else { "  " };
                let text = format!("{prefix}{checkbox} {label}");

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
