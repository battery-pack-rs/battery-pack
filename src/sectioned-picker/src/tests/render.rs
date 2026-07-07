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
" ▼ Dependencies:                                            "
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
    // A radio section renders checked/unchecked as ●/○ rather than [x]/[ ].
    let mut state = PickerState::new(vec![radio_section(
        "HAL:",
        &[("stm32f4", true), ("nrf52840", false)],
    )]);
    let output = render_to_string(60, 10, "embedded v0.1", &mut state);
    assert!(
        output.contains("● stm32f4"),
        "checked bullet missing:\n{output}"
    );
    assert!(
        output.contains("○ nrf52840"),
        "unchecked bullet missing:\n{output}"
    );
    assert!(
        !output.contains("[x]"),
        "radio must not use checkbox glyphs"
    );
}

#[test]
fn render_checkbox_items_use_squares() {
    // Regression: checkbox sections keep [x]/[ ] glyphs.
    let mut state = PickerState::new(vec![section("Utils:", &[("a", true), ("b", false)])]);
    let output = render_to_string(60, 10, "pack v1", &mut state);
    assert!(
        output.contains("[x] a"),
        "checked square missing:\n{output}"
    );
    assert!(
        output.contains("[ ] b"),
        "unchecked square missing:\n{output}"
    );
    assert!(!output.contains('●'), "checkbox must not use radio glyphs");
}

#[test]
fn render_collapsed_section_shows_chevron() {
    // A collapsed section header shows ▶; expanded shows ▼.
    let mut state = PickerState::new(vec![section("Utils:", &[("a", false)]).collapsed()]);
    let output = render_to_string(60, 10, "pack v1", &mut state);
    assert!(
        output.contains("▶ Utils:"),
        "collapsed chevron missing:\n{output}"
    );
    assert!(
        !output.contains("  [ ] a"),
        "collapsed items must be hidden"
    );

    let mut expanded = PickerState::new(vec![section("Utils:", &[("a", false)])]);
    let output = render_to_string(60, 10, "pack v1", &mut expanded);
    assert!(
        output.contains("▼ Utils:"),
        "expanded chevron missing:\n{output}"
    );
}

#[test]
fn render_radio_section_header_shows_constraint() {
    // Radio section headers include the "(pick at most one)" hint.
    let mut state = PickerState::new(vec![radio_section("HAL:", &[("a", false)])]);
    let output = render_to_string(70, 10, "embedded v0.1", &mut state);
    assert!(
        output.contains("(pick at most one)"),
        "constraint hint missing:\n{output}"
    );
}

#[test]
fn render_radio_multiple_selected_shows_all_filled() {
    // A pre-existing multi-selection renders both items as filled bullets.
    let mut state = PickerState::new(vec![radio_section(
        "Allocator:",
        &[("jemalloc", true), ("mimalloc", true)],
    )]);
    let output = render_to_string(60, 12, "svc v1", &mut state);
    assert!(
        output.contains("● jemalloc"),
        "first fill missing:\n{output}"
    );
    assert!(
        output.contains("● mimalloc"),
        "second fill missing:\n{output}"
    );
}

#[test]
fn render_warning_banner_for_pre_existing_conflict() {
    // With >1 radio item checked on open, a ⚠ warning banner is shown.
    let mut state = PickerState::new(vec![radio_section(
        "Allocator:",
        &[("jemalloc", true), ("mimalloc", true)],
    )]);
    let output = render_to_string(70, 12, "svc v1", &mut state);
    assert!(output.contains('⚠'), "warning glyph missing:\n{output}");
    assert!(
        output.contains("Multiple selections"),
        "warning text missing:\n{output}"
    );
}

#[test]
fn render_item_with_description() {
    // Item descriptions are shown inline after the label.
    let mut state = PickerState::new(vec![
        crate::Section::new(
            "HAL:",
            vec![SectionItem::new("stm32f4", false).with_description("STM32F4xx family")],
        )
        .radio(),
    ]);
    let output = render_to_string(70, 10, "embedded v0.1", &mut state);
    assert!(
        output.contains("STM32F4xx family"),
        "description missing:\n{output}"
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
" ▼ Features:                                                "
" > [x] observability                                        "
"   [ ] resilience                                           "
"                                                            "
" ▼ Dependencies:                                            "
"   [x] tokio                                                "
"                                                            "
" ▼ Actions:                                                 "
"   [ ] Add `ci` template                                    "
"                                                            "
"                                                            "
"                                                            "
" fancy v1.0  ↑↓/jk Navigate | Space Toggle | ←/→ Collapse/ex"

"#]]
    );
}
