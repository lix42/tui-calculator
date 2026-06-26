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

### Animation (optional layer)

"May apply more animation" — e.g. cycle the hue offset over time, or pulse the
focused cell's brightness. This needs a **wall-clock phase**. `UiState::tick`
already runs once per loop iteration and `flash_at: Instant` already exists, so
add an `animation_start: Instant` and derive a phase from `elapsed()`. The run
loop polls at 100 ms, which paces a smooth-enough cycle; bump the poll rate only
if the animation looks choppy.

> **Cross-cutting note:** the time source here is the same gap `web-ratzilla`
> hits — `std::time::Instant` **panics on `wasm32-unknown-unknown`**. If rainbow
> animation lands before the web port, prefer the `web-time` crate's `Instant`
> (a drop-in) from the start so the web build doesn't have to retrofit it. See
> `web-ratzilla.md`.

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
