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

## Why: the stringly-typed `press_button(&str)` weakness

Concrete motivation surfaced during `button-nav`. `App::press_button` takes a
`&str` label and ends in a catch-all:

```rust
_ => self.push_digit(label), // digits and "."
```

This trusts the caller completely. `press_button("a")` does not error — it falls
into `push_digit`, which `current.push_str("a")`s, so the display shows `a`
until the next operator/`=` silently drops it (`"a".parse::<f64>()` fails). Any
unmatched string (`"foo"`, `""`, …) is treated as a number-in-progress; the
catch-all doesn't even confirm it's a digit.

It is **not a live bug**: every current caller pre-validates. Labels come only
from `key_char_to_label` (a fixed allowlist), the `BUTTONS` grid (valid by
construction), and — added in `button-nav` — `App::register_press(&str)`, which
is fed the same validated labels. But the invariant "the `&str` is a real
button label" is enforced by discipline at each call site, not by the type.

`button-nav` widened the surface: `register_press(&str)` is a second
`&str`-typed entry point with the same implicit contract. Both should migrate.

**Fix (this task):** make illegal states unrepresentable. Resolve input to an
`Action` enum at the edge (`KeyCode → Action`, `"5" → Action`, `click →
Action`), then have `press_button(Action)` match an enum with no `_` arm —
total over its input, validation done once. Sketch:

```rust
enum Action {
    Digit(Digit),       // validated 0..=9 — see note below
    Dot,
    Op(char),           // '+', '-', '*', '/'
    LParen, RParen,
    Clear, Backspace, Equals,
}
```

This subsumes the existing `key_char_to_label` mapping and the `&str` labels
threaded through `press_button` / `register_press`.

### Enforcing the digit invariant — why `Digit(u8)` alone won't do it

Rust enum **variants inherit the enum's visibility**: the fields of a `pub
enum`'s variants are always public, and Rust does not allow a visibility
modifier on a variant field. So `Action::Digit(u8)` cannot have a "private
constructor" — anyone could still write `Action::Digit(42)` and break the
`0..=9` invariant. (Structs are the asymmetry here: a struct *can* have private
fields.)

The fix is a validated newtype with a private field and a fallible constructor,
kept in its own module so the constructor is the only construction path:

```rust
mod action {
    pub struct Digit(u8); // field private to this module

    impl Digit {
        pub fn new(n: u8) -> Option<Digit> {
            (n <= 9).then_some(Digit(n)) // or impl TryFrom<u8>
        }
        pub fn get(self) -> u8 { self.0 }
    }

    pub enum Action {
        Digit(Digit),
        // …
    }
}
```

`Action::Digit(Digit)` stays public, but the only way to obtain a `Digit` is
`Digit::new` (which rejects `10..=255`), so an out-of-range digit is
unconstructable by type. The module boundary is the enforcement — within
`action` you could still call `Digit(42)`, so the type must live in its own
small module whose only blessed path is `new` / `try_from`.

## Dependency

Should be done after **tui-skeleton** and **key-input** exist, so the natural
home for `UiState` is clear. Do not split prematurely — wait until the UI layer
has enough shape to receive it.
