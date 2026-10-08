# web-msg: Backend-Neutral `Key` → `Msg` Mapper in Core

**Done:** 2026-10-07. Lives in `crates/core/src/input.rs`; see progress.md.

> Sub-task of [web-ratzilla](web-ratzilla.md) (split 2026-10-06). Realizes the
> deferred "Unified `Msg` enum" note in `progress.md` → Known Issues / Deferred.

## Goal

Move the *decision* of what each key does out of `main.rs`'s crossterm-typed
`handle_event` into a pure, backend-agnostic mapper in `calculator-core`, so the
native and web entry points share one definition and only translate their own
event type into a neutral `Key`.

## Design

```rust
// calculator-core
pub enum KeyCode { Char(char), Enter, Backspace, Esc, Tab, Left, Right, Up, Down, /* … */ }
pub struct Key { pub code: KeyCode, pub ctrl: bool, pub alt: bool }

pub enum Msg {
    Apply(Action),        // through `activate` (focus-follow, flash, fever, grace)
    MoveFocus(Dir),       // NOT (i32, i32) — the stale doc predates `Dir`
    ActivateFocused,      // Space
    SetQuickMode(bool),   // `i` enters, Esc leaves
    CycleLayout, ResumeAuto, ToggleTheme,
    Copy, Quit,           // effects — handled by each entry point, see below
}

pub fn key_to_msg(key: Key, quick_mode: bool) -> Option<Msg>;
pub fn apply_msg(app: &mut App, ui: &mut UiState, msg: Msg);
```

- **`key_to_msg` is not a pure `Key → Msg` table**: quick-mode reassigns `hjkl`/…
  to digits, so it takes `quick_mode` as an input. It must preserve every rule in
  today's `handle_event`: Ctrl-C quits before bare `c` clears; the quick block
  runs before the nav gate and repeats the `!(ctrl|alt)` guard (else Ctrl-U types
  `4`); nav *letters* go inert in quick-mode but arrows still navigate; Esc never
  quits.
- **`activate` moves into core** with `apply_msg` and stays the single input
  funnel (apply → `register_press` → `register_press_fever`; on a successful `=`,
  drift + `register_grace` *after* `register_press`).
- **`Copy` / `Quit` are effects, not core state changes.** The clipboard is
  per-platform (arboard vs `navigator.clipboard`) and quitting means nothing on
  the web, so each entry point matches these two itself; `apply_msg` treats them
  as no-ops (or the entry intercepts them before calling it). This is why no
  `cfg`-gated clipboard module is needed — the old plan's section 2 is superseded.
- Mouse, resize and paste stay entry-point wiring (they already call
  backend-agnostic `UiState`/`App` methods: `button_at`, `copy_hit`,
  `auto_select`, `apply_str`).

## Implementation Suggestion

- Lix writes `key_to_msg` (the quick-mode / nav / modifier precedence — ~15 lines
  of their own design); scaffold the types and tests around it.
- Port the `main.rs` key tests to core as pure `key_to_msg` tests first, so they
  pin the behavior before the native code is rewired.
- Native `handle_event` shrinks to: crossterm `KeyEvent` (Press only) → `Key` →
  `key_to_msg` → `Copy`/`Quit` locally, else `apply_msg`.

## How to Verify

- Every key test that lived in `main.rs` exists in core against `key_to_msg`
  (quick-mode, Ctrl/Alt chords, Esc-never-quits, nav-letter inertness).
- `cargo test --workspace` green; native app behaves identically (manual pass
  over the README key tables).

## Dependencies

- [web-core-split](web-core-split.md)
