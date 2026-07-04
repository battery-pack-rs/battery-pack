//! Rendering tests: verify the picker's visual output using TestBackend.
//!
//! These render the picker into an in-memory terminal buffer and assert on the
//! text content. This catches regressions in layout, styling, and scroll behavior
//! that pure state tests cannot detect.

use crate::render::render_picker;
use crate::state::{PickerState, section};
use ratatui::Terminal;
use ratatui::backend::TestBackend;
use snapbox::{assert_data_eq, str};

/// Render the picker into an in-memory buffer and return the text content.
fn render_to_string(width: u16, height: u16, title: &str, state: &mut PickerState) -> String {
    let backend = TestBackend::new(width, height);
    let mut terminal = Terminal::new(backend).unwrap();
    terminal
        .draw(|frame| render_picker(frame, title, state, &[]))
        .unwrap();
    terminal.backend().to_string()
}

// ============================================================================
// Basic rendering
// ============================================================================

#[test]
fn renders_title_and_sections() {
    let mut state = PickerState::new(vec![
        section("Features:", &[("logging", true), ("metrics", false)]),
        section("Actions:", &[("Add `ci` template", false)]),
    ]);
    let output = render_to_string(50, 12, "my-pack v1.0", &mut state);

    assert!(output.contains("my-pack v1.0"), "title missing");
    assert!(output.contains("Features:"), "features header missing");
    assert!(output.contains("Actions:"), "actions header missing");
    assert!(output.contains("[x] logging"), "checked item missing");
    assert!(output.contains("[ ] metrics"), "unchecked item missing");
    assert!(
        output.contains("Add `ci` template"),
        "template action missing"
    );
}

#[test]
fn cursor_shown_on_first_item() {
    let mut state = PickerState::new(vec![section("S:", &[("first", false), ("second", false)])]);
    let output = render_to_string(40, 8, "test", &mut state);

    assert!(output.contains("> [ ] first"), "cursor indicator missing");
    assert!(!output.contains("> [ ] second"), "cursor on wrong item");
}

#[test]
fn cursor_moves_with_navigation() {
    let mut state = PickerState::new(vec![section("S:", &[("a", false), ("b", false)])]);
    state.set_visible_height(10);
    state.move_down();

    let output = render_to_string(40, 8, "test", &mut state);

    assert!(!output.contains("> [ ] a"), "cursor still on first");
    assert!(output.contains("> [ ] b"), "cursor not on second");
}

#[test]
fn toggle_updates_checkbox() {
    let mut state = PickerState::new(vec![section("S:", &[("item", false)])]);
    state.toggle();

    let output = render_to_string(40, 8, "test", &mut state);
    assert!(output.contains("[x] item"), "toggle not reflected");
}

#[test]
fn footer_shows_keybindings() {
    let mut state = PickerState::new(vec![section("S:", &[("a", false)])]);
    let output = render_to_string(160, 8, "test", &mut state);

    assert!(output.contains("Navigate"), "navigation hint missing");
    assert!(output.contains("Toggle"), "toggle hint missing");
    assert!(output.contains("Confirm"), "confirm hint missing");
    assert!(output.contains("Cancel"), "cancel hint missing");
}

// ============================================================================
// Custom actions — footer rendering
// ============================================================================

#[test]
fn footer_includes_custom_action_labels() {
    use crate::PickerAction;

    let mut state = PickerState::new(vec![section("S:", &[("a", false)])]);
    let actions = vec![
        PickerAction {
            key: 'p',
            label: "Preview",
            handler: Box::new(|_ctx| {}),
        },
        PickerAction {
            key: 'o',
            label: "Open",
            handler: Box::new(|_ctx| {}),
        },
    ];

    let backend = TestBackend::new(80, 8);
    let mut terminal = Terminal::new(backend).unwrap();
    terminal
        .draw(|frame| render_picker(frame, "test", &mut state, &actions))
        .unwrap();
    let output = terminal.backend().to_string();

    assert!(output.contains("p Preview"), "action 'p Preview' missing");
    assert!(output.contains("o Open"), "action 'o Open' missing");
}

// ============================================================================
// Snapshot tests — full rendered output
// ============================================================================

#[test]
fn snapshot_single_section() {
    let mut state = PickerState::new(vec![section(
        "Dependencies:",
        &[
            ("tokio (1.38)", true),
            ("serde (1.0)", true),
            ("anyhow (1)", false),
        ],
    )]);
    let output = render_to_string(60, 10, "cli-pack v2.0", &mut state);
    assert_data_eq!(
        output,
        str![[r#"
"────────────────────────────────────────────────────────────"
" Dependencies:                                              "
" > [x] tokio (1.38)                                         "
"   [x] serde (1.0)                                          "
"   [ ] anyhow (1)                                           "
"                                                            "
"                                                            "
"                                                            "
"                                                            "
" cli-pack v2.0  ↑↓/jk Navigate | Space Toggle | a Toggle sec"

"#]]
    );
}

#[test]
fn snapshot_multiple_sections() {
    let mut state = PickerState::new(vec![
        section(
            "Features:",
            &[("observability", true), ("resilience", false)],
        ),
        section("Dependencies:", &[("tokio", true)]),
        section("Actions:", &[("Add `ci` template", false)]),
    ]);
    let output = render_to_string(60, 14, "fancy v1.0", &mut state);
    assert_data_eq!(
        output,
        str![[r#"
"────────────────────────────────────────────────────────────"
" Features:                                                  "
" > [x] observability                                        "
"   [ ] resilience                                           "
"                                                            "
" Dependencies:                                              "
"   [x] tokio                                                "
"                                                            "
" Actions:                                                   "
"   [ ] Add `ci` template                                    "
"                                                            "
"                                                            "
"                                                            "
" fancy v1.0  ↑↓/jk Navigate | Space Toggle | a Toggle sectio"

"#]]
    );
}
