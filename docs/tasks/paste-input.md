# paste-input: Paste an Expression Directly

## Requirement

Let the user paste a whole expression (e.g. `78-65*5` or `(1+2)*3`) into the
calculator and have it loaded as input in one step, instead of relying on the
characters arriving as individual keystrokes.

## Why the current behavior is not enough

Without bracketed paste enabled, a terminal paste is delivered as a *burst of
individual `Char` key events* — as if typed very fast. Each one flows through
`handle_event` → `key_char_to_label` → `activate`, so pasting `1+2=` happens to
mostly work. But it is fragile, and `button-nav` made it worse:

1. **Spaces break it.** `button-nav` bound `KeyCode::Char(' ')` to "activate the
   focused button." A pasted `1 + 2` sends real space chars, each re-pressing
   whatever is focused → garbage instead of `1+2`.
2. **Focus strobes + flashes.** Every pasted char goes through `activate` →
   `register_press`, so focus jumps cell-to-cell and each flashes. Visually
   chaotic for a paste.
3. **Newlines evaluate mid-stream.** A trailing newline sends `Enter` →
   `evaluate`; a multi-line paste evaluates partway through.
4. **Unknown chars vanish silently** (letters, `,`, `$`) — `key_char_to_label`
   returns `None` and they are swallowed with no feedback.

## Design: bracketed paste

Enable [bracketed paste](https://en.wikipedia.org/wiki/Bracketed-paste) so the
terminal wraps pasted text in escape markers (CSI `?2004h` … `?2004l`) and
crossterm delivers it as a single `Event::Paste(String)` to parse deliberately,
rather than replaying it as fake keystrokes.

API verified against crossterm 0.29 docs (Context7 `/crossterm-rs/crossterm`):

- `Event::Paste(String)` — "Only emitted if bracketed paste has been enabled."
- `EnableBracketedPaste` writes CSI `?2004h`; pair with `DisableBracketedPaste`.
- **Both are gated behind the `bracketed-paste` cargo feature** and
  `#[cfg(feature = "bracketed-paste")]`. We currently build crossterm with only
  `["event-stream"]`, so `Event::Paste` is not even compiled in yet.

## Implementation Suggestion

1. **Cargo.toml** — add the feature:
   `crossterm = { version = "0.29", features = ["event-stream", "bracketed-paste"] }`
2. **`main.rs` setup/teardown** — `EnableBracketedPaste` alongside
   `EnableMouseCapture` in `setup_terminal`; `DisableBracketedPaste` in
   `restore_terminal` **and** in the panic hook (same discipline as mouse
   capture: undo it before leaving the alternate screen).
3. **`handle_event`** — add an `Event::Paste(text)` arm that funnels the string
   through one well-defined path. Call `press_button` directly, **not**
   `activate`, so a paste does not strobe focus or fire a flash per char:

   ```rust
   Event::Paste(text) => {
       for ch in text.chars() {
           if ch.is_whitespace() {
               continue; // strip spaces/tabs/newlines
           }
           if let Some(label) = key_char_to_label(ch) {
               app.press_button(label); // reuse the keyboard mapping
           }
           // unmapped chars silently skipped (see decision below)
       }
   }
   ```

   Reuses `key_char_to_label`, so keyboard input and paste share one definition
   of what a valid character is.

## Open Design Decisions

- **Newlines** — *ignore* (load the expression; user presses `=`) vs *treat as
  `=`* (auto-evaluate a pasted `1+2\n`). Leaning **ignore**: pasting should not
  trigger evaluation; whitespace-strip is least surprising.
- **Unknown chars** — *skip silently* (lenient: `$1,000` → `1000`) vs *reject the
  whole paste with an error* (strict). Skip is friendlier but can quietly mangle
  input; reject is safer but noisier.

## Interaction With Other Tasks

- Independent of `button-nav` (focus navigation) — deliberately split out: this
  touches `Cargo.toml`, terminal setup/teardown, and the event loop, a different
  concern from focus.
- Overlaps `app-ui-state`: once input is resolved to an `Action` enum at the
  edge, the paste loop should produce `Action`s too, not `&str` labels — sharing
  the same parse-once boundary as keyboard/mouse.

## How to Test

Manual verification:
1. `cargo run`, then paste `78-65*5` — the full expression appears in the
   display (no strobing focus), ready to evaluate.
2. Paste `(1 + 2) * 3` — spaces are stripped; expression reads `(1+2)×3`.
3. Press `=` — evaluates correctly.

## Dependencies

- **tui-skeleton** — terminal setup/teardown and the event loop.
- **key-input** — reuses `key_char_to_label`.
