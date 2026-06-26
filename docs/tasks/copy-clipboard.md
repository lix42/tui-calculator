# copy-clipboard: Copy Result to System Clipboard

## Requirement

After a successful evaluation, let the user copy the result to the system
clipboard. A `[y Copy]` affordance appears in the display box; pressing `y`/`Y`
or clicking it copies the result and shows a brief `Copied!` status. The
affordance disappears as soon as new input dismisses the result.

## Design

Copy is **not** an `Action`. It's a side-effecting command on the *result*, not
a calculator state transition, so it stays out of `App::apply`'s pure, total
match and out of the crossterm-free `action.rs`. It's routed in `main.rs`
alongside the other non-`App` inputs (quit, focus moves) — the I/O boundary that
already owns the terminal. (This mirrors the deferred-`Msg`-enum note in
`progress.md`: copy is a `Quit`-tier command, not an `Action`.)

The grid (`BUTTONS`) stays a fixed `static const`; the affordance lives in the
display area, so none of the grid/focus/hit-test machinery becomes dynamic.

- **`App::copy_text() -> Option<String>`** — the single source for both "is
  there something to copy?" and "what to copy". `Some(result)` only in
  `Mode::Evaluated`; `None` while editing or after an error. The UI reads
  `is_some()` to decide whether to draw the affordance.
- **`UiState`** gains `copy_rect` (captured each draw, like `button_rects`, for
  click hit-testing via `copy_hit`) and a transient `status` message
  (`set_status` / `status_text`, expired by the existing `tick` after
  `STATUS_DURATION` ≈ 1.5s — long enough to read, unlike the 120ms press flash).
- **`ui.rs`** renders `[y Copy]` (or the live status, which wins) left-aligned in
  the display's top row and reserves its column width, so the right-aligned
  expression is shrunk to never overlap the persistent hint. It records the rect
  on `UiState`, or `Rect::ZERO` when nothing is shown.
- **`main.rs`** routes `y`/`Y` and a click on `copy_hit` to `do_copy`, which
  calls `copy_to_clipboard` (a one-shot `arboard::Clipboard::set_text`) and sets
  the status to `Copied!` / `Copy failed`.

### Cross-platform note

`copy_to_clipboard` is a one-shot set. On macOS/Windows the text persists after
exit. On **Linux/X11** clipboard contents are tied to the owning process's
lifetime, so a copy may not survive the app exiting unless a clipboard manager
is running — documented as a code comment, not handled, by design.

## How to Test

Unit (pure, no clipboard touched):
- `App::copy_text` is `Some(result)` after `=`, `None` while editing, `None`
  after a div-by-zero error, `None` once a fresh digit dismisses the result.
- `UiState::copy_hit` tests against the stored rect (false when `Rect::ZERO`);
  `set_status`/`status_text` round-trip and survive a fresh `tick`.

Manual (the actual clipboard write is not unit-tested — it touches the system
clipboard):
1. Type `2+3`, press `=` — result shows `5`, `[y Copy]` appears.
2. Press `y` — `Copied!` flashes; paste elsewhere shows `5`.
3. Type a new digit — the affordance disappears.
4. Evaluate again, click the affordance — result copied.

## Dependencies

- **app-display-split** — `Mode::Evaluated` is what gates `copy_text`.
- **ui-display** — the display box the affordance renders into.
- **button-nav** / **mouse-input** — the key/click routing `do_copy` plugs into.
- `arboard` crate (already in `Cargo.toml`).
