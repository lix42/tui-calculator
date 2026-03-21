# mouse-input: Mouse Click Support

## Requirement

Allow clicking on calculator buttons with the mouse to activate them.

## Design

From the design doc:
- Click on a button → activate it
- Each button occupies a known `Rect`; hit-test on click coordinates

The mouse position from crossterm gives `(column, row)` coordinates. Compare against the stored `Rect` for each button to determine which button was clicked.

## Implementation Suggestion

- Store button rects from the last render pass (e.g., in `App` or a separate struct passed between draw and event handling)
- In the event handler, match `Event::Mouse(MouseEvent { kind: MouseEventKind::Down(MouseButton::Left), column, row, .. })`
- Iterate over stored button rects, find which one contains `(column, row)`
- If found, call `app.press_button(label)` and update `app.focus` to the clicked button
- If click is outside all buttons, ignore

## How to Test

Manual verification:
1. `cargo run` — buttons are visible
2. Click on `7` — `7` appears in expression
3. Click on `+` — `+` appended
4. Click on `=` — expression evaluates
5. Click on `C` — expression clears
6. Click outside buttons — nothing happens

## Dependencies

- **ui-buttons** — provides the button `Rect` positions for hit-testing
- **app-state** — `press_button()` to handle the click action
- **tui-skeleton** — mouse capture must be enabled in terminal setup
