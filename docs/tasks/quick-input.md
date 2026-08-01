# quick-input: Modifier-Held Quick Keyboard Input

## Requirement

Let the user enter values without moving their fingers off the home row: while a
**modifier is held**, a cluster of keyboard keys maps directly to on-screen
buttons — e.g. `h`→`4`, `j`→`5`, `k`→`6`, `l`→`-`. Show a small **tip** on each
mapped on-screen button so the mapping is discoverable.

## Decided, and shipped — read before the design below

The design below was written *before* implementation and proposed an **`Alt`-held**
trigger with an `h/j/k/l` → `4/5/6/-` map. **Both were superseded.** What shipped:

1. **A sticky mode, not a held modifier.** `i` enters, `Esc` leaves. `Alt` was
   rejected because default macOS Terminal.app composes `Option`+`h` into the dead
   key `˙` and delivers *no* `ALT` modifier at all, so an `Alt`-triggered feature
   would silently do nothing on the primary dev machine and depend on a per-user
   terminal setting elsewhere. A mode also has explicit on/off transitions, which
   dissolves caveat (1) below: the tips can show exactly while it's active, which is
   the "only while held" UX the caveat says a TUI cannot deliver.
2. **A spatially faithful numpad, not four keys.** `u i o` / `j k l` / `m` →
   `456` / `123` / `0`, because on QWERTY those keys sit directly beneath `7 8 9`.
   Plus `a s d f` → `+ - × ÷` and `[` `]` → `(` `)` (parens without Shift). The
   digit row and `.` are deliberately unmapped — they already type themselves.
3. **`Esc` is no longer a quit key anywhere.** Entering on `i` invites the vim
   reflex of double-tapping `Esc`; the second tap would otherwise have quit and
   discarded the expression. `q` and `Ctrl-C` remain the only exits.
4. **Tips render in each button's top border**, via `Block::title`, and only while
   the mode is on — so they double as the mode indicator. The cell interior is one
   column wide after padding, so there was no room beside the glyph.

Treat the sections below as the original analysis, not as the current contract.

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

Unit (all shipped and passing):
- `quick_map` returns the documented labels and `None` for unmapped chars; every
  mapped label is a real button (`from_label(label).is_some()`) **and exists on all
  three pads**, so a key can't resolve to a button the active pad lacks.
- `quick_key` is the exact inverse, and `QUICK_MAP` has no duplicate key or label
  (both lookups take the first match, so a duplicate would be silently shadowed).
- In-mode, `j k l` type `1 2 3` and `i` types `5` rather than re-entering the mode;
  out of mode the same keys still navigate — the two readings don't collide.
- Ctrl/Alt chords are ignored in-mode (`Ctrl-U` must not type `4`), matching the
  navigation block's existing modifier gate.
- `Esc` never quits, and double-tapping it leaves the expression intact.
- The tip renders **in the border row** of its button and not over the glyph
  (a `TestBackend` render assertion), and no tips appear when the mode is off.

Manual:
1. `cargo run`, then press `i`; the mapped buttons show their key in the border.
2. Tap `u i o j k l m` — `4 5 6 1 2 3 0` are entered, each cell flashes, focus
   follows.
3. `h` does nothing; the arrow keys still navigate. `Esc` leaves the mode, and the
   tips disappear.

## Dependencies

- **key-input** / **button-nav** — the `handle_event` routing and the nav gate
  this extends.
- **mouse-input** — shares the `activate` funnel the mapped keys reuse.
- **ui-buttons** — the per-cell rendering the tips attach to.
- **layout-config** (soft) — tips render per-cell; sequence after it, and
  validate mapped labels against the active pad.

## Open Questions — all resolved

- **Trigger modifier**: resolved to *neither*. A sticky mode (`i` / `Esc`) replaced
  the modifier entirely; see the decision record at the top for why `Alt` failed on
  macOS and `Ctrl` was never viable.
- **Tip visibility policy**: resolved to **on only while the mode is on**, which the
  mode made possible without the DOM `keyup` wiring caveat (1) says a held-only
  overlay would need. They serve as the mode indicator too.
- **The cluster**: resolved to the full numpad-in-place map, not the four keys this
  document originally specified.

One caveat survives for the web port: the quick map is a pure `char → label` table
(`QUICK_MAP` in `action.rs`, no crossterm types), so ratzilla can reuse it verbatim —
but the *mode* replaces the `keydown`-only problem rather than inheriting it, since
nothing depends on observing a modifier's release.
