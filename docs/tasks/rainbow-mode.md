# rainbow-mode: Per-Digit Rainbow Color Mode

## Requirement

Add a **rainbow color mode**: each digit `0`–`9` gets its own color, applied
consistently to **both** the on-screen button (focus highlight included) and the
digits as they appear in the display. Optionally layer in light **animation**
(e.g. a slow hue cycle or a shimmer on the focused cell). The mode is toggled at
runtime; the default monochrome look stays the default.

## Design

### A presentation-only feature — no `App` change

Color is a rendering concern, so this lives entirely on the `UiState` /
`ui.rs` side of the App/UiState split. `App` keeps returning plain strings from
`display_lines()`; the UI decides how to color them. Add a `ColorMode` to
`UiState`:

```rust
enum ColorMode { Mono, Rainbow }
```

toggled by a key routed in `main.rs` (like the layout switch — not an `Action`,
since it doesn't change calculator state).

### The digit→color map

One function, the single source of truth for both surfaces:

```rust
fn digit_color(d: u8) -> Color; // 0..=9 → a fixed hue
```

Use ten evenly-spaced hues (HSV→RGB, or a hand-picked palette). Operators,
parens, `=`, `C`, `⌫` stay neutral (a single accent color), so the rainbow reads
as "the numbers", not noise.

### Coloring the display per-character

`app::display_string` returns a flat `String` today, and the display is rendered
as a single right-aligned `Line`. To color individual digits the UI must build
the line from per-character `Span`s instead:

```rust
// in ui.rs, rainbow branch:
let spans = top.chars().map(|c| match c {
    '0'..='9' => Span::styled(c.to_string(), Style::new().fg(digit_color(...))),
    _ => Span::raw(c.to_string()),
});
Line::from(spans.collect::<Vec<_>>())
```

(Sketch — kept short for clarity.) In the real implementation, map each digit
char to a `&'static str` (e.g. index a `["0", …, "9"]` table by `c as usize -
'0' as usize`) so a `Span` borrows it instead of allocating a fresh `String` per
character via `to_string()` every frame; operators can keep `Span::raw` over a
borrowed slice too.

Keep this in `ui.rs`: `App` stays rendering-agnostic (it already hands back
strings; turning a string into colored spans is pure presentation). The mono
branch keeps the existing `Line::from(top)` path unchanged.

### Coloring the buttons

`button_styles(focused, pressed)` returns one of three `&'static ButtonStyle`
presets today. In rainbow mode a digit button's `text_style` (and optionally its
focused `border_style`) takes `digit_color(d)`. Because the presets are
`&'static`, a per-digit color can't be a static — `button_styles` will need to
return an **owned** `ButtonStyle` (or take the label and overlay a color) in
rainbow mode. Simplest: keep the three presets for structure (border type,
weight) and overlay `.fg(digit_color(d))` for digit cells when the mode is on.

### Animation (a follow-up, not part of the first static pass)

Static per-digit colors ship first (see Implementation Notes). Animation is a
**separate, later pass** — the design below is captured so it isn't re-derived.

#### Philosophy: transient effects, not an always-on phase

Continuous animation reads as *busy*. So the model is **event-driven and
quiescent by default**: nothing moves until the user does something, each effect
is time-limited and decays, and the terminal settles back to the static palette.
The one deliberate exception is a single, subtle **always-on breath** (on the
display or the focused cell).

This is the shape the press flash *already* has — `flash: Option<(cell)>` +
`flash_at: Instant`, triggered at `register_press`, expired in `tick` after
`FLASH_DURATION`. So the work is to **generalize the flash into a small effect
model** rather than bolt on N ad-hoc timers:

```rust
struct Effect { kind: EffectKind, origin: Origin, started: Instant, duration: Duration }
// origin is a cell (row, col) or a Dir, depending on kind.
```

Rendering derives a **per-cell intensity from `started.elapsed()`** and modulates
the static `digit_color` on top. The press flash becomes one `EffectKind`.

#### The effects and their triggers (all trigger data already exists)

| Effect | Trigger | Origin | Priority | Notes |
|---|---|---|---|---|
| **Ripple** | any cell-mapped input (`register_press`) | pressed cell | first | radiates outward; the flash, generalized |
| **Global hue drift** | successful `=` | none (global) | first | fire on `action == Equals && app.copy_text().is_some()`; settles back to the palette |
| **Breath (display)** | always on | display window | first | the *lone* always-on effect; keep amplitude small |
| **Copy / paste** | later | display area | first | different `kind`; not a grid ripple |
| **Directional wave** | focus move (`move_focus`) | `Dir` + destination cell | **deferred** | UX-uncertain — see below |
| **Breath (focused cell)** | always on | focused cell | **deferred** | UX-uncertain — see below |

