# layout-config: De-hardcoded Button Grid with Cell Spanning

## Purpose & Scope

The button grid is hard-coded as a 5×4 grid of `&'static str`, with the `5` and
`4` baked in as const generics across `ui_state.rs` and `ui.rs`. This task has
two goals, in priority order:

1. **Remove the hard-coded layout assumption.** Even if we only ever shipped one
   keypad, the const `5`/`4` scattered through rendering, focus, and hit-testing
   is bad: the grid shape is not data, it's compiled-in structure. Make the
   layout a *value* the rest of the code reads, with no dimension known at
   compile time.
2. **Support buttons that span multiple cells** — a wide `0`, a tall `=` — so
   important keys can be bigger. This is the part that shapes the data structure,
   so it belongs with the de-hardcode work rather than bolted on later.

This task ships **one standard keypad** and gets the model right. It is the
foundation two follow-ups build on **without re-opening this data model**:

- **`layout-registry`** — multiple named pads + a manual switch key.
- **`layout-auto`** — auto-select the pad that best fits the terminal shape
  (narrow-tall vs. wide-short), on resize.

**Non-goals (explicitly out of scope):**
- Multiple pads, runtime switching, and shape-based auto-selection — deferred to
  the two follow-ups above. Core ships a single pad.
- Any new calculator keys, functions, or a scientific mode. This task adds **no
  new behavior** to the calculator — only the ability to describe and render a
  layout (including spans) as data. Adding buttons/functions is a separate task
  that consumes this one.

## Why this is the "big" one

The grid dimensions are baked in at *compile time* — most painfully as **const
generics**, which cannot take a runtime value:

- `ui_state.rs`
  - `pub const BUTTONS: [[&str; 4]; 5]` — the grid itself.
  - `button_rects: [[Rect; 4]; 5]` — fixed-size array field.
  - `UiState::new` seeds focus at `(4, 3)` (the `=` cell) and `button_rects` at
    `[[Rect::ZERO; 4]; 5]`.
  - `move_focus` derives bounds from `BUTTONS.len()` / `BUTTONS[0].len()` — good,
    but assumes cells *are* buttons (no spanning).
  - `LABEL_POS` / `position_of` build a reverse index from `BUTTONS` as a
    process-global `LazyLock`.
- `ui.rs::draw_buttons`
  - `[Constraint::Max(5); 5]`, `[Constraint::Length(7); 4]`, `.areas::<5>(area)`,
    `.areas::<4>(*row_area)`, `[[Rect::ZERO; 4]; 5]` — **all** carry the `5` and
    `4` as const generics.
  - `centered_panel(frame.area(), 28, 29)` and the `Length(4) / Length(25)`
    split size the panel from the 4-col × 5-row grid implicitly.
- `main.rs` indexes `BUTTONS[r][c]` in the mouse and Space paths.

`Layout::areas::<N>()` returns a fixed `[Rect; N]`; `N` must be known at compile
time. This task is fundamentally a refactor from fixed arrays to `Vec`/slice-based
layout, plus a cell-spanning model.

## Design

### Cells are not buttons: a spanning model

The old model — `BUTTONS[r][c]` is a button — breaks the moment a button can be
wider or taller than one cell. We separate two things:

- A **lattice**: an `R × C` grid of equal cells. Purely geometry: it gives the
  coordinate lines used to size and place everything.
- **Buttons**: each button owns a *rectangular region* of lattice cells,
  `(row, col, row_span, col_span)`. A normal key is `1×1`; a wide `0` is `1×2`;
  a tall `=` is `2×1`.

```rust
pub struct Keypad {
    pub name: &'static str,
    pub rows: u16,               // lattice dimensions
    pub cols: u16,
    buttons: Vec<Button>,        // compiled from the authored occupancy grid
    occupancy: Vec<Vec<usize>>,  // [row][col] -> index into `buttons`
}

struct Button {
    label: &'static str,
    row: u16,
    col: u16,
    row_span: u16,
    col_span: u16,
}
```

### Authoring vs. internal representation

Authoring a `Vec<Button>` with explicit coordinates by hand is verbose and easy
to get wrong (gaps, overlaps). Instead **author the pad as an occupancy grid** of
tokens and *compile* it into the struct above at startup:

