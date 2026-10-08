# TUI Calculator

A terminal-based calculator built with Rust and [Ratatui](https://ratatui.rs).

## Features

- Expression-based input (e.g. `78-65*5`)
- Keyboard input: digits, `.`, `+-*/`, `()`, `c` to clear
- HJKL / arrow keys to navigate buttons
- Mouse click support, and paste a whole expression at once
- Button grid UI similar to macOS Calculator, in three shapes that adapt to the
  terminal's proportions
- **Fever mode**: typing drives a visual ladder — plain → colored highlights →
  animation → full rainbow, with the meter living in the display's bottom border
- **Quick input**: a home-row numpad mode for typing without leaving the home row
- **Copy result to clipboard**: after evaluating, a `[y Copy]` hint appears in the
  display. Press `y` or click it to copy the result. It disappears when new input
  begins.

## Usage

```sh
cargo run
```

### In the browser

**Try it: <https://lix42.github.io/tui-calculator/>**

The same calculator runs in a browser tab, built with
[Ratzilla](https://github.com/ratatui/ratzilla) and [Trunk](https://trunkrs.dev).
Every push to `main` redeploys it. To run it locally:

```sh
rustup target add wasm32-unknown-unknown
cargo install trunk
cd crates/web && trunk serve   # then open http://127.0.0.1:8080
```

Every key below works the same, except:

- `q` does nothing: a tab isn't quit, it's closed.
- `Ctrl` and `Cmd` chords are left to the browser, so `Cmd-C` / `Ctrl-C` copy
  the selection and `Ctrl-−` zooms instead of reaching the calculator. Use `y`
  to copy the result.
- The palette starts dark or light to match the OS setting; `t` still toggles it.
- Pasting an expression isn't supported yet.

### Controls

| Key             | Action               |
|-----------------|----------------------|
| `0-9`, `.`      | Input digits         |
| `+-*/`          | Operators            |
| `(`, `)`        | Parentheses          |
| `=` or `Enter`  | Evaluate             |
| `c`             | Clear                |
| `Backspace`     | Delete last char     |
| Arrow keys/HJKL | Move button focus    |
| `Space`         | Press focused button |
| Mouse click     | Press button         |
| Paste           | Enter a whole expression at once |
| `q` or `Ctrl-C` | Quit                 |

`Esc` is deliberately **not** a quit key — it leaves quick input (below) and is
otherwise inert, so that double-tapping it can never discard an expression.

### Display and layout

| Key   | Action                                                       |
|-------|--------------------------------------------------------------|
| `y`   | Copy the result to the clipboard (only after evaluating)      |
| `Tab` | Switch to the next keypad, pinning it against resizes         |
| `a`   | Un-pin and resume automatic shape-based keypad selection      |
| `t`   | Toggle the dark/light palette                                 |

Three keypads ship — a 5×4 standard pad, a tall-narrow one, and a wide-short one.
Resizing the terminal picks whichever best fits its shape, unless `Tab` has pinned
one.

### Fever mode

Typing drives a four-stage visual ladder. The meter *is* the display box's
bottom border — each stage drives it 0 → 100 % of width in that stage's color,
then resets with a new color overlaying on top. The score runs on a `0..4`
scale, one unit per stage.

| Stage | Score    | Buttons / display                                           | Meter        |
|------:|:---------|:------------------------------------------------------------|:-------------|
|   1   | 0 → 1    | Plain. No palette color, no decorative animation.           | Dim gray     |
|   2   | 1 → 2    | Mono highlights come alive on focus and press.              | Medium gray  |
|   3   | 2 → 3    | Press ripple + display breath animations unlock.            | Bright gray  |
|   4   | 3 → 4    | Full per-digit rainbow + the hue drift on successful `=`.   | Drifting hue |

At score `0` the bottom border is simply not drawn — the display's bottom
edge is only visible once the meter has started filling. Once a stage
completes, its color stays on the left while the next stage's color fills
from the right; a `<` marker sits at the layer boundary so you can see where
the current fill head is without having to tell two grays apart by eye.

- **Climb:** `+0.15` per press (one press = 15 % of a stage, so climbing a
  stage from zero takes about 7 presses net of decay).
- **Decay:** `-0.05`/second of idle time. One stage drains in 20 seconds, the
  full ladder in 80.
- **Reading grace:** after a successful `=`, decay freezes for 2.5 seconds —
  a short pause to look at the result doesn't cost altitude.
- **Hysteresis:** `±0.08` around each stage threshold, so a score hovering at
  a boundary doesn't flap between stages.

There is no manual rainbow toggle. The way to see rainbow is to type.

### Quick input

Press `i` to enter quick input, `Esc` to leave. While it is on, the right hand
becomes a numpad in place — `u i o` sit directly under `7 8 9` on the keyboard, and
`j k l` under those — and each mapped button shows its key in its border.

| Key       | Enters      |
|-----------|-------------|
| `u` `i` `o` | `4` `5` `6` |
| `j` `k` `l` | `1` `2` `3` |
| `m`         | `0`         |
| `a` `s` `d` `f` | `+` `-` `×` `÷` |
| `[` `]`     | `(` `)`     |

The digit row and `.` need no mapping — they already type themselves. While quick
input is on, `hjkl` stop moving focus (the arrow keys still do), and every
unmapped key keeps its normal meaning.

### After evaluation

When a result is displayed, a `[y Copy]` hint appears at the top of the display.

| Key / action       | Effect                              |
|--------------------|-------------------------------------|
| `y` or click the hint | Copy the result to the clipboard |
| Any digit/op       | Dismiss the hint, start new input   |
| `c`                | Clear the result and the hint       |
