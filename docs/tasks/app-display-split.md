# app-display-split: Tokenized Expression, Separate from Display

## Background

After evaluation, pressing an operator should continue the calculation from the
result — without losing precision. The current implementation reuses the
*formatted result string* as the left operand (`app.rs:44–56`), which truncates
to 10 digits and corrupts the value.

Example of the bug:
```
1 → / → 3 → = → *  → 3 → =
```
Current: result is formatted to `"0.3333333333"`, so the next step computes
`0.3333333333 * 3 ≈ 0.9999999999`.
Correct: `1`.

The root cause is round-tripping a value through a *truncated display string*.
Verified: the full-precision `f64` round-trips exactly (`(1.0/3.0)*3.0 == 1.0`),
while the 10-digit string does not (`0.9999999999`).

## Solution Overview

Stop storing the expression as a `String`. Store it as a **vector of tokens**,
keep the in-progress number as a small **text buffer**, and keep the post-`=`
display as a separate **result** field.

```rust
enum Token {
    Number(f64),   // a committed value (typed or computed)
    Op(char),      // '+', '-', '*', '/'
    LParen,        // '('
    RParen,        // ')'
}

enum Mode {
    Editing,        // building an expression
    Evaluated,      // post-`=`: expr == [Number(value)]; Copy enabled
    Error(String),  // last eval failed; holds the message to show
}

pub struct App {
    pub expr: Vec<Token>,   // committed tokens (internal truth)
    pub current: String,    // in-progress number being typed, e.g. "1.50"
    pub mode: Mode,         // editing vs post-`=` (gates Copy / ⌫ / fresh digit)
    pub focus: (usize, usize),
    pub should_quit: bool,
}
```

### Why `Mode` and not a cached result string

The earlier sketch used `result: Option<String>`. That field did two jobs; only
one is worth storing:

- **The displayed result string is *not* stored** — it is a pure function of the
  value, and after `=` the value already lives in `expr` as `Number(value)`. The
  display layer regenerates it with `format_number` on demand. Caching it is
  redundant state that can drift.
- **The post-`=` mode *is* stored**, explicitly. It drives Copy / ⌫-clears /
  digit-starts-fresh, and it is not safe to infer. (`expr == [Number(_)] &&
  current.is_empty()` happens to be a unique post-eval signature today, but that
  leans on a brittle global invariant and cannot represent an **error** result,
  which is not an `f64` and so cannot sit in `expr`.) `Mode::Error` gives that
  message a home.

Why tokens instead of one `String` (the originally-proposed approach of adding a
`display` field and wrapping the expression in parens on every `=`):

- Wrapping in parens grows the expression without bound:
  `1/3 = *3 = /7 =` → `((1/3)*3)/7` → … Pressing `=` should *simplify*, not
  accrete. Collapsing to a single `Number(f64)` keeps it flat.
- Storing committed values as `f64` (not a string) preserves full precision, so
  the bug above disappears with no special-case string surgery — the result is
  already `Num(f64)` at the head of `expr`, so a following operator just appends.

Why not `Token::Number(String)` (keep numbers as text, parse only at eval):

- It reintroduces the format-roundtrip footgun: after `=` you must format the
  result `f64` back into the token, and any non-full-precision format brings the
  bug back. Keeping committed numbers as `f64` separates *internal value* from
  *user-typed text* cleanly. `current` is the only place raw keystrokes live.

## Roles of the three fields

- **`expr`** — the committed, evaluatable structure. Internal truth.
- **`current`** — the number the user is currently typing. The only place
  in-progress text (trailing `.`, trailing zeros, leading `0.`) can be
  represented faithfully; an `f64` cannot. Finalized into a `Number(f64)` token
  when an operator, `)`, or `=` follows.
- **`mode`** — `Editing` while building an expression; `Evaluated` in the
  pristine post-`=` state (a three-way signal: **Copy is enabled**, **⌫ clears**,
  **a digit starts fresh**); `Error(msg)` when the last eval failed. The
  displayed value is *not* stored here — it is rendered on demand from `expr`.

## State Transition Table

`press_button` behavior. "finalize `current`" = if `current` is non-empty, parse
it to `f64`, push `Number`, then clear `current`.

"Post-`=`" means `mode` is `Evaluated` or `Error`. The two differ only where
noted (operator and Copy); otherwise they behave the same.

