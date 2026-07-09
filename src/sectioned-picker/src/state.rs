//! Picker state: navigation, toggle, scroll, and result extraction.

#[cfg(test)]
use crate::SectionItem;
use crate::{Section, SelectionMode};

/// A single item in the picker — either a section header or a selectable entry.
pub(crate) enum Entry {
    Header {
        title: String,
        mode: SelectionMode,
        collapsed: bool,
    },
    Item {
        label: String,
        checked: bool,
        description: Option<String>,
    },
}

/// The internal state of the picker widget.
///
/// All sections are flattened into a single `entries` vec of headers and items.
/// Navigation moves the cursor between *nav stops* — visible items plus the
/// headers of collapsed sections — so a collapsed section can still be focused
/// and re-expanded. When nothing is collapsed, the nav stops are exactly the
/// items, matching a plain multi-select list.
pub struct PickerState {
    pub(crate) entries: Vec<Entry>,
    /// (section_idx, item_ordinal) for each entry. Header entries carry their
    /// section index and item ordinal 0.
    coords: Vec<(usize, usize)>,
    /// Per-entry visibility; item entries in a collapsed section are hidden.
    /// Header entries are always visible.
    pub(crate) visible: Vec<bool>,
    /// Entry indices that are currently focusable; rebuilt on collapse/expand.
    nav_stops: Vec<usize>,
    /// Cursor position as an index into `nav_stops`.
    cursor: usize,
    /// Scroll offset (in rendered lines) for the viewport.
    pub(crate) scroll: usize,
    /// Height of the visible content area (updated each frame from terminal size).
    pub(crate) visible_height: usize,
    /// Transient inline error shown after a rejected confirm; cleared on the
    /// next keypress.
    pub(crate) confirm_error: Option<String>,
}

impl PickerState {
    /// Create a new picker state from the given sections.
    pub fn new(sections: Vec<Section>) -> Self {
        // Flatten sections into a single entries vec, recording per-entry
        // coordinates and initial visibility.
        let mut entries = Vec::new();
        let mut coords = Vec::new();
        let mut visible = Vec::new();

        for (section_idx, section) in sections.into_iter().enumerate() {
            entries.push(Entry::Header {
                title: section.title,
                mode: section.selection_mode,
                collapsed: section.collapsed,
            });
            coords.push((section_idx, 0));
            visible.push(true); // headers are always visible
            for (item_idx, item) in section.items.into_iter().enumerate() {
                coords.push((section_idx, item_idx));
                visible.push(!section.collapsed);
                entries.push(Entry::Item {
                    label: item.label,
                    checked: item.checked,
                    description: item.description,
                });
            }
        }

        let mut state = Self {
            entries,
            coords,
            visible,
            nav_stops: Vec::new(),
            cursor: 0,
            scroll: 0,
            visible_height: 0,
            confirm_error: None,
        };
        state.rebuild_nav_stops();
        state
    }

    /// Rebuild the list of focusable entries: every visible item, plus the
    /// header of each collapsed section. Entry order is preserved.
    fn rebuild_nav_stops(&mut self) {
        self.nav_stops = (0..self.entries.len())
            .filter(|&i| self.is_nav_stop(i))
            .collect();
    }

    /// True if entry `i` is focusable — a visible item or a collapsed header.
    fn is_nav_stop(&self, i: usize) -> bool {
        match &self.entries[i] {
            Entry::Header { collapsed, .. } => *collapsed,
            Entry::Item { .. } => self.visible[i],
        }
    }

    /// True if there are no focusable entries.
    pub fn is_empty(&self) -> bool {
        self.nav_stops.is_empty()
    }

    /// Move the cursor up to the previous nav stop.
    pub fn move_up(&mut self) {
        if self.cursor > 0 {
            self.cursor -= 1;
            self.ensure_cursor_visible();
        }
    }

    /// Move the cursor down to the next nav stop.
    pub fn move_down(&mut self) {
        if self.cursor + 1 < self.nav_stops.len() {
            self.cursor += 1;
            self.ensure_cursor_visible();
        }
    }

    /// The entry index the cursor currently points at.
    pub(crate) fn current_entry_idx(&self) -> usize {
        self.nav_stops[self.cursor]
    }

