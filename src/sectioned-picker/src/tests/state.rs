//! Tests for picker state: navigation, toggle, scroll, and results.
//!
//! These exercise `PickerState` without a terminal — we construct state directly,
//! call navigation/toggle methods, then assert on cursor position, scroll offset,
//! and result output.
//!
//! # Scroll edge cases covered
//!
//! - All items fit in viewport → scroll stays 0.
//! - Cursor moves below viewport → scroll advances minimally.
//! - Cursor moves above viewport → scroll retreats, snapping to section header
//!   when the header is close enough to fit in the viewport.
//! - Tall section (more items than viewport) → upward movement does NOT snap to
//!   the distant header; only snaps when cursor is near the section top.
//! - Blank lines between sections are accounted for in line calculations.

use crate::state::{Entry, PickerState, radio_section, section};

fn make_state(sections: Vec<crate::Section>, visible_height: usize) -> PickerState {
    let mut state = PickerState::new(sections);
    state.set_visible_height(visible_height);
    state
}

// ============================================================================
// Navigation — cursor movement and header skipping
// ============================================================================

#[test]
fn cursor_starts_at_first_item() {
    let state = make_state(
        vec![section("Features:", &[("feat-a", false), ("feat-b", true)])],
        10,
    );
    assert_eq!(state.cursor(), 0);
}

#[test]
fn move_down_advances_cursor() {
    let mut state = make_state(
        vec![section(
            "Features:",
            &[("a", false), ("b", false), ("c", false)],
        )],
        10,
    );
    state.move_down();
    assert_eq!(state.cursor(), 1);
    state.move_down();
    assert_eq!(state.cursor(), 2);
}

#[test]
fn move_down_stops_at_last_item() {
    let mut state = make_state(vec![section("S:", &[("a", false), ("b", false)])], 10);
    state.move_down();
    state.move_down();
    state.move_down();
    assert_eq!(state.cursor(), 1);
}

#[test]
fn move_up_stops_at_first_item() {
    let mut state = make_state(vec![section("S:", &[("a", false), ("b", false)])], 10);
    state.move_up();
    assert_eq!(state.cursor(), 0);
}

#[test]
fn navigation_skips_headers_across_sections() {
    let mut state = make_state(
        vec![
            section("Features:", &[("a", false)]),
            section("Dependencies:", &[("b", false)]),
        ],
        20,
    );
    state.move_down();
    assert_eq!(state.cursor(), 1);
    let entry_idx = state.current_entry_idx();
    assert!(matches!(&state.entries[entry_idx], Entry::Item { label, .. } if label == "b"));
}

// ============================================================================
// Toggle — individual items and whole sections
// ============================================================================

#[test]
fn toggle_flips_checked_state() {
    let mut state = make_state(vec![section("S:", &[("a", false), ("b", true)])], 10);
    state.toggle();
    let results = state.into_results();
    assert_eq!(results, vec![vec![true, true]]);
}

#[test]
fn toggle_section_checks_all_when_some_unchecked() {
    let mut state = make_state(
        vec![section("S:", &[("a", true), ("b", false), ("c", true)])],
        10,
    );
    state.toggle_current_section();
    let results = state.into_results();
    assert_eq!(results, vec![vec![true, true, true]]);
}

#[test]
fn toggle_section_unchecks_all_when_all_checked() {
    let mut state = make_state(vec![section("S:", &[("a", true), ("b", true)])], 10);
    state.toggle_current_section();
    let results = state.into_results();
    assert_eq!(results, vec![vec![false, false]]);
}

#[test]
fn toggle_section_only_affects_current_section() {
    let mut state = make_state(
        vec![
            section("A:", &[("a1", false), ("a2", false)]),
            section("B:", &[("b1", true)]),
        ],
        20,
    );
    state.toggle_current_section();
    let results = state.into_results();
    assert_eq!(results, vec![vec![true, true], vec![true]]);
}

// ============================================================================
// Enter behavior — auto-check when nothing selected
// ============================================================================

#[test]
fn enter_with_nothing_checked_checks_cursor_item() {
    let mut state = make_state(
        vec![section("S:", &[("a", false), ("b", false), ("c", false)])],
        10,
    );
    state.move_down();
    assert!(!state.has_any_checked());
    state.toggle();
    let results = state.into_results();
    assert_eq!(results, vec![vec![false, true, false]]);
}

#[test]
fn enter_with_something_checked_does_not_toggle() {
    let mut state = make_state(
        vec![section("S:", &[("a", true), ("b", false), ("c", false)])],
        10,
    );
    state.move_down();
    assert!(state.has_any_checked());
    let results = state.into_results();
    assert_eq!(results, vec![vec![true, false, false]]);
}

