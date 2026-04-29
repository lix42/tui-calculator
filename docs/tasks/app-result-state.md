# app-result-state: Refactor Result Field to a Proper Enum

## Background

`App.result` is currently `Option<String>`. It conflates two distinct states — a
numeric result and an error message — in a single string. The only way to
distinguish them is to call `parse::<f64>()`, which is a code smell: it couples
downstream logic to the display format of the number.

## Goal

Introduce a typed enum so the two cases are explicit:

```rust
pub enum EvalResult {
    Value(f64),
    Error(String),
}
```

Change `App.result` to `Option<EvalResult>`.

## Changes Required

- Define `EvalResult` in `src/app.rs` (or a new `src/eval_result.rs`).
- Update `App::evaluate()` to store `EvalResult::Value(f64)` or
  `EvalResult::Error(String)` instead of a formatted string.
- Move `format_number` to be called at display time (UI layer), not at eval
  time. The model stores the raw `f64`; the UI formats it.
- Remove all `res.parse::<f64>()` call sites; replace with a `match` on
  `EvalResult`.
- Update unit tests accordingly.

## Dependency

Should be done after **app-state** and ideally alongside or before
**app-display-split**, since both touch how `result` is stored and consumed.