```rust
// A button that repeats across adjacent cells IS a spanning button.
&[
    &["C", "(", ")", "÷"],
    &["7", "8", "9", "×"],
    &["4", "5", "6", "-"],
    &["1", "2", "3", "+"],
    &["0", "0", ".", "="],   // "0" spans two columns
]
```

The compile step:
1. Scans the grid; each distinct token becomes one `Button` whose region is the
   **bounding box** of the cells carrying that token.
2. **Validates** that every token's cells form a *filled rectangle* (no ragged or
   disjoint spans) and that every cell is covered exactly once. A malformed pad
   is a programming error caught at startup, not a runtime surprise.
3. Fills `occupancy[r][c]` with the owning button's index.

Trade-off: the token doubles as the button's identity, so **labels must be unique
within a pad** (true for a calculator). If that ever bites, the fallback is
explicit `{label, row, col, span}` placement — noted under Open Questions. The
occupancy grid is chosen because it reads like the actual keypad and, more
importantly, the `occupancy` map it produces makes focus *and* hit-testing fall
out for free (below).

### Layout algorithm: lattice + span-union

This replaces the const-generic `areas::<N>` with a runtime-sized `split`, and
adds spanning:

```rust
// One split per axis gives the coordinate lattice (runtime-sized).
let col_x = Layout::horizontal(vec![Constraint::Length(CELL_W); cols]).split(area);
let row_y = Layout::vertical(vec![Constraint::Max(CELL_H); rows]).split(area);

// Each button's rect is the bounding box of the lattice cells it spans:
for b in keypad.buttons() {
    let x = col_x[b.col].x;
    let y = row_y[b.row].y;
    let right = col_x[b.col + b.col_span - 1];   // last spanned column
    let bottom = row_y[b.row + b.row_span - 1];  // last spanned row
    let rect = Rect { x, y,
        width:  right.x  + right.width  - x,
        height: bottom.y + bottom.height - y };
    draw_button(frame, b.label, focused, pressed, rect);
}
```

Alternatives considered and rejected:
- **Nested splits** (split rows, then split each row into columns): handles wide
  buttons for free but *cannot* express a tall button — rows are cut first, so
  nothing straddles two of them.
- **Explicit absolute placement** of every rect: full freedom but discards
  Ratatui's responsiveness; you'd re-derive coordinates per terminal size.

The lattice keeps constraint-based responsiveness *and* supports 2-D spans.

### Focus and the reverse index

Keep **focus as a lattice cell `(row, col)`** — the smallest delta from today's
code — and resolve it to a button through `occupancy` wherever a button is
needed:

- `move_focus` still clamps a `(dr, dc)` step into the pad's `rows`/`cols`
  bounds. Moving across a wide `0` takes two steps (both its cells share the same
  button, so the highlight doesn't change on the first) — acceptable here. A
  nicer "one press per button" navigation is the separate **`focus-per-button`**
  task.
- `focused_label` / highlight / press-flash resolve `occupancy[focus] -> button`
  and act on the whole button region, so a spanning button highlights as one.
- **Reverse index** (`label -> cell`) can no longer be a single global
  `LazyLock` over `BUTTONS`. Make it a method/field on `Keypad`, built during the
  compile step (we're already walking every cell). `position_of` returns the
  button's *anchor* (top-left) cell.

### Hit-testing over a spanning grid

Hit-test against **one union rect per button**, not per-cell rects:

```rust
button_rects: Vec<Rect>,                        // one entry per button, its region
fn button_at(&self, col, row) -> Option<usize>  // returns a button index
```

rebuilt each draw (as now, via `set_button_rects`). The tempting alternative —
keep a per-cell `Vec<Vec<Rect>>` and resolve `occupancy[r][c] -> button` — works
*only* while the lattice tiles contiguously (`Length`/`Max` constraints, no
`spacing`, `Flex::Start`): then adjacent cells of one button abut and every pixel
is covered. But the moment a pad introduces `spacing` or a slack-distributing
`Flex`, the **seam between two cells of the same button becomes dead space** and
a click there misses. Storing per-cell rects bakes in that fragility.

The union rect avoids it by construction. Because the compile step **validated
every span as a filled rectangle**, a button's bounding box *is* its exact region
— no over-coverage, no internal seam. A click anywhere inside a spanning button
(seams included) hits it; the only misses are the deliberate gutters *between
different buttons*, which is the desired behavior.

`button_at` now returns a button index. Since focus stays lattice-cell-based, a
click resolves the button to its **anchor cell** (the same top-left cell the
label→cell reverse index yields) to set focus. Rendering already draws each
button once over this union rect, so rendering and hit-testing read the same
geometry.

### Panel sizing derived, not literal

`centered_panel` and the display/grid vertical split currently encode `28`/`29`/
`25` for a 4×5 grid. Derive them from the pad: `panel_width = cols * CELL_W +
frame`, `grid_height = rows * CELL_H`, so the panel is sized from the layout
rather than magic numbers. (This is also what lets the follow-up pads of a
different shape center correctly, for free.)

## Implementation Notes

- This refactor should land **before** `rainbow-mode` and `quick-input`: both
  touch `draw_buttons` and per-cell rendering, so doing the array→`Vec` +
  spanning move once avoids re-threading them.
- Keep a green-checkpoint discipline:
  1. **De-hardcode with no behavior change.** Introduce `Keypad`, compile the
     existing 5×4 grid (all `1×1` buttons) through it, replace the const-generic
     `areas::<N>` with slice `split`, change `button_rects` to `Vec<Rect>` (one
     per button), and move the reverse index onto `Keypad`. Tests stay green.
     *This is the pure mechanical refactor.*
  2. **Add spanning.** The `Button`/`occupancy` model, the compile+validate step,
     the span-union rects, and focus/hit-test resolving through `occupancy`.
     Prove it by making one key span (e.g. a tall `=` or wide `0`).
- The `move_focus` clamp test and `button_at` hit-test test in `ui_state.rs`
  already pin the invariants; extend them to a spanning pad.

## How to Test

Unit:
- **Compile/validate:** a well-formed occupancy grid compiles to the expected
  buttons and `occupancy`; a ragged or disjoint span is rejected (panics/errors
  at startup); every cell is covered exactly once.
- **Round-trip:** every label resolves to its anchor cell and back.
- `move_focus` clamps within the pad's bounds.
- **Spanning focus:** stepping onto a wide/tall button keeps the highlight on the
  one button across all its covered cells; `focused_label` returns that label.
- `button_at` resolves clicks to a button index, and a click anywhere on a
  spanning button's region — including an internal seam — resolves to that
  button; a click in a gutter between buttons resolves to `None`.

Manual:
1. `cargo run`; the standard pad renders as before (with whichever key is made to
   span rendered as one larger button); the panel centers.
2. Type an expression; the display/result is unaffected by the refactor.
3. Click a spanning button (wide `0` / tall `=`) anywhere on its area — hit-test
   lands on the right button. Click a `1×1` button — as before.

## Dependencies

- **app-ui-state** — `UiState` owns the grid, focus, and `button_rects` this task
  generalizes.
- **ui-buttons** — the `draw_buttons` rendering being made dynamic + spanning.
- **mouse-input** — `button_at` hit-testing over the (now `Vec`) rects.

## Follow-ups this unblocks

- **`layout-registry`** — a `Vec<Keypad>` registry, a manual switch key routed in
  `main.rs` (like copy/focus-moves, *not* an `Action`), and the active-index +
  override state. Each pad carries a `default_focus`; a switch clamps focus into
  the new pad's bounds, falling back to its `default_focus`. Pure additions on
  top of this task's model.
- **`layout-auto`** — pick the best-fit pad for the terminal shape on resize
  (per-pad shape hint / `fits(w, h)` score), with the manual override from
  `layout-registry` taking precedence. Depends on `layout-registry`.
- **`focus-per-button`** — step one button per key press instead of one lattice
  cell, so crossing a spanning button is a single press.

## Open Questions

- **Non-unique labels.** The occupancy-grid authoring uses the label as the
  button's identity, so labels must be unique within a pad. If a future pad needs
  a repeated label, switch that pad to explicit `{label, row, col, span}`
  placement (the internal model already supports it).
