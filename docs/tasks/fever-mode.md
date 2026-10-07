# fever-mode: Typing-Driven Four-Stage Visual Ladder

> **Shipped shape.** The score runs on `0..=FEVER_MAX` (= 4.0), one unit per
> stage — the pre-implementation draft below assumed a `0..=1` meter with
> stages spanning 0.25 each. Thresholds landed at `1 / 2 / 3`, hysteresis at
> `±0.08`, and the meter geometry is **per-stage** (each stage drives the
> fill 0 → 100 % in its own color, previous stage's color visible on the
> left as a base layer, `<` marker at the boundary). Stages 1-3 use three
> grays (dim / medium / bright); stage 4 is a slowly-rotating hue. See the
> "Meter rendering" section further down for the final design, and the
> fever-mode entry in `docs/progress.md` for the implementation walk-through.

## Goal

Turn the user's typing pace into a visual reward: as they type, a meter climbs
through four discrete stages, each enabling more of the UI's visual repertoire.
Idle time decays the meter back down. Fever **replaces** the manual `r` toggle —
the only path to the rainbow is to type.

The four stages, each one unit of the `0..=4` meter (originally drafted as
0.25 of a `0..=1` meter — rescaled during implementation; see the shipped-shape
note above):

1. **plain** — no digit coloring, no mono highlight accent, no decorative
   animation. The press flash still fires (it's input confirmation, not
   decoration).
