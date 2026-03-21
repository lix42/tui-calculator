# app-state: Application State and Core Logic

## Requirement

Implement `src/app.rs` with the `App` struct that holds all calculator state and provides methods to manipulate it. This is the "model" layer — no rendering, no terminal I/O.

## Design

State fields (from design doc):
- `expression: String` — current input expression (e.g. `"78-65*5"`)
- `result: Option<String>` — evaluated result or error message
- `focus: (usize, usize)` — `(row, col)` of focused button in the grid
- `should_quit: bool` — signals the event loop to exit

Button grid layout (5x4):
```
["C",  "(",  ")",  "÷"]
["7",  "8",  "9",  "×"]
["4",  "5",  "6",  "-"]
["1",  "2",  "3",  "+"]
["⌫",  "0",  ".",  "="]
```

State transitions after evaluation:
- Pressing a digit → clear result, start new expression
- Pressing an operator → append operator to result value (continue calculation)

## Implementation Suggestion

- `App::new()` — initialize with empty expression, no result, focus at (4,3) (the `=` button)
- `App::press_button(label: &str)` — dispatch based on label: digit/op appends, `"C"` clears, `"⌫"` backspaces, `"="` evaluates
- `App::evaluate()` — call `eval::eval(&self.expression)`, store result
- `App::clear()` — reset expression and result
- `App::backspace()` — pop last char from expression
- `App::move_focus(dr: i32, dc: i32)` — move focus with clamping
- `App::focused_label() -> &str` — get label at current focus
- A `BUTTONS` constant: `[[&str; 4]; 5]` for the grid layout

Map display characters to expression characters: `"÷"` → `"/"`, `"×"` → `"*"`.

## How to Test

Unit tests in `src/app.rs`:

```
cargo test
```

- `press_button("5")` then `press_button("+")` then `press_button("3")` → expression is `"5+3"`
- `press_button("=")` after `"5+3"` → result is `Some("8")`
- After result shown, `press_button("2")` → expression is `"2"`, result cleared
- After result shown, `press_button("+")` → expression is `"8+"`, result cleared
- `clear()` resets everything
- `backspace()` removes last char
- `move_focus` clamps within grid bounds

## Dependencies

- **eval-parser** — `evaluate()` calls `eval::eval()`