**Sequencing — build the `first` effects, then trial the two deferred ones.**
The two focus-triggered effects are held back on purpose: focus moves are
frequent (every `hjkl`/arrow), so a **directional wave on every move** and a
**breath on the focused cell** are the most likely to feel busy or distracting.
They stay in the model (same `Effect` abstraction, no extra plumbing) but land
*after* the others are in and tuned, so they can be trialled against a working
baseline and dropped cheaply if they don't earn their place. The always-on
breath, if kept at all, starts on the **display window** — the calmer location —
before the focused-cell variant is even attempted.

**"No cell → no ripple" is free.** Ripple keys off `register_press`'s cell, and
paste bypasses `register_press` entirely (`handle_event` routes it to
`app.apply_str`) while copy fires from the display affordance. So paste and copy
produce no grid ripple *without any special-casing* — the existing input funnel
already draws that line. The `=` success peek at `App` is read-only (ui.rs
already reads `&App`), so it doesn't leak state into `App`.

#### Concurrency: latest-wins, with composing ripples as a stretch goal

Baseline is **latest-wins**: a new effect clears the current one, so user input
during a hue drift cancels the drift. **Stretch goal: ripples compose** — rapid
input leaves overlapping ripples instead of each cancelling the last.

Model the effect state as a **small bounded collection** so both are just
*insertion policies* on one container, not different data structures:
- a non-ripple effect (wave, drift) clears the collection and inserts one
  (latest-wins);
- a ripple **replaces** (baseline) or **appends up to a cap** (stretch).

The cap keeps held-down input from spawning unbounded ripples. Designing the
container this way up front makes the stretch a one-line policy flip.

#### Prerequisites and pacing

- Add an `animation_start: Instant` (the wall-clock phase the breath and drift
  read). The run loop already `draw`s every iteration with a 100 ms `poll` cap,
  so it repaints at ~10 fps *unconditionally* — transient effects cost nothing
  extra. The breath is what commits the loop to staying awake; if a genuinely
  quiet idle terminal ever matters, gate the redraw on "an effect is active or an
  event arrived," and the breath becomes the thing keeping it running.
- `digit_color` stays a **pure `(d) -> Color`**; effects modulate on top of it.
  Only the global drift ever needs a phase, and only while it's running.

> **Time source — already settled.** `ui_state.rs` imports `Instant` from
> `web-time`, not `std::time` (swapped 2026-07-31, ahead of both this pass and
> `web-ratzilla`, because `std`'s panics on `wasm32-unknown-unknown`). Use the
> module's existing import; there is no clock decision left to make here.

This design now lives in its own task — **`rainbow-animation`** — since this one
shipped as the static pass. See `docs/tasks/rainbow-animation.md`; the sections
above stay here as the record of what was designed alongside the static palette.

## Implementation Notes

- Best sequenced **after** `layout-config`, since both rework `draw_buttons` and
  per-cell styling; doing rainbow on top of the `Vec`-based grid avoids redoing
  the per-cell color threading.
- Start without animation (static per-digit colors on display + buttons), get it
  green, then add the animated hue offset as a second checkpoint — animation is
  the part most likely to need tuning and is easiest to bisect on its own.
- Watch contrast: some hues on the default terminal background are unreadable.
  Pick the palette against a dark background and avoid pure blue on black, etc.

## How to Test

Unit:
- `digit_color` is total over `0..=9` and distinct per digit (no two equal).
- Mode toggle flips `ColorMode` and is idempotent per press.
- (If the display path is refactored) a helper that maps a display string to
  spans tags exactly the digit characters and leaves operators neutral.

Manual:
- `cargo run`, toggle rainbow — digits in the grid and the display are colored;
  operators stay neutral; focus highlight still reads clearly over a colored
  digit.
- Type `1234567890` — ten distinct colors visible.
- With animation on, the hue cycle is smooth and doesn't pin the CPU (the 100 ms
  poll is the pacing budget).

## Dependencies

- **ui-buttons** — the button styling this extends.
- **app-display-split** — `display_lines()` supplies the strings the UI colors.
- **layout-config** (soft) — shares the `draw_buttons` render path; sequence
  after it.
