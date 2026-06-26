use crate::action::Action;
use crate::eval::{self, Token};

/// What state the calculator is in. Drives both input handling and rendering.
///
/// `Editing` is the normal "building an expression" state. The two post-`=`
/// states share most behavior (a digit starts fresh, ⌫ clears) but differ on
/// operators and on what the display shows:
/// - `Evaluated(prev)` — last eval succeeded; `expr` is collapsed to a single
///   `Number(value)`. `prev` is the pre-eval display string, kept only so the
///   top line can still show the expression that produced the result.
/// - `Error(msg)` — last eval failed; `expr` is left intact and `msg` is shown.
#[derive(Debug)]
enum Mode {
    Editing,
    Evaluated(String),
    Error(String),
}

pub struct App {
    expr: Vec<Token>, // committed tokens — internal truth, full precision
    current: String,  // in-progress number being typed, e.g. "1.50"
    mode: Mode,       // editing vs post-`=` (gates Copy / ⌫ / fresh digit)
    pub should_quit: bool,
}

impl App {
    pub fn new() -> Self {
        Self {
            expr: Vec::new(),
            current: String::new(),
            mode: Mode::Editing,
            should_quit: false,
        }
    }

    /// Apply a resolved [`Action`]. Total over the enum — no catch-all arm — so
    /// every input the boundary admits maps to a defined effect. This is the
    /// single entry point for input; `Action`s are built in `action.rs` from
    /// keyboard chars, grid labels, or clicks before they ever reach here.
    pub fn apply(&mut self, action: Action) {
        match action {
            Action::Digit(d) => self.push_digit(d.get()),
            Action::Dot => self.push_dot(),
            Action::Op(op) => self.push_operator(op),
            Action::LParen => self.push_lparen(),
            Action::RParen => self.push_rparen(),
            Action::Clear => self.clear(),
            Action::Backspace => self.backspace(),
            Action::Equals => self.evaluate(),
        }
    }

    /// Apply a pasted string by routing each character through
    /// [`Action::from_label`] and feeding the result to [`apply`] — the
    /// calculator's single "ingest a string" entry point.
    ///
    /// Resolving via `from_label` (the display-glyph boundary), *not*
    /// `from_key` (keyboard ASCII), is deliberate: text copied out of the
    /// display carries the glyphs the calculator renders — `×` and `÷` — so a
    /// copied expression pasted back round-trips instead of having its
    /// operators silently dropped. `from_label` maps those two glyphs and
    /// delegates every other character to `from_key`, so ASCII `*`/`/` and the
    /// rest still resolve. Glyph knowledge stays solely in `action.rs`.
    ///
    /// Best-effort and per-character, not expression-validated: each char is
    /// applied in sequence, and any that `from_label` doesn't recognize
    /// (spaces, letters, newlines) is silently skipped — the same no-op the
    /// keyboard gives an unmapped key. A large paste may arrive split across
    /// several `Event::Paste`s; each is applied statefully, so the outcome is
    /// the same as a single event.
    ///
    /// [`apply`]: App::apply
    pub fn apply_str(&mut self, s: &str) {
        // `from_label` takes a `&str`; render each char into a stack buffer so
        // there's no per-char heap allocation.
        let mut buf = [0u8; 4];
        for ch in s.chars() {
            if let Some(action) = Action::from_label(ch.encode_utf8(&mut buf)) {
                self.apply(action);
            }
        }
    }

    /// True in either post-`=` state. These share the "input starts fresh"
    /// behavior; callers branch on the specific variant where they differ.
    fn is_post_eval(&self) -> bool {
        matches!(self.mode, Mode::Evaluated(_) | Mode::Error(_))
    }

    /// Commit the in-progress `current` buffer as a `Number` token, if any.
    /// Called before pushing an operator/paren or evaluating.
    fn finalize_current(&mut self) {
        if self.current.is_empty() {
            return;
        }
        if let Ok(n) = self.current.parse::<f64>() {
            self.expr.push(Token::Number(n));
        }
        self.current.clear();
    }

    /// A fresh digit, dot, or `(` after `=` discards the previous calculation
    /// and returns to `Editing`. No-op while already editing.
    fn reset_if_post_eval(&mut self) {
        if self.is_post_eval() {
            self.expr.clear();
            self.current.clear();
            self.mode = Mode::Editing;
        }
    }

