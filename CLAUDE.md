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

Currently a single `src/main.rs` stub. The README describes the target design:

- **Input**: expression-based (digits, `+-*/`, parentheses), keyboard (HJKL/arrows for button navigation), and mouse click support
- **UI**: Ratatui-based TUI with a button grid layout
- **Evaluation**: parse and evaluate arithmetic expressions with operator precedence

Key dependencies: `ratatui` for terminal UI rendering, `crossterm` (with `event-stream` feature) for terminal input handling.
