# eval-parser: Expression Parser and Evaluator

## Requirement

Implement a recursive descent parser in `src/eval.rs` that evaluates arithmetic expressions. This is the computational core of the calculator — pure logic with no UI dependencies.

Supported syntax:
- Decimal numbers: `42`, `3.14`, `.5`
- Operators: `+`, `-`, `*`, `/`
- Parentheses: `(`, `)`
- Unary minus: `-5`, `-(3+2)`
- Operator precedence: `*` and `/` bind tighter than `+` and `-`

Returns `Result<f64, String>` — the error string is displayed in the UI result area.

## Design

Recursive descent grammar (from design doc):

```
expr   = term (('+' | '-') term)*
term   = factor (('*' | '/') factor)*
factor = '-' factor | '(' expr ')' | number
number = [0-9]+ ('.' [0-9]+)?
```

Implement a `Parser` struct that holds the input chars and a cursor position. Each grammar rule becomes a method.

Division by zero should return `Err("Division by zero".into())`.

Public API: `pub fn eval(input: &str) -> Result<f64, String>`

## Implementation Suggestion

- Define a `Parser { chars: Vec<char>, pos: usize }` struct (private)
- Methods: `parse_expr()`, `parse_term()`, `parse_factor()`, `parse_number()`
- Helper: `peek()` returns current char, `advance()` moves cursor
- Skip whitespace between tokens
- After parsing, verify `pos == chars.len()` to catch trailing junk
- Expose a single `pub fn eval(input: &str) -> Result<f64, String>`

## How to Test

Unit tests in `src/eval.rs` using `#[cfg(test)]`:

```
cargo test
```

Test cases:
- Basic arithmetic: `"2+3"` → `5.0`
- Precedence: `"2+3*4"` → `14.0`
- Parentheses: `"(2+3)*4"` → `20.0`
- Unary minus: `"-5"` → `-5.0`, `"-(3+2)"` → `-5.0`
- Decimals: `"3.14*2"` → `6.28`
- Division by zero: `"1/0"` → `Err`
- Nested: `"((1+2)*(3+4))/7"` → `3.0`
- Empty input: `""` → `Err`
- Invalid input: `"2++3"` → `Err`

## Dependencies

None — this is a standalone module with no dependencies on other tasks.