2. **colored highlights** — focus and press use the per-key palette color
   (today's mono look).
3. **colored highlights + animation** — adds the press ripple and the always-on
   display breath.
4. **rainbow + animation** — per-digit rainbow hues on display and grid, plus
   the hue drift on successful `=`.

## Decided — read these before the design

The design cycle (see the conversation that prompted this doc) settled a few
things worth keeping explicit:

- **Fever replaces `r`.** No manual mono/rainbow toggle. `t` (theme) stays — it
  is orthogonal to fever stages and acts in both.
- **4 stages, not 3.** Stage 1 ("plain-plain") and stage 2 ("colored, no
  animation") are *new* states — stage 2 is today's mono, stage 1 is new. Each
  stage transition is a different axis (color appears → animation appears →
  rainbow replaces mono), so the ladder is three independent switches, not one
  dial.
- **Snap, don't blend between stages.** Within a stage the meter grows smoothly;
  crossing a threshold snaps the visual state. Blending would be ambiguous
  because the stages differ in *kind*, not degree.
- **Press flash stays always on.** It is input confirmation, not decoration.
  Only ripple / breath / drift gate on `animated()`.
- **Silent transitions.** No one-off flash on stage-up or stage-down; the
  meter's color/length change *is* the signal. If a celebration tells itself
  later, it's a one-shot ripple fired from the meter — a cheap retrofit.

## Design

### Mechanic (rate math)

A score in `0..=FEVER_MAX`, where **one unit is one stage** (not a 0..1
progress bar across four bands). The 0..4 shape is what the constants read
against — a reader sees "+0.15 per press, −0.05 per second, thresholds at
1 / 2 / 3" and immediately knows one press moves ~15 % of a stage.

| Constant        | Value         | What it is                                 |
|-----------------|---------------|--------------------------------------------|
| `FEVER_MAX`     | `4.0`         | Top of the meter (one unit per stage)      |
| `FEVER_CLIMB`   | `0.15`/press  | How much a press adds                      |
| `FEVER_DECAY`   | `0.05`/sec    | How fast the score falls when idle         |
| `FEVER_GRACE`   | `2.5 s`       | Decay-pause after a successful `=`         |
| `FEVER_HYSTERESIS` | `0.08`     | Band around each threshold to avoid flaps  |
| Thresholds      | 1.0 / 2.0 / 3.0 | Stage boundaries (hysteresis around each) |

Consequences:

- Steady-state typing rate (climb balances decay) is `0.05 / 0.15 ≈ 0.33`
  presses/sec, i.e. one every ~3 s — the maintenance pace.
- 10 presses in 15 s is `+1.5` from presses, `−0.75` from decay → net `+0.75`,
  which is three-quarters of one stage. A burst gets you most of the way to
  the next stage without skipping over any.
- One stage drains in 20 s of idle time; the full ladder in 80 s.
- These are tunable later — all are `const` in `ui_state.rs`; the pure helpers
  mean no clock is involved in tests of either.

### Score representation vs. renderer

The renderer never sees the raw `0..=4` score; `UiState::fever_fill_fraction()`
exposes **sub-stage progress** on `0..=1` — `(score − lower_threshold_of_stage)
.clamp(0, 1)`, not an overall `score / FEVER_MAX` normalization. Each stage
drives the fill 0 → 100 % of width in its own color, then resets with the
previous stage's color persisting as a base layer. The 0..=4 shape stays an
implementation detail of `ui_state.rs`, and the renderer's geometry code
reads "width × fraction" without any mental scaling.

### Reading grace on `=`

A successful `=` (the existing `app.copy_text().is_some()` gate — `Mode::Evaluated`,
same guard the hue drift uses) sets `reading_grace_until = Instant::now() + 2.5s`.
While that time has not arrived, decay is suppressed; a short reading pause after
hitting `=` doesn't cost altitude.

A syntax error or an `=` on an empty expression leaves the grace untouched — it
is a reward for a result, not for pressing `=`.

### Hysteresis

Each threshold has a `±FEVER_HYSTERESIS` band. Once in stage N, you stay there
until the score has clearly crossed the boundary — crossing `0.25` up to `0.27`
promotes, crossing back down to `0.23` demotes. Right at `0.25` nothing flips.
This is a pure function of `(current_stage, score)` and ships with its own unit
tests.

### Lazy time

The meter is advanced in two places:

- `UiState::tick()` (called once per draw iteration) applies decay from
  `fever_last_tick` to `Instant::now()`, respecting any active grace window, and
  recomputes the cached stage.
- `register_press_fever` (called from `activate` after `register_press`) applies
  decay, bumps the score by `FEVER_CLIMB`, clamps to `1.0`, and recomputes.

All other reads (`stage()`, `score()`) just return the cached values. Tests
drive the pure math (`score_after_decay`, `next_stage`) directly without a clock.

### Effect gating

Fever adds no new `EffectKind`s. Instead, the renderer reads `ui.stage()` and
decides what to show. The data model stays unchanged (press flash and ripple
still both fire on every press; drift still fires on every successful `=`), and
only *rendering* branches on stage:

| Visual element     | Gate                                   |
|--------------------|-----------------------------------------|
| Press chip-flash   | always rendered                         |
| Colored highlights (focus/press) on buttons | `stage.colored_highlights()` (≥ 2) |
| Rainbow digit hues (display + button text) | `stage.rainbow()` (= 4) |
| Press ripple       | `stage.animated()` (≥ 3)                |
| Display breath     | `stage.animated()` (≥ 3)                |
| Hue drift          | gated via `ColorMode` (Mono at stages 1-3 → `palette_for` returns resting palette) |

Rainbow drift is "free-gated" by `color_mode()` returning `Mono` at stages 1-3:
`palette_for` already drops the drift for Mono. No separate gate needed.

### Stage 1 (plain) focus/press

At stage 1 the current `button_style` can't be used — it draws from the palette
regardless. A new `plain_style(focused, pressed)` returns a color-free style:

- Focused: `BorderType::Thick` (shape distinction, no color)
- Pressed: `Modifier::REVERSED` on the block (visible flash, no color)
- Resting: `REGULAR_STYLE` unchanged

This keeps the press flash readable in stage 1 without reintroducing color.

### Meter rendering

The meter *is* the display box's bottom border — zero extra layout footprint.
Geometry is **per-stage**: each stage drives the fill 0 → 100 % of width in
its own color, then the next stage overlays from the right while the previous
stage's color stays on the left as a base layer. (The first pass used an
overall `score / FEVER_MAX` fill; it collapsed four stages into one bar, and
the stage 1 color was impossible to distinguish from the resting foreground
across the first 25 % of the bar. The per-stage geometry gives each stage its
whole width of visual real estate.)

- **Three regions, drawn right-to-left**:
  - **Current fill** (`round(fever_fill_fraction * area.width)` cells on the
    right): the current stage's color.
  - **Marker** (one cell at the leftmost of the current fill, when
    `0 < fill < width`): a `<` glyph in the current stage's color. The three
    stage grays are deliberately close in lightness; the marker is what lets
    the eye find the current fill head without reading subtle differences.
  - **Base layer** (the remaining cells on the left): the previous stage's
    color (stages 2-4), or the border characters *erased with a space*
    (stage 1 — "below stage 1" is literally nothing, so the display's bottom
    edge disappears until the meter has climbed into it).
- **Per-stage color**:
  - Stage 1: dim gray (HSLuv `L≈50` on Dark, `L≈55` on Light; `S=0`).
  - Stage 2: medium gray (`L≈70` / `L≈32`).
  - Stage 3: bright gray (`L≈92` / `L≈10`).
  - Stage 4: a slowly-rotating hue (period 20 s), driven off
    `fever_meter_hue_phase()` (parallel to `breath_phase()`).
- **Why grays for 1-3?** Stage 4 is the only place a hue appears — the ladder
  *earns* color. An earlier design had stage 3 as a warm orange hue; it fought
  stage 4's drifting rainbow for color space whenever the drift rotated near
  orange. Three grays sidestep that whole conflict.

`fever_fill_fraction` returns the sub-stage progress, not overall. At stage
Three with score 2.5, fraction is `0.5` and the right half of the border is
stage-Three gray; the left half is stage-Two gray (the previous completed
layer); stage One's gray is covered by stage Two's and is invisible.

Implementation: render the display block as today, then draw the three
regions directly to the buffer via
`frame.buffer_mut()[(x, y)].set_fg(…)` / `.set_symbol(…)`. The corner
characters are included in the fill — the whole bottom edge reads as one
meter when the fraction is non-zero.

### `ColorMode` after fever

The `ColorMode` enum stays (it is still the renderer's internal view of "is this
rainbow?"), but it is now **derived** from stage, not stored:

```rust
pub fn color_mode(&self) -> ColorMode {
    if self.stage() == FeverStage::Four { ColorMode::Rainbow } else { ColorMode::Mono }
}
```

The `color_mode` field is removed. `toggle_color_mode` is removed. Tests that
drove rainbow via that toggle climb the meter instead.

## Implementation Suggestion

Three commits, in order:

1. **Design doc + TASKS entry** (this file + a `[ ] fever-mode` line). Shipped
   first so the design is reviewable in isolation.
2. **Fever core.** `FeverStage` enum + pure helpers (`score_after_decay`,
   `next_stage`) + the four new `UiState` fields + methods
   (`register_press_fever`, `register_grace`, `stage`, `score`), with
   `tick`/`activate` wiring. Remove `r`/`R` from `main.rs`; drive
   `color_mode()` from stage. Delete the two tests that drove the removed
   toggle; update `drift_is_rainbow_only_but_the_theme_still_applies` to reach
   mono via "stage < 4" instead of a toggle. Build + 136+ tests green.
3. **Renderer gating + meter visual + docs.** `plain_style` for stage 1;
   gate ripple / breath / drift behind `stage.animated()`/`rainbow()`; render
   the meter on the display's bottom border. Update README (keys table drops
   `r`; add a Fever section). Update CLAUDE.md's architecture description.
   TestBackend coverage for stage-1 rendering and meter fill.

### Keeping tests clock-free

The design is deliberately shaped so no test needs a clock:

- `score_after_decay(prev, elapsed_secs)` is a pure `f64` fn.
- `next_stage(current, score)` is a pure fn, hysteresis included.
- Reading grace is tested via `apply_decay(prev, elapsed, grace_remaining)`
  (the pure form of the lazy getter) at chosen phases.

## How to Verify

- `cargo test` stays green throughout; `cargo clippy` and `cargo fmt` clean.
- Pure helpers covered: climbing promotes exactly at the upper hysteresis edge,
  falling demotes exactly at the lower edge, flapping at the boundary is a
  no-op.
- Reading grace: with grace active, `apply_decay(prev, 1.0, grace_remaining=1.0)`
  returns the original score (no decay); grace that expires mid-interval decays
  only the post-grace portion.
- Meter render: at `score=0.5` on a `28×29` buffer, half the display's bottom
  row carries the meter color; the rest stays default foreground.
- Stage 1 render: no palette color appears on any button text or border; focus
  shows via `BorderType::Thick`; pressed shows via `REVERSED`.
- Stage gating: ripples don't fire at stages 1-2 (no fever is actually fired,
  just not *rendered*; `effects()` still contains the ripple); drift renders
  only at stage 4; breath only at stage ≥ 3.
