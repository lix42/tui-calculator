# rainbow-animation: Event-Driven Effect Model for Rainbow Mode

## Goal

Add the animation pass that `rainbow-mode` deliberately deferred: transient,
event-driven color effects layered on top of the shipped static palette. The
terminal stays visually quiet by default — effects fire on user input, decay, and
settle back to the static colors.

`rainbow-mode` shipped the static per-glyph palette (`ui::glyph_color`) and closed
as done. This task is the second half of that design.

## Design

**The design is already written** — see the "Animation (a follow-up, not part of
the first static pass)" section of [rainbow-mode](rainbow-mode.md) (the philosophy,
the `Effect` model, the effect/trigger table, and the concurrency policy). It is
not duplicated here; that section is the spec. The summary:

- **Generalize the press flash into a small effect model.** `UiState` already has
  `flash: Option<…>` + `flash_at: Instant` expired by `tick`. Rather than adding N
  ad-hoc timers, widen it to `Effect { kind, origin, started, duration }` and make
  the press flash one `EffectKind`.
- **Rendering derives per-cell intensity from `started.elapsed()`** and modulates
  the static `glyph_color` on top. `glyph_color` stays a pure `(glyph) -> Color`;
  effects never rewrite the palette, only modulate it.
- **Ship the `first` effects, then trial the two `deferred` ones.** Ripple (on
  `register_press`), global hue drift (on a successful `=`), and the display
  breath land first. The directional wave and focused-cell breath are held back
  because focus moves are frequent enough that both risk reading as busy — they
  use the same `Effect` abstraction, so trialling them later costs no plumbing.
- **Latest-wins concurrency**, with composing ripples as a stretch goal. Model the
  effect state as one bounded collection so the stretch is an insertion-policy
  flip, not a data-structure change.

### What has changed since that design was written

- **The clock is settled.** `ui_state.rs` imports `Instant` from `web-time`
  (swapped 2026-07-31). The `animation_start` field this task adds uses the
  module's existing import — there is no `std` vs `web-time` decision left, and no
  coordination with `web-ratzilla` required.
- **`ColorMode` defaults to `Rainbow`**, and `Theme` (Dark|Light) landed alongside
  it. Effects must be legible on **both** themes — the light theme is where a
  modulated hue is most likely to wash out. `ui.rs`'s `loud`/`knockout` pair
  documents the same trap for highlights.
- **The pad is no longer a fixed 5×4.** Effects with a spatial origin (the ripple)
  must derive distance from the *active* keypad's lattice, and a spanning button
  (wide `0`, tall `=`) occupies several cells — decide whether a ripple measures
  from the pressed cell or the button's anchor.

## Implementation Suggestion

- Land it in two checkpoints, as `rainbow-mode` itself did: the effect model with
  the press flash re-expressed through it (**no visible behavior change, all 136
  tests still green**) — then the new effects on top. That makes a regression in
  the refactor bisectable away from a tuning problem in the effects.
- The run loop already redraws every iteration with a 100 ms `poll` cap, so it
  repaints at ~10 fps unconditionally — transient effects cost nothing extra. The
  always-on breath is what commits the loop to staying awake; if a genuinely idle
  terminal ever matters, that's the point to gate redraws on "an effect is active
  or an event arrived."
- "No cell → no ripple" is free: paste routes through `app.apply_str` and copy
  fires from the display affordance, so neither reaches `register_press`. The
  existing input funnel already draws the line without special-casing.

## How to Verify

- `cargo test` stays green throughout; `cargo clippy` and `cargo fmt` clean.
- **Checkpoint 1** — the press flash, re-expressed as an `EffectKind`, keeps its
  existing `ui_state` tests passing unchanged (`register_press_moves_focus_and_flashes`,
  `tick_keeps_fresh_flash`, `switch_clears_stale_flash`).
- **Effect expiry is unit-testable without rendering**: construct an `Effect` with
  a known `started`, assert intensity at t=0, mid-decay, and past `duration`.
- **Concurrency policy is unit-testable**: assert a non-ripple effect clears the
  collection, and that ripples respect the cap.
- **Manual**: run `cargo run`, hold a digit key (ripples must stay bounded, not
  spawn unboundedly), press `=` on a valid expression (drift fires and settles),
  toggle `t` mid-effect (legible on both themes), toggle `r` to `Mono` (effects
  must not leak color into mono mode).

## Open Questions

- **`Effect` shape.** `rainbow-mode.md` sketches `origin` as "a cell `(row, col)`
  or a `Dir`, depending on kind" — which is a per-variant payload, so it likely
  wants to live *in* `EffectKind` rather than as a sibling field that's meaningless
  for half the variants. Worth settling before writing it.
- **Does `Mono` suppress effects entirely, or keep intensity-only (no hue) ones?**
  A press ripple has a plausible monochrome reading; the hue drift does not.
- **Ripple origin on a spanning button** — pressed cell, or the button's anchor?

## Dependencies

None outstanding — [rainbow-mode](rainbow-mode.md) (the static palette this
modulates) and [layout-config](layout-config.md) (the `Vec`-based grid the ripple
measures over) are both done. Independent of
[web-ratzilla](web-ratzilla.md): the `web-time` swap that once linked them has
already landed, so the two can proceed in either order or in parallel.
