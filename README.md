# TUI Calculator

A terminal-based calculator built with Rust and [Ratatui](https://ratatui.rs).

## Features

- Expression-based input (e.g. `78-65*5`)
- Keyboard input: digits, `.`, `+-*/`, `()`, `c` to clear
- HJKL / arrow keys to navigate buttons
- Mouse click support
- Button grid UI similar to macOS Calculator
- **Copy result to clipboard**: after evaluating an expression, a "Copy" button appears (focused by default). Press `Space`/`Enter` or click it to copy the result to the system clipboard. The button disappears when new input begins.

## Usage

```sh
cargo run
```

### Controls

| Key             | Action              |
|-----------------|---------------------|
| `0-9`, `.`      | Input digits        |
| `+-*/`          | Operators           |
| `(`, `)`        | Parentheses         |
| `=` or `Enter`  | Evaluate            |
| `c`             | Clear               |
| `Backspace`     | Delete last char    |
| Arrow keys/HJKL | Move button focus   |
| `Space`/`Enter` | Press focused button|
| Mouse click     | Press button        |
| `q` or `Esc`    | Quit                |

### After Evaluation

When a result is displayed, a **Copy** button appears and receives focus automatically.

| Key             | Action                        |
|-----------------|-------------------------------|
| `Space`/`Enter` | Copy result to clipboard      |
| Any digit/op    | Dismiss Copy, start new input |
| `c`             | Clear result and Copy button  |
