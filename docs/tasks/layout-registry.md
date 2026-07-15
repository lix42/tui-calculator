# layout-registry: Multiple Named Pads + Manual Switch

## Requirement

`layout-config` ships a single keypad routed through the `Keypad` model. This
task makes the app hold **several** named pads and lets the user **switch between
them by key**. It is a pure addition on top of `layout-config`'s data model — it
does not re-open `Keypad` / `Button` / `occupancy`.

Auto-selecting a pad by terminal shape is a *further* layer (`layout-auto`); this
task only provides the registry and a manual trigger it can later drive.

## Design

### A registry and an active pad

```rust
static LAYOUTS: &[Keypad] = &[STANDARD, TALL, WIDE]; // compiled at startup

pub struct UiState {
    // ...existing fields...
    layout: usize,               // index into LAYOUTS: the active pad
    // (layout-auto adds an `override: Option<usize>`; not needed yet)
}
```

`UiState` gains `cycle_layout()` / `set_layout(i)` and exposes the active
`&Keypad` for rendering, focus, and hit-testing (everything already reads the pad
through one accessor after `layout-config`).

### Per-pad default focus, clamp on switch

Each pad names its own "home" cell, so switching lands somewhere sensible:

```rust
struct Keypad { /* ...+ */ default_focus: (u16, u16) }
```

`layout-config` keeps focus as a lattice cell. On a switch, the old `(row, col)`
may be out of the new pad's bounds (a `(4,3)` focus is invalid on a 3×3 pad), so
**clamp into the new bounds, then snap to the button there**; if the old cell is
out of range, fall back to the new pad's `default_focus`. The reverse index and
`button_rects` are already per-pad, so they refresh with the active pad for free.

### The trigger — routed in `main.rs`, not an `Action`

Switching transforms no calculator state, so — like copy and focus-moves — it is
routed at the I/O boundary in `main.rs`, **not** through `App::apply` / the
`Action` enum. Bind a key (e.g. `Tab`) to `ui.cycle_layout()`. `App`
(expr/current/mode) is untouched, consistent with the App/UiState split.

### Panel re-centering

`layout-config` already derives panel size from the active pad's dims, so a pad
of a different shape re-centers with no extra work; this task just exercises that
path at runtime when the active pad changes.

## Implementation Notes

- Land after `layout-config`; depends on its `Keypad` model.
- Green checkpoints: (1) add the registry + `default_focus` + `set_layout`/
  `cycle_layout` with the trigger, still defaulting to the standard pad (no
  behavior change until the key is pressed); (2) add a second and third pad of
  different shapes to prove switching + re-centering.

## How to Test

Unit:
- `cycle_layout` advances and wraps around `LAYOUTS`.
- Switching from a `(4,3)` focus on a 5×4 pad to a smaller pad clamps focus into
  range (and falls back to `default_focus` when the old cell is gone).
- Each pad's reverse index and `button_at` resolve against that pad's shape.

Manual:
1. `cargo run`; press the switch key — the grid visibly changes shape, the panel
   re-centers, a sensible cell is focused.
2. Type an expression, switch mid-expression — `expr`/result unaffected.
3. Click a button on the newly active pad — hit-test lands on the right button.

## Dependencies

- **layout-config** — the `Keypad` / `Button` / `occupancy` model and the
  single-accessor active-pad plumbing this task multiplies.

## Open Questions

- **Layout persistence.** Should the chosen pad survive restart (a config file)?
  Out of scope; note for a later `config-persist` task.
