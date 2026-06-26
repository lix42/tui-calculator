# quick-input: Modifier-Held Quick Keyboard Input

## Requirement

Let the user enter values without moving their fingers off the home row: while a
**modifier is held**, a cluster of keyboard keys maps directly to on-screen
buttons — e.g. `h`→`4`, `j`→`5`, `k`→`6`, `l`→`-`. Show a small **tip** on each
mapped on-screen button so the mapping is discoverable.

## A terminal-reality caveat (read first)

Two facts about terminal input shape this design:

1. **No modifier-only event.** Terminals do not emit a "Alt went down" event on
   its own — a modifier is only ever reported *attached to a key event*. So an
   overlay that appears *purely while the modifier is held* (and disappears on
   release), before any key is pressed, is **not achievable in the TUI**. The web
   is only marginally better: Ratzilla's `on_key_event` surfaces **`keydown`
   only** (its backend registers `KEY_EVENT_TYPES = &["keydown"]` and its
   `KeyEvent` carries no press/release kind), so a held-only overlay there isn't
   free either — it would need **separate DOM `keyup` wiring** outside the
   `on_key_event` path to know when the modifier was released. Treat held-only as
   "web-only *and* extra wiring", not a backend freebie.
2. **`Ctrl` is a poor trigger in a terminal.** The control register is heavily
   overloaded: `Ctrl-H` arrives as Backspace, `Ctrl-J` as Enter, `Ctrl-C` is the
   app's quit, `Ctrl-L` is redraw by convention, and several `Ctrl`+letter combos
   are swallowed or remapped before the app sees them. The existing
   `handle_event` already *deliberately* gates navigation off when `Ctrl`/`Alt`
   is present (so `Ctrl-H` keeps its Backspace meaning). Reusing `Ctrl` for
   quick-input would collide head-on with that.

**Recommendation:** trigger on **`Alt`** (reported cleanly as `KeyModifiers::ALT`
and far less overloaded), and surface the mapping with an **always-on faint tip**
(or a toggled legend), not a held-only overlay. The user said "e.g. ctrl" — this
is the one decision worth confirming at implementation time; the rest of the
design is modifier-agnostic.

## Design

### A pure quick-map, resolved at the boundary

Define the mapping as data, keyboard char → grid label (the same alphabet
`Action::from_label` already speaks):

```rust
// e.g. in action.rs (pure, crossterm-free) or a small quickmap.rs
fn quick_map(ch: char) -> Option<&'static str> {
    Some(match ch {
        'h' => "4", 'j' => "5", 'k' => "6", 'l' => "-",
        // ...extend to a fuller home-row cluster
        _ => return None,
    })
}
```

In `main.rs`, the key handler checks the trigger modifier **before** the normal
routing: if `key.modifiers.contains(ALT)` and `quick_map(ch)` resolves, run the
mapped label through the existing `activate(app, ui, Action::from_label(label))`
funnel — so a quick-input keystroke gets focus-follow and the press flash for
free, exactly like a click or a normal key. This slots in next to the existing
`Ctrl-C` and nav-gate checks at the top of the `Event::Key` arm.

Keeping the map a pure `char → label` function (no crossterm types) means the web
port can reuse it verbatim against ratzilla's `KeyCode`.

### The on-cell tips

Render a small hint glyph in a corner of each mapped button (e.g. a dim
superscript letter `ʰ` / `ʲ` / `ᵏ` / `ˡ`, or a `[h]` in the cell's top-left).
`draw_button` gains an optional `tip: Option<char>`; `draw_buttons` looks up the
reverse of `quick_map` for each cell and passes the tip when quick-input display
is enabled. Because tips live in the cell render, they ride on top of whatever
`layout-config` produces.

**When to show them**, given caveat (1):
- **Recommended:** a faint, always-on tip (low-contrast so it doesn't clutter),
  or a `?`-toggled legend. Discoverable without needing a modifier-down event.
- The "show only while held" variant is web-build-only **and** needs explicit
  DOM `keyup` wiring (Ratzilla's `on_key_event` is keydown-only — see caveat 1),
  so it's a later add-on, not a freebie.

### Interaction with focus navigation

`h/j/k/l` **unmodified** already mean move-focus (vim nav). That's exactly why the
modifier gate matters: `Alt-h` = quick-input `4`, bare `h` = move focus left. The
existing nav gate (`!modifiers.intersects(CONTROL | ALT)`) already prevents
`Alt-h` from being read as navigation, so the two interpretations stay disjoint —
verify this holds and add a test.

## Implementation Notes

- This is mostly a `main.rs` routing change plus a `ui.rs` per-cell tip; the
  calculator core (`app.rs`, `eval.rs`) is untouched, and `action.rs` only gains
  a pure lookup table.
- Sequence **after** `layout-config` if possible: the tip overlay is part of
  per-cell rendering, which that task reshapes. The quick-map labels should be
  validated against the *active* layout (a mapped label that isn't on the current
  pad simply shows no tip and does nothing).
- Decide the full cluster, not just `h/j/k/l` → `4/5/6/-`. A natural extension is
  the home row → number row, but keep every target a real button label so
  `from_label` resolves it.

## How to Test

Unit:
- `quick_map` returns the documented labels and `None` for unmapped chars; every
  mapped label is a real button (`from_label(label).is_some()`).
- With the trigger modifier set, a mapped key drives the calculator (`Alt-j`
  applies `5`); without it, the same key still navigates (`h` moves focus left) —
  the two paths don't collide.
- The reverse lookup used for tips agrees with `quick_map`.

Manual:
1. `cargo run`; the mapped buttons show their tips (per the chosen display
   policy).
2. Hold `Alt` and tap `h j k l` — `4 5 6 -` are entered, each cell flashes, focus
   follows.
3. Without `Alt`, `h j k l` still navigate the grid.

## Dependencies

- **key-input** / **button-nav** — the `handle_event` routing and the nav gate
  this extends.
- **mouse-input** — shares the `activate` funnel the mapped keys reuse.
- **ui-buttons** — the per-cell rendering the tips attach to.
- **layout-config** (soft) — tips render per-cell; sequence after it, and
  validate mapped labels against the active pad.

## Open Questions

- **Trigger modifier**: `Alt` (recommended) vs `Ctrl` (requested as an example).
  Confirm before implementing — it changes which terminal chords are at risk.
- **Tip visibility policy**: always-on faint vs toggle-on legend. Held-only is a
  web-build-only option that additionally needs DOM `keyup` wiring (see caveat 1).
