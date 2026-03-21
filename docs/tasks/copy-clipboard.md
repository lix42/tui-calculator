# copy-clipboard: Copy Result to System Clipboard

## Requirement

After evaluation, provide a way to copy the result to the system clipboard. A "Copy" button appears, auto-focused. Pressing Space/Enter/click copies the result. The button is dismissed when new input begins.

## Design

From CLAUDE.md:
> After evaluation, a "Copy" button appears (auto-focused). Space/Enter/click copies result to system clipboard. Button dismissed on new input.

This modifies the UI and app state:
- New state: `show_copy_button: bool`
- After `evaluate()` succeeds, set `show_copy_button = true`
- The Copy button renders near/over the display area
- On activation: use `arboard` crate to copy `result` string to clipboard
- On any new digit/operator input: set `show_copy_button = false`

## Implementation Suggestion

- Add `show_copy_button: bool` to `App`
- After successful evaluation, set it to `true`
- In `press_button()`, when any digit/operator is pressed, set it to `false`
- Add a `copy_to_clipboard()` method that uses `arboard::Clipboard::new()` and `set_text()`
- In UI: when `show_copy_button` is true, render a "Copy" button in the display area (e.g., bottom-right of the display box)
- In event handler: when copy button is visible and Space/Enter is pressed, call `copy_to_clipboard()`
- Handle `arboard` errors gracefully (e.g., show "Copy failed" briefly)

## How to Test

Manual verification:
1. Type `2+3`, press `=` — result shows `5`, Copy button appears
2. Press Space/Enter — result is copied (paste somewhere to verify)
3. Type a new digit — Copy button disappears
4. Evaluate again, click the Copy button — result copied

## Dependencies

- **app-state** — state management for the copy button
- **ui-display** — rendering the copy button overlay
- **key-input** or **button-nav** — handling activation of the copy button
