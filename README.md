# TUI Calculator

A terminal-based calculator built with Rust and [Ratatui](https://ratatui.rs).

## Features

- Expression-based input (e.g. `78-65*5`)
- Keyboard input: digits, `.`, `+-*/`, `()`, `c` to clear
- HJKL / arrow keys to navigate buttons
- Mouse click support, and paste a whole expression at once
- Button grid UI similar to macOS Calculator, in three shapes that adapt to the
  terminal's proportions
- Per-digit rainbow coloring, with dark and light palettes
- **Quick input**: a home-row numpad mode for typing without leaving the home row
- **Copy result to clipboard**: after evaluating, a `[y Copy]` hint appears in the
  display. Press `y` or click it to copy the result. It disappears when new input
  begins.

## Usage

```sh
cargo run
```

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
| `r`   | Toggle per-digit rainbow coloring                             |
| `t`   | Toggle the dark/light palette                                 |

Three keypads ship — a 5×4 standard pad, a tall-narrow one, and a wide-short one.
Resizing the terminal picks whichever best fits its shape, unless `Tab` has pinned
one.

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
