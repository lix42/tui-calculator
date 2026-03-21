# ui-buttons: Render Button Grid with Focus

## Requirement

Render the 5x4 button grid below the display box. The currently focused button should be visually highlighted. Each button occupies a rectangular area that will later be used for mouse hit-testing.

## Design

Button layout (from design doc):
```
│  C    (    )    ÷   │  row 0
│  7    8    9    ×   │  row 1
│  4    5    6    -   │  row 2
│  1    2    3    +   │  row 3
│  ⌫    0    .    =   │  row 4
```

- Each button is a bordered cell with centered text
- Focused button: highlighted background or distinct border color
- The grid should fill the available space below the display

## Implementation Suggestion

- Use `Layout::vertical` to create 5 equal rows, then `Layout::horizontal` for 4 columns in each row
- Each button: `Paragraph::new(label).alignment(Center)` inside a `Block` with borders
- Focused button: use `Style::default().bg(Color::Yellow).fg(Color::Black)` or similar
- Store the computed `Rect` for each button in a structure accessible to the mouse handler (e.g., return a `Vec<Vec<Rect>>` from the draw function, or store in `App`)
- Consider storing button rects in `App` so mouse-input task can use them for hit-testing

## How to Test

Manual verification:
1. `cargo run` — button grid renders with all 20 buttons visible
2. A default button is highlighted (focused)
3. Grid resizes reasonably with terminal size changes
4. Button labels are centered and readable

## Dependencies

- **tui-skeleton** — provides the frame
- **app-state** — reads `app.focus` to know which button to highlight, reads `BUTTONS` for labels
