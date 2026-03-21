# High-Level Design

## Overview

A TUI calculator that evaluates mathematical expressions, inspired by the macOS Calculator app. Built with Rust (2024 edition) and Ratatui.

## Architecture

```
src/
  main.rs        -- Entry point, terminal setup, event loop
  app.rs         -- Application state and logic
  ui.rs          -- Rendering (display + button grid)
  eval.rs        -- Expression parser and evaluator
```

### Module Responsibilities

**main.rs** - Terminal init/restore, main loop (draw + handle events).

**app.rs** - Core state machine:
- `expression: String` -- the current input expression (e.g. "78-65*5")
- `result: Option<String>` -- evaluated result (e.g. "-247")
- `focus: (row, col)` -- currently focused button in the grid
- Methods: `press_button()`, `evaluate()`, `clear()`, `backspace()`

**ui.rs** - Renders two areas:
1. **Display box** -- two right-aligned lines:
   - Top line: the expression (smaller/dimmer when result is shown)
   - Bottom line: the result or current input
2. **Button grid** -- 5 rows x 4 columns of buttons with focus highlight

**eval.rs** - Recursive descent parser for arithmetic expressions:
- Supports: `+`, `-`, `*`, `/`, `(`, `)`, decimal numbers, unary minus
- Operator precedence: `* /` before `+ -`
- Returns `Result<f64, String>` for error display

## UI Layout

```
┌──────────────────────────┐
│              78-65×5     │  <- expression (right-aligned)
│                -247      │  <- result (right-aligned, bold)
├──────────────────────────┤
│  C    (    )    ÷        │  row 0
│  7    8    9    ×        │  row 1
│  4    5    6    -        │  row 2
│  1    2    3    +        │  row 3
│  ⌫    0    .    =        │  row 4
└──────────────────────────┘
```

## Input Handling

Three input methods, all mapped to the same actions:

### 1. Direct keyboard
- Digits `0-9`, `.` -> append to expression
- `+`, `-`, `*`, `/` -> append operator
- `(`, `)` -> append parenthesis
- `c` -> clear
- `Backspace` -> delete last char
- `Enter` or `=` -> evaluate
- `q` or `Esc` -> quit

### 2. Button navigation (keyboard)
- `H/Left`, `J/Down`, `K/Up`, `L/Right` -> move focus
- `Space` or `Enter` -> activate focused button

### 3. Mouse
- Click on a button -> activate it
- Each button occupies a known `Rect`; hit-test on click coordinates

## State Transitions

```
[Idle] --digit/op--> [Editing expression]
  |                        |
  |                   Enter/=
  |                        |
  |                   [Show result]
  |                        |
  |                   digit/op -> [Editing: result cleared, start new]
  |
  +--- c ------------> [Clear -> Idle]
```

After evaluation:
- Pressing a digit starts a new expression (clears old)
- Pressing an operator appends to the result (continues calculation)

## Expression Evaluator

Recursive descent grammar:

```
expr   = term (('+' | '-') term)*
term   = factor (('*' | '/') factor)*
factor = '-' factor | '(' expr ')' | number
number = [0-9]+ ('.' [0-9]+)?
```

Division by zero returns an error string displayed in the result area.

## Dependencies

- `ratatui 0.30` -- TUI framework
- `crossterm 0.29` -- terminal backend, keyboard/mouse events
