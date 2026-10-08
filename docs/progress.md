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
  (`Evaluated` / `Error` replace `Option<String>`). **Re-verified 2026-07-31** and
  marked `[x]` — it had been left at `[~]` ever since, showing as permanently
  in-progress. All five of its "Changes Required" hold, but the shape differs from
  the spec and the difference is worth knowing:
  - No `EvalResult` type was ever created. Value-vs-error is typed by `Mode`
    variant instead, which is what killed the `parse::<f64>()` round-trip.
  - **`Mode::Evaluated(String)` is not the old smell resurfacing.** That `String`
    is a snapshot of the *input expression* for the display's top line; the result
    itself lives in `expr` as `Token::Number(f64)`, since `evaluate` collapses the
    expression to a single `Number`. `format_number` runs only inside
    `display_string`, i.e. at render time.
  - This beats the specced `EvalResult::Value(f64)`, which would have held the
    result in a second place alongside `expr`. One source of truth is why a
    chained calculation keeps full precision instead of round-tripping through a
    formatted string.

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

### copy-clipboard — `src/app.rs`, `src/ui_state.rs`, `src/ui.rs`, `src/main.rs`
Copy the result to the system clipboard via a `[y Copy]` display-box affordance.
6 new unit tests (65 total), all passing.

Key implementation details:
- **Copy is deliberately not an `Action`.** It's a side-effecting command on the
  *result*, not a calculator state transition, so adding it to the `Action` enum
  would either break `App::apply`'s total, catch-all-free match (a `Copy => {}`
  no-op arm is a lie) or put clipboard I/O into the crossterm-free `action.rs`.
  Instead it's routed in `main.rs` next to quit/focus-moves — the same "not an
  `App` action, handled at the I/O boundary" tier the deferred-`Msg`-enum note
  below describes. The grid (`BUTTONS`) stays a fixed `static const`; the
  affordance lives in the display area, so no grid/focus/hit-test code became
  dynamic.
- **`App::copy_text() -> Option<String>`** is the single source for both "is
  there something to copy?" and "what to copy": `Some(display_string(...))` only
  in `Mode::Evaluated`, `None` in `Editing`/`Error`. The UI reads `is_some()` to
  decide whether to draw the affordance, so it auto-dismisses the instant new
  input leaves `Evaluated` (a fresh digit → `Editing` → `None`). An error
  message is never copyable.
