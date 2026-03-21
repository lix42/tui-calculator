# TUI Calculator

A terminal-based calculator built with Rust and [Ratatui](https://ratatui.rs).

## Features

- Expression-based input (e.g. `78-65*5`)
- Keyboard input: digits, `.`, `+-*/`, `()`, `c` to clear
- HJKL / arrow keys to navigate buttons
- Mouse click support
- Button grid UI similar to macOS Calculator

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
