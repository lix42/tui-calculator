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

### tui-skeleton — `src/main.rs`, `src/ui.rs`
Terminal lifecycle, main event loop, and a stub renderer. No unit tests
(manual verification: launch, quit via `q`/`Esc`/`Ctrl+C`, terminal restored).

Key implementation details:
- `setup_terminal`: `enable_raw_mode` → `EnterAlternateScreen` →
  `EnableMouseCapture`. `restore_terminal` reverses in the right order
  (mouse capture off *before* leaving alt screen).
- `install_panic_hook` chains a custom hook in front of the original so the
  terminal is restored on panic before the default panic message prints.
- Main loop polls `event::poll(100ms)` and dispatches to `handle_event`.
  `app.should_quit` is the exit signal.
- `handle_event` filters `KeyEventKind::Press` (Windows fires Press / Repeat /
  Release for every keystroke; without the filter every tap counts multiple
  times). Quit keys: `q`, `Esc`, `Ctrl+C`. Mouse / resize / paste events fall
  through to a no-op.
- `Ctrl+C` is handled explicitly — in raw mode the kernel does *not* turn it
  into `SIGINT`; the app receives the keypress and must act on it.
- `ui::draw` is a stub (`Block::bordered().title("Calculator")`); real layout
  comes in `ui-display` and `ui-buttons`.

`Tui` is deliberately concrete: `Terminal<CrosstermBackend<Stdout>>`. The
`Backend` trait already abstracts rendering inside `ui::draw`, so making
`main.rs` generic over `B: Backend` would only abstract the part that's
already abstract — setup, teardown, and event reading are inherently
crossterm-specific. If a non-terminal backend is ever needed, the right
factoring is a separate binary, not generics here.

Build currently emits 11 "never used" warnings for `App` methods and the
`eval` module: nothing in `handle_event` yet calls `press_button`,
`evaluate`, etc. These clear as soon as `key-input` lands.

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

**ui-display** — `docs/tasks/ui-display.md`
Render the display box (top of the screen, showing expression and/or result).
