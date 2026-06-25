# Progress

## Completed

### eval-parser — `src/eval.rs`
Recursive-descent evaluator over `&[Token]`. Handles `+-*/`, parentheses,
operator precedence, and unary minus. Returns `Result<f64, String>`. 7 unit
tests in `token_tests`, all passing.

> Originally a `&str` recursive-descent parser (`eval`/`Parser`, 8 tests).
> `app-display-split` replaced it with `eval_tokens` over `Token`s built in
> `app.rs`, and `eval-cleanup` (#6) deleted the now-unreachable string parser
> and its tests. This section describes the current token-based form.

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

### app-display-split — `src/eval.rs`, `src/app.rs`, `src/ui.rs`
Tokenized internal expression, fixing the post-`=` precision bug. 17 new unit
tests (35 total), all passing.

Key implementation details:
- `eval::Token` (`Number(f64) | Op(char) | LParen | RParen`) + `eval_tokens`,
  a recursive-descent evaluator over `&[Token]` mirroring the original grammar.
  The `&str` `eval` and `Parser` are kept but now unreachable from the binary.
- `App` fields are now `expr: Vec<Token>`, `current: String` (in-progress
  number being typed — the only place trailing `.` / leading `0.` can be
  represented faithfully), and `mode: Mode` (`Editing | Evaluated(String) |
  Error(String)`). `mode`, `expr`, `current` are private; `ui.rs` goes through
  `display_lines()`.
- **Precision fix**: on `=`, `expr` collapses to `[Token::Number(value)]`. A
  following operator just appends to it, so the full-precision `f64` head is
  preserved across chained calculations — `1 ÷ 3 = × 3 =` now returns exactly
  `1`. Test: `full_precision_preserved_through_operator`.
- `Mode::Evaluated(snapshot)` carries the pre-eval display string so the
  two-line display (dim expression on top, bold result on bottom, established
  in `ui-display`) survives the rewrite. `Mode::Error(msg)` holds the failure
  message directly — no more `parse::<f64>()` discrimination.
- Backspace token rule (`backspace_editing`): one keypress = one visible char.
  Pop `current`, else pop a token; a popped `Number` is pulled back via
  `format_number` *and* has its last digit dropped in the same press (without
  that second `pop`, the keypress wouldn't change the display). Backspace in
  the post-`=` state clears like `C`. Test: `backspace_trace_78_minus_65`.
- `app::display_string(&[Token], &str)` is the single live-rendering function;
  `format_number` remains the only place an `f64` becomes display text. The
  old `expr_to_display` / `display_to_expr` string-replace helpers are gone —
  input is captured as `Op` tokens, never via string substitution.
- Subsumed `app-result-state`: the `Mode` enum does that task's job
  (`Evaluated` / `Error` replace `Option<String>`).

### key-input — `src/main.rs`, `src/app.rs`
Direct keyboard input wired into the event loop, plus a `-0` display fix. 3 new
unit tests for the key mapping (33 total), all passing.

Key implementation details:
- `handle_event` now routes keys to `App`: `Backspace`→`backspace`,
  `Enter`→`evaluate`, and printable `Char(ch)` through `key_char_to_label` →
  `press_button`. Quit keys remain `q` / `Esc` / `Ctrl+C`.
- `Ctrl+C` is checked *before* the bare-`c` mapping (which clears) so the two
  don't collide. Quit and clear are distinct: `c`/`C` → `"C"` (clear), `q` →
  quit.
- `key_char_to_label(ch) -> Option<&'static str>` maps a typed character to the
  grid label `press_button` expects, so keyboard and button grid share one
  definition of input behavior. ASCII diverges from the display glyphs only for
  `*`→`×` and `/`→`÷`; everything else is 1:1. Unmapped keys return `None`.
- Wiring these calls clears the long-standing "never used" warnings on
  `press_button`, `evaluate`, `clear`, `backspace`. (`move_focus` /
  `focused_label` are still unused — they belong to `button-nav`.)
- **`-0` fix** (`format_number`): `{:.10}` can round a tiny ±epsilon
  (e.g. `0.5-0.4-0.1 ≈ -2.8e-17`) to zero magnitude while keeping the sign,
  printing `"-0"`. The formatter now trims on the `&str` slice and returns
  plain `"0"` for that case. Test: `near_zero_negative_epsilon_formats_as_zero`.

### button-nav — `src/main.rs`, `src/app.rs`, `src/ui.rs`
HJKL/arrow focus navigation, Space/Enter activation, and a momentary "pressed"
flash. 6 new unit tests (39 total), all passing.

Key implementation details:
- `handle_event` checks `focus_delta(code)` *first* and `return`s on a match, so
  HJKL/arrows move focus only (no activation, no flash). `focus_delta` maps
  Left/H, Down/J, Up/K, Right/L (vim + arrows, both cases) to `(dr, dc)`;
  everything else is `None` and falls through to activation.
- Every activating key funnels through `activate(app, label)` =
  `press_button(label)` + `register_press(label)`, so keyboard, grid, and (later)
  mouse share one path and focus follows input. Space activates the focused
  label; **Enter always evaluates** (`activate(app, "=")`) and Backspace routes
  through its `"⌫"` label — both rely on `press_button`'s label dispatch.
- `focused_label` return type widened `&str` → `&'static str` (it returns a
  `BUTTONS` const), decoupling it from `&self` so `activate(app,
  app.focused_label())` can hold the label while mutably borrowing `app`.
- **Press flash** (no terminal key-release event exists): `App` gains
  `flash: Option<(usize,usize)>` + `flash_at: Instant`. `register_press` sets
  them, `is_pressed` queries, and `tick()` (called once per run-loop iteration
  before draw) clears the flash after `FLASH_DURATION` (120 ms). `flash` is a
  field distinct from `focus` because the two diverge when you navigate during
  the flash window. Expiry is paced by the 100 ms event poll.
- `position_of(label)` is the inverse of `BUTTONS[r][c]`, backed by a
  `static LABEL_POS: LazyLock<HashMap<&str,(usize,usize)>>` reverse index built
  once on first lookup and derived from `BUTTONS` (single source of truth).
- **UI**: `button_styles(focused, pressed)` returns a `&'static ButtonStyle`
  struct (`block_style` / `text_style` / `border_style` / `border_type`) instead
  of the old `(Style, Style)` tuple, so a state can recolor the frame or swap the
  line characters independently of the fill. Three `static` presets
  (`REGULAR`/`FOCUSED`/`PRESSED`); `pressed` takes precedence since a pressed
  cell is always also focused. `Color::Reset` on the pressed border keeps it
  visible (theme-independent) over the cyan fill. Returning `&'static` requires
  the presets be `static` (a fixed address to borrow), not `const`.

### mouse-input — `src/app.rs`, `src/ui.rs`, `src/main.rs`
Left-click activation via hit-testing stored button rects. 1 new unit test (42
total), all passing.

Key implementation details:
- `App` gains `button_rects: [[Rect; 4]; 5]` (init `Rect::ZERO`). `ui::draw_buttons`
  records each cell's screen `Rect` as it renders and hands the grid to
  `App::set_button_rects` once per frame — so the hit-test always matches what's
  on screen (the panel re-centers on resize). This made `draw` / `draw_buttons`
  take `&mut App`.
- `App::button_at(col, row)` walks the grid and returns the first cell whose rect
  `contains` the click, else `None`. Rects span the cell *including* the border,
  so a click on the frame still counts — no inset math. The layout tiles without
  overlap (and `Rect::contains` is half-open), so the first match is unambiguous.
- Coordinates line up because crossterm mouse `column`/`row` are 0-based absolute
  cells and `frame.area()` starts at `(0,0)`; the stored rects are absolute, so no
  area needs to reach `handle_event`.
- `handle_event` gets an `Event::Mouse` arm: `Down(Left)` → `button_at` →
  `activate(app, BUTTONS[r][c])`, reusing the shared funnel so a click gets
  focus-follow and the press flash for free. The arm `return`s for every mouse
  event, so non-left/non-down events are inert. Mouse capture was already enabled
  in `setup_terminal`, so no terminal-setup change was needed.

### app-ui-state — `src/action.rs`, `src/ui_state.rs`, `src/app.rs`, `src/main.rs`, `src/ui.rs`
Split UI state out of `App` into `UiState`, and replaced the stringly-typed
`press_button(&str)` path with a typed `Action` input boundary. Net −177 lines
across the three existing files while adding two modules. 9 new `action` tests
(50 total), all passing. Done in three green checkpoints: (1) `action.rs`,
(2) behavior-preserving `UiState` extraction, (3) the `Action` rewire.

Key implementation details:
- **`action.rs`** (new) — the typed boundary, deliberately **crossterm-free**
  (pure domain logic). `Action` (`Digit(Digit) | Dot | Op(char) | LParen |
  RParen | Clear | Backspace | Equals`) is the one normalized alphabet
  `App::apply` consumes; `Op(char)` holds the *eval* operator (`*`/`/`), not the
  display glyph. `Digit` is a newtype with a **private** field and a fallible
  `Digit::new` (0..=9): enum variant fields inherit the enum's visibility and
  can't be made private, so the newtype-in-its-own-module is what makes an
  out-of-range digit unconstructable by type. Resolvers: `from_key(char)`
  (keyboard ASCII — operators are `Op(ch)` since the keystroke *is* the eval
  char), `from_label(&str)` (grid glyphs; only `× ÷ ⌫` diverge, every other
  label delegates to `from_key` via `char: FromStr`), and `label()` (the inverse,
  used to drive focus/flash).
- **`ui_state.rs`** (new) — `UiState { focus, flash, flash_at, button_rects }`
  with `move_focus / focused_label / register_press / is_pressed /
  set_button_rects / button_at / tick`, plus `BUTTONS`, `FLASH_DURATION`,
  `position_of` + `LABEL_POS`. Moved verbatim from `App` (the 7 UI tests moved
  with it). `register_press` kept its `&str`-label signature — it's a legitimate
  label→position UI lookup, not the `App` contract the task flagged.
- **`app.rs`** — `App` slimmed to `expr / current / mode / should_quit`.
  `apply(Action)` replaces `press_button(&str)` with a **total match, no `_`
  arm**. `push_digit` now takes a `u8` (`char::from(b'0' + digit)`); the dot path
  split into `push_dot`; the shared post-`=` reset factored into
  `reset_if_post_eval`. Tests drive `App` through a `press(&mut app, label)`
  helper that resolves via `from_label`.
- **`main.rs`** (decision A) — `key_to_action(KeyCode) -> Option<Action>` is the
  single keyboard→Action map: it owns `Enter → Equals` and `Backspace →
  Backspace` (which arrive as `KeyCode`s, not chars) and delegates `Char(ch)` to
  `from_key`. Navigation (`focus_delta`), Space (activate-focused via
  `from_label`), and quit stay separate because they aren't `App` actions — Space
  in particular *can't* be a static map entry since its effect depends on runtime
  focus. `activate(app, ui, Action)` applies then flashes `action.label()`.
  `key_char_to_label` deleted (subsumed by `from_key`).
- **`ui.rs`** — `draw` takes `&App` + `&mut UiState`; `draw_display(&App)`,
  `draw_buttons(&mut UiState)`.

### paste-input — `src/main.rs`, `src/app.rs`
Paste a whole expression via bracketed paste. 8 new unit tests (59 total), all
passing.

Key implementation details:
- **Bracketed paste had to be enabled first.** `Event::Paste` only fires when
  the terminal is in bracketed-paste mode; `setup_terminal` previously enabled
  only `EnterAlternateScreen` + `EnableMouseCapture`, so paste events never
  arrived (an earlier note here that the loop "already discards `Event::Paste`"
  was true of the match but moot in practice). `EnableBracketedPaste` is now
  threaded through all three lifecycle points alongside mouse capture:
  `setup_terminal` (enable), `restore_terminal` (disable, ordered *before*
  `LeaveAlternateScreen`), and `install_panic_hook` (disable on panic). **No
  `Cargo.toml` change was needed** (contra this task's old plan note): the
  `EnableBracketedPaste`/`Event::Paste` API is `#[cfg(feature =
  "bracketed-paste")]`-gated, but that feature is a crossterm *default* and the
  project never sets `default-features = false`, so it was compiled in all along
  (`cargo tree -e features` confirms it active, also via `ratatui-crossterm`).
- **`App::apply_str(&str)`** is the single "ingest a string" entry point: it
  loops `s.chars()`, resolves each through `Action::from_label`, and feeds the
  `Some` case to `apply`. Chars with no calculator meaning (spaces, stray
  letters) resolve to `None` and are skipped — so `"78 - 65"` pastes as `78-65`.
  The valid-char policy lives entirely in `action.rs`; `apply_str` and the
  `main.rs` paste arm are both ignorant of which chars are valid (single source
  of truth).
- **Resolves via `from_label`, not `from_key`** (fix from PR review): paste uses
  the *display-glyph* boundary, not keyboard ASCII, so an expression copied out
  of the display (which renders `×`/`÷`, not `*`/`/`) pastes back and round-trips
  instead of having its operators silently dropped — `78-65×5` had mis-parsed as
  `78-655`. `from_label` maps the two glyphs and delegates everything else to
  `from_key`, so ASCII input still resolves. Chosen over an inline `×`→`*`
  normalize table (the reviewer's suggestion) because that would duplicate glyph
  knowledge `from_label` already owns. Test: `paste_display_glyphs_round_trip`.
- Because every char goes through the same `apply` the keyboard uses, post-`=`
  reset, operator precedence, and a trailing `=` (which evaluates) all come for
  free — `"2+2="` evaluates in one event. No reimplemented calculator logic.
- **`handle_event`** gains an `Event::Paste(text)` arm that calls
  `app.apply_str(&text)` and `return`s. It deliberately bypasses `activate`, so
  a paste is one logical edit — no per-character focus move or press flash.

## Known Issues / Deferred

- **`Action::Op(char)` is a convention-enforced invariant (follow-up to
  app-ui-state)**: unlike `Digit` (private field, unconstructable when invalid),
  `Op(char)` can hold any `char` — the "only `+ - * /`" contract lives in the
  `from_key`/`from_label` resolvers, not the type. Safe today because those two
  resolvers are the only construction path, but `Action::label()`'s `Op(_) =>
  "-"` arm would render a stray operator silently. A future `enum Op { Add, Sub,
  Mul, Div }` would make `label()` exhaustive and the invariant structural;
  deferred because the evaluator consumes the raw `char` (real churn) and it
  can't trigger today. Surfaced by the type-design review during `/ship`.
- **Unified `Msg` enum (follow-up to app-ui-state)**: considered and deferred
  (option B). The keyboard handling in `main.rs` could collapse to one total
  `fn from_key(KeyEvent) -> Option<Msg>` where `enum Msg { Apply(Action),
  MoveFocus(i32,i32), ActivateFocused, Quit }` spans all three subsystems
  (App / UiState / lifecycle) — the Elm-style "message" pattern. We chose option
  A instead (keep `Action` as the pure `App`-only alphabet; let `main.rs` route
  events to the right subsystem) to keep `action.rs` crossterm-free and stay in
  scope. Revisit if the event routing in `handle_event` grows more cases.

## Next Task

**copy-clipboard** is the last remaining task — copy the result to the system
clipboard. The `arboard` dependency is already in `Cargo.toml` but unused. Worth
a focused pass on its own: `arboard` has real platform quirks (e.g. X11
clipboard ownership on Linux is tied to process lifetime), so this is more than
a one-liner despite the dep already being present.
