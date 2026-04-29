# Progress

## Completed

### eval-parser — `src/eval.rs`
Recursive descent parser and evaluator. Handles `+-*/`, parentheses, unary
minus, decimals, and whitespace. Returns `Result<f64, String>`. 8 unit tests,
all passing.

### app-state — `src/app.rs`
`App` struct with all calculator state and methods. 10 unit tests, all passing.

Key implementation details:
- `BUTTONS: [[&str; 4]; 5]` — 5×4 grid, default focus at `(4,3)` (`"="`)
- `press_button(&str)` dispatches to `clear / backspace / evaluate / append`
- `append` maps display chars to expression chars (`"÷"→"/"`, `"×"→"*"`)
- Post-eval state tracked via `result.is_some()`: digit → fresh expression,
  operator → continue from result value
- `format_number`: integers as `"8"` (not `"8.0"`), decimals trimmed to 10
  places with trailing zeros stripped

`src/main.rs` has a temporary placeholder that exercises `App`; it will be
replaced entirely by `tui-skeleton`.

## Known Issues / Deferred

Three follow-up tasks were added to `docs/TASKS.md` during implementation:

- **app-result-state**: `result: Option<String>` conflates a numeric result and
  an error message. Should become `Option<EvalResult>` where `EvalResult` is
  `Value(f64) | Error(String)`. Avoids using `parse::<f64>()` as a
  discriminator and lets the UI format the number rather than the model.

- **app-display-split**: Post-eval operator continuation currently builds the
  new expression from the *display string* (`"0.3333..."`), losing the original
  expression. Should keep the original expression and wrap it in parens when
  needed (e.g. `"1+3"` → `"(1+3)*"`), while the display shows the formatted
  result. Requires a separate `display: String` field.

- **app-ui-state**: `App` currently holds both app state (`expression`,
  `result`, `should_quit`) and UI state (`focus`, `BUTTONS`, `move_focus`,
  `focused_label`). UI state should eventually move to a `UiState` struct, with
  keyboard/mouse handlers resolving input to an `Action` enum before passing to
  `App`. Deferred until `tui-skeleton` and `key-input` exist.

## Next Task

**tui-skeleton** — `docs/tasks/tui-skeleton.md`
Terminal setup and event loop using Ratatui + Crossterm.
