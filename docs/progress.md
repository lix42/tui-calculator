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
- `append` maps display chars to expression chars via `display_to_expr`
  (`"÷"→"/"`, `"×"→"*"`); the inverse `expr_to_display` is used by the UI
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
- `ui::draw` was a stub; real layout implemented in `ui-display`.

`Tui` is deliberately concrete: `Terminal<CrosstermBackend<Stdout>>`. The
`Backend` trait already abstracts rendering inside `ui::draw`, so making
`main.rs` generic over `B: Backend` would only abstract the part that's
already abstract — setup, teardown, and event reading are inherently
crossterm-specific. If a non-terminal backend is ever needed, the right
factoring is a separate binary, not generics here.

Build currently emits 11 "never used" warnings for `App` methods and the
`eval` module: nothing in `handle_event` yet calls `press_button`,
`evaluate`, etc. These clear as soon as `key-input` lands.

### ui-display — `src/ui.rs`, `src/app.rs`
Renders the calculator display box. No unit tests (manual verification: launch,
type an expression, press `=`, observe two-line display).

Key implementation details:
- `draw` splits the frame vertically: `Constraint::Length(4)` for the display
  box (2 border + 2 content rows), `Constraint::Fill(1)` for the button area.
- `Block::bordered().padding(Padding::horizontal(1))` draws the border;
  `block.inner(area)` is called *before* `render_widget` to capture the inner
  rect before the block is moved.
- Inner area split into two `Fill(1)` rows. When result is `Some`: top row =
  dim expression, bottom row = bold result. When `None`: top empty, bottom =
  bold expression. Both rows right-aligned via `Line::right_aligned()`.
- `expr_to_display` / `display_to_expr` extracted as `pub fn` in `app.rs` so
  both conversion directions live in the same module. `expr_to_display` replaces
  `*`→`×` and `/`→`÷`; used in `draw`. `display_to_expr` is the inverse; used
  in `append`.

### ui-buttons — `src/ui.rs`
Renders the 5×4 button grid with focus highlight. No unit tests (manual
verification: launch, confirm button grid visible with `=` highlighted cyan).

Key implementation details:
- `draw` reduced to a 28×29 centered panel; delegates to `draw_display` (renamed
  from the inline code in `ui-display`) and `draw_buttons`.
- `centered_panel(area, w, h)` uses `Fill(1) / Length / Fill(1)` twice — first
  vertically, then horizontally — to position a fixed-size rect in the middle of
  any terminal area. Standard Ratatui centering pattern.
- `draw_buttons` allocates `[Length(5); 5]` rows and `[Length(7); 4]` cols. Fixed
  sizes rather than `Fill(1)` so buttons don't stretch on large terminals.
- Each button: `Block::bordered().padding(Padding::symmetric(2, 1))` +
  `Paragraph::new(label).centered()`. Horizontal padding 2 compensates for the
  ~2:1 tall-to-wide cell aspect ratio in most monospace fonts.
- `button_styles(focused)` returns `(block_style, text_style)`: focused =
  `fg(Cyan)` on both block and text, plus `BOLD` on text. Chose color + weight
  over blink (blink is stripped by most modern terminals and signals error/alert
  by convention rather than selection).
- `draw` now discards `_button_area` entirely — the `ui-display` stub is gone.

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

**key-input** — direct keyboard input handling
