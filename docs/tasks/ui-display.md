# ui-display: Render Display Box

## Requirement

Implement the top portion of the calculator UI in `src/ui.rs`: the display box showing the current expression and result. Two right-aligned lines inside a bordered box.

## Design

From the design doc:
```
┌──────────────────────────┐
│              78-65×5     │  <- expression (right-aligned)
│                -247      │  <- result (right-aligned, bold)
├──────────────────────────┤
```

Behavior:
- When no result: bottom line shows the expression (bold), top line is empty
- When result is shown: top line shows expression (dimmer), bottom line shows result (bold)
- Both lines are right-aligned with some padding

Display characters: replace `*` with `×` and `/` with `÷` in the expression display.

## Implementation Suggestion

- `pub fn draw(frame: &mut Frame, app: &App)` as the main entry point
- Use `Layout::vertical` to split into display area (top) and button area (bottom)
- Display box: use a `Block` with borders, render two `Line` widgets inside with `Alignment::Right`
- Style: expression line uses `Style::default().dim()` when result is present, result line uses `Style::default().bold()`
- Calculate the split: display gets ~3 rows height, buttons get the rest

## How to Test

Manual verification:
1. `cargo run` — display box renders with borders
2. Type an expression — it appears right-aligned
3. Press `=` — expression moves to top line (dimmed), result on bottom (bold)

## Dependencies

- **tui-skeleton** — provides the terminal and frame to draw into
- **app-state** — reads `app.expression` and `app.result` for rendering
