# tui-skeleton: Terminal Setup and Event Loop

## Requirement

Implement `src/main.rs` with terminal initialization, the main event loop, and clean terminal restoration. This is the minimal shell that ties the app together — it should compile and run (showing a blank or minimal screen) even before the UI rendering tasks are done.

## Design

Standard Ratatui + Crossterm pattern:
1. Enable raw mode, enter alternate screen, enable mouse capture
2. Create `Terminal<CrosstermBackend<Stdout>>`
3. Loop: `terminal.draw(|f| ui::draw(f, &app))`, then poll for events
4. On quit: disable mouse capture, leave alternate screen, disable raw mode

The event loop should be synchronous (blocking `crossterm::event::read()`).

## Implementation Suggestion

- `main()` calls `setup_terminal()`, runs the loop, then `restore_terminal()`
- Use `crossterm::event::poll(Duration::from_millis(100))` + `read()`
- Pass events to a handler function (can be a stub initially, just handling `q`/`Esc` to quit)
- Wrap the loop in a `Result` and use `restore_terminal()` in both success and panic paths (use `std::panic::set_hook` or catch_unwind)
- Declare `mod app; mod ui; mod eval;` — create stub files for `ui.rs` and `app.rs` if they don't exist yet

## How to Test

Manual verification:
1. `cargo run` — app launches, shows blank/minimal screen
2. Press `q` or `Esc` — app exits cleanly, terminal is restored properly
3. No raw-mode artifacts left in the shell after exit

Also verify: `cargo build` succeeds with no warnings.

## Dependencies

- **app-state** — creates and owns the `App` instance
- **Note**: `ui::draw` and event handling can be stubs initially