    fn push_digit(&mut self, digit: u8) {
        self.reset_if_post_eval();
        // `digit` is a validated `Digit` (0..=9), so this is always an ASCII
        // digit char.
        self.current.push(char::from(b'0' + digit));
    }

    fn push_dot(&mut self) {
        self.reset_if_post_eval();
        if self.current.contains('.') {
            return; // reject a second '.' in the same number
        }
        if self.current.is_empty() {
            // Bare "." doesn't parse as f64; normalize to "0." so the number is
            // well-formed from the first keystroke.
            self.current.push_str("0.");
            return;
        }
        self.current.push('.');
    }

    fn push_operator(&mut self, op: char) {
        match self.mode {
            // Value is already at the head of `expr` as a full-precision
            // Number — just continue from it. This is the precision fix: the
            // operator never round-trips through a formatted string.
            Mode::Evaluated(_) => self.mode = Mode::Editing,
            // Error has no usable value; start over, then take the operator
            // (a leading '-' is valid unary minus; others fail at eval).
            Mode::Error(_) => {
                self.expr.clear();
                self.current.clear();
                self.mode = Mode::Editing;
            }
            Mode::Editing => self.finalize_current(),
        }
        self.expr.push(Token::Op(op));
    }

    fn push_lparen(&mut self) {
        if self.is_post_eval() {
            self.reset_if_post_eval();
        } else {
            self.finalize_current();
        }
        self.expr.push(Token::LParen);
    }

    fn push_rparen(&mut self) {
        if self.is_post_eval() {
            return; // rare; treat as a no-op
        }
        self.finalize_current();
        self.expr.push(Token::RParen);
    }

    pub fn evaluate(&mut self) {
        if self.is_post_eval() {
            return; // re-eval of a finished result is a no-op
        }
        self.finalize_current();
        if self.expr.is_empty() {
            return;
        }
        // Snapshot the expression's display *before* collapsing, so Evaluated
        // can still show it on the top line.
        let snapshot = display_string(&self.expr, "");
        match eval::eval_tokens(&self.expr) {
            Ok(value) => {
                // Collapse to a single Number — keeps the expression flat
                // across chained calculations and preserves full precision.
                self.expr = vec![Token::Number(value)];
                self.mode = Mode::Evaluated(snapshot);
            }
            Err(msg) => self.mode = Mode::Error(msg),
        }
    }

    pub fn clear(&mut self) {
        self.expr.clear();
        self.current.clear();
        self.mode = Mode::Editing;
    }

    pub fn backspace(&mut self) {
        if self.is_post_eval() {
            // Right after `=`, ⌫ clears everything (same as C): the original
            // expression is gone (collapsed to one Number), so there is
            // nothing to resume editing.
            self.clear();
            return;
        }
        self.backspace_editing();
    }

    /// Remove exactly one visible character while editing.
    ///
    /// One keypress must delete exactly one character of what's on screen.
    /// The display is `display_string(&self.expr, &self.current)`, so:
    ///
    ///   1. If `self.current` is non-empty → pop its last char. Done.
    ///   2. Otherwise pop the last token of `self.expr`:
    ///        - Op / LParen / RParen → it's gone; that *was* the visible char.
    ///        - Number(n) → pull it back into the edit buffer:
    ///          `self.current = format_number(n);`
    ///          then **immediately drop its last digit in the same press**:
    ///          `self.current.pop();`
    ///
    /// The "pull a Number in AND drop a digit in one press" detail is the
    /// load-bearing part. Without the second step, the press that pulls the
    /// number into `current` wouldn't change the display at all, so a
    /// backspace would visually do nothing. `format_number` (below) is in
    /// scope — it gives the same text the display showed, so editing is WYSIWYG.
    ///
    /// Worked trace (`78-65`, one ⌫ per row) — the test `backspace_trace_78_minus_65`
    /// checks exactly this:
    ///
    ///   current | expr      | display
    ///   "65"    | [78, -]   | 78-65
    ///   "6"     | [78, -]   | 78-6    (popped current char)
    ///   ""      | [78, -]   | 78-     (popped current char)
    ///   ""      | [78]      | 78      (popped the Op token)
    ///   "7"     | []        | 7       (pulled 78, dropped the 8)
    ///   ""      | []        | (empty)
    fn backspace_editing(&mut self) {
        if !self.current.is_empty() {
            self.current.pop();
            return;
        }
        if let Some(Token::Number(n)) = self.expr.pop() {
            self.current = format_number(n);
            self.current.pop();
        }
    }

