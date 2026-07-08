//! Rendering tests: verify the picker's visual output using TestBackend.
//!
//! These render the picker into an in-memory terminal buffer and assert on the
//! text content. This catches regressions in layout, styling, and scroll behavior
//! that pure state tests cannot detect.

use crate::SectionItem;
use crate::render::render_picker;
use crate::state::{PickerState, radio_section, section};
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

    let backend = TestBackend::new(160, 8);
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
" ▼ Dependencies: (2 items selected)                         "
" > [x] tokio (1.38)                                         "
"   [x] serde (1.0)                                          "
"   [ ] anyhow (1)                                           "
"                                                            "
"                                                            "
"                                                            "
"                                                            "
" cli-pack v2.0  ↑↓/jk Navigate | Space Toggle | ←/→ Collapse"

"#]]
    );
}

// ============================================================================
// Radio, collapse, descriptions, and warning banner
// ============================================================================

#[test]
fn render_radio_items_use_bullet_symbols() {
    let mut state = PickerState::new(vec![radio_section(
        "HAL:",
        &[("stm32f4", true), ("nrf52840", false)],
    )]);
    let output = render_to_string(60, 10, "embedded v0.1", &mut state);
    assert_data_eq!(
        output,
        str![[r#"
"────────────────────────────────────────────────────────────"
" ▼ HAL: (stm32f4 selected)                                  "
" > ● stm32f4                                                "
"   ○ nrf52840                                               "
"                                                            "
"                                                            "
"                                                            "
"                                                            "
"                                                            "
" embedded v0.1  ↑↓/jk Navigate | Space Toggle | ←/→ Collapse"

"#]]
    );
}

#[test]
fn render_checkbox_items_use_squares() {
    let mut state = PickerState::new(vec![section("Utils:", &[("a", true), ("b", false)])]);
    let output = render_to_string(60, 10, "pack v1", &mut state);
    assert_data_eq!(
        output,
        str![[r#"
"────────────────────────────────────────────────────────────"
" ▼ Utils: (a selected)                                      "
" > [x] a                                                    "
"   [ ] b                                                    "
"                                                            "
"                                                            "
"                                                            "
"                                                            "
"                                                            "
" pack v1  ↑↓/jk Navigate | Space Toggle | ←/→ Collapse/expan"

"#]]
    );
}

#[test]
fn render_collapsed_section_shows_chevron() {
    let mut state = PickerState::new(vec![section("Utils:", &[("a", false)]).collapsed()]);
    let output = render_to_string(60, 10, "pack v1", &mut state);
    assert_data_eq!(
        output,
        str![[r#"
"────────────────────────────────────────────────────────────"
" ▶ Utils: (pick any number)                                 "
"                                                            "
"                                                            "
"                                                            "
"                                                            "
"                                                            "
"                                                            "
"                                                            "
" pack v1  ↑↓/jk Navigate | Space Toggle | ←/→ Collapse/expan"

"#]]
    );
}

#[test]
fn render_expanded_section_shows_down_chevron() {
    let mut state = PickerState::new(vec![section("Utils:", &[("a", false)])]);
    let output = render_to_string(60, 10, "pack v1", &mut state);
    assert_data_eq!(
        output,
        str![[r#"
"────────────────────────────────────────────────────────────"
" ▼ Utils: (pick any number)                                 "
" > [ ] a                                                    "
"                                                            "
"                                                            "
"                                                            "
"                                                            "
"                                                            "
"                                                            "
" pack v1  ↑↓/jk Navigate | Space Toggle | ←/→ Collapse/expan"

"#]]
    );
}

#[test]
fn render_radio_section_header_shows_constraint() {
    let mut state = PickerState::new(vec![radio_section("HAL:", &[("a", false)])]);
    let output = render_to_string(70, 10, "embedded v0.1", &mut state);
    assert_data_eq!(
        output,
        str![[r#"
"──────────────────────────────────────────────────────────────────────"
" ▼ HAL: (pick at most one)                                            "
" > ○ a                                                                "
"                                                                      "
"                                                                      "
"                                                                      "
"                                                                      "
"                                                                      "
"                                                                      "
" embedded v0.1  ↑↓/jk Navigate | Space Toggle | ←/→ Collapse/expand | "

"#]]
    );
}

#[test]
fn render_radio_multiple_selected_shows_all_filled() {
    let mut state = PickerState::new(vec![radio_section(
        "Allocator:",
        &[("jemalloc", true), ("mimalloc", true)],
    )]);
    let output = render_to_string(60, 12, "svc v1", &mut state);
    assert_data_eq!(
        output,
        str![[r#"
" ⚠ Multiple selections in "Allocator:" — pick one to resolve"
"────────────────────────────────────────────────────────────"
" ▼ Allocator: (2 items selected)                            "
" > ● jemalloc                                               "
"   ● mimalloc                                               "
"                                                            "
"                                                            "
"                                                            "
"                                                            "
"                                                            "
"                                                            "
" svc v1  ↑↓/jk Navigate | Space Toggle | ←/→ Collapse/expand"

"#]]
    );
}

#[test]
fn render_warning_banner_for_pre_existing_conflict() {
    let mut state = PickerState::new(vec![radio_section(
        "Allocator:",
        &[("jemalloc", true), ("mimalloc", true)],
    )]);
    let output = render_to_string(70, 12, "svc v1", &mut state);
    assert_data_eq!(
        output,
        str![[r#"
" ⚠ Multiple selections in "Allocator:" — pick one to resolve          "
"──────────────────────────────────────────────────────────────────────"
" ▼ Allocator: (2 items selected)                                      "
" > ● jemalloc                                                         "
"   ● mimalloc                                                         "
"                                                                      "
"                                                                      "
"                                                                      "
"                                                                      "
"                                                                      "
"                                                                      "
" svc v1  ↑↓/jk Navigate | Space Toggle | ←/→ Collapse/expand | a Toggl"

"#]]
    );
}

#[test]
fn render_item_with_description() {
    let mut state = PickerState::new(vec![
        crate::Section::new(
            "HAL:",
            vec![SectionItem::new("stm32f4", false).with_description("STM32F4xx family")],
        )
        .radio(),
    ]);
    let output = render_to_string(70, 10, "embedded v0.1", &mut state);
    assert_data_eq!(
        output,
        str![[r#"
"──────────────────────────────────────────────────────────────────────"
" ▼ HAL: (pick at most one)                                            "
" > ○ stm32f4    STM32F4xx family                                      "
"                                                                      "
"                                                                      "
"                                                                      "
"                                                                      "
"                                                                      "
"                                                                      "
" embedded v0.1  ↑↓/jk Navigate | Space Toggle | ←/→ Collapse/expand | "

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
" ▼ Features: (observability selected)                       "
" > [x] observability                                        "
"   [ ] resilience                                           "
"                                                            "
" ▼ Dependencies: (tokio selected)                           "
"   [x] tokio                                                "
"                                                            "
" ▼ Actions: (pick any number)                               "
"   [ ] Add `ci` template                                    "
"                                                            "
"                                                            "
"                                                            "
" fancy v1.0  ↑↓/jk Navigate | Space Toggle | ←/→ Collapse/ex"

"#]]
    );
}

// ============================================================================
// Header hint text (selection summary)
// ============================================================================

#[test]
fn radio_header_shows_pick_at_most_one_when_nothing_selected() {
    let mut state = PickerState::new(vec![radio_section("HAL:", &[("esp32", false), ("nrf52840", false)])]);
    let output = render_to_string(60, 8, "test", &mut state);
    assert!(
        output.contains("HAL: (pick at most one)"),
        "expected 'pick at most one' hint, got:\n{output}"
    );
}

#[test]
fn radio_header_shows_selected_item_name() {
    let mut state = PickerState::new(vec![radio_section("HAL:", &[("esp32", false), ("nrf52840", true)])]);
    let output = render_to_string(60, 8, "test", &mut state);
    assert!(
        output.contains("HAL: (nrf52840 selected)"),
        "expected selected item name in header, got:\n{output}"
    );
}

#[test]
fn checkbox_header_shows_pick_any_when_nothing_selected() {
    let mut state = PickerState::new(vec![section("Drivers:", &[("ssd1306", false), ("bme280", false)])]);
    let output = render_to_string(60, 8, "test", &mut state);
    assert!(
        output.contains("Drivers: (pick any number)"),
        "expected 'pick any number' hint, got:\n{output}"
    );
}

#[test]
fn checkbox_header_shows_single_selected_item_name() {
    let mut state = PickerState::new(vec![section("Drivers:", &[("ssd1306", true), ("bme280", false)])]);
    let output = render_to_string(60, 8, "test", &mut state);
    assert!(
        output.contains("Drivers: (ssd1306 selected)"),
        "expected selected item name in header, got:\n{output}"
    );
}

#[test]
fn checkbox_header_shows_count_when_multiple_selected() {
    let mut state = PickerState::new(vec![section("Drivers:", &[("ssd1306", true), ("bme280", true), ("lis3dh", false)])]);
    let output = render_to_string(60, 8, "test", &mut state);
    assert!(
        output.contains("Drivers: (2 items selected)"),
        "expected count in header, got:\n{output}"
    );
}

// ============================================================================
// Collapsed header highlight
// ============================================================================

#[test]
fn collapsed_header_is_highlighted_when_focused() {
    // After collapsing, the cursor lands on the section header and the render
    // must apply the cursor highlight (Cyan background) to that header line.
    use ratatui::style::Color;

    let mut state = PickerState::new(vec![
        section("Utils:", &[("a", false), ("b", false)]),
        section("Other:", &[("c", false)]),
    ]);
    // Cursor starts on first item in "Utils:". Collapse moves it to the header.
    state.collapse_current();

    let backend = TestBackend::new(60, 10);
    let mut terminal = Terminal::new(backend).unwrap();
    terminal
        .draw(|frame| render_picker(frame, "test", &mut state, &[]))
        .unwrap();

    // Row 1 is the header line (row 0 is the top border). The collapsed header
    // should have the Cyan cursor highlight background.
    let buf = terminal.backend().buffer();
    let header_cell = &buf[(2, 1)]; // column 2 = start of "▶" after the padding
    assert_eq!(
        header_cell.bg, Color::Cyan,
        "collapsed header should be highlighted with Cyan background when focused"
    );
}
