# sectioned-picker

`sectioned-picker` is an interactive terminal multi-select picker with
non-selectable section headers. It uses [ratatui] and supports checkbox and
radio sections, keyboard navigation, collapsible sections, item descriptions,
and custom key actions.

```rust,no_run
use sectioned_picker::{PickerOutcome, Section, SectionItem, run_picker};

let sections = vec![
    Section::new(
        "Features:",
        vec![
            SectionItem::new("logging", true),
            SectionItem::new("metrics", false),
        ],
    ),
    Section::new(
        "Output:",
        vec![
            SectionItem::new("human-readable", true),
            SectionItem::new("JSON", false),
        ],
    )
    .radio(),
];

match run_picker("Choose options", sections, Vec::new())? {
    PickerOutcome::Confirmed(selections) => println!("{selections:?}"),
    PickerOutcome::Cancelled => println!("cancelled"),
}
# Ok::<(), anyhow::Error>(())
```

## Controls

- Arrow keys or `j`/`k` move between items.
- Space toggles the current item.
- `a` toggles every item in a checkbox section.
- Left and Right collapse or expand a section.
- Enter confirms the selection.
- Escape cancels the picker.

## License

Licensed under either of [Apache License, Version 2.0](LICENSE-APACHE) or
[MIT license](LICENSE-MIT), at your option.

[ratatui]: https://ratatui.rs/
