# button-nav: Button Navigation with HJKL/Arrows

## Requirement

Allow navigating the button grid using HJKL (vim-style) or arrow keys, and activating the focused button with Space or Enter.

## Design

Key mappings (from design doc):
- `H` or `Left` → move focus left
- `J` or `Down` → move focus down
- `K` or `Up` → move focus up
- `L` or `Right` → move focus right
- `Space` or `Enter` → activate focused button (calls `press_button`)

Focus wraps or clamps at grid boundaries (clamping is simpler and recommended).

Note: `Enter` has dual purpose — it both activates the focused button AND evaluates directly. Since activating `=` also evaluates, this is consistent.

## Implementation Suggestion

- In the event handler, match arrow keys and HJKL to `app.move_focus(dr, dc)`
- `Space` → `app.press_button(app.focused_label())`
- Need to be careful with key conflict: `Enter` should evaluate (same as pressing `=`), which is consistent with activating any focused button since `=` button triggers evaluate
- Consider: `Enter` always evaluates (direct keyboard behavior takes precedence), `Space` activates whatever button is focused

## How to Test

Manual verification:
1. `cargo run` — see a highlighted button
2. Press arrow keys or HJKL — focus moves visibly
3. Focus stops at grid edges (doesn't wrap or crash)
4. Navigate to `5`, press Space — `5` appends to expression
5. Navigate to `=`, press Space — expression evaluates

## Dependencies

- **app-state** — `move_focus()`, `focused_label()`, `press_button()`
- **ui-buttons** — visual feedback of focus movement
- **tui-skeleton** — event loop
