# layout-auto: Shape-Aware Automatic Pad Selection

## Requirement

Pick the keypad that best fits the current terminal **shape** automatically — a
tall pad for a narrow-tall terminal, a wide pad for a wide-short one — and
re-pick on resize. A user who manually switches pads (via `layout-registry`)
should **override** the automatic choice, and that override should stick until
cleared.

This is the layer that delivers the original "flexible layout for different
screen shapes" goal. It sits on top of `layout-registry` (it needs multiple pads
and the active-pad plumbing to choose among).

## Design

### Each pad advertises what it suits

```rust
struct Keypad {
    // ...+
    fits: fn(width: u16, height: u16) -> i32, // higher score = better fit
}
```

A pad scores itself against the terminal dimensions (e.g. reward matching aspect
ratio and enough room for its `cols * CELL_W` × `rows * CELL_H`; penalize a pad
that would overflow). The selector takes the highest score:

```rust
fn select_for(w: u16, h: u16) -> usize {
    LAYOUTS.iter().enumerate()
        .max_by_key(|(_, pad)| pad.fits(w, h))
        .map(|(i, _)| i).unwrap()
}
```

### Auto vs. sticky manual override

Model the choice as an override on top of the auto pick:

```rust
struct UiState {
    // ...+
    override_layout: Option<usize>, // Some => user pinned a pad; None => follow auto
}
```

- **Auto:** on launch and on every resize, if `override_layout` is `None`, set
  `layout = select_for(w, h)` and re-clamp focus (reusing `layout-registry`'s
  switch path).
- **Override:** the manual switch key from `layout-registry` sets
  `override_layout = Some(i)`; resize then leaves the pad pinned. Provide a way to
  clear it (e.g. a dedicated key, or cycling back past the last pad) to resume
  auto.

Resize is observed in `main.rs`'s event loop (crossterm `Event::Resize`), routed
at the I/O boundary like the other non-`Action` UI concerns.

## Implementation Notes

- Land after `layout-registry`; depends on its registry + switch path.
- Green checkpoints: (1) add `fits` + `select_for` and wire auto-select on
  `Event::Resize` with `override_layout` always `None` (pure auto); (2) make the
  manual switch set the override and add a clear path.
- Keep `fits` scores simple and total-ordered so `select_for` is deterministic; a
  tie should resolve to a stable default (e.g. the standard pad).

## How to Test

Unit:
- `select_for(w, h)` returns the shape-appropriate pad for representative
  narrow-tall and wide-short dimensions; ties resolve to the documented default.
- A resize with `override_layout == None` changes the active pad; with
  `override_layout == Some(i)` it stays pinned.
- Clearing the override resumes auto-selection on the next resize.

Manual:
1. `cargo run`; resize narrow-tall then wide-short — the pad shape follows.
2. Press the switch key, then resize — the pad *stays* put (override honored).
3. Clear the override — the next resize snaps back to the best-fit pad.

## Dependencies

- **layout-registry** — the `Vec<Keypad>` registry, active-index, and switch/
  clamp path this task drives automatically.
- **layout-config** — the underlying `Keypad` model (transitively).

## Open Questions

- **Override persistence.** Should a pinned pad survive restart? Same
  `config-persist` question raised by `layout-registry`; out of scope here.
- **Fit heuristic tuning.** The exact `fits` scoring (aspect weight vs. absolute
  room) is best tuned against real terminals once the pads exist; start simple.