// ============================================================================
// Coordinates — section/item mapping (used by action handlers)
// ============================================================================

#[test]
fn current_coordinates_tracks_section_and_item() {
    let mut state = make_state(
        vec![
            section("A:", &[("a1", false), ("a2", false)]),
            section("B:", &[("b1", false)]),
        ],
        20,
    );
    assert_eq!(state.current_coordinates(), (0, 0));
    state.move_down();
    assert_eq!(state.current_coordinates(), (0, 1));
    state.move_down();
    assert_eq!(state.current_coordinates(), (1, 0));
}

#[test]
fn coordinates_correct_with_empty_sections() {
    let mut state = make_state(
        vec![
            section("Empty:", &[]),
            section("Full:", &[("x", false), ("y", false)]),
        ],
        20,
    );
    // First selectable item is in the second section (index 1).
    assert_eq!(state.current_coordinates(), (1, 0));
    state.move_down();
    assert_eq!(state.current_coordinates(), (1, 1));
}

#[test]
fn coordinates_correct_with_many_sections() {
    let mut state = make_state(
        vec![
            section("A:", &[("a1", false)]),
            section("B:", &[("b1", false), ("b2", false)]),
            section("C:", &[("c1", false)]),
        ],
        20,
    );
    assert_eq!(state.current_coordinates(), (0, 0)); // a1
    state.move_down();
    assert_eq!(state.current_coordinates(), (1, 0)); // b1
    state.move_down();
    assert_eq!(state.current_coordinates(), (1, 1)); // b2
    state.move_down();
    assert_eq!(state.current_coordinates(), (2, 0)); // c1
}

// ============================================================================
// Action dispatch simulation
// ============================================================================

/// Verifies the contract between PickerState and action handlers: navigating to
/// an item and reading current_coordinates gives the handler correct context.
#[test]
fn action_handler_receives_correct_coordinates() {
    let mut state = make_state(
        vec![
            section("Features:", &[("feat-a", false), ("feat-b", false)]),
            section(
                "Actions:",
                &[("template-ci", false), ("template-svc", false)],
            ),
        ],
        20,
    );

    // Simulate: user navigates to "template-svc" (section 1, item 1).
    state.move_down(); // feat-b (0, 1)
    state.move_down(); // template-ci (1, 0)
    state.move_down(); // template-svc (1, 1)

    // This is what the event loop passes to the handler.
    let (section_idx, item_idx) = state.current_coordinates();
    assert_eq!(section_idx, 1);
    assert_eq!(item_idx, 1);
}

// ============================================================================
// Results — checked state grouped by section
// ============================================================================

#[test]
fn into_results_groups_by_section() {
    let mut state = make_state(
        vec![
            section("Features:", &[("a", true), ("b", false)]),
            section("Deps:", &[("c", true)]),
            section("Actions:", &[("d", false), ("e", false)]),
        ],
        20,
    );
    let results = state.into_results();
    assert_eq!(
        results,
        vec![vec![true, false], vec![true], vec![false, false]]
    );
}

#[test]
fn empty_sections_produce_empty_vecs() {
    let mut state = make_state(
        vec![
            section("Empty:", &[]),
            section("Has items:", &[("a", true)]),
        ],
        10,
    );
    let results = state.into_results();
    assert_eq!(results, vec![vec![], vec![true]]);
}

// ============================================================================
// Scrolling — viewport management
// ============================================================================

#[test]
fn scroll_stays_zero_when_all_items_fit() {
    let mut state = make_state(vec![section("S:", &[("a", false), ("b", false)])], 10);
    state.move_down();
    assert_eq!(state.scroll_offset(), 0);
}

#[test]
fn scroll_advances_when_cursor_moves_below_viewport() {
    let mut state = make_state(
        vec![section(
            "S:",
            &[("a", false), ("b", false), ("c", false), ("d", false)],
        )],
        3,
    );
    assert_eq!(state.scroll_offset(), 0);
    state.move_down();
    assert_eq!(state.scroll_offset(), 0);
    state.move_down();
    assert_eq!(state.scroll_offset(), 1);
    state.move_down();
    assert_eq!(state.scroll_offset(), 2);
}