- **`UiState`** gained `copy_rect` (captured each draw like `button_rects`;
  `set_copy_rect` / `copy_hit` for click hit-testing — `Rect::ZERO` when hidden,
  and zero-area rects contain no point, so `copy_hit` is false then) and a
  transient `status: Option<(String, Instant)>` (`set_status` / `status_text`).
  It's an owned `String`, not `&'static str`, so a failure carries the real
  `arboard` error detail — a TUI has no log, so the status line is the only place
  the cause can surface. The existing `tick` expires it after `STATUS_DURATION`
  (1500ms) — much longer than the 120ms `FLASH_DURATION` because the message is
  text to *read*, not a blink.
- **`ui.rs`**: `draw_display` now takes `&mut UiState`. `draw_copy_affordance`
  renders the live status (which wins) or else `[y Copy]` when copyable,
  left-aligned in the display's top row, and returns the column width to reserve.
  `draw_display` shrinks the right-aligned expression's area by that width so a
  long expression can't render over the persistent hint. The status reserves
  `0` (it's momentary post-action feedback and may use the whole row), so it can
  briefly overlap the dim expression — acceptable. Only the hint is clickable;
  the status is feedback, not a target. `COPY_HINT` is ASCII, so `str::len()` is
  its render width / clickable-rect width.
- **`main.rs`**: `y`/`Y` (vim-yank; `Ctrl+C` is taken by quit in raw mode) and a
  left-click on `copy_hit` both route to `do_copy`, which calls
  `copy_to_clipboard` (one-shot `arboard::Clipboard::new()?.set_text(text)`) and
  sets the status to `Copied!` or `Copy failed: {e}` (the real `arboard::Error`
  via `Display` — `no clipboard` on headless/SSH is permanent, `clipboard busy`
  is transient, and they want different responses). The mouse arm checks
  `copy_hit` *before* `button_at` since the affordance is outside the grid.
- **Cross-platform**: one-shot set; persists after exit on macOS/Windows. On
  Linux/X11 clipboard contents are tied to process lifetime, so a copy may not
  survive exit without a clipboard manager — documented as a code comment, not
  handled (chosen scope; dev is on macOS).
- The actual clipboard write isn't unit-tested (it touches the system
  clipboard), but `do_copy`'s no-op guard *is*: `do_copy_is_noop_without_a_result`
  drives the `copy_text() == None` path, which returns before the clipboard call,
  so it sets no status. `copy_text` (the decision) is fully covered. The success
  status path is verified manually per the task's test steps.

### layout-config — `src/layout.rs` (new), `src/ui_state.rs`, `src/ui.rs`, `src/main.rs`
De-hardcoded the button grid: the const-generic 5×4 (`[[&str; 4]; 5]`,
`areas::<N>`) is gone. The layout is now *data* — a `Keypad` value the rest of
the UI reads — with cell spanning in the model. Shipped as #17. Landed in the two
green checkpoints the task file prescribed (mechanical de-hardcode first, then the
spanning model).

Key implementation details:
- **`src/layout.rs` (new, pure — no ratatui/crossterm).** A pad is *authored* as
  an occupancy grid of label tokens and `compile`d into a `Keypad` at startup:
  - `Keypad { rows, cols, buttons: Vec<Button>, occupancy: Vec<Vec<usize>>,
    label_pos: HashMap<&str,(usize,usize)> }`; `Button { label, row, col,
    row_span, col_span }`. A token repeated across adjacent cells *is* a spanning
    button (its region is the bounding box of its cells).
  - `compile` **validates and panics** on malformed static data: non-rectangular
    grid, or a token whose cells don't fill their bounding box (ragged/L-shaped
    span, or the same label reused disjointly). Buttons come out in reading order.
  - The reverse index (`label → anchor cell`) moved off the old process-global
    `LazyLock<BUTTONS>` onto `Keypad::position_of`, built during the same compile
    walk. `button_index_at(r,c)` resolves a cell → covering button in O(1).
  - `STANDARD` is 5×4, **all 1×1** (spanning exists in the model + is unit-tested
    with wide/tall pads, but the shipped pad uses none yet). The grid now carries
    `⌫` at (4,0) alongside `C`/`(`/`)`.
- **`src/ui_state.rs`.** `UiState` owns the active `Keypad`. Focus stays a lattice
  cell `(usize,usize)` (smallest delta from before) and resolves to a button
  through `occupancy` wherever one is needed: `focused_label`, `is_button_focused`,
  `is_button_pressed`, `register_press` all go through the occupancy map, so a
  spanning button reads/flashes as one unit. `button_rects` is now `Vec<Rect>`
  (one **union rect per button**, not per cell); `button_at` returns a *button
  index* by hit-testing those union rects — a click anywhere on a spanning button,
  internal seams included, hits it. Focus homes on `"="` via `position_of` (was
  the hardcoded `(4,3)`).
- **`src/ui.rs`.** `draw_buttons` splits the area once per axis into a runtime
  coordinate lattice (`Layout::split` → `Rc<[Rect]>`; **no const generics**) and
  draws each button once over the bounding box of the cells it spans. Panel size
  derives from the active pad's dims × `CELL_W`/`CELL_H` + `DISPLAY_H` (7/5/4), so
  a differently-shaped pad re-centers for free — no magic `28`/`29`/`25`.
- **`src/main.rs`.** The mouse path resolves `button_at` → `button_label` →
  `Action::from_label`; Space activates `focused_label`. No `BUTTONS[r][c]`
  indexing anymore.
- Tests: `layout.rs` covers compile/occupancy/reading-order, wide+tall spans, and
  the three rejection cases (disjoint, L-shape, ragged). `ui_state.rs` tests
  extended to the `Keypad`-backed focus/hit-test. All green.

**Carry-forward for `layout-registry` / `focus-per-button`:** the active pad is
read through a single accessor (`ui.keypad()`), so multiplying pads doesn't
re-open this model. **Gotcha:** `Keypad` allocates (`Vec`, `HashMap`), so it
*cannot* be a `static`/`const` — the task file's sketch of
`static LAYOUTS: &[Keypad] = &[STANDARD, TALL, WIDE]` won't compile. The registry
must be a runtime `Vec<Keypad>` built in `UiState::new` (each via `compile`).
`default_focus` doesn't exist yet — `UiState::new` hardcodes the `"="` home; a
per-pad home is `layout-registry`'s to add.

### layout-registry — `src/layout.rs`, `src/ui_state.rs`, `src/main.rs`

**Status:** done · 2026-07-21

Multiple named pads + a manual switch key, a pure addition on `layout-config`'s
`Keypad` model — the model didn't re-open, exactly as the carry-forward predicted.

- **`src/layout.rs`.** `Keypad` gained a `default_focus: (usize,usize)` field +
  accessor, resolved *at compile time from a label* — `compile(grid,
  default_focus_label)` looks the label up in `label_pos` and **panics if it's not
  on the pad** (same "malformed static data is a programming error" stance as the
  span invariants). Added a second real pad `TALL` (6×4: wide `0` 1×2, wide `+`
  1×3, tall `=` 2×1) so spanning is finally exercised on a shipped pad and there's
  something to switch *to*. Both pads home on `"="`.
- **`src/ui_state.rs`.** The single `keypad` field became a **runtime `Vec<Keypad>`
  registry + active index** (`layouts` / `layout`), built in `new` (`vec![standard,
  tall]`) — *not* a `static`, per the carry-forward gotcha. Index 0 active at
  startup, so behavior is unchanged until the user switches. `keypad()` now returns
  `&layouts[layout]`; every downstream reader was already going through that
  accessor, so nothing else changed. Added `cycle_layout()` → `set_layout(i)`, which
  drops the stale press-flash and fixes up focus via `resolve_focus`.
- **`resolve_focus(old, pad)` — the one load-bearing decision, policy "preserve,
  else default".** If the old lattice cell is in-bounds on the new pad, keep it but
  **snap to the covering button's anchor** (via `button_index_at` → `button(idx).
  row/col`), so focus never lands on a non-anchor cell of a span; if out of bounds,
  fall back to `pad.default_focus()`. The snap is the subtle part: without it,
  switching onto a wide `0`/tall `=` would leave focus on a dead interior cell.
- **`src/main.rs`.** `KeyCode::Tab => ui.cycle_layout()` — routed at the I/O
  boundary like copy and focus-moves, **not** an `Action`: switching transforms no
  calculator state, so it stays out of `App::apply`'s pure match.
- Tests: `layout.rs` covers `default_focus` (incl. the unknown-label panic) and the
  tall pad's spans; `ui_state.rs` covers cycle+wrap and all three `resolve_focus`
  branches (preserve, snap-to-anchor, fall-back); `main.rs` covers the Tab route.
  80 tests green, `cargo clippy` clean.

**Carry-forward for `layout-auto`:** pads live in `ui.layouts` (a `Vec<Keypad>`)
with `ui.set_layout(i)` as the switch primitive — `layout-auto` should call the
same primitive on resize, gated behind a manual-override flag so an explicit Tab
wins over shape-based auto-select. Each pad has `default_focus` but **no shape hint
yet**; a per-pad `fits(w,h)`/aspect score is `layout-auto`'s to add. `Tab` is the
one switch trigger today; auto-select must not fight it.

### layout-auto — `src/layout.rs`, `src/ui_state.rs`, `src/main.rs`, `src/ui.rs`

**Status:** done · 2026-07-22

Shape-aware automatic pad selection on resize, with a manual pin taking
precedence. 12 new tests (92 total), `cargo clippy` clean.

- **Pads reshaped first (a prerequisite the task file didn't call out).** The two
  existing pads were both 4 cols wide, so aspect-ratio scoring had nothing to bite
  on. `TALL` became genuinely tall-narrow (7×3, wide `=` span) and a new `WIDE`
  pad (3×7, tall `=` span) was added. Aspect ratios (`need_h/need_w`) now spread
  cleanly: standard ≈ 1.04, tall ≈ 1.86, wide ≈ 0.39. Registry is `[standard,
  tall, wide]`. The two span directions are still each exercised on a real pad
  (tall's horizontal `=`, wide's vertical `=`).
- **`Keypad::fit_score(w, h) -> i32`** (in `layout.rs`, pure). Totally-ordered so
  `select_for` has a unique max. **Two tiers:** a pad that overflows the terminal
  (`w < cols*CELL_W || h < rows*CELL_H + DISPLAY_H`) returns `-1_000_000_000 -
  overflow` — below every fitting pad, and least-overflow wins when *nothing*
  fits; a fitting pad scores the integer cross-multiplied aspect distance
  `-(|need_h*w - h*need_w| * SCALE / need_w)` (closest shape wins, no floats).
  **The overflow gate is load-bearing**, not decoration: without it a landscape
  terminal 1 column too narrow for `wide` (e.g. 48×29) still scores `wide` best on
  aspect and picks a pad that can't fit while `standard` fits — `centered_panel`
  would then clip the last button column. Regression test: `select_for(48, 29) == 0`.
- **The `/ need_w` normalisation is equally load-bearing** — caught by Codex on PR
  #19, after the gate fix. The cross-product `|need_h*w - h*need_w|` is the true
  ratio distance scaled by `need_w * w`. `w` is shared by every pad and cancels out
  of the ranking, but `need_w` is per-pad (28 / 21 / 49), so leaving it in penalises
  a pad in proportion to its own width and **biases selection toward narrow pads**.
  At 60×40 all three pads fit and `wide` is the closest ratio match (0.39 vs the
  terminal's 0.67; standard is 1.04), yet the unnormalised score ranked standard
  first (-620) over wide (-820) — a landscape terminal getting the squarish pad.
  Dividing by `need_w` flips it (-16.7 vs -22.1). `SCALE = 1024` keeps resolution
  through the integer division; the arithmetic moved to `i64` so the scaled product
  can't overflow (worst case ≈1.6e8, still far above the -1e9 overflow tier and
  well inside `i32`). Regression tests: `fit_score_normalises_away_pad_width` and
  `select_for(60, 40) == 2`. **Lesson:** cross-multiplication is valid for *ordering
  two ratios against each other*, but a per-pad *score* needs a common denominator —
  the four original acceptance shapes (30×45, 70×40, 40×40, 48×29) all happened to
  agree either way, so only a case where every pad fits could expose it.
- **Cell geometry moved to `layout.rs`.** `CELL_W`/`CELL_H`/`DISPLAY_H` were
  `ui.rs`-private; `fit_score` needs them to know physical fit, so they're now
  `pub const` in `layout.rs` (single source of truth) and `ui.rs` imports them.
- **`ui_state.rs`.** `select_for(w, h)` scans the registry keeping the incumbent
  unless a later pad *strictly* beats it, so ties resolve to the earliest pad
  (standard) — the documented default. `auto_select(w, h)` caches the size (into
  new field `term_size`) *before* the pinned early-return, then switches only when
  the best index actually changes (so a resize within a shape band preserves focus
  and any in-progress flash). New field `override_layout: Option<usize>`:
  `cycle_layout` (Tab) now sets `Some(next)` to **pin**, `resume_auto` clears it
  and re-picks for the cached `term_size`.
- **`main.rs`.** `Event::Resize → auto_select`, `a`/`A` → `resume_auto` (the
  counterpart to Tab — Tab pins, `a` un-pins; `a` had no calculator meaning so it
  collides with nothing), and the initial pad is seeded from `terminal.size()`
  before the loop (resize events don't fire at startup). All routed at the I/O
  boundary, not as `Action`s — consistent with copy/switch.
- **UX decision (user):** override is cleared by a **dedicated `a` key**, not by
  cycling Tab past the last pad — Tab and `a` stay one-job-each.
- **Implementation split:** `fit_score`'s body was written by the user; the review
  caught a missing overflow gate (aspect-only) and an unnecessary cast, both fixed.

### focus-per-button — `src/layout.rs`, `src/ui_state.rs`, `src/main.rs`

**Status:** done · 2026-07-24

Grid navigation now steps one *button* per key press, not one lattice cell, so
crossing a spanning button (tall pad's wide `=`, wide pad's tall `=`) costs a
single press. 100 tests (was 92 at layout-auto), `cargo clippy` clean.

- **Focus stayed a lattice cell (user's design call).** The task file floated
  "lattice cell → button index"; we kept `focus: (usize, usize)` and instead made
  `move_focus` skip the current button's covered cells. The deciding case was the
  wide pad's tall `=`: entering it sideways from `+` at (2,5), the return trip must
  land back on `+`. Resting on the **entry cell** (2,6) makes that reversible;
  snapping to the button's anchor (1,6) would send Left to `⌫` instead. So *within
  a pad* focus may sit on a non-anchor span cell — deliberately unlike the
  anchor-snap `resolve_focus` still does on a pad *switch*.
- **`Dir` enum + `Keypad::step` (both in `layout.rs`, pure).** `move_focus(dr, dc)`
  became `move_focus(Dir)`; `focus_delta` in `main.rs` became `focus_dir ->
  Option<Dir>`. A `Dir` (not a delta pair) makes the "unit, single-axis step" rule
  structural — the skip-walk is only correct for unit steps, so a diagonal or
  stride-2 delta is now unrepresentable rather than a comment. `Keypad::step(row,
  col, dir) -> Option<(usize,usize)>` is the pure one-cell step; the free
  `next_button_cell` in `ui_state.rs` loops it, skipping cells owned by the start
  button, until it lands on a different button or runs off the edge.
- **`step` returns `Option`, never clamps — load-bearing.** A clamped edge step
  returns the same cell forever, so `next_button_cell`'s `while let Some` would
  spin. `None` at the edge is what terminates the walk (`move_focus` then no-ops).
  Test `move_focus_within_a_span_is_a_noop_at_the_edge` guards the spin case.
- **`step` body was the user's contribution (learning mode).** First cut clamped
  and had `usize` underflow (`row - 1` at row 0) plus an off-by-one bound; corrected
  to a total match over `Dir` (no catch-all) with `checked_add_signed` + a single
  `< rows/cols` bounds check. The total match means a future `Dir` variant is a
  compile error here, matching the codebase's "total match" discipline.
- **`/ship` review actions.** Type-design review flagged the original guarded match
  (`Dir::Up if row > 0 => …`, `_ => None`) as opting out of exhaustiveness → moved
  to the total-match form above. Comment review caught both `step` and `Dir` docs
  naming `move_focus` as the loop-holder when it's `next_button_cell` → fixed.
  Test review found two matrix corners (horizontal span crossed from its *far*
  cell; landing onto a horizontal span perpendicular) → added
  `crossing_a_horizontal_span_from_its_far_cell_takes_one_press` and
  `entering_a_horizontal_span_is_reversible`.

### rainbow-mode — `src/ui.rs`, `src/ui_state.rs`, `src/main.rs`, `Cargo.toml`

**Status:** done · 2026-07-28 (static color pass; animation intentionally deferred)

Per-glyph color mode for both the display and the button grid, toggled at runtime.
120 tests (was 115 before the ship review added 5), `cargo clippy`/`fmt` clean.

- **Presentation-only, on `UiState` not `App`.** `ColorMode` (Mono|Rainbow) and
  `Theme` (Dark|Light) are rendering state — they change no calculator state — so
  they live on `UiState` and are toggled by `r`/`t` routed at the I/O boundary in
  `main.rs`, exactly like the Tab/`a` pad side effects. `App` still hands back plain
  strings; `ui.rs` decides the color.
- **`glyph_color` is the single source of truth** for both surfaces, built in
  **HSLuv** via ratatui's `palette` feature (added to `Cargo.toml`) so the ten digit
  hues read as evenly bright — a naive HSL palette makes yellow glare and blue go
  muddy on a dark background. Digits `0`–`9` on a 36° grid; operators/parens
  hand-picked *off* that grid so an operator between two digits can't share a
  neighbour's hue; `=`/`C`/`⌫`/`.` stay neutral so color reads as *the expression*.
- **Two highlight shapes, `filled_style` + `outline_style`.** Rainbow fills on focus
  (the key's hue) and press (`loud`). Mono derives its accents from the same palette
  (the old static cyan is gone): colored keys outline-on-focus / fill-on-press, and
  neutral keys use `loud` with the shapes **swapped** (fill-on-focus / outline-on-press)
  — a user call, because a faint neutral outline read too close to the plain resting
  cell.
- **UX iteration (user-driven).** Landed over several rounds: rainbow default on,
  standard 5×4 the launch pad, mono focus borrowing the palette hue, then the
  neutral-key swap. Defaults flipped: `ColorMode::Rainbow` and the standard pad at
  launch (`run()` dropped the startup `auto_select` seed; resize still adapts).
- **`/ship` review actions.** Code + test review caught a real bug: `filled_style`
  knocked the glyph out in `knockout(theme)`, which on the **Light** theme is white —
  so a `Color::White` fill rendered white-on-white (invisible glyph + chip lost in
  the background), hitting the default rainbow press flash and the mono neutral focus.
  Fixed by introducing `loud(theme)` (white on dark / black on light) as the exact
  opposite of `knockout`, so a loud-filled chip always keeps a legible glyph;
  regression test `loud_filled_highlights_stay_legible_on_the_light_theme`. Also
  added light-theme neutral-fill, `styled_line` dispatch, and a startup-standard-pad
  guard; fixed 5 stale comments (mono-cyan rationale, `Theme` "only affects rainbow",
  the seeded-pad line in `CLAUDE.md`).
- **Deferred: animation.** The static palette is the whole of this pass. The
  event-driven effect model (ripple/drift/breath generalizing the press flash) is
  captured in the "Animation" section of `docs/tasks/rainbow-mode.md` and was
  later promoted to its own task, **`rainbow-animation`** (2026-07-31), so it
  stops being invisible notes attached to a closed task.

### quick-input — `src/action.rs`, `src/main.rs`, `src/ui_state.rs`, `src/ui.rs`

**Status:** done · 2026-07-31

A home-row numpad mode with per-button key tips. 136 tests (was 120 at
rainbow-mode), `cargo clippy`/`fmt` clean. Shipped as #22.

- **A sticky mode, not the task file's `Alt`-held modifier — the load-bearing
  decision.** Default macOS Terminal.app composes `Option`+`h` into the dead key
  `˙` and delivers **no `ALT` modifier at all**, so an Alt-triggered feature would
  silently do nothing on the dev machine and depend on a per-user terminal setting
  (iTerm2/Ghostty's "Option as Meta") elsewhere. `i` enters, `Esc` leaves. The mode
  also *dissolves* the task file's caveat (1): it says a "visible only while held"
  overlay is impossible in a TUI because terminals emit no modifier-down event —
  but a mode has explicit on/off transitions, so the tips can show exactly while
  it's active. The decision record now lives at the top of
  `docs/tasks/quick-input.md`; two reviewers flagged the deviation by reading the
  superseded spec as the contract, which is why it's recorded there.
- **The map is a numpad *in place* (user's design).** `u i o` / `j k l` / `m` →
  `456` / `123` / `0`, because on QWERTY those keys sit physically beneath `7 8 9`;
  `a s d f` → `+ - × ÷`; `[` `]` → `(` `)` (parens without Shift). The digit row and
  `.` are deliberately unmapped — a digit key already types its digit, and `.` is
  already the decimal point sitting bottom-right where a numpad puts it. Mapping
  either would be a no-op entry that also earned a pointless on-screen tip.
- **`QUICK_MAP` is a `const &[(char, &str)]` read in both directions**
  (`quick_map` / `quick_key`), not two `HashMap`s. It's compile-time data, so a
  `const` slice lives in rodata with no `LazyLock` or allocation — the shape this
  codebase already moved *away* from when `layout-config` retired
  `static LABEL_POS: LazyLock<HashMap<…>>`. (`Keypad::label_pos` stays a `HashMap`
  because a pad is *compiled at runtime* and allocates anyway.) At 13 entries a
  linear scan beats SipHash, and one table read both ways makes the
  forward/reverse agreement structural rather than maintained — the inverse test is
  nearly tautological by construction. Values are *labels*, so callers resolve
  through the existing `Action::from_label` boundary and quick keys reuse the
  shared `activate` funnel, inheriting focus-follow and the press flash.
- **`Esc` is no longer a quit key anywhere** (`q` / `Ctrl-C` only). Entering on `i`
  invites the vim reflex of double-tapping `Esc`; the second tap would otherwise
  quit and discard the expression — the exact mishap the mode's key choice courts.
  This changed behavior *outside* the feature, so it was a deliberate user call.
  Test: `esc_never_quits_and_double_tapping_it_is_safe`.
- **`hjkl` go inert in-mode; the arrow keys still navigate.** `j k l` type digits,
  so leaving `h` navigating would make one row of keys behave two ways at once.
  Arrows keep working exactly as in vim's insert mode, so focus is never stranded.
- **Tips ride in each button's top border** via `Block::title`, and only while the
  mode is on — so they double as the mode indicator (the mode is never silently
  active). The border, not the interior: after `Padding::symmetric(2, 1)` a
  `CELL_W = 7` cell has a **one-column** interior, so there is no room beside the
  glyph. `draw_button` now takes a `ButtonView` struct because clippy's
  `too_many_arguments` fires at 8 — which also named the two bare `bool`s at the
  call site.
- **New testing capability: `TestBackend` render assertions.** `ui.rs` now renders
  the real UI onto a `28×29` buffer (the standard pad's exact panel size) and
  asserts on cells — the tip at `(8, 14)` is the `5` button's *border* row while the
  glyph stays centered at `(10, 16)`. First render test in the repo; use it for
  future layout claims instead of arguing them in prose.
- **Review actions (two rounds, both real bugs).** `/code-review --fix` caught the
  quick-mode block missing the `!intersects(CONTROL | ALT)` guard the navigation
  block has — `Ctrl-U` typed `4`, `Ctrl-L` typed `3`, `Alt-D` typed `×`, so a
  reflexive kill-line silently corrupted the expression. Fixed + regression test
  `quick_mode_ignores_ctrl_and_alt_chords`. It also flagged the double-`Esc` quit
  above. A Codex pass then found `README.md` documented `Esc` as a quit key, which
  surfaced that the README had **rotted across four PRs**: it described a focusable
  "Copy button" pressed with `Space`/`Enter` that never shipped (copy is the
  `[y Copy]` display affordance, `y` or click) and omitted `Tab`, `a`, `r`, `t`,
  `y`. Corrected, and `CLAUDE.md` now carries a rule to keep it in sync —
  `docs/` is not the user-facing surface.

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

## web-time swap — `Cargo.toml`, `src/ui_state.rs`

**Status:** done · 2026-07-31 (not a task; a 2-line prerequisite extracted from
`web-ratzilla` and done standalone)

`ui_state.rs` now imports `Instant` from `web-time` instead of `std::time`. 136
tests green, clippy/fmt clean — the native build is bit-for-bit equivalent, since
web-time re-exports `std`'s `Instant` on native and only swaps in
`performance.now()` on `wasm32`.

- **Why it wasn't made a task.** It was named as a prerequisite in *three* places
  (`web-ratzilla.md` gap 3, `rainbow-mode.md`'s cross-cutting blockquote, and this
  log's Next Task section), each saying "coordinate so it happens once" — but the
  change is one `Cargo.toml` line and one `use`. Doing it outright *deletes* the
  coordination problem instead of tracking it; all three notes have been removed.
- **`rainbow-animation` never actually depended on it.** The animation works fine
  on native with `std::time::Instant`; `web-time` was only ever a *preference* so
  the web port wouldn't retrofit. Drawing that edge would have been a false
  dependency, wrongly showing the animation task as blocked.
- **The dep is deliberately NOT target-gated.** `ui_state.rs` is shared code the
  native binary compiles too, so `[target.'cfg(target_arch = "wasm32")'.dependencies]`
  would break native with an unresolved `web_time` import. It belongs in plain
  `[dependencies]`; only `ratzilla`/`web-sys` are genuinely wasm-only. Rationale is
  in a comment on the dep so it can't be "tidied" later.

## rainbow-animation — `src/ui_state.rs`, `src/ui.rs`, `src/layout.rs`, `src/main.rs`

**Status:** done · 2026-09-20 · **the ripple's look changed after the user's visual
pass** (see "Review round" below) — the wave they approved was the buggy one, so
the current appearance is not yet eyeballed.

The event-driven effect model deferred out of `rainbow-mode`. 157 tests (was 136
at `quick-input`), `cargo clippy`/`fmt` clean. Landed in the two checkpoints the
task file prescribed.

### The three open questions, settled

- **`Effect` shape** — per-variant payloads, as the task file suspected. `Effect
  { kind, started }` with `EffectKind = Press { cell } | Ripple { cell } | Drift`;
  a shared `origin` field would need a meaningless "no origin" case for the global
  `Drift`. Went one step further than the spec and **dropped the stored
  `duration`**, deriving it from the kind — the duration is a property of what the
  effect *is*, so a per-instance copy is a second source of truth free to disagree
  with the constant.
- **Does `Mono` suppress effects?** — **No, it supports them** (user's call). This
  cost nothing in the end: `ripple_color` and `breath_color` build in HSLuv with
  **lightness** as the varying term, and HSLuv at zero saturation *is* a gray, so
  mono rides the identical code path with `saturation = 0.0` — no branch. Only the
  hue drift is rainbow-only, because mono has no hues to rotate.
- **Ripple origin on a spanning button** — neither of the two options the task file
  offered. "Pressed cell" isn't available (`register_press` takes a *label* and
  resolves via `position_of`, so only the anchor is ever known — the mouse path
  goes index → label → anchor too), and "anchor" would radiate from the top-left
  corner of a wide `=`. Instead `Keypad::button_distance` measures **rect-to-rect**:
  distance to the *nearest cell* of the pressed button. Degenerates to plain cell
  distance for a `1×1` key, so it's a strict generalization with no special case
  and no new plumbing.

### Checkpoint 1 — the effect model (no visible change)

`flash: Option<(usize,usize)>` + `flash_at` → `effects: Vec<Effect>`. All five
existing flash tests kept their assertions, reading through a new `#[cfg(test)]
flash_cell()`. 136 green throughout.

- **A `Vec` for a collection that only ever holds one trigger's effects** is
  deliberate: it makes the "ripples compose" stretch an insertion-policy flip in
  `start_effects` rather than a change to how the state is stored, exactly as the
  design intended.
- `set_layout` drops only **cell-anchored** effects (`kind.cell().is_some()`),
  which is behavior-identical today and lets the later global `Drift` ride through
  a pad switch for free.

### Checkpoint 2 — the effects

- **Ripple** (`EffectKind::Ripple`, 800 ms) — kept **separate** from the 120 ms
  `Press` chip because the two run on different clocks: the chip is a blink, the
  wave has ground to cover. One press starts both via `start_effects`.
- **Curve shape was the user's contribution** (learning mode): given three
  candidates — travelling band / decaying glow / per-ring delayed flash — they
  picked the **per-ring delayed flash**. Landed verbatim; only the constants moved.
- **The constants had a real bug, and it's the interesting one.** Ring `d` fades
  out at `phase = delay*d + fade`. With the sketch values (`0.12`/`0.4`) that's
  `1.36` for the 8-cell corner-to-corner distance on the tall/wide pads — but
  `progress()` clamps `phase` at `1.0`, so the outermost ring was still at ~90%
  brightness when the effect expired and vanished. **It is not fixable by
  lengthening `RIPPLE_DURATION`**: phase is normalized, so duration sets
  wall-clock speed while the truncation lives entirely in whether two
  dimensionless constants sum past 1.0. Fixed by `0.075`/`0.25` at a 800 ms
  duration — *identical* 60 ms-per-ring propagation and ~200 ms fade to what the
  user chose, but the tail now completes, with ~8 frames instead of ~5 at the
  100 ms poll. Guarded by `ripple_completes_before_expiring_on_every_pad`, which
  computes each shipped pad's real max distance (so a future bigger pad fails the
  test instead of shipping the pop); verified non-vacuous by restoring the old
  constants and watching it fail at `0.5999999`.
- **`apply_ripple` yields entirely on a focused/pressed key.** Found during
  review: the pressed key is always at distance 0, i.e. full ripple intensity at
  the exact instant its own flash fires, so without the guard every press
  overwrote its `loud` chip border with a mid-lightness hue — undoing
  `rainbow-mode`'s light-theme legibility work.
- **Hue drift** (`EffectKind::Drift`, 1400 ms, rainbow-only) — fired from
  `activate` in `main.rs` on `Action::Equals && app.copy_text().is_some()`, the
  existing read-only "is there a result?" question, so an error can't claim a
  success. `drift_offset` is a **half-sine: zero at both ends**, so the palette
  leaves and returns to its resting hues continuously — the same continuity
  lesson the ripple's tail taught, applied up front. `DRIFT_SPAN = 54°` is ~1.5
  steps of the 36° digit grid: the palette visibly moves, but a digit lands
  *between* its neighbours' hues rather than squarely on one.
- **`Palette { theme, drift }` replaced the bare `Theme`** in every hue-building
  signature. `frame_palette` resolves it once per frame, so the display and the
  grid can't disagree — a drift that reached one and not the other would read as a
  rendering bug. Mechanical but wide; the alternative was threading a second
  parameter through a dozen signatures and doing it again for the next
  palette-wide modulation.
- **Display breath** (always-on, 4200 ms) — the one effect that is *not* an
  `Effect`. It has no trigger and never expires, so it reads a free-running
  `animation_start` clock via `UiState::breath_phase()`; modelling something
  always running as something just started would mean re-inserting it forever.
  A full sine so the loop point is invisible, on the display *border* so it can
  never make the expression harder to read, small amplitude because it is the only
  thing on screen that moves unprompted.

### Review round (`/code-review --fix`, 2026-09-20) — one real visual bug

**The ripple was darkening the border, not lighting it.** `ripple_color`'s
lightness ramp was anchored at absolute HSLuv endpoints (`Dark: 20→90`) while a
resting button's `border_style` is `Style::new()` — **no `fg` at all**, so it
renders in the terminal's *default foreground*, already ~L 80 on a dark theme. With
`apply_ripple` pre-scaling by `RIPPLE_CEILING = 0.55`, the wave peaked at L 58.5 and
faded toward L 21: every ring dimmed the border below its resting brightness, then
**snapped back to bright** when it crossed `RIPPLE_FLOOR` and `apply_ripple` handed
back the unstyled base. Once per ring, per press — the same pop the ring-fade
invariant exists to prevent, at the other boundary.

The fix generalizes the continuity lesson: **anchor the ramp at the resting
appearance**, so the hand-off where painting stops is invisible, then move away
from the background (`RESTING_BORDER_L_*` → `RIPPLE_PEAK_L_*`). `RIPPLE_CEILING`
is gone — with the endpoints now naming the actually-reachable range, a separate
scale factor only made the declared peak unreachable.

Fixing it surfaced a **second** bug the first fix walked into: peaking at L 100
made the rainbow ripple *pure white regardless of hue* (HSLuv at either extreme is
white/black at any saturation), erasing the hue at its most visible moment — the
same "no contrast left at the extreme" trap as the light-theme `loud`/`knockout`
bug. So the peak now stops short (93 / 9) and **saturation ramps with intensity**
too: the border grows *into* its hue from the resting gray. Rainbow is carried by
saturation (there is little lightness headroom above an already-bright resting
border), mono by lightness (it has no hue to grow into, so `full_saturation` is 0)
— still one code path, each mode using the channel it actually has.

**Why the tests didn't catch it:** `ripple_brightens_on_dark_and_darkens_on_light`
compared two *ripple* colors to each other and never to the resting style, so it
passed vacuously while the ripple ran backwards. Two colors differing tells you
nothing about direction. Rewritten to assert on the lightness numbers via a new
pure `ripple_lightness`, plus `ripple_hands_off_to_the_resting_border_without_a_jump`
pinning the continuity contract between the ramp and the floor.

Also fixed in the same round:
- `drift_is_rainbow_only_but_the_theme_still_applies` was **clock-flaky**: it
  asserted `drift != 0.0` immediately after `register_drift()`, which only held
  because `elapsed()` happened to exceed 0. `drift_offset(0.0)` is exactly `0.0`,
  so a coarse clock fails it — not hypothetical here, since `Instant` comes from
  `web-time` and `performance.now()` is deliberately quantized on wasm. Split the
  pure `palette_for(mode, theme, drift_phase)` out of `frame_palette` and assert at
  a chosen phase, keeping one live-wiring assertion that doesn't depend on elapsed
  time. **Lesson for the rest of this feature:** `progress()`-based assertions must
  pick their own phase, never read one from a clock.
- Two stale doc comments (a "500 ms / five frames" pacing line left over from
  before the constants moved to 800 ms, and `RIPPLE_CEILING`'s description).
- `docs/tasks/rainbow-animation.md` still presented the superseded design as the
  plan; it now opens with the three decisions that diverged, per the precedent
  `quick-input.md` set.

**Knowingly not fixed — a policy question, not a defect.** Any keypress during the
1400 ms drift calls `start_effects`, which clears the collection, so the drift dies
mid-sweep and every hue snaps back by up to 54° in one frame — precisely the
discontinuity `drift_offset`'s half-sine is shaped to avoid at its natural end.
Cancelling *is* the documented latest-wins policy from the design, so changing it
is a design decision rather than a bug fix. If it reads badly in practice, the
one-line change is to `retain` the global `Drift` in `start_effects`.

### Notes for whoever picks this up next

- **The motion itself is verified by eye, not by test.** Everything above is
  unit-tested, but the end-to-end wiring *in motion* isn't: a `TestBackend` render
  right after a press only ever shows distance 0 (ring 1 hasn't started at
  `phase ≈ 0`), so asserting a travelling ring needs a `sleep`, and this repo
  deliberately has none — see `tick_keeps_fresh_flash`. So the pure curves are
  guarded, the *feel* is not. **Re-tune by eye after touching any constant**
  (`RIPPLE_RING_DELAY`/`_FADE`, `DRIFT_SPAN`, `BREATH_PERIOD`, `breath_color`'s
  lightness range) — a green suite does not mean it still looks right.
- **The two deferred focus-triggered effects** (directional wave on `move_focus`,
  breath on the focused cell) are still deferred, per the design — they use the
  same `Effect` abstraction, so trialling them costs no plumbing.
- **The "ripples compose" stretch** is untouched: flip `start_effects` to retain
  live ripples and append up to a cap.
- No user-facing keys changed, so `README.md` needed no update.

## fever-mode — `src/ui_state.rs`, `src/ui.rs`, `src/main.rs`

**Shipped** 2026-10-06 as PR #25 (branch `fever-mode`). 180 tests pass;
`cargo fmt`, `cargo clippy`, `cargo clippy --tests` clean. Followed the
`/ship:ship` workflow; two reviewers ran (ship diff-reviewer surfaced doc
drift between the first draft and the final shipped shape, fixed in
`a4f6e7e` + `f56fd64`; Codex found no actionable regressions). An earlier
`/code-review` surfaced six findings; five fixed in `b5da8d8` (dead
`ColorMode::default`, stale test clamp, "defense in depth" mis-claim,
stage-4 color-capture clock race, missing composed test for plain-style +
ripple gate), the sixth skipped as speculative about a non-linear decay
model that doesn't exist.

Turns the user's typing pace into a visual reward: a `0..=FEVER_MAX` (= 4.0)
meter climbs on each press (`+0.15`), decays when idle (`-0.05/s`), and a
successful `=` pauses decay for 2.5 s so reading the result doesn't cost
altitude. Four stages (`FeverStage::One..Four`) snap at `1.0 / 2.0 / 3.0`
thresholds with `±0.08` hysteresis — one unit per stage, so the climb and
decay constants read as "per-stage" rather than against an abstract
progress bar. Each stage enables more of the UI: `One` is plain (no palette
color, no animation), `Two` turns colored mono highlights on, `Three` adds
ripple + display breath, `Four` is full rainbow + hue drift. The meter *is*
the display box's bottom border — **per-stage geometry**, each stage drives
the fill 0 → 100 % of width in its own color, with the previous stage's
color persisting as a base layer on the left and a one-cell `<` marker at
the boundary. Stages 1-3 are three grays (dim / medium / bright, moving away
from the background per theme); stage 4 is a slowly-rotating hue — grays
below the top so stage 4's drift is the one place color appears.

**Replaces `r`.** The `r`/`R` toggle for `ColorMode` is gone; `color_mode()` is
now derived from stage, and `ColorMode` the enum stays as the renderer's
internal view only. `t` (theme) stays as a user preference, orthogonal to fever.
**Paste is intentionally outside fever.** `Event::Paste` routes to
`App::apply_str`, which never calls `activate` — so pasting `78-65*5=` neither
climbs the meter nor fires the reading grace. Paste is one logical edit, and
one edit is one press's worth of climb at most; the fever mechanic is tied to
*typing* pace, not keyboard-independent input.

### What landed where

- **`ui_state.rs`** — `FeverStage` enum with three predicate methods
  (`colored_highlights`, `animated`, `rainbow`), four new fields on `UiState`
  (`fever_score`, `fever_stage`, `fever_last_tick`, `reading_grace_until`),
  three pure helpers (`score_after_decay`, `decayed_with_grace`, `next_stage`),
  and the lazy-decay machinery (`apply_decay` private + `register_press_fever`
  + `register_grace`). `tick()` catches up outstanding decay before every draw.
  `fever_meter_hue_phase()` is the stage-Four meter's free-running 20 s clock,
  parallel to `breath_phase()`.
- **`main.rs`** — `activate` fires `register_press_fever` after
  `register_press`; a successful `=` fires `register_drift` *and*
  `register_grace`. The `r`/`R` branch is deleted. A new test asserts `r` is
  now inert.
- **`ui.rs`** — `draw_display` gates the breath on `stage.animated()` and
  overlays the fever meter on the bottom border (`draw_fever_meter` renders
  three regions: previous-stage base, `<` marker, current-stage fill — colors
  resolved through `fever_meter_color` / `base_meter_color` / `stage_meter_color`,
  with `meter_gray(theme, GrayLevel)` for stages 1-3 and `stage_four_meter`
  for the drifting hue). `draw_buttons` gates ripple extraction on
  `stage.animated()` so stages 1-2 short-circuit before touching `effects`.
  `draw_button` dispatches to a new `plain_style` at stage 1
  (`BorderType::Thick` on focus, `REVERSED` on press, no palette color
  anywhere). `frame_palette` gates the drift on `stage.rainbow()`;
  `palette_for` also refuses to drift under `ColorMode::Mono` as part of its
  own contract (the two checks are tautologically equivalent since
  `color_mode()` is derived from `stage == Four`, so they can't disagree — the
  second is `palette_for`'s own property, not a backstop).

### Decisions worth preserving

- **Press flash stays always on.** It is input confirmation, not decoration —
  gating it would make a terminal with any input lag unusable at low stages.
  The `plain_style` press branch uses `Modifier::REVERSED` (no explicit color,
  so the Light theme is covered for free; `loud`/`knockout` would have been a
  trap here).
- **Effects fire unconditionally, render conditionally.** `register_press`
  still inserts both `Press` and `Ripple` into `effects` at every stage;
  `register_drift` still inserts `Drift` on every successful `=`. The data
  model stays untouched — only the renderer branches on stage. This keeps
  tests that assert on `effects()` membership green (e.g.
  `drift_fires_only_on_a_successful_evaluation`) and lets a late stage climb
  pick up effects already in flight.
- **No stage-up celebration flash.** The meter's color/length change *is* the
  signal. If a one-off tell proves wanted, it's a cheap retrofit: one
  `start_effects` call with a `Ripple { cell: meter_cell }`.
- **"Visible meter" without an extra widget.** The display box already has a
  bottom border; `draw_fever_meter` is a *post-render overlay* that only
  recolors specific cells' `fg` via `frame.buffer_mut()[(x, y)].set_fg(...)`.
  Zero extra layout space, and the `╰───╯` characters already drawn carry the
  meter.
- **Lazy decay, not a timer.** The meter is advanced only when `tick` runs or
  `register_press_fever` is called — never from a background thread. The run
  loop's 100 ms repaint cadence paces decay naturally, and the pure helpers
  mean every tune-knob is unit-testable without a sleep.

### Test coverage

- Pure math: `score_after_decay` (rate + zero floor), `decayed_with_grace`
  (whole-interval pause + partial pause + no-grace equivalence to plain
  decay), `next_stage` (climbs at upper edge of hysteresis, falls at lower
  edge, holds in deadband around each of `1.0 / 2.0 / 3.0`).
- State: startup is `(One, 0.0, Mono)`; `register_press_fever` climbs by
  `FEVER_CLIMB`; 30 presses reach `Four` (one unit per stage); `register_grace`
  + manual `apply_decay(now)` freezes the score and then decays only the
  post-grace portion. Grace boundary pinned to `fever_last_tick + FEVER_GRACE`
  directly to sidestep the `Instant::now()` micro-slip between `new()` and
  the call. `fever_fill_fraction` resets at each stage boundary and clamps
  within the hysteresis band.
- Rendering (TestBackend at 28×29): the meter's whole bottom row is erased
  (space characters) at startup — "no bottom border" at score 0;
  stage-Two renders both the stage-1 base and the stage-2 current layers;
  stage-Three shows the `<` marker at the layer boundary;
  stage-Four's whole border shares a single hue (test asserts uniformity, not
  a specific color — stage 4's `fever_meter_hue_phase` is a real-time clock
  that would race `fever_meter_color()` with `render()`);
  `plain_style` sets no palette color at stage One;
  the display border carries no breath fg below stage Three;
  and `stage_one_button_borders_stay_uncolored_after_a_press` fires a press
  (which inserts a `Ripple` effect in `effects` at any stage) and asserts
  the adjacent button's border stays uncolored — defends the `view.ripple = 0.0`
  gate at `draw_buttons` against regression.
- Integration: `r_key_is_inert_after_fever_took_over` guards the removal.
  `drift_is_rainbow_only_but_the_theme_still_applies` reaches mono through
  the fresh-UiState default (stage One) rather than the deleted toggle.

### Carry-forwards

- **README has new content, not just a column drop.** The keys table loses
  `r` and gains a whole Fever section with the four-stage table and the rate
  math. The feature bullet changed from "per-digit rainbow coloring" to
  "fever mode". This is exactly the pattern CLAUDE.md warns about — README
  drift is cheap to introduce, so the whole feature-set description must
  land with every keymap change.
- **The three meter knobs** are `meter_gray`'s per-theme lightness tiers
  (Dark: 50/70/92, Light: 55/32/10), `stage_four_meter`'s saturation+lightness
  (from `theme_sl`), and `FEVER_METER_PERIOD` (20 s). All picked by taste; a
  playtest-driven tweak would most likely nudge the Dim/Medium separation on
  the Dark theme (50→55 would be a modest "stage 1 reads brighter" nudge).
- **`+0.15/press` is a 15 % climb *per stage*.** The score runs on `0..=4`
  (one unit per stage), not `0..=1` — the first implementation used a 0..1
  scale and the climb felt too fast (one press was 60 % of a stage), so the
  internal score was rescaled to 0..4 while the constants stayed. 10 presses
  in 15 s now net `+0.75` of one stage — a burst climbs noticeably but
  doesn't skip.

- **Meter geometry is per-stage, not overall.** The first visual pass filled
  the whole bar from `score / FEVER_MAX` and recolored it per stage; the
  stage-1 band (0..25 %) was impossible to distinguish from the resting
  foreground, and only four cells of real estate landed on the stage you
  were actually in. Rewritten so each stage drives its own 0 → 100 % fill in
  its own color (`fever_fill_fraction` now returns sub-stage progress). The
  previous stage's color stays as a base layer on the left; the current
  stage's color overlays from the right; a one-cell `<` marker sits at the
  layer boundary — the three stage grays are intentionally close in
  lightness, so the marker is what carries the "here's the current head"
  signal without relying on fine color discrimination. At score 0 the bottom
  border is literally erased (space characters) because "below stage 1"
  layer is nothing.

- **Grayscale 1-3, hue only at 4.** An earlier draft used a warm orange hue
  for stage 3; it collided with stage 4's drifting rainbow whenever the
  drift rotated near orange. Three grays (dim / medium / bright, each one
  step further from the background per theme — same `loud`/`knockout`/
  ripple/breath convention) sidestep the conflict and give stage 4 the only
  color-carrying slot. Grays mean the three stage colors are hard to tell
  apart on their own, which is exactly what the `<` marker exists to
  compensate for.

## Next Task

With fever-mode shipped, the layout arc + rainbow pass + fever pass are all
behind us. **`web-ratzilla` is the only remaining work**, split on 2026-10-06
into `web-spike` / `web-core-split` / `web-msg` / `web-entry` / `web-deploy` /
`web-paste` (optional) — see their stub sections below and the banner in
`tasks/web-ratzilla.md`. **Start with `web-spike` and `web-core-split` in
parallel** (no shared files). The carry-forwards below still apply; they're
mostly for `web-msg` (the `activate` funnel) and `web-entry` (pacing).

- **`web-ratzilla`** — Ratzilla WASM build + Cloudflare Pages deploy. Known gaps:
  event-loop inversion → a `Msg` enum (see the deferred note above — this is the
  task that would justify it), `arboard` → `navigator.clipboard`, and a crate
  split. **Carry-forward from `quick-input`:** `QUICK_MAP` is a pure
  `char → label` table with no crossterm types, so ratzilla can reuse it verbatim;
  and the sticky mode *sidesteps* ratzilla's keydown-only `on_key_event`
  limitation rather than inheriting it, since nothing depends on observing a
  modifier's release. It's oversized for one task and splits naturally into three
  (extract core + `Msg` / web entry + clipboard / Trunk + deploy) — worth doing
  once the crate-shape open question is settled.

  **Carry-forward from `rainbow-animation`:** the core it extracts is now bigger
  in one specific way — `ui_state.rs` gained the `Effect`/`EffectKind` model and a
  free-running `animation_start` clock, and `ui.rs` gained the `Palette` and the
  three animation curves. All of it is already backend-agnostic: the curves are
  pure functions of normalized numbers (no `Instant` reaches the renderer), and
  the clock is `web-time`'s, so the wasm build gets `performance.now()` for free.
  The one thing to watch is **pacing**: the constants are tuned against the native
  loop's ~10 fps redraw, and ratzilla drives rendering from
  `requestAnimationFrame` (~60 fps). The effects will be *smoother* on the web,
  not broken — but the always-on breath means `draw_web`'s closure never has an
  idle frame, so the "gate redraws on an effect being active" note in
  `rainbow-animation.md` matters more there than it does natively.

  **Carry-forward from `fever-mode`:** `ui_state.rs` grew another always-on
  clock (`fever_last_tick` + lazy decay via `apply_decay`, called from
  `tick()`), so the web build's gate-redraws-on-change plan now has to let
  "any press within the last 20 s" count as a reason to repaint — the meter
  decays visibly between frames and the stage-4 hue phase cycles every 20 s.
  All of the fever math is pure (`score_after_decay` / `decayed_with_grace`
  / `next_stage` / `fever_fill_fraction`), no renderer dependency, so the
  web port needs no changes to the math itself. `register_press_fever` and
  `register_grace` plug into `activate`, which the extract-core-and-Msg
  split will have to preserve as the single input funnel.

## web-spike — throwaway branch `spike/web-ratzilla` (not merged)
Status: done (2026-10-07). Goal: prove Trunk + Ratzilla + Cloudflare end-to-end
and record answers to the seven questions in `tasks/web-spike.md` here. Live
probe: https://tui-calculator-spike.i-70e.workers.dev (throwaway; delete the
`tui-calculator-spike` Worker once `web-deploy-cf` ships).

**2026-10-07.** Toolchain: rustc 1.99, `wasm32-unknown-unknown`, trunk 0.21.14,
ratzilla 0.3.1 (ratatui 0.30.x, `default-features = false`). Probe app at
`spike/` on the spike branch: shows key/mouse events, frame size + fps,
calculator glyphs, titled bordered buttons; `?backend=dom|canvas|webgl2`; `p`
toggles our own preventDefault; `y` writes the clipboard. Driven in Chrome via
DevTools; claims below were observed, plus a read of ratzilla's source.

**Build gotcha:** without default features, `ratatui-core` links against
`critical-section` and the final wasm bin fails to link
(`undefined symbol: _critical_section_1_0_acquire`). Ratzilla only pulls an
impl into its *dev*-deps, so the web bin must add
`critical-section = { version = "1", features = ["std"] }`. A wasm **lib** build
doesn't hit it (no link step).

**Q1 — browser defaults: Ratzilla does no `preventDefault` at all** (none in its
source). Observed: an unhandled Tab moves focus from the grid to `<body>`, and
then **every later key is lost** (Ratzilla's listener is on the focusable grid
element, not the document). A document-level **capture-phase** keydown listener
that calls `preventDefault` for Tab / Space / `/` / `'` / Backspace fixes it; it
runs before Ratzilla's listener and doesn't stop propagation, so Ratzilla still
gets the key.