| Input | Post-`=` (`Evaluated` / `Error`) | `Editing` |
|---|---|---|
| **digit / `.`** | fresh start: clear `expr`, `current = ch`, `mode = Editing` | append `ch` to `current` |
| **operator** (`+ − × ÷`) | `Evaluated`: append `Op` (value already at head of `expr`), `mode = Editing`. `Error`: fresh start, then push `Op` | finalize `current`, push `Op` |
| **`(`** | fresh start: `expr = [LParen]`, `mode = Editing` | finalize `current`, push `LParen` |
| **`)`** | rare; treat as fresh / no-op | finalize `current`, push `RParen` |
| **`=`** | re-eval (effectively a no-op) | finalize `current`, eval `expr`; on success **collapse** to `expr = [Number(value)]`, `mode = Evaluated`; on failure `mode = Error(msg)` |
| **⌫** | **clear all** (same as `C`) | token rule (below) |
| **`C`** | clear all | clear all |

### Backspace (⌫) token rule — `Editing`

One keypress removes exactly one visible character:

1. If `current` is non-empty → pop its last char. Done.
2. Else pop the last token of `expr`:
   - `Op` / `LParen` / `RParen` → removed; done (that was the visible char).
   - `Number(n)` → pull it back to edit: `current = format_number(n)`, **then
     `current.pop()`** to drop its last digit in the *same* press.

The "pull number in **and** drop a digit in one press" detail is load-bearing:
without it, the press that pulls the number into the buffer would not change the
display, so a backspace would appear to do nothing.

Worked trace, `78-65`, one ⌫ per row:

| `current` | `expr` | display |
|---|---|---|
| `"65"` | `[78, −]` | `78-65` |
| `"6"` | `[78, −]` | `78-6` |
| `""` | `[78, −]` | `78-` |
| `""` | `[78]` | `78`  (popped `−`) |
| `"7"` | `[]` | `7`   (pulled `78`, dropped `8`) |
| `""` | `[]` | (empty) |

Pulling a `Number` uses its **display** string (`format_number`), so editing is
WYSIWYG. Editing a *computed* result this way truncates it to what was shown —
matching iOS calculator behavior. This does **not** weaken the precision
guarantee: `1/3 = * 3 =` still yields exactly `1`, because that path goes through
the *operator* (which keeps the full `f64` head token), never through ⌫.

### Backspace right after `=`

Distinct from the token rule above: in the pristine `Evaluated` state, ⌫
**clears everything** like `C`. The original `1/3` expression is gone (collapsed
to a single `f64`), so there is nothing to "resume editing", and this is the only
state where Copy applies — treating it as a clean reset is simplest.

But `1/3 = +` then ⌫ ⌫ behaves like normal editing, because the `+` already left
the post-`=` state (`mode = Editing`):

```
1/3=    → expr=[Number(0.333…)]               mode=Evaluated   ← Copy; ⌫ = clear
  +     → expr=[Number(0.333…), Op('+')]       mode=Editing     ← normal editing
  ⌫     → pop Op('+')      → expr=[Number(0.333…)]
  ⌫     → pull Number(0.333…) → current="0.333333333" (editable, truncated)
```

## Evaluation

`eval::eval` currently takes `&str`. Either render `expr` (plus a finalized
`current`) back into a string for it, or add a token-based entry point — pick
whichever is less disruptive when implementing.

## Display Rendering

Rendered on demand from state — nothing pre-formatted is stored:
- `Mode::Error(msg)` → show `msg`.
- otherwise (`Editing` or `Evaluated`) → live rendering of `expr` + `current`
  (numbers via `format_number`, operators mapped back to `×`/`÷`). In
  `Evaluated`, `expr` is just `[Number(value)]`, so this naturally renders the
  result — no special case needed.

`format_number` (`app.rs:119`) remains the single place an `f64` becomes a
display string.

## Input-validation details (handle at the edges, not in the state machine)

- Reject a second `.` within `current`.
- A leading bare operator on empty input: the parser already supports unary `-`
  via `factor = '-' factor`; other leading operators can be ignored or left to
  fail at eval.

## Dependency

Best done after **app-result-state**, since storing `EvalResult` as a raw `f64`
is what lets the result collapse into a `Number(f64)` token cleanly.
