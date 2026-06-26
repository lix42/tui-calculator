# layout-config: Configurable, Runtime-Switchable Button Layouts

## Requirement

The button grid is hard-coded as a 5×4 grid today. Support **multiple named
layouts** (e.g. the current standard pad, a compact pad, a wider/scientific pad)
and let the user **switch layout at runtime** — focus, hit-testing, and the
keyboard map must all keep working across the switch.

## Why this is the "big" one

The grid dimensions are baked in at *compile time* in several places — most
painfully as **const generics**, which cannot take a runtime value:

- `ui_state.rs`
  - `pub const BUTTONS: [[&str; 4]; 5]` — the grid itself.
  - `button_rects: [[Rect; 4]; 5]` — fixed-size array field.
  - `UiState::new` seeds focus at `(4, 3)` (the `=` cell) and `button_rects` at
    `[[Rect::ZERO; 4]; 5]`.
  - `move_focus` already derives bounds from `BUTTONS.len()` / `BUTTONS[0].len()`
    (good — it assumes only that the grid is rectangular).
  - `LABEL_POS` / `position_of` build a reverse index from `BUTTONS` as a
    process-global `LazyLock` — fine while there is exactly one grid, wrong once
    the grid can change.
- `ui.rs::draw_buttons`
  - `[Constraint::Max(5); 5]`, `[Constraint::Length(7); 4]`, `.areas::<5>(area)`,
    `.areas::<4>(*row_area)`, `[[Rect::ZERO; 4]; 5]` — **all** carry the `5` and
    `4` as const generics.
  - `centered_panel(frame.area(), 28, 29)` and the `Length(4) / Length(25)`
    split size the panel from the 4-col × 5-row grid implicitly.
- `main.rs` indexes `BUTTONS[r][c]` in the mouse and Space paths.

`Layout::areas::<N>()` returns a fixed `[Rect; N]`; `N` must be known at compile
time. A runtime-chosen layout can't use it. This task is fundamentally a refactor
from fixed arrays to `Vec`-backed, slice-based layout.

## Design

### A `Keypad` value as the single source of truth

Introduce a `Keypad` (in `ui_state.rs`, or a new `layout.rs`):

```rust
pub struct Keypad {
    pub name: &'static str,
    rows: Vec<Vec<&'static str>>, // rectangular: every row same length
}
```

Provide a static registry of available pads and a cursor into it:

```rust
static LAYOUTS: &[Keypad] = &[STANDARD, COMPACT, WIDE]; // built at startup
```

`UiState` holds the **active** keypad index and exposes `cycle_layout()` /
`set_layout(i)`. `BUTTONS` (the const) is replaced by `keypad.rows`. Keep every
pad **rectangular** (all rows equal length) so `move_focus` and hit-testing stay
trivially correct — a ragged grid would force span-aware navigation. Cells that
need to look wider (a double-width `0`) are deferred; note it under Open
Questions rather than complicating v1.

### Replace const-generic layout with slice-based `split`

In `draw_buttons`, build constraints from the active dims and use `Layout::split`
(returns an `Rc<[Rect]>`, runtime-sized) instead of `areas::<N>`:

```rust
let (rows, cols) = keypad.dims();
let row_areas = Layout::vertical(vec![Constraint::Max(5); rows]).split(area);
// ... per row:
let cells = Layout::horizontal(vec![Constraint::Length(7); cols]).split(row_area);
```

`button_rects` becomes `Vec<Vec<Rect>>`, rebuilt each draw (it already is, via
`set_button_rects`). `button_at` already iterates by index, so it only needs the
field type changed.

### Focus and the reverse index become per-layout

- `move_focus` keeps deriving bounds from the active pad's dims.
- On a layout switch, **clamp** focus into the new bounds (a `(4,3)` focus is
  invalid on a 3×3 pad) and rebuild the label→position index. `LABEL_POS` can no
  longer be a single global `LazyLock`; make `position_of` a method on `Keypad`
  (build the map when the pad becomes active, or compute lazily per pad). The
  grid is small, so an on-switch rebuild is cheap and keeps the index honest.
- Default focus per pad: store a `default_focus` on `Keypad` (each pad names its
  own "home" cell, e.g. `=`), rather than the hard-coded `(4, 3)`.

### Panel sizing derived, not literal

`centered_panel` and the display/grid vertical split currently encode `28`/`29`/
`25` for a 4×5 grid. Derive them: `panel_width = cols * CELL_W + frame`,
`grid_height = rows * CELL_H`, so a wider or taller pad still centers correctly.

### Switching at runtime — the trigger

Layout switch is **not** an `Action` (it doesn't transform calculator state),
mirroring the copy decision: route it in `main.rs` at the I/O boundary alongside
quit and focus-moves. Bind a key (e.g. `Tab`, or a chord) to `ui.cycle_layout()`.
Switching is a `UiState` concern only — `App` (expr/current/mode) is untouched,
consistent with the App/UiState split.

## Implementation Notes

- This refactor should land **before** `rainbow-mode` and `quick-input`: both
  touch `draw_buttons` and per-cell rendering, so doing the array→`Vec` move once
  avoids re-threading them.
- Keep a green checkpoint discipline: (1) introduce `Keypad` and route the
  existing 5×4 grid through it with **no behavior change** (tests stay green),
  (2) add the registry + `cycle_layout` + the trigger, (3) add the second/third
  pad. Step 1 is a pure mechanical refactor and is where the const-generic →
  slice change happens.
- The `move_focus` clamp test and `button_at` hit-test test in `ui_state.rs`
  already pin the invariants; extend them to a non-5×4 pad.

## How to Test

Unit:
- `Keypad::dims` / `position_of` for each registered pad; round-trip every label
  to a position and back.
- `move_focus` clamps within a 3×3 pad and a 5×4 pad.
- `cycle_layout` advances and wraps; focus is clamped into the new pad's bounds
  (switch from `(4,3)` on 5×4 to a smaller pad lands in-range).
- `button_at` resolves clicks against a `Vec<Vec<Rect>>` of non-5×4 shape.

Manual:
1. `cargo run`; press the layout key — grid visibly changes shape; the panel
   re-centers; a default cell is focused.
2. Type an expression, switch layout mid-expression — `expr`/result unaffected.
3. Click a button on the new layout — hit-test lands on the right cell.

## Dependencies

- **app-ui-state** — `UiState` owns the grid, focus, and `button_rects` this task
  generalizes.
- **ui-buttons** — the `draw_buttons` rendering being made dynamic.
- **mouse-input** — `button_at` hit-testing over the (now `Vec`) rects.

## Open Questions

- **Ragged grids / cell spanning** (a double-width `0`): deferred. v1 keeps every
  pad rectangular so navigation and hit-testing stay simple. Span support would
  need `move_focus` to skip covered cells and `button_at` to map a span to one
  logical cell.
- **Layout persistence**: should the chosen layout survive restart (a config
  file)? Out of scope here; note for a later `config-persist` task.