**Q2 — resize: DomBackend loses ALL input after any window resize.** Its resize
handler (`dom.rs` `reset_grid`) swaps in a *new* grid `<div>`, but the key and
mouse listeners (and `tabindex`) stay on the old, detached one. Observed after a
resize: focus on `<body>`, keys and clicks both dead. The new size *does* show up
as `frame.area()` (139×39 → 69×39), so `auto_select` from `draw_web` works.
WebGl2 keeps input across a resize only because its canvas never resizes (it
stays 70×44). → `web-entry` must not rely on `on_key_event` / `on_mouse_event`
with DomBackend; see "Consequences" below. Worth an upstream issue/PR to
ratzilla (re-attach callbacks in `reset_grid`).

**Q3 — renderer: DomBackend**, with one rendering workaround.
- *DomBackend:* browser font (Fira Code), box-drawing / `×` / `÷` / `−` / `⌫`,
  truecolor and titled borders all render correctly. **Bug:** `Modifier::REVERSED`
  with default colors renders **white-on-white** (DOM style is
  `color: rgb(255,255,255); background-color: rgb(255,255,255)`), because
  `Reset` fg and bg both resolve to white before the swap. This hits our stage-1
  `plain_style` press flash, which is exactly REVERSED with no fg/bg.
- *WebGl2Backend:* REVERSED correct, but its own bitmap font atlas mangles `⌫`
  and drops `−`; fixed-size canvas; pulls ~1.2 MB of atlas into the wasm.