    /// Find the `[header_idx, section_end)` entry range of the section that
    /// contains entry `entry_idx`. `section_end` is exclusive (the next header
    /// or the end of the entries vec).
    fn section_bounds(&self, entry_idx: usize) -> (usize, usize) {
        let header_idx = (0..=entry_idx)
            .rev()
            .find(|&i| matches!(self.entries[i], Entry::Header { .. }))
            .unwrap_or(0);
        let section_end = ((header_idx + 1)..self.entries.len())
            .find(|&i| matches!(self.entries[i], Entry::Header { .. }))
            .unwrap_or(self.entries.len());
        (header_idx, section_end)
    }

    /// The selection mode of the section that contains the cursor.
    pub(crate) fn current_section_mode(&self) -> SelectionMode {
        let (header_idx, _) = self.section_bounds(self.current_entry_idx());
        match &self.entries[header_idx] {
            Entry::Header { mode, .. } => *mode,
            _ => SelectionMode::Checkbox,
        }
    }

    /// Toggle the checked state of the item under the cursor.
    ///
    /// In a radio section, checking an item first unchecks every other item in
    /// the section; toggling an already-checked item unchecks it when it's the
    /// only selection (so empty is reachable), but *keeps* it checked when
    /// multiple items were selected (resolving a pre-existing conflict by
    /// confirming the toggled choice). A no-op when the cursor is on a header.
    pub fn toggle(&mut self) {
        let idx = self.current_entry_idx();
        if !matches!(self.entries[idx], Entry::Item { .. }) {
            return;
        }

        if self.current_section_mode() == SelectionMode::Radio {
            let was_checked = matches!(self.entries[idx], Entry::Item { checked: true, .. });
            let (header_idx, section_end) = self.section_bounds(idx);

            // Count how many siblings are currently checked (before clearing).
            let checked_count = (header_idx + 1..section_end)
                .filter(|&i| matches!(self.entries[i], Entry::Item { checked: true, .. }))
                .count();

            // Clear all siblings.
            for i in header_idx + 1..section_end {
                if let Entry::Item { checked, .. } = &mut self.entries[i] {
                    *checked = false;
                }
            }

            // Re-check the toggled item unless it was the sole selection (allowing
            // deselect-to-zero in the normal single-select case).
            let keep = !was_checked || checked_count > 1;
            if keep && let Entry::Item { checked, .. } = &mut self.entries[idx] {
                *checked = true;
            }
            return;
        }

        if let Entry::Item { checked, .. } = &mut self.entries[idx] {
            *checked = !*checked;
        }
    }

    /// Toggle all items in the section that contains the cursor.
    ///
    /// In a checkbox section: if any item is unchecked, checks all; otherwise
    /// unchecks all. In a radio section this is a no-op — a radio section can
    /// never have every item selected.
    pub fn toggle_current_section(&mut self) {
        if self.current_section_mode() == SelectionMode::Radio {
            return;
        }

        let (header_idx, section_end) = self.section_bounds(self.current_entry_idx());

        let all_checked = (header_idx + 1..section_end)
            .all(|i| matches!(self.entries[i], Entry::Item { checked: true, .. }));
        let target = !all_checked;

        for i in header_idx + 1..section_end {
            if let Entry::Item { checked, .. } = &mut self.entries[i] {
                *checked = target;
            }
        }
    }

    /// Uncheck every item in the section that contains the cursor.
    ///
    /// Bound to Backspace in radio sections to clear the current pick.
    pub fn clear_current_section(&mut self) {
        let (header_idx, section_end) = self.section_bounds(self.current_entry_idx());
        for i in header_idx + 1..section_end {
            if let Entry::Item { checked, .. } = &mut self.entries[i] {
                *checked = false;
            }
        }
    }

    /// Backspace handler: clears the current section only when it is radio.
    pub fn backspace(&mut self) {
        if self.current_section_mode() == SelectionMode::Radio {
            self.clear_current_section();
        }
    }

    /// The (section_idx, item_idx) for the current cursor position.
    ///
    /// When the cursor rests on a collapsed header, the item index is 0.
    pub fn current_coordinates(&self) -> (usize, usize) {
        self.coords[self.current_entry_idx()]
    }

