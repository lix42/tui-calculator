# key-input: Direct Keyboard Input Handling

## Requirement

Handle direct keyboard input: typing digits, operators, and control keys to manipulate the expression directly, without using the button grid.

## Design

Key mappings (from design doc):
- Digits `0-9`, `.` → append to expression
- `+`, `-`, `*`, `/` → append operator
- `(`, `)` → append parenthesis
- `c` → clear
- `Backspace` → delete last char
- `Enter` or `=` → evaluate
- `q` or `Esc` → quit

These keys work regardless of button focus — they're direct shortcuts.

## Implementation Suggestion

- In the event loop (main.rs), match on `crossterm::event::Event::Key`
- Map `KeyCode::Char('0'..='9')`, `KeyCode::Char('+')`, etc. to `app.press_button()`
- Map `KeyCode::Char('c')` to `app.clear()`
- Map `KeyCode::Backspace` to `app.backspace()`
- Map `KeyCode::Enter` and `KeyCode::Char('=')` to `app.evaluate()`
- Map `KeyCode::Char('q')` and `KeyCode::Esc` to `app.should_quit = true`
- Only handle `KeyEventKind::Press` to avoid double-triggering on key release

## How to Test

Manual verification:
1. `cargo run` — type `2+3`, see expression update
2. Press `Enter` — result `5` appears
3. Press `c` — expression clears
4. Press `Backspace` — last character removed
5. Press `q` — app exits

## Dependencies

- **tui-skeleton** — provides the event loop
- **app-state** — `press_button()`, `clear()`, `backspace()`, `evaluate()` methods
- **ui-display** — to see the expression/result updating (visual verification)