    /// The two display lines: `(top, bottom)`. The single rendering entry point
    /// for `ui.rs`, which keeps `mode` private to this module.
    pub fn display_lines(&self) -> (String, String) {
        let live = display_string(&self.expr, &self.current);
        match &self.mode {
            // Bottom shows the live expression; nothing above it yet.
            Mode::Editing => (String::new(), live),
            // Top: the expression that was evaluated; bottom: the result
            // (expr is now [Number(value)], so `live` renders it).
            Mode::Evaluated(prev) => (prev.clone(), live),
            // Top: the expression that failed; bottom: the error message.
            Mode::Error(msg) => (live, msg.clone()),
        }
    }

    /// The result string to copy to the clipboard, or `None` when there's
    /// nothing copyable.
    ///
    /// Only a *successful* evaluation yields a value. By the time the mode is
    /// `Evaluated`, `evaluate` has already collapsed `expr` to a single `Number`,
    /// so `display_string` here renders exactly the result line the display shows.
    /// `Editing` and `Error` return `None` — there's no finished result to copy,
    /// and an error message is never copyable. The UI reads `is_some()` to decide
    /// whether to show the copy affordance, so this is the single source for both
    /// "can copy?" and "what to copy".
    pub fn copy_text(&self) -> Option<String> {
        match self.mode {
            Mode::Evaluated(_) => Some(display_string(&self.expr, &self.current)),
            Mode::Editing | Mode::Error(_) => None,
        }
    }
}

/// Renders the committed `expr` tokens plus the in-progress `current` buffer
/// into the string shown in the display. Numbers go through `format_number`;
/// operators map to their display glyphs (`*`→`×`, `/`→`÷`). This is the
/// display-side inverse of the input mapping in `apply`.
pub fn display_string(expr: &[Token], current: &str) -> String {
    let mut out = String::new();
    for token in expr {
        match token {
            Token::Number(n) => out.push_str(&format_number(*n)),
            Token::Op('*') => out.push('×'),
            Token::Op('/') => out.push('÷'),
            Token::Op(c) => out.push(*c),
            Token::LParen => out.push('('),
            Token::RParen => out.push(')'),
        }
    }
    out.push_str(current);
    out
}