    /// True if at least one item is checked across all sections.
    pub fn has_any_checked(&self) -> bool {
        self.entries
            .iter()
            .any(|e| matches!(e, Entry::Item { checked: true, .. }))
    }

    /// Collapse the section that contains the cursor and focus its header, so
    /// the section can be re-expanded from that position.
    pub fn collapse_current(&mut self) {
        let (header_idx, section_end) = self.section_bounds(self.current_entry_idx());
        if let Entry::Header { collapsed, .. } = &mut self.entries[header_idx] {
            *collapsed = true;
        }
        for i in header_idx + 1..section_end {
            self.visible[i] = false;
        }
        self.rebuild_nav_stops();
        self.focus_entry(header_idx);
        self.ensure_cursor_visible();
    }

    /// Expand the section that contains the cursor and focus its first item (if
    /// any), restoring normal traversal through the section.
    pub fn expand_current(&mut self) {
        let (header_idx, section_end) = self.section_bounds(self.current_entry_idx());
        if let Entry::Header { collapsed, .. } = &mut self.entries[header_idx] {
            *collapsed = false;
        }
        for i in header_idx + 1..section_end {
            self.visible[i] = true;
        }
        self.rebuild_nav_stops();
        // Prefer the first item of the expanded section; fall back to the header.
        let target = (header_idx + 1..section_end)
            .find(|&i| matches!(self.entries[i], Entry::Item { .. }))
            .unwrap_or(header_idx);
        self.focus_entry(target);
        self.ensure_cursor_visible();
    }

    /// Point the cursor at the nav stop for `entry_idx`, or the nearest nav stop
    /// at or before it if that entry is not itself a stop.
    fn focus_entry(&mut self, entry_idx: usize) {
        self.cursor = match self.nav_stops.iter().position(|&e| e == entry_idx) {
            Some(pos) => pos,
            None => self
                .nav_stops
                .iter()
                .rposition(|&e| e <= entry_idx)
                .unwrap_or(0),
        };
    }

    /// Count checked items in the section whose header is at `header_idx`.
    fn section_checked_count(&self, header_idx: usize) -> usize {
        let (_, section_end) = self.section_bounds(header_idx);
        (header_idx + 1..section_end)
            .filter(|&i| matches!(self.entries[i], Entry::Item { checked: true, .. }))
            .count()
    }

    /// Build the parenthetical hint for a section header based on its mode and
    /// current selection state.
    pub(crate) fn section_hint(&self, header_idx: usize) -> String {
        let Entry::Header { mode, .. } = &self.entries[header_idx] else {
            return String::new();
        };
        let (_, section_end) = self.section_bounds(header_idx);

        // Collect checked item labels.
        let checked: Vec<&str> = (header_idx + 1..section_end)
            .filter_map(|i| match &self.entries[i] {
                Entry::Item {
                    label,
                    checked: true,
                    ..
                } => Some(label.as_str()),
                _ => None,
            })
            .collect();

        match (mode, checked.len()) {
            (SelectionMode::Radio, 0) => " (pick at most one)".to_string(),
            (SelectionMode::Radio, 1) => format!(" ({} selected)", checked[0]),
            (SelectionMode::Radio, n) => format!(" ({n} items selected)"),
            (SelectionMode::Checkbox, 0) => " (pick any number)".to_string(),
            (SelectionMode::Checkbox, 1) => format!(" ({} selected)", checked[0]),
            (SelectionMode::Checkbox, n) => format!(" ({n} items selected)"),
        }
    }

    /// The title of the first radio section with more than one item checked, if
    /// any. Used to show a warning banner for a pre-existing conflicting state.
    pub fn radio_conflict_title(&self) -> Option<&str> {
        for (i, entry) in self.entries.iter().enumerate() {
            if let Entry::Header {
                title,
                mode: SelectionMode::Radio,
                ..
            } = entry
                && self.section_checked_count(i) > 1
            {
                return Some(title.as_str());
            }
        }
        None
    }

    /// Validate radio constraints and, if satisfied, extract the results.
    ///
    /// Returns `Err` naming the offending section when a radio section has more
    /// than one item checked.
    pub fn try_confirm(&mut self) -> Result<Vec<Vec<bool>>, String> {
        if let Some(title) = self.radio_conflict_title() {
            return Err(format!("category '{title}' allows at most one selection"));
        }
        Ok(self.into_results())
    }