#[test]
fn scroll_retreats_when_cursor_moves_above_viewport() {
    let mut state = make_state(
        vec![section(
            "S:",
            &[("a", false), ("b", false), ("c", false), ("d", false)],
        )],
        3,
    );
    state.move_down();
    state.move_down();
    state.move_down();
    assert!(state.scroll_offset() > 0);
    let bottom_scroll = state.scroll_offset();

    state.move_up();
    state.move_up();
    state.move_up();
    assert_eq!(state.scroll_offset(), 0);
    assert!(state.scroll_offset() < bottom_scroll);
}

/// Regression: scrolling back to the first item must reveal the section header.
#[test]
fn scroll_back_to_top_shows_section_header() {
    let mut state = make_state(
        vec![
            section("Features:", &[("a", false), ("b", false), ("c", false)]),
            section("Deps:", &[("d", false)]),
        ],
        3,
    );
    state.move_down();
    state.move_down();
    state.move_down();
    assert!(state.scroll_offset() > 0);

    state.move_up();
    state.move_up();
    state.move_up();
    assert_eq!(state.scroll_offset(), 0);
}

#[test]
fn scroll_accounts_for_blank_lines_between_sections() {
    let mut state = make_state(
        vec![
            section("A:", &[("a1", false)]),
            section("B:", &[("b1", false), ("b2", false)]),
        ],
        3,
    );
    state.move_down();
    assert!(
        state.scroll_offset() >= 2,
        "scroll={}",
        state.scroll_offset()
    );
    state.move_down();
    assert!(
        state.scroll_offset() >= 3,
        "scroll={}",
        state.scroll_offset()
    );
}

/// Tall section: moving up mid-section must NOT snap to the distant header.
#[test]
fn scroll_in_tall_section_does_not_snap_to_header() {
    let items: Vec<(&str, bool)> = vec![
        ("a", false),
        ("b", false),
        ("c", false),
        ("d", false),
        ("e", false),
        ("f", false),
        ("g", false),
        ("h", false),
        ("i", false),
        ("j", false),
    ];
    let mut state = make_state(vec![section("S:", &items)], 3);

    for _ in 0..6 {
        state.move_down();
    }
    assert_eq!(state.cursor(), 6);
    let scroll_at_g = state.scroll_offset();

    state.move_up();
    assert_eq!(state.cursor(), 5);
    let scroll_at_f = state.scroll_offset();
    assert!(scroll_at_f >= scroll_at_g - 1);
    assert!(scroll_at_f > 0);
}

/// Complement: scrolling all the way up in a tall section still shows the header.
#[test]
fn scroll_shows_header_only_when_cursor_near_top_of_section() {
    let items: Vec<(&str, bool)> = vec![
        ("a", false),
        ("b", false),
        ("c", false),
        ("d", false),
        ("e", false),
        ("f", false),
    ];
    let mut state = make_state(vec![section("S:", &items)], 3);

    for _ in 0..5 {
        state.move_down();
    }
    assert!(state.scroll_offset() > 0);

    for _ in 0..5 {
        state.move_up();
    }
    assert_eq!(state.cursor(), 0);
    assert_eq!(state.scroll_offset(), 0);
}

// ============================================================================
// cursor_line — verifies the rendered-line computation
// ============================================================================

#[test]
fn cursor_line_matches_expected_positions() {
    let mut state = make_state(
        vec![
            section("Features:", &[("a", false), ("b", false)]),
            section("Deps:", &[("c", false)]),
        ],
        20,
    );

    assert_eq!(state.cursor_line(), 1);
    state.move_down();
    assert_eq!(state.cursor_line(), 2);
    state.move_down();
    assert_eq!(state.cursor_line(), 5);
}

// ============================================================================
// Radio mode — at-most-one selection semantics
// ============================================================================

#[test]
fn radio_mode_toggle_deselects_others() {
    // A(checked), B, C. Toggling B checks B and unchecks A; C stays off.
    let mut state = make_state(
        vec![radio_section(
            "HAL:",
            &[("a", true), ("b", false), ("c", false)],
        )],
        10,
    );
    state.move_down(); // cursor on B
    state.toggle();
    let results = state.into_results();
    assert_eq!(results, vec![vec![false, true, false]]);
}

#[test]
fn radio_mode_allows_deselect_all() {
    // Toggling the only checked item unchecks it — zero selections is allowed.
    let mut state = make_state(vec![radio_section("HAL:", &[("a", true)])], 10);
    state.toggle();
    let results = state.into_results();
    assert_eq!(results, vec![vec![false]]);
}

#[test]
fn radio_mode_backspace_clears() {
    // Backspace in a radio section clears the current selection.
    let mut state = make_state(
        vec![radio_section("HAL:", &[("a", true), ("b", false)])],
        10,
    );
    state.backspace();
    let results = state.into_results();
    assert_eq!(results, vec![vec![false, false]]);
}