/// Converts an evaluated `f64` into a display string: whole numbers render as
/// integers (`8`, not `8.0`); everything else is trimmed to 10 decimal places
/// with trailing zeros stripped. The single place an `f64` becomes display text.
fn format_number(val: f64) -> String {
    if val == 0.0 {
        return "0".to_string(); // handles -0.0
    }
    if val.fract() == 0.0 && val.abs() < 1e15 {
        return format!("{}", val as i64);
    }
    // otherwise: trim trailing zeros after N decimal places
    let s = format!("{:.10}", val);
    let trimmed = s.trim_end_matches('0').trim_end_matches('.');
    // {:.10} can round a tiny ±epsilon (e.g. 0.5-0.4-0.1 ≈ -2.8e-17) down to
    // zero magnitude while keeping the sign, yielding "-0"; show plain "0".
    if trimmed == "-0" {
        "0".to_string()
    } else {
        trimmed.to_string()
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    /// Apply the action a grid label resolves to. Lets these tests drive `App`
    /// with button labels (`"5"`, `"×"`, `"⌫"`) while `apply` takes a typed
    /// `Action` — `from_label` is the same edge the real UI resolves through.
    fn press(app: &mut App, label: &str) {
        app.apply(Action::from_label(label).expect("known button label"));
    }

    // --- building an expression ---

    #[test]
    fn digit_builds_current() {
        let mut app = App::new();
        press(&mut app, "5");
        assert_eq!(app.current, "5");
        assert!(app.expr.is_empty());
    }

    #[test]
    fn operator_finalizes_current() {
        let mut app = App::new();
        for b in ["5", "+", "3"] {
            press(&mut app, b);
        }
        assert_eq!(app.expr, vec![Token::Number(5.0), Token::Op('+')]);
        assert_eq!(app.current, "3");
    }

    #[test]
    fn leading_dot_normalizes_to_zero_dot() {
        // Bare "." doesn't parse as f64, so a leading "." is normalized to
        // "0." up front. Without this, finalize would silently drop the
        // buffer and `.+` would jump straight to `+` on the display.
        let mut app = App::new();
        press(&mut app, ".");
        assert_eq!(app.current, "0.");
        press(&mut app, "5");
        assert_eq!(app.current, "0.5");

        // `.` then `=` now resolves to 0, not a blank display.
        let mut app = App::new();
        press(&mut app, ".");
        press(&mut app, "=");
        assert_eq!(app.expr, vec![Token::Number(0.0)]);
        assert_eq!(app.display_lines().1, "0");

        // `.` after an operator still works.
        let mut app = App::new();
        for b in ["1", "+", ".", "5", "="] {
            press(&mut app, b);
        }
        assert_eq!(app.expr, vec![Token::Number(1.5)]);
    }

    #[test]
    fn second_dot_is_rejected() {
        let mut app = App::new();
        for b in ["1", ".", "5", ".", "2"] {
            press(&mut app, b);
        }
        assert_eq!(app.current, "1.52"); // the second '.' is ignored
    }

    // --- evaluation ---

    #[test]
    fn evaluate_collapses_to_number() {
        let mut app = App::new();
        for b in ["5", "+", "3", "="] {
            press(&mut app, b);
        }
        assert_eq!(app.expr, vec![Token::Number(8.0)]);
        assert_eq!(app.display_lines().1, "8");
        assert!(matches!(app.mode, Mode::Evaluated(_)));
    }

    #[test]
    fn evaluated_keeps_expression_on_top_line() {
        let mut app = App::new();
        for b in ["7", "8", "-", "6", "5", "×", "5", "="] {
            press(&mut app, b);
        }
        assert_eq!(
            app.display_lines(),
            ("78-65×5".to_string(), "-247".to_string())
        );
    }

    #[test]
    fn digit_after_result_starts_fresh() {
        let mut app = App::new();
        for b in ["5", "+", "3", "=", "2"] {
            press(&mut app, b);
        }
        assert!(app.expr.is_empty());
        assert_eq!(app.current, "2");
        assert!(matches!(app.mode, Mode::Editing));
    }

    #[test]
    fn operator_after_result_continues_from_value() {
        let mut app = App::new();
        for b in ["5", "+", "3", "=", "+"] {
            press(&mut app, b);
        }
        assert_eq!(app.expr, vec![Token::Number(8.0), Token::Op('+')]);
        assert!(matches!(app.mode, Mode::Editing));
    }

    #[test]
    fn full_precision_preserved_through_operator() {
        // 1 ÷ 3 = × 3 =  →  exactly 1. This is the bug the task fixes:
        // continuing through the operator keeps the full-precision f64 at the
        // head of `expr`, never round-tripping through "0.3333333333".
        let mut app = App::new();
        for b in ["1", "÷", "3", "=", "×", "3", "="] {
            press(&mut app, b);
        }
        assert_eq!(app.expr, vec![Token::Number(1.0)]);
        assert_eq!(app.display_lines().1, "1");
    }

    #[test]
    fn parens_evaluate_correctly() {
        let mut app = App::new();
        for b in ["(", "1", "+", "2", ")", "×", "3", "="] {
            press(&mut app, b);
        }
        assert_eq!(app.expr, vec![Token::Number(9.0)]);
        assert_eq!(app.display_lines().1, "9");
    }

    #[test]
    fn division_by_zero_sets_error() {
        let mut app = App::new();
        for b in ["1", "÷", "0", "="] {
            press(&mut app, b);
        }
        assert!(matches!(app.mode, Mode::Error(_)));
        assert_eq!(app.display_lines().1, "division by zero");
    }

    #[test]
    fn keyboard_operator_multiplies_through_apply() {
        // The keyboard route: ASCII `*`/`/` resolve via `from_key` to
        // `Op('*')`/`Op('/')` and drive multiply/divide through `apply`
        // end-to-end. (The grid reaches the same `Op` via the `×`/`÷` glyphs.)
        let mut app = App::new();
        for ch in ['6', '*', '7', '='] {
            app.apply(Action::from_key(ch).expect("mapped key"));
        }
        assert_eq!(app.expr, vec![Token::Number(42.0)]);
        assert_eq!(app.display_lines().1, "42");
    }

    // --- clear / backspace ---

    #[test]
    fn clear_resets_all() {
        let mut app = App::new();
        for b in ["5", "+", "3", "="] {
            press(&mut app, b);
        }
        app.clear();
        assert!(app.expr.is_empty());
        assert!(app.current.is_empty());
        assert!(matches!(app.mode, Mode::Editing));
    }

    #[test]
    fn backspace_pops_current_char() {
        let mut app = App::new();
        for b in ["7", "8", "-", "6", "5"] {
            press(&mut app, b);
        }
        press(&mut app, "⌫");
        assert_eq!(app.current, "6");
        assert_eq!(app.expr, vec![Token::Number(78.0), Token::Op('-')]);
    }

    #[test]
    fn backspace_trace_78_minus_65() {
        let mut app = App::new();
        for b in ["7", "8", "-", "6", "5"] {
            press(&mut app, b);
        }
        assert_eq!(app.display_lines().1, "78-65");
        press(&mut app, "⌫");
        assert_eq!(app.display_lines().1, "78-6");
        press(&mut app, "⌫");
        assert_eq!(app.display_lines().1, "78-");
        press(&mut app, "⌫");
        assert_eq!(app.display_lines().1, "78"); // popped the Op token
        press(&mut app, "⌫");
        assert_eq!(app.display_lines().1, "7"); // pulled 78, dropped the 8
        press(&mut app, "⌫");
        assert_eq!(app.display_lines().1, "");
    }

    #[test]
    fn backspace_after_result_clears() {
        let mut app = App::new();
        for b in ["5", "+", "3", "="] {
            press(&mut app, b);
        }
        press(&mut app, "⌫");
        assert!(app.expr.is_empty());
        assert!(app.current.is_empty());
        assert!(matches!(app.mode, Mode::Editing));
    }

    // --- display_string ---

    #[test]
    fn display_string_maps_operators_to_glyphs() {
        assert_eq!(
            display_string(
                &[Token::Number(6.0), Token::Op('/'), Token::Number(2.0)],
                ""
            ),
            "6÷2"
        );
        assert_eq!(
            display_string(
                &[Token::Number(3.0), Token::Op('*'), Token::Number(4.0)],
                ""
            ),
            "3×4"
        );
    }

    #[test]
    fn display_string_appends_current() {
        let expr = [Token::Number(78.0), Token::Op('-')];
        assert_eq!(display_string(&expr, "65"), "78-65");
    }

    #[test]
    fn display_string_empty_is_blank() {
        assert_eq!(display_string(&[], ""), "");
    }

    #[test]
    fn near_zero_negative_epsilon_formats_as_zero() {
        // 0.5-0.4-0.1 lands at ~-2.8e-17 in f64, which {:.10} rounds to zero
        // magnitude but with a stray sign. The display must read "0", not "-0".
        let mut app = App::new();
        for b in ["0", ".", "5", "-", "0", ".", "4", "-", "0", ".", "1", "="] {
            press(&mut app, b);
        }
        assert_eq!(app.display_lines().1, "0");
    }

    // --- paste ---

    #[test]
    fn paste_builds_expression() {
        // A whole expression pasted at once lands the same as typing it: the
        // last number is still in `current`, the rest committed to `expr`.
        let mut app = App::new();
        app.apply_str("78-65*5");
        assert_eq!(app.display_lines().1, "78-65×5");
        assert_eq!(app.current, "5");
        assert!(matches!(app.mode, Mode::Editing));
    }

    #[test]
    fn paste_display_glyphs_round_trip() {
        // Text copied from the calculator's own display carries the `×`/`÷`
        // glyphs, not ASCII `*`/`/`. Pasting it back must evaluate correctly:
        // `apply_str` resolves via `from_label`, which maps the glyphs. (With
        // `from_key`, `×` was dropped and `78-65×5` mis-parsed as `78-655`.)
        let mut app = App::new();
        app.apply_str("78-65×5=");
        assert_eq!(app.expr, vec![Token::Number(-247.0)]);
        assert_eq!(app.display_lines().1, "-247");

        let mut div = App::new();
        div.apply_str("84÷2=");
        assert_eq!(div.display_lines().1, "42");
    }

    #[test]
    fn paste_skips_whitespace_and_unmapped_chars() {
        // Spaces and stray letters resolve to `None` in `from_label`, so they're
        // dropped — a spaced "78 - 65" pastes identically to "78-65".
        let mut spaced = App::new();
        spaced.apply_str("78 - 65");
        assert_eq!(spaced.display_lines().1, "78-65");

        let mut junk = App::new();
        junk.apply_str("7a8");
        assert_eq!(junk.current, "78");
    }

    #[test]
    fn paste_with_trailing_equals_evaluates() {
        // `=` flows through the same boundary, so a pasted expression ending in
        // `=` evaluates in one event — no separate keypress needed.
        let mut app = App::new();
        app.apply_str("2+2=");
        assert_eq!(app.expr, vec![Token::Number(4.0)]);
        assert_eq!(app.display_lines().1, "4");
        assert!(matches!(app.mode, Mode::Evaluated(_)));
    }

    #[test]
    fn paste_after_result_starts_fresh() {
        // Pasting a digit after `=` hits the same post-eval reset as typing one,
        // because each char still routes through `apply`.
        let mut app = App::new();
        app.apply_str("5+3=");
        app.apply_str("9");
        assert_eq!(app.current, "9");
        assert!(app.expr.is_empty());
        assert!(matches!(app.mode, Mode::Editing));
    }

    #[test]
    fn paste_parens_and_decimals_evaluate() {
        // The headline paste case: a real expression with parens and decimals,
        // evaluated. Exercises the `(`, `)`, and `.` glyphs through the paste
        // boundary — none of the other paste tests reach those.
        let mut app = App::new();
        app.apply_str("(1.5+2.5)*2=");
        assert_eq!(app.expr, vec![Token::Number(8.0)]);
        assert_eq!(app.display_lines().1, "8");
    }

    #[test]
    fn paste_into_existing_expression_continues() {
        // Paste usually lands on top of input already on screen. `apply_str`
        // must not reset: typing 78 then pasting "*5" continues to 78×5.
        let mut app = App::new();
        press(&mut app, "7");
        press(&mut app, "8");
        app.apply_str("*5");
        assert_eq!(app.expr, vec![Token::Number(78.0), Token::Op('*')]);
        assert_eq!(app.current, "5");
        assert_eq!(app.display_lines().1, "78×5");
    }

    #[test]
    fn paste_skips_newlines() {
        // Clipboard text often carries a newline (copying "1+1" from a code
        // block grabs the trailing \n). `\n` resolves to None in from_key, so
        // it's dropped just like a space — the paste reads as if joined.
        let mut app = App::new();
        app.apply_str("1+1\n2");
        assert_eq!(app.display_lines().1, "1+12");
    }

    // --- copy ---

    #[test]
    fn copy_text_some_after_eval() {
        let mut app = App::new();
        for b in ["2", "+", "3", "="] {
            press(&mut app, b);
        }
        assert_eq!(app.copy_text().as_deref(), Some("5"));
    }

    #[test]
    fn copy_text_none_while_editing() {
        let mut app = App::new();
        for b in ["2", "+", "3"] {
            press(&mut app, b);
        }
        assert_eq!(app.copy_text(), None);
    }

    #[test]
    fn copy_text_none_on_error() {
        let mut app = App::new();
        for b in ["1", "÷", "0", "="] {
            press(&mut app, b);
        }
        assert!(matches!(app.mode, Mode::Error(_)));
        assert_eq!(app.copy_text(), None);
    }

    #[test]
    fn copy_text_dismissed_by_new_input() {
        // A fresh digit after `=` returns to Editing, so the result is no longer
        // copyable — the UI affordance disappears the same way.
        let mut app = App::new();
        for b in ["2", "+", "3", "="] {
            press(&mut app, b);
        }
        assert!(app.copy_text().is_some());
        press(&mut app, "7");
        assert_eq!(app.copy_text(), None);
    }

    #[test]
    fn format_number_normalizes_negative_zero() {
        assert_eq!(format_number(-2.7755575615628914e-17), "0");
        // a genuine small value is still shown, not snapped away
        assert_eq!(format_number(-0.0001), "-0.0001");
    }

    #[test]
    fn display_string_renders_parens_and_single_number() {
        let parens = [
            Token::LParen,
            Token::Number(1.0),
            Token::Op('+'),
            Token::Number(2.0),
            Token::RParen,
        ];
        assert_eq!(display_string(&parens, ""), "(1+2)");
        assert_eq!(display_string(&[Token::Number(-247.0)], ""), "-247");
    }
}