- **Manual**: run `cargo run`, press 10 digits in ~5s and watch the meter climb
  through all four stages; stop typing and watch it decay back; hit `=` on a
  valid expression and verify the 2.5s reading grace freezes the meter before
  decay resumes.

## Open Questions

- **Stage 4 meter: slow hue rotation vs. drifting during actual `=` sweeps?**
  Decided: slow continuous rotation (period ~20 s). An actual `=`-sweep is a
  display-wide palette rotation; the meter is small enough that one more
  concurrent cycle wouldn't read against it, so the simpler "meter has its own
  slow clock" wins.
- **Mono accent color choice at stage 2.** `loud(theme)` is the plan; the
  shipped mono's focus-outline already uses the per-key palette color via
  `button_style`. The meter color at stage 2 (`loud`) is distinct from the
  button highlight at stage 2 (per-key hue outline). That is intentional — the
  meter is a per-stage *signal*, the button highlight is per-key. If this reads
  wrong, swap the meter stage-2 color to the warm hue family stage 3 uses.

## Dependencies

None outstanding. Builds on `rainbow-animation` (effect model + ripple + drift
+ breath), `layout-config` (the per-stage render decisions already consult the
palette), and the `web-time` swap (`Instant` is available). Independent of
`web-ratzilla` — fever is pure UI, no clipboard or event-loop implications.