- *CanvasBackend:* worst — gapped box-drawing lines, REVERSED ignored entirely.
- Pacing: ~82–120 fps (rAF on a high-refresh display) with a per-frame hue cycle,
  no visible jank on the DOM backend.

**Q4 — core without crossterm: yes.** The six core modules (action, app, eval,
layout, ui_state, ui) as a lib with
`ratatui = { version = "0.30", default-features = false, features = ["palette"] }`
+ `web-time`: **all 154 tests pass natively** (including the `TestBackend` render
tests), it builds for `wasm32-unknown-unknown`, and `cargo tree` shows zero
crossterm/arboard. `web-core-split` can use these features from the start.

**Q5 — mouse: exact.** `on_mouse_event` reports `SingleClick(Left)` with grid
`col`/`row`; a click on a button's top-right border cell came back as exactly
the expected `(27, 13)`, and a `Rect::contains` hit-test (same shape as
`button_at`) resolved the right button. (Before a resize; see Q2.)

**Q6 — modifiers: Cmd is invisible.** Ratzilla's `KeyEvent` has no `meta`; Cmd-C
arrives as `Char('c')` with ctrl/alt/shift all false. In the calculator that
**clears the expression**; Cmd-T → `t` toggles theme, Cmd-A → `a` resumes auto.
Our own document listener can read `KeyboardEvent.metaKey` and drop Cmd chords
before they reach the mapper (leaving the browser's own copy/paste intact).

**Q7 — build/size/clipboard.** `trunk build --release` with `data-wasm-opt="z"`,
`opt-level="z"`, LTO: **DomBackend-only 256 KB raw / 108 KB gzip** (the
all-backends probe was 1.4 MB / 1.1 MB, almost all of it WebGl2's atlas).
`navigator.clipboard.writeText` from a keydown handler resolved (`Copied!`) on
localhost.

**Q7 — deploy: Cloudflare Pages is now Workers Static Assets.** With wrangler
4.148, `wrangler pages project create` / `pages deploy` *delegate* to Workers
Static Assets and use the **cwd** as the asset directory. Run from `spike/`, it
swept up `target/` and failed on a 28 MiB debug `.wasm` (25 MiB per-file limit);
nothing was deployed. Cloudflare's docs now steer static sites to Workers. What
worked: an assets-only `wrangler.jsonc` (`name`, `compatibility_date`,
`assets.directory = "./dist"`, **no `main`**) + `wrangler deploy`. Served at
`*.workers.dev` over HTTPS with `application/wasm` and brotli. On the live URL:
renders at ~120 fps, keys arrive, `navigator.clipboard.writeText` → `Copied!`, no
console errors apart from a favicon 404. Right after a deploy the edge returned
404 for individual assets for roughly 10–30 s. Smoke tests should retry, not fail
on the first 404. `wrangler deploy` also appends wrangler entries to the
nearest `.gitignore`.

### Consequences for later tasks
- **web-entry:** use DomBackend. Own the input instead of Ratzilla's callbacks:
  one document-level keydown listener (capture) that (a) drops Cmd/meta chords,
  (b) `preventDefault`s Tab/Space/`/`/`'`/Backspace, (c) converts
  `KeyboardEvent` → core `Key` directly (we get `metaKey` for free, and it
  survives resizes). Mouse: a document/body click listener mapping
  `clientX/Y` → cell via the grid element's rect and size (or fix ratzilla
  upstream and use `on_mouse_event`). Add the `critical-section` dep.
- **REVERSED white-on-white:** `plain_style`'s press flash needs an explicit
  fg/bg on the web (or a ratzilla fix in its color mapping). Decide in
  `web-entry`; it's the one place the "terminal default drives the flash" design
  doesn't carry over.
- **web-core-split:** use `default-features = false, features = ["palette"]`
  for core's ratatui from day one (Q4).
- **web-deploy:** target **Workers Static Assets** (assets-only `wrangler.jsonc`
  → `wrangler deploy`), not `wrangler pages deploy`. Keep `wrangler.jsonc` in the
  web crate dir with `assets.directory` pointed at Trunk's `dist/`, and never
  let the asset dir default to a directory containing `target/`. Add a favicon.
- **web-msg:** `Key` needs a `meta` flag (or the web entry filters Cmd chords
  before building a `Key`); the native side always sets it false.

## web-core-split — `Cargo.toml`, `crates/core/`, `src/main.rs`
Status: done (2026-10-07). Goal: workspace with a backend-free `calculator-core`
lib + thin native bin; pure relocation, test suite unchanged.

**Shape.** The root package stays the native binary (`tui-calculator`,
`src/main.rs`) and is also the workspace root, so `cargo run` is unchanged.
`crates/core` (`calculator-core`) holds action, app, eval, layout, ui_state and
ui, moved with `git mv` so history follows them. `lib.rs` just declares them
`pub mod`. `crates/web` will be the next member. No `[workspace.dependencies]`:
the core wants ratatui's default features off and the binary wants them on, and
Cargo can't turn defaults back on for a member that inherits a
`default-features = false` workspace entry. Two plain entries are clearer.

**What actually had to change** (besides imports in `main.rs`):
- Nothing needed widening for `main.rs`'s own code: everything it uses was
  already `pub`.
- **Test-only getters.** `main.rs` tests call four `#[cfg(test)]` `UiState`
  getters (`focus`, `layout_index`, `override_layout`, `fever_score`), and
  `cfg(test)` isn't visible to another crate's tests. They're now gated
  `#[cfg(any(test, feature = "test-support"))]`, and the root crate enables
  `test-support` only through its **dev**-dependency on the core. So the web
  crate never sees them, and making them unconditionally `pub` would have leaked
  the raw focus cell into the core's API. `web-msg` moves those tests into core,
  after which this can go back to plain `cfg(test)`.
- **`impl Default` for `App` and `UiState`**, delegating to `new()`. Clippy's
  `new_without_default` only fires on *exported* types, so becoming a library
  surfaced it.

**Verified.** `cargo test --workspace`: 154 core + 25 native = **179**, the same
as before the move. `cargo clippy --workspace --all-targets` and
`cargo clippy -p calculator-core` (without `test-support`) are clean, and
`cargo fmt --all --check` passes. `cargo build -p calculator-core --target
wasm32-unknown-unknown` succeeds, and `cargo tree -p calculator-core` has no
crossterm or arboard. Native smoke test through a pty: `78-65*5` Enter shows
`-247`, and `q` exits with status 0 within about 0.1 s, matching `main`. (A first
harness run looked like a hang on quit. It was the harness, which stopped
reading the pty, so the app's terminal-restore writes blocked.)

**For `web-msg` / `web-entry`.** Plain `cargo test` at the root now runs only
the binary's 25 tests, so use `--workspace` (CLAUDE.md updated). The web crate
should depend on `calculator-core` only; ratatui comes re-exported via ratzilla,
so match versions (0.30.x).

## web-msg — `crates/core/src/input.rs` (new), `src/main.rs`
Status: done (2026-10-07). Goal: neutral `Key` → `Msg` mapper + `apply_msg` in
core so native and web share one definition of what each key does.

**What landed.** A new core module, `input.rs`:
- `Key { code: KeyCode, ctrl, alt }`, with the core's own `KeyCode` (`Other`
  for anything unused).
- `Msg`, with ten variants.
- `key_to_msg(key, quick_mode)`.
- `apply_msg`, total over `Msg`.
- `activate`, moved verbatim with its ordering comments.
- `key_to_action` and `focus_dir`, retyped on the core `KeyCode`.

`main.rs` dropped from 730 to 331 lines. Its key handling is now
`to_key(crossterm KeyEvent)` → `key_to_msg` → `apply_msg`, with `Msg::Copy`
intercepted for `do_copy`/arboard. Mouse, resize and paste routing are
unchanged (all native).

**Decisions.**
- **No `meta` on `Key`.** The merged `web-entry` design drops Cmd chords before
  building a `Key`, and terminals never report Cmd.
- **`Copy` is a documented no-op in `apply_msg`** rather than a separate
  "effect" type. Both entry points intercept it, the web sketch already matched
  on it, and a test pins the no-op (`copy_is_left_to_the_entry_point`). `Quit`
  sets `app.should_quit`, which is ordinary `App` state; the web never reads it.
- **Quick-mode enter/leave are two variants**, not `SetQuickMode(bool)`: the
  triggers are one-way keys, so the names read better at call sites.
- **Inert nav letter:** `key_to_msg` returns `None` early inside the quick-mode
  block, rather than adding a condition to the navigation rule, so the rule
  order reads top to bottom. (Lix chose to have me write `key_to_msg` and will
  review it.)

**Verified.**
- `cargo test --workspace`: **182 core + 5 native = 187.** The 22 key tests
  moved from `main.rs` to `input::tests`, which drive state through
  `key_to_msg` → `apply_msg`. Five tests are new: Ctrl-C vs bare `c`, the full
  command table, meaningless keys → `None`, Alt-Esc still leaves quick-mode,
  and Copy is a no-op in core. `main.rs` keeps native-only tests: `to_key`
  translation (twice), an end-to-end crossterm → rules wiring test, resize
  routing, and `do_copy`.
- **Differential check against `main`** (scratch crate, not committed): old
  `handle_event` key branch verbatim vs new `to_key` → `key_to_msg` →
  `apply_msg`. That covered all 95 printable ASCII keys + 11 named keys × 6
  modifier combos × 5 starting states (fresh, quick-mode on, mid-expression,
  evaluated, pinned pad + quick-mode + typed digit), comparing display, focus,
  quick-mode, pad, override, theme, quit flag and effect count. **3,180 cases,
  0 differences.** A planted bug (new path ignores Esc) produced 12
  differences, so the check does catch real changes.
- Clippy (workspace and core-only) clean, fmt clean, core builds for wasm32.
- PTY smoke test: quick-mode `k d j m` Enter gives the same screen as the
  `main` build (`2×10` / `20`), and Ctrl-C exits with status 0 in ~0.1 s.

**test-support narrowed.** The native tests now use only `layout_index` and
`override_layout`, so `focus` and `fever_score` went back to plain
`cfg(test)`. The feature stays for the other two.

**For `web-entry`.** Build a `Key` from `KeyboardEvent` (`key()` →
`KeyCode::Char` for single chars, plus the named keys; `ctrlKey`/`altKey`). Drop
`metaKey` chords before that. Call `preventDefault` exactly when `key_to_msg`
returns `Some`, intercept `Msg::Copy` (navigator.clipboard) and ignore
`Msg::Quit`; everything else goes to `apply_msg`. Clicks use the core
`activate`.

## web-entry — `crates/web/`, `.github/workflows/rust.yml`, `README.md`
Status: done (2026-10-08). Goal: Ratzilla entry point with keys, mouse, copy and
animations working under `trunk serve`.

**2026-10-08.** New workspace member `crates/web` (`calculator-web`, bin):
DomBackend, `Rc<RefCell<Web>>` state, document-level `keydown` (capture) and
`mousedown` listeners, `tick` + `ui::draw` in `draw_web`, `auto_select` when
`frame.area()` changes, `navigator.clipboard` copy. CI's wasm step now builds
`calculator-web` (which covers the core). README gained an "In the browser"
section.

**Found while building (not in the spike):**
- **DomBackend renders `Color::Reset` as hard-coded white**: `color: rgb(255,255,255)`
  for any default fg, and a white background for a `REVERSED` cell with a default
  bg. The spike saw this only as the stage-1 white-on-white flash, but it also
  makes the **Light theme unreadable** (default-colored text drawn white on a
  light page). Fix, web-only: after `ui::draw`, `resolve_default_colors` swaps
  every `Reset` for the theme's own default fg/bg. In the browser the entry
  point *is* the terminal, so it supplies the defaults; the foregrounds are the
  HSLuv grays at `RESTING_BORDER_L_*` so the ripple anchor holds exactly. No
  core change, and `plain_style` keeps its "terminal default" design.
- **`frame.area()` ≠ the DOM grid.** `DomBackend::size()` is the window size in
  cells minus one; the grid is sized from `<body>`. Clicks therefore count the
  DOM's own rows and cells, not `frame.area()`. `#grid { width: fit-content }`
  makes its rect ÷ count the cell size (a block `div` spans the whole body, which
  skews the column math toward the right edge; Ratzilla's own mouse code has
  that skew).

**Deviations from the spec, deliberate:**
- **Ctrl chords are dropped as well as Cmd.** Ctrl is the browser-shortcut key
  on Windows/Linux: Ctrl-C is copy (and would be `Msg::Quit`), Ctrl-− zooms (and
  would type `−`). No Ctrl chord does anything useful in the calculator on the
  web, so they all keep their browser meaning.
- **`Msg::Quit` returns before `preventDefault`**, so `q` keeps its (empty)
  default: a key that does nothing shouldn't cancel anything.
- **`mousedown`, not `click`.** Matches native's `MouseEventKind::Down`, fires
  before the mouse-up, and still counts as a user gesture for the clipboard.
- Theme seeded from `prefers-color-scheme` (the spec's optional item). *Reverted 2026-10-08: the web app always starts Dark, like native (Lix's call).*

**Verified** in Chrome via DevTools on `trunk serve`:
- `78-65*5` Enter → `-247`.
- The theme was seeded light from the OS setting, and the wide pad was
  auto-selected.
- The stage-1 press flash is a dark fill, not white on white.
- Clicks on all four corner cells of seven buttons each hit their own button,
  including the tall `=`.
- After a resize (1100×700 → 700×900), corner clicks and keys still work.
- `preventDefault` fired exactly for Tab, Space, ArrowDown, `/`, `'` and
  Backspace, and not for `q`, F5 or `x`.
- Cmd-C with `42` on screen left it unchanged and kept its browser default.
- `y` → `writeText("42")` resolved and the status line read `Copied!`.
- Fever climbed to the stage-4 rainbow.
- The console showed no errors, only Trunk's `integrity` preload warning.
- Natively, `cargo test --workspace` passes. Clippy is clean, both natively and
  for `wasm32`.
- Not checked by automation: holding `⌫` (the handler doesn't filter repeats,
  so it follows the native behavior by construction), and a real paste in
  another tab. My `navigator.clipboard.readText()` probe hung on Chrome's
  permission prompt, so the copy was confirmed by wrapping `writeText` instead.

## web-deploy — `Cargo.toml`, `crates/web/index.html`, `.github/workflows/deploy-web.yml`, `README.md`
Status: done (2026-10-08). Goal: public URL with a repeatable build, plus
README web section.

**2026-10-08.** Rescoped: GitHub Pages here, Cloudflare in `web-deploy-cf`
(Lix wants both; Pages first).

- **Release build:** `[profile.wasm-release]` (`opt-level = "z"`, LTO, one
  codegen unit) plus `data-wasm-opt="z"`. The `.wasm` is **307 KB raw, 139 KB
  gzip, 114 KB brotli** (1.5 MB before wasm-bindgen and wasm-opt). The JS loader
  is 32 KB, 6.5 KB gzip. That's close to the spike's DOM-only 256 KB counter, so
  the whole calculator costs about 50 KB of wasm on top of Ratzilla.
- **`--public-url ./`** makes `dist/` host-agnostic. It was checked by serving
  `dist/` under `/tui-calculator/` with `python3 -m http.server`: the wasm loaded
  from the subpath, `78-65*5=` gave `-247`, and the console showed no errors.
- **Workflow:** the `build` job uploads with `upload-pages-artifact@v5`, then
  the `pages` job runs `deploy-pages@v5`. Pages was enabled with
  `build_type=workflow` through the API, so its URL is
  https://lix42.github.io/tui-calculator/. Concurrency group `pages`, which
  never cancels a deploy that's in flight.
- Tried and left out: `wrangler.jsonc` and `wrangler-action`. They wait for
  `web-deploy-cf`, along with the repo secrets.
- **Pitfall:** a `trunk build` racing a running `trunk serve` on the same
  `dist/` fails with "error writing JS loader file". Stop the serve first.

## web-deploy-cf — `crates/web/wrangler.jsonc`, `.github/workflows/deploy-web.yml`
Status: not started. Goal: the same `dist/` on Cloudflare Workers Static Assets.

## web-paste — `calculator-web` (optional)
Status: not started. Goal: DOM paste → `App::apply_str`, matching native
bracketed paste.
