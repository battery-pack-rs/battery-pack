//! Picker state: navigation, toggle, scroll, and result extraction.

use crate::Section;
#[cfg(test)]
use crate::SectionItem;

/// A single item in the picker — either a section header or a selectable entry.
pub(crate) enum Entry {
    Header(String),
    Item { label: String, checked: bool },
}

/// The internal state of the picker widget.
///
/// All sections are flattened into a single `entries` vec of headers and items.
/// The `selectable` vec stores indices into `entries` for items only, so the
/// cursor always lands on a selectable item.
pub struct PickerState {
    pub(crate) entries: Vec<Entry>,
    /// Indices into `entries` that are selectable (non-header).
    selectable: Vec<usize>,
    /// For each selectable item, its (section_idx, item_idx) coordinates.
    coordinates: Vec<(usize, usize)>,
    /// Current cursor position within `selectable`.
    cursor: usize,
    /// Scroll offset (in rendered lines) for the viewport.
    pub(crate) scroll: usize,
    /// Height of the visible content area (updated each frame from terminal size).
    pub(crate) visible_height: usize,
}

impl PickerState {
    /// Create a new picker state from the given sections.
    pub fn new(sections: Vec<Section>) -> Self {
        let mut entries = Vec::new();
        let mut selectable = Vec::new();
        let mut coordinates = Vec::new();

        for (section_idx, section) in sections.into_iter().enumerate() {
            entries.push(Entry::Header(section.title));
            for (item_idx, item) in section.items.into_iter().enumerate() {
                selectable.push(entries.len());
                coordinates.push((section_idx, item_idx));
                entries.push(Entry::Item {
                    label: item.label,
                    checked: item.checked,
                });
            }
        }

        Self {
            entries,
            selectable,
            coordinates,
            cursor: 0,
            scroll: 0,
            visible_height: 0,
        }
    }

    /// True if there are no selectable items.
    pub fn is_empty(&self) -> bool {
        self.selectable.is_empty()
    }

    /// Move the cursor up one selectable item.
    pub fn move_up(&mut self) {
        if self.cursor > 0 {
            self.cursor -= 1;
            self.ensure_cursor_visible();
        }
    }

    /// Move the cursor down one selectable item.
    pub fn move_down(&mut self) {
        if self.cursor < self.selectable.len().saturating_sub(1) {
            self.cursor += 1;
            self.ensure_cursor_visible();
        }
    }

    /// Toggle the checked state of the item under the cursor.
    pub fn toggle(&mut self) {
        let idx = self.selectable[self.cursor];
        if let Entry::Item { checked, .. } = &mut self.entries[idx] {
            *checked = !*checked;
        }
    }

    /// Toggle all items in the section that contains the cursor.
    ///
    /// If any item in the section is unchecked, checks all; otherwise unchecks all.
    pub fn toggle_current_section(&mut self) {
        let cursor_entry_idx = self.selectable[self.cursor];

        let header_idx = (0..=cursor_entry_idx)
            .rev()
            .find(|&i| matches!(self.entries[i], Entry::Header(_)))
            .unwrap_or(0);

        let section_end = ((header_idx + 1)..self.entries.len())
            .find(|&i| matches!(self.entries[i], Entry::Header(_)))
            .unwrap_or(self.entries.len());

        let all_checked = (header_idx + 1..section_end)
            .all(|i| matches!(self.entries[i], Entry::Item { checked: true, .. }));
        let target = !all_checked;

        for i in header_idx + 1..section_end {
            if let Entry::Item { checked, .. } = &mut self.entries[i] {
                *checked = target;
            }
        }
    }

    /// The (section_idx, item_idx) for the current cursor position.
    pub fn current_coordinates(&self) -> (usize, usize) {
        self.coordinates[self.cursor]
    }

    /// True if at least one item is checked across all sections.
    pub fn has_any_checked(&self) -> bool {
        self.entries
            .iter()
            .any(|e| matches!(e, Entry::Item { checked: true, .. }))
    }

    /// Extract checked state grouped by section (matching original input order).
    ///
    /// Consumes the state and returns a `Vec<Vec<bool>>` where each inner vec
    /// corresponds to a section in the original input order.
    pub fn into_results(&mut self) -> Vec<Vec<bool>> {
        let entries = std::mem::take(&mut self.entries);
        let mut results: Vec<Vec<bool>> = Vec::new();
        let mut current_section: Vec<bool> = Vec::new();
        let mut seen_header = false;

        for entry in entries {
            match entry {
                Entry::Header(_) => {
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

    /// The entry index for the current cursor position.
    pub(crate) fn current_entry_idx(&self) -> usize {
        self.selectable[self.cursor]
    }

    /// Compute which rendered line the cursor currently occupies.
    ///
    /// The rendered layout inserts a blank line before each section header
    /// (except the first), so the line count is not simply the entry index.
    pub(crate) fn cursor_line(&self) -> usize {
        Self::entry_to_line(&self.entries, self.selectable[self.cursor])
    }

    /// Compute the rendered line of the section header for the current cursor item.
    fn section_header_line(&self) -> usize {
        let cursor_entry_idx = self.selectable[self.cursor];
        let header_entry_idx = (0..=cursor_entry_idx)
            .rev()
            .find(|&i| matches!(self.entries[i], Entry::Header(_)))
            .unwrap_or(0);
        Self::entry_to_line(&self.entries, header_entry_idx)
    }

    /// Map an entry index to its rendered line number.
    fn entry_to_line(entries: &[Entry], target: usize) -> usize {
        let mut line = 0;
        for (i, entry) in entries.iter().enumerate() {
            if i == target {
                return line;
            }
            match entry {
                Entry::Header(_) => {
                    if i > 0 {
                        line += 1; // blank separator before non-first headers
                    }
                    line += 1; // the header itself
                }
                Entry::Item { .. } => {
                    line += 1;
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

/// Helper to construct sections concisely (useful in tests and examples).
#[cfg(test)]
pub fn section(title: impl Into<String>, items: &[(&str, bool)]) -> Section {
    Section {
        title: title.into(),
        items: items
            .iter()
            .map(|(label, checked)| SectionItem {
                label: label.to_string(),
                checked: *checked,
            })
            .collect(),
    }
}
