# CLAUDE.md

This file provides guidance to Claude Code (claude.ai/code) when working with code in this repository.

## Project

TUI calculator built with Rust (edition 2024) and Ratatui 0.30 / Crossterm 0.29. Expression-based input (e.g. `78-65*5`) with a button grid UI similar to macOS Calculator.

## Commands

- `cargo run` — run the app
- `cargo build` — build
- `cargo test` — run all tests
- `cargo test <test_name>` — run a single test
- `cargo clippy` — lint
- `cargo fmt` — format

## Architecture

Modules under `src/`:

- **`main.rs`** — terminal setup/teardown and the event loop. `handle_event` resolves each key/mouse event to an `Action` (or a focus move) and dispatches it; `key_to_action` is the single keyboard→`Action` map. A left-click resolves to a button index via `ui.button_at`, then to its label via `ui.button_label`. `Event::Paste` feeds the pasted string to `App::apply_str`. Copy-to-clipboard (`y`/`Y` or a click on the display affordance → `do_copy` → `arboard`) and pad-switching (`Tab` → `ui.cycle_layout`) are routed *here*, not as an `Action`: they're side effects on the UI/result that change no calculator state, so they stay out of `App::apply`'s pure total match and out of `action.rs`. **Gotcha:** bracketed paste must be enabled in `setup_terminal` (`EnableBracketedPaste`) or `Event::Paste` never fires.
- **`action.rs`** — the typed input boundary. An `Action` enum plus a validated `Digit` newtype (private field, fallible `Digit::new`, so an out-of-range digit is unrepresentable). `from_key` (keyboard ASCII) and `from_label` (button-grid glyphs) resolve raw input into an `Action` *before* it reaches `App`, so illegal input is rejected at the edge instead of mishandled downstream. Pure domain logic — no crossterm dependency.
- **`app.rs`** — `App` holds the calculator state (`expr` tokens, in-progress `current` number, `mode`) and the logic. `App::apply(Action)` is the single input entry point: a total match over the enum, no catch-all. `App::apply_str(&str)` ingests a pasted string, routing each char through `Action::from_label` (the display-glyph boundary, so pasted `×`/`÷` round-trip; unmapped chars skipped) into `apply`.
- **`layout.rs`** — the button layout as *data*, not compiled-in dimensions. A `Keypad` is a lattice of equal cells plus `Button`s that each own a rectangular region (`row/col/row_span/col_span`), so a button can span cells (a wide `0`, a tall `=`). Pads are authored as an occupancy grid of label tokens (a token repeated across adjacent cells *is* a span) and `compile`d at startup, which **validates** the invariants (rectangular grid; each token fills its bounding box — no ragged/disjoint spans) and builds the `cell → button index` occupancy map. Panics on a malformed pad (static data → a programming error). Ships two pads — `Keypad::standard` (all `1×1`) and `Keypad::tall` (6×4, exercising real spans: wide `0`/`+`, tall `=`); each carries a `default_focus` resolved from a label at `compile` time (a missing label panics). Shape-based auto-select on resize is the remaining follow-up (`layout-auto`). Pure — no ratatui/crossterm dependency.
- **`ui_state.rs`** — `UiState`: a `Vec<Keypad>` registry + active index (all reads funnel through `keypad()`, so multiplying pads didn't re-open the model), button-grid focus (a lattice `(row, col)`), the momentary press flash, the per-button screen rects used for mouse hit-testing, and the copy affordance's rect + transient status message. `button_at` returns a *button index*, hit-testing each button's whole union rect (so a click anywhere on a spanning button — internal seams included — hits it). Focus/flash resolve to a button through the keypad's occupancy map (`is_button_focused` / `is_button_pressed`). `set_layout`/`cycle_layout` switch the active pad and fix up per-pad state via the free `resolve_focus` (preserve the old cell if in-bounds — snapped to its button's anchor — else the pad's `default_focus`), dropping the stale flash and resizing the hit-test rects. Deliberately separate from `App` (rendering/input-routing vs. calculator logic); `focus` is private, mutated only through bounds-safe methods.
- **`eval.rs`** — recursive-descent evaluator over `Token`s with operator precedence, parentheses, and unary minus.
- **`ui.rs`** — Ratatui rendering: the display box (reads `&App`) and the button grid (reads `&mut UiState`). `draw_buttons` splits the area into a runtime-sized coordinate lattice (`Layout::split` → `Rc<[Rect]>`, no const generics) and draws each button once over the bounding box of the cells it spans; the panel is sized from the active keypad's dims and the `CELL_W`/`CELL_H`/`DISPLAY_H` constants, not magic numbers.

Input is **expression-based** (digits, `+-*/`, parentheses); keyboard (HJKL/arrows navigate the grid), mouse clicks, and bracketed paste all funnel through the same `Action` boundary.

Key dependencies: `ratatui` (TUI rendering), `crossterm` (terminal input), `arboard` (system clipboard, `wayland-data-control` feature on for pure-Wayland support — used by `do_copy` in `main.rs`). **Clipboard gotcha:** on Linux the copied text is served by the live `Clipboard` instance, so `copy_to_clipboard` keeps one handle in a process-lifetime `thread_local` (`CLIPBOARD`) rather than building one per copy — a fresh-per-copy handle would let `set_text` report success while the paste silently fails. Even so, on Linux the text may not survive app exit without a clipboard manager (macOS/Windows persist it).
