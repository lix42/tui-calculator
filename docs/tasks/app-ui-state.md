# app-ui-state: Extract UI State from App

## Background

`App` currently holds both application state (expression, result) and UI state
(focus, BUTTONS grid, move_focus, focused_label). These have different
lifecycles and concerns: UI state is about rendering and input routing; app
state is about calculator logic.

Discussed during app-state implementation. Keeping them together for now to
avoid premature splitting, but they should be separated before the UI layer
grows.

## Goal

Move UI state into its own struct, likely in a new `src/ui_state.rs`:

```rust
pub struct UiState {
    pub focus: (usize, usize),
}

impl UiState {
    pub fn new() -> Self { ... }
    pub fn move_focus(&mut self, dr: i32, dc: i32) { ... }
    pub fn focused_label(&self) -> &str { ... }
}
```

`BUTTONS` moves to `ui_state.rs` (or a shared `src/buttons.rs`) since it is
layout data, not calculator logic.

`App` is left with only: `expression`, `result`, `should_quit`.

## Input Boundary

Once UI state is separate, the keyboard/mouse handlers own a `UiState` and
resolve input events to `Action` values before passing them to `App`. The `App`
never sees focus or grid layout.

## Dependency

Should be done after **tui-skeleton** and **key-input** exist, so the natural
home for `UiState` is clear. Do not split prematurely — wait until the UI layer
has enough shape to receive it.
