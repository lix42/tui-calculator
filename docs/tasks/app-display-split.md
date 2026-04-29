# app-display-split: Separate Display String from Internal Expression

## Background

After evaluation, pressing an operator should continue the calculation from the
original expression — not from the formatted display string. The current
implementation uses the result string as the left operand (e.g. `"0.3333..."`),
which discards the original expression and loses precision.

Example of the bug:
```
1 → / → 3 → = → *  → 3 → =
```
Current: expression becomes `"0.3333333333*3"` ≈ `0.9999...`  
Correct: expression should be `"(1/3)*3"` = exactly `1`

## Goal

Add a `display: String` field to `App` that tracks what the screen shows,
independently of `expression` (the internal string used for evaluation).

```rust
pub struct App {
    pub expression: String,  // internal: "1/3*3"
    pub display: String,     // shown on screen: "0.3333... ×"
    ...
}
```

## State Transitions

**After `=`:**
- `expression` stays as-is (`"1/3"`)
- `display` is set to the formatted result (`"0.3333..."`)
- `result` is set (signals post-eval state)

**Operator pressed while in post-eval state:**
- Wrap expression in parens if it contains `+` or `-` (safe default: always
  wrap), then append the operator: `expression = "(1/3)*"`
- `display = "<result> <op symbol>"` (e.g. `"0.3333... ×"`)
- `result = None`

**Digit/paren pressed while in post-eval state:**
- `expression = ch` (fresh start)
- `display = ch`
- `result = None`

**Any other edit (append, backspace):**
- `expression` and `display` stay in sync — both updated together.

## Parens Wrapping Rule

Always wrapping in parens is safe and simple: `(1+3)*`, `(1/3)*`. Redundant
parens (`(2*3)*`) are never incorrect. Optimising to omit redundant parens can
be a later refinement.

## Dependency

Best done after **app-result-state**, since the display formatting decision
(what string to show) belongs to the UI once `EvalResult` stores a raw `f64`.