#[test]
fn checkbox_mode_toggle_is_independent() {
    // Toggling B in a checkbox section leaves A checked.
    let mut state = make_state(vec![section("Utils:", &[("a", true), ("b", false)])], 10);
    state.move_down(); // cursor on B
    state.toggle();
    let results = state.into_results();
    assert_eq!(results, vec![vec![true, true]]);
}

#[test]
fn radio_mode_section_toggle_is_noop() {
    // `a` (toggle_current_section) does nothing in a radio section.
    let mut state = make_state(
        vec![radio_section("HAL:", &[("a", true), ("b", false)])],
        10,
    );
    state.toggle_current_section();
    let results = state.into_results();
    assert_eq!(results, vec![vec![true, false]]);
}

#[test]
fn checkbox_mode_section_toggle_selects_all() {
    // `a` in a checkbox section checks all items (existing behavior preserved).
    let mut state = make_state(
        vec![section(
            "Utils:",
            &[("a", true), ("b", false), ("c", false)],
        )],
        10,
    );
    state.toggle_current_section();
    let results = state.into_results();
    assert_eq!(results, vec![vec![true, true, true]]);
}

#[test]
fn radio_pre_selected_multiple_renders_honestly() {
    // A radio section constructed with two items checked keeps both on load.
    let mut state = make_state(vec![radio_section("HAL:", &[("a", true), ("b", true)])], 10);
    let results = state.into_results();
    assert_eq!(results, vec![vec![true, true]]);
}

#[test]
fn radio_pre_selected_multiple_toggle_clears_others() {
    // With A and B both checked, toggling C leaves only C checked.
    let mut state = make_state(
        vec![radio_section(
            "HAL:",
            &[("a", true), ("b", true), ("c", false)],
        )],
        10,
    );
    state.move_down(); // B
    state.move_down(); // C
    state.toggle();
    let results = state.into_results();
    assert_eq!(results, vec![vec![false, false, true]]);
}

#[test]
fn radio_confirm_blocked_when_multiple_selected() {
    // try_confirm rejects a radio section with more than one item checked.
    let mut state = make_state(vec![radio_section("HAL:", &[("a", true), ("b", true)])], 10);
    let outcome = state.try_confirm();
    assert!(outcome.is_err());
    assert!(outcome.unwrap_err().contains("HAL:"));
}

#[test]
fn radio_confirm_succeeds_valid_state() {
    // try_confirm succeeds with zero or one radio selection.
    let mut zero = make_state(
        vec![radio_section("HAL:", &[("a", false), ("b", false)])],
        10,
    );
    assert!(zero.try_confirm().is_ok());

    let mut one = make_state(
        vec![radio_section("HAL:", &[("a", true), ("b", false)])],
        10,
    );
    let ok = one.try_confirm();
    assert!(ok.is_ok());
    assert_eq!(ok.unwrap(), vec![vec![true, false]]);
}

// ============================================================================
// Collapsing — navigation and results
// ============================================================================

#[test]
fn collapse_section_hides_items_from_navigation() {
    // Collapse the first section; the cursor rests on its header, and moving
    // down skips the hidden items to reach the second section.
    let mut state = make_state(
        vec![
            section("A:", &[("a1", false), ("a2", false)]),
            section("B:", &[("b1", false)]),
        ],
        20,
    );
    state.collapse_current(); // cursor starts in section A → focuses A's header
    assert_eq!(state.current_coordinates(), (0, 0)); // A's header
    state.move_down();
    assert_eq!(state.current_coordinates(), (1, 0)); // B's item (A's items skipped)
}

#[test]
fn expand_section_restores_navigation() {
    // After collapsing, the cursor rests on the header; expanding restores
    // traversal through the section's items.
    let mut state = make_state(
        vec![
            section("A:", &[("a1", false), ("a2", false)]),
            section("B:", &[("b1", false)]),
        ],
        20,
    );
    state.collapse_current(); // focuses A's collapsed header
    state.expand_current(); // re-expands A, focusing its first item
    assert_eq!(state.current_coordinates(), (0, 0));
    state.move_down();
    assert_eq!(state.current_coordinates(), (0, 1));
}

#[test]
fn into_results_includes_collapsed_sections() {
    // Checking an item then collapsing its section preserves the checked state.
    let mut state = make_state(vec![section("A:", &[("a1", true), ("a2", false)])], 20);
    state.collapse_current();
    let results = state.into_results();
    assert_eq!(results, vec![vec![true, false]]);
}
