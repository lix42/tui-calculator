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

- **`main.rs`** — terminal setup/teardown and the event loop. `handle_event` resolves each key/mouse event to an `Action` (or a focus move) and dispatches it; `key_to_action` is the single keyboard→`Action` map. `Event::Paste` feeds the pasted string to `App::apply_str`. Copy-to-clipboard (`y`/`Y` or a click on the display affordance → `do_copy` → `arboard`) is routed *here*, not as an `Action`: it's a side effect on the result, so it stays out of `App::apply`'s pure total match and out of `action.rs`. **Gotcha:** bracketed paste must be enabled in `setup_terminal` (`EnableBracketedPaste`) or `Event::Paste` never fires.
- **`action.rs`** — the typed input boundary. An `Action` enum plus a validated `Digit` newtype (private field, fallible `Digit::new`, so an out-of-range digit is unrepresentable). `from_key` (keyboard ASCII) and `from_label` (button-grid glyphs) resolve raw input into an `Action` *before* it reaches `App`, so illegal input is rejected at the edge instead of mishandled downstream. Pure domain logic — no crossterm dependency.
- **`app.rs`** — `App` holds the calculator state (`expr` tokens, in-progress `current` number, `mode`) and the logic. `App::apply(Action)` is the single input entry point: a total match over the enum, no catch-all. `App::apply_str(&str)` ingests a pasted string, routing each char through `Action::from_label` (the display-glyph boundary, so pasted `×`/`÷` round-trip; unmapped chars skipped) into `apply`.
- **`ui_state.rs`** — `UiState`: button-grid focus, the momentary press flash, the on-screen cell geometry used for mouse hit-testing, and the copy affordance's rect + transient status message. Deliberately separate from `App` (rendering/input-routing vs. calculator logic); `focus` is private, mutated only through bounds-safe methods.
- **`eval.rs`** — recursive-descent evaluator over `Token`s with operator precedence, parentheses, and unary minus.
- **`ui.rs`** — Ratatui rendering: the display box (reads `&App`) and the button grid (reads `&mut UiState`).

Input is **expression-based** (digits, `+-*/`, parentheses); keyboard (HJKL/arrows navigate the grid), mouse clicks, and bracketed paste all funnel through the same `Action` boundary.

Key dependencies: `ratatui` (TUI rendering), `crossterm` (terminal input), `arboard` (system clipboard — used by `do_copy` in `main.rs`). **Clipboard gotcha:** the copy is a one-shot `set_text`; on Linux/X11 clipboard contents are tied to process lifetime, so a copy may not survive app exit without a clipboard manager (macOS/Windows persist it).