    /// Store a transient inline error to show after a rejected confirm.
    pub(crate) fn set_confirm_error(&mut self, msg: String) {
        self.confirm_error = Some(msg);
    }

    /// Clear any transient confirm error (called on the next keypress).
    pub(crate) fn clear_confirm_error(&mut self) {
        self.confirm_error = None;
    }

    /// Extract checked state grouped by section (matching original input order).
    ///
    /// Collapsed sections are included — collapse only hides items from the UI,
    /// never from the results.
    pub fn into_results(&mut self) -> Vec<Vec<bool>> {
        let entries = std::mem::take(&mut self.entries);
        let mut results: Vec<Vec<bool>> = Vec::new();
        let mut current_section: Vec<bool> = Vec::new();
        let mut seen_header = false;

        for entry in entries {
            match entry {
                Entry::Header { .. } => {
                    if seen_header {
                        results.push(std::mem::take(&mut current_section));
                    }
                    seen_header = true;
                }
                Entry::Item { checked, .. } => {
                    current_section.push(checked);
                }
            }
        }
        if seen_header {
            results.push(current_section);
        }
        results
    }

    /// Compute which rendered line the cursor currently occupies.
    ///
    /// The rendered layout inserts a blank line before each section header
    /// (except the first), so the line count is not simply the entry index.
    pub(crate) fn cursor_line(&self) -> usize {
        self.entry_to_line(self.current_entry_idx())
    }

    /// Compute the rendered line of the section header for the current cursor.
    fn section_header_line(&self) -> usize {
        let (header_entry_idx, _) = self.section_bounds(self.current_entry_idx());
        self.entry_to_line(header_entry_idx)
    }

    /// Map an entry index to its rendered line number.
    ///
    /// Each header and each visible item occupies one line; a blank separator
    /// precedes every non-first header. Items hidden by a collapsed section
    /// contribute no lines, mirroring the render loop.
    fn entry_to_line(&self, target: usize) -> usize {
        let mut line = 0;
        for (i, entry) in self.entries.iter().enumerate() {
            if i == target {
                return line;
            }
            match entry {
                Entry::Header { .. } => {
                    if i > 0 {
                        line += 1; // blank separator before non-first headers
                    }
                    line += 1; // the header itself
                }
                Entry::Item { .. } => {
                    if self.visible[i] {
                        line += 1;
                    }
                }
            }
        }
        line
    }

    /// Adjust scroll so the cursor stays within the visible viewport.
    ///
    /// When scrolling up, we try to include the section header if the cursor is
    /// close enough that both cursor and header fit in one viewport. For tall
    /// sections where the header is far above, we just scroll to the cursor.
    pub(crate) fn ensure_cursor_visible(&mut self) {
        if self.visible_height == 0 {
            return;
        }
        let cursor_line = self.cursor_line();

        // Scroll up if cursor is above viewport.
        if cursor_line < self.scroll {
            let header_line = self.section_header_line();
            // Show the header too, but only if cursor-to-header distance fits.
            if cursor_line - header_line < self.visible_height {
                self.scroll = header_line;
            } else {
                self.scroll = cursor_line;
            }
        }

        // Scroll down if cursor is below viewport.
        if cursor_line >= self.scroll + self.visible_height {
            self.scroll = cursor_line - self.visible_height + 1;
        }
    }

    /// Current cursor index (for testing).
    pub fn cursor(&self) -> usize {
        self.cursor
    }

    /// Current scroll offset (for testing).
    pub fn scroll_offset(&self) -> usize {
        self.scroll
    }

    /// Set the visible height (normally set during render; exposed for testing).
    pub fn set_visible_height(&mut self, h: usize) {
        self.visible_height = h;
    }
}

/// Helper to construct a checkbox section concisely (used in tests).
#[cfg(test)]
pub fn section(title: impl Into<String>, items: &[(&str, bool)]) -> Section {
    Section::new(
        title,
        items
            .iter()
            .map(|(label, checked)| SectionItem::new(*label, *checked))
            .collect(),
    )
}

/// Helper to construct a radio section concisely (used in tests).
#[cfg(test)]
pub fn radio_section(title: impl Into<String>, items: &[(&str, bool)]) -> Section {
    section(title, items).radio()
}
