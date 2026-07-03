//! Interactive terminal multi-select picker with non-selectable section headers.
//!
//! This crate provides a full-screen ratatui-based picker widget that groups
//! selectable items under bold section headers. It's designed for CLI tools that
//! need users to choose from categorized options.
//!
//! # Features
//!
//! - **Sectioned layout** — items grouped under bold, non-selectable headers
//! - **Checkboxes** — `[x]`/`[ ]` with `>` cursor indicator
//! - **Keyboard navigation** — arrow keys, j/k, Space to toggle, Enter to confirm
//! - **Section toggle** — `a` checks/unchecks all items in the current section
//! - **Smart scrolling** — keeps cursor visible; snaps to section header when near top
//! - **Custom actions** — bind arbitrary keys to caller-defined handlers that can
//!   take over the terminal (e.g., for previews)
//!
//! # Example
//!
//! ```no_run
//! use sectioned_picker::{Section, SectionItem, PickerAction, PickerOutcome, run_picker};
//!
//! let sections = vec![
//!     Section {
//!         title: "Features:".to_string(),
//!         items: vec![
//!             SectionItem { label: "logging".to_string(), checked: true },
//!             SectionItem { label: "metrics".to_string(), checked: false },
//!         ],
//!     },
//!     Section {
//!         title: "Dependencies:".to_string(),
//!         items: vec![
//!             SectionItem { label: "tokio (1.38)".to_string(), checked: true },
//!         ],
//!     },
//! ];
//!
//! match run_picker("my-app v1.0", sections, Vec::new()).unwrap() {
//!     PickerOutcome::Confirmed(results) => {
//!         // results[0] = [true, false] — features section
//!         // results[1] = [true]        — dependencies section
//!     }
//!     PickerOutcome::Cancelled => {}
//! }
//! ```
//!
//! # Scrolling behavior
//!
//! When the list exceeds the viewport height, the view scrolls to keep the
//! cursor visible:
//!
//! - **Near section top:** scrolling up snaps to the section header when it fits
//!   in the viewport alongside the cursor.
//! - **Tall sections:** does NOT snap to a distant header on every up-movement;
//!   only snaps when the cursor is close enough to the top of its section.
//!
//! # Enter behavior
//!
//! If no items are checked when the user presses Enter, the item under the
//! cursor is checked before submitting. This makes single-item selection a
//! one-key operation (navigate + Enter).

mod render;
mod state;

#[cfg(test)]
mod tests;

use crossterm::event::{self, Event, KeyCode, KeyEventKind, KeyModifiers};
use std::time::Duration;

pub use render::render_picker;
pub use state::PickerState;

/// A section of items in the picker.
pub struct Section {
    pub title: String,
    pub items: Vec<SectionItem>,
}

/// A selectable item within a section.
pub struct SectionItem {
    pub label: String,
    pub checked: bool,
}

/// Context passed to action handlers when a custom key is pressed.
///
/// Provides access to the current cursor position and the terminal for
/// full-screen takeover (e.g., rendering a preview).
pub struct ActionContext<'a> {
    section_idx: usize,
    item_idx: usize,
    terminal: &'a mut ratatui::DefaultTerminal,
}

impl ActionContext<'_> {
    /// Which section the cursor is in (0-indexed, matching input order).
    pub fn section(&self) -> usize {
        self.section_idx
    }

    /// Which item within the section the cursor is on (0-indexed).
    pub fn item(&self) -> usize {
        self.item_idx
    }

    /// Mutable access to the terminal for drawing custom screens.
    pub fn terminal(&mut self) -> &mut ratatui::DefaultTerminal {
        self.terminal
    }
}

/// Handler type for picker actions.
pub type ActionHandler<'a> = Box<dyn FnMut(&mut ActionContext<'_>) + 'a>;

/// A caller-defined action bound to a key.
///
/// When the user presses `key`, the picker calls `handler` with an
/// [`ActionContext`] that provides the current section/item coordinates and
/// mutable terminal access. The handler may take over the screen (e.g., for a
/// preview) and should return when done — the picker redraws automatically.
pub struct PickerAction<'a> {
    pub key: char,
    pub label: &'a str,
    pub handler: ActionHandler<'a>,
}

/// The outcome of a picker interaction.
pub enum PickerOutcome {
    /// User confirmed — returns checked state per section (matching input order).
    Confirmed(Vec<Vec<bool>>),
    /// User cancelled (Esc).
    Cancelled,
}

/// Run an interactive sectioned multi-select picker.
///
/// Sections are rendered with bold headers; items below them have checkboxes.
/// Navigation skips headers automatically. Optional `actions` bind keys to
/// caller-defined handlers that receive the terminal for full-screen takeover.
pub fn run_picker(
    title: &str,
    sections: Vec<Section>,
    actions: Vec<PickerAction<'_>>,
) -> anyhow::Result<PickerOutcome> {
    let mut state = PickerState::new(sections);

    if state.is_empty() {
        return Ok(PickerOutcome::Confirmed(Vec::new()));
    }

    let mut terminal = ratatui::init();
    let result = run_picker_loop(&mut terminal, title, &mut state, actions);
    ratatui::restore();
    result
}

fn run_picker_loop(
    terminal: &mut ratatui::DefaultTerminal,
    title: &str,
    state: &mut PickerState,
    mut actions: Vec<PickerAction<'_>>,
) -> anyhow::Result<PickerOutcome> {
    let action_keys: Vec<char> = actions.iter().map(|a| a.key).collect();

    loop {
        terminal.draw(|frame| render_picker(frame, title, state, &actions))?;

        if event::poll(Duration::from_millis(100))?
            && let Event::Key(key) = event::read()?
        {
            if key.kind != KeyEventKind::Press {
                continue;
            }

            if key.modifiers.contains(KeyModifiers::CONTROL) && key.code == KeyCode::Char('c') {
                return Ok(PickerOutcome::Cancelled);
            }

            match key.code {
                KeyCode::Up | KeyCode::Char('k') => state.move_up(),
                KeyCode::Down | KeyCode::Char('j') => state.move_down(),
                KeyCode::Char(' ') => state.toggle(),
                KeyCode::Enter => {
                    if !state.has_any_checked() {
                        state.toggle();
                    }
                    return Ok(PickerOutcome::Confirmed(state.into_results()));
                }
                KeyCode::Esc | KeyCode::Char('q') => {
                    return Ok(PickerOutcome::Cancelled);
                }
                KeyCode::Char('a') => state.toggle_current_section(),
                KeyCode::Char(c) => {
                    if let Some(idx) = action_keys.iter().position(|&k| k == c) {
                        let (section_idx, item_idx) = state.current_coordinates();
                        let mut ctx = ActionContext {
                            section_idx,
                            item_idx,
                            terminal,
                        };
                        (actions[idx].handler)(&mut ctx);
                    }
                }
                _ => {}
            }
        }
    }
}
