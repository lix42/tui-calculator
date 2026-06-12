use std::collections::HashMap;
use std::sync::LazyLock;
use std::time::{Duration, Instant};

use crate::eval::{self, Token};

/// How long a button stays in its "pressed" look after activation. Terminals
/// have no key-release event, so the press is shown as a brief flash that the
/// run loop's `tick` clears once this much time has passed.
const FLASH_DURATION: Duration = Duration::from_millis(120);

pub const BUTTONS: [[&str; 4]; 5] = [
    ["C", "(", ")", "÷"],
    ["7", "8", "9", "×"],
    ["4", "5", "6", "-"],
    ["1", "2", "3", "+"],
    ["⌫", "0", ".", "="],
];

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
    pub focus: (usize, usize),
    flash: Option<(usize, usize)>, // button showing its momentary "pressed" look
    flash_at: Instant,             // when the current flash began (see FLASH_DURATION)
    pub should_quit: bool,
}

impl App {
    pub fn new() -> Self {
        Self {
            expr: Vec::new(),
            current: String::new(),
            mode: Mode::Editing,
            focus: (4, 3),
            flash: None,
            flash_at: Instant::now(),
            should_quit: false,
        }
    }

    pub fn press_button(&mut self, label: &str) {
        match label {
            "C" => self.clear(),
            "⌫" => self.backspace(),
            "=" => self.evaluate(),
            "(" => self.push_lparen(),
            ")" => self.push_rparen(),
            "÷" => self.push_operator('/'),
            "×" => self.push_operator('*'),
            "+" => self.push_operator('+'),
            "-" => self.push_operator('-'),
            _ => self.push_digit(label), // digits and "."
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

    fn push_digit(&mut self, ch: &str) {
        if self.is_post_eval() {
            // Fresh start: a digit after `=` discards the previous calculation.
            self.expr.clear();
            self.current.clear();
            self.mode = Mode::Editing;
        }
        if ch == "." {
            if self.current.contains('.') {
                return; // reject a second '.' in the same number
            }
            if self.current.is_empty() {
                // Bare "." doesn't parse as f64; normalize to "0." so the
                // number is well-formed from the first keystroke.
                self.current.push_str("0.");
                return;
            }
        }
        self.current.push_str(ch);
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
            self.expr.clear();
            self.current.clear();
            self.mode = Mode::Editing;
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

    pub fn move_focus(&mut self, dr: i32, dc: i32) {
        let rows = BUTTONS.len() as i32;
        let cols = BUTTONS[0].len() as i32;
        self.focus.0 = (self.focus.0 as i32 + dr).clamp(0, rows - 1) as usize;
        self.focus.1 = (self.focus.1 as i32 + dc).clamp(0, cols - 1) as usize;
    }

    /// `&'static` because `BUTTONS` is a `'static` const — returning it untied
    /// from `&self` lets the caller hold the label while mutably borrowing the
    /// app (e.g. `let l = app.focused_label(); app.press_button(l);`).
    pub fn focused_label(&self) -> &'static str {
        BUTTONS[self.focus.0][self.focus.1]
    }

    /// Record that `label` was just activated: focus follows it and its press
    /// flash starts. No-op if the label isn't on the grid. The run loop's
    /// `tick` clears the flash after `FLASH_DURATION`.
    pub fn register_press(&mut self, label: &str) {
        if let Some(pos) = position_of(label) {
            self.focus = pos;
            self.flash = Some(pos);
            self.flash_at = Instant::now();
        }
    }

    /// Whether the button at `pos` is currently showing its pressed flash.
    pub fn is_pressed(&self, pos: (usize, usize)) -> bool {
        self.flash == Some(pos)
    }

    /// Expire the press flash once it has been visible for `FLASH_DURATION`.
    /// Called once per run-loop iteration before drawing.
    pub fn tick(&mut self) {
        if self.flash.is_some() && self.flash_at.elapsed() >= FLASH_DURATION {
            self.flash = None;
        }
    }
}

/// Reverse index of `BUTTONS`: label → grid position. Built once on first
/// lookup (`LazyLock`) and derived from `BUTTONS`, so it stays in sync with the
/// grid — `BUTTONS` is the single source of truth — at no startup cost.
static LABEL_POS: LazyLock<HashMap<&'static str, (usize, usize)>> = LazyLock::new(|| {
    let mut map = HashMap::new();
    for (r, row) in BUTTONS.iter().enumerate() {
        for (c, label) in row.iter().enumerate() {
            map.insert(*label, (r, c));
        }
    }
    map
});

/// Grid position of `label`, or `None` if no button carries it. The inverse of
/// `BUTTONS[r][c]`; used to make focus follow keyboard input and to locate the
/// cell to flash.
pub fn position_of(label: &str) -> Option<(usize, usize)> {
    LABEL_POS.get(label).copied()
}

/// Renders the committed `expr` tokens plus the in-progress `current` buffer
/// into the string shown in the display. Numbers go through `format_number`;
/// operators map to their display glyphs (`*`→`×`, `/`→`÷`). This is the
/// display-side inverse of the keystroke mapping in `press_button`.
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

    // --- building an expression ---

    #[test]
    fn digit_builds_current() {
        let mut app = App::new();
        app.press_button("5");
        assert_eq!(app.current, "5");
        assert!(app.expr.is_empty());
    }

    #[test]
    fn operator_finalizes_current() {
        let mut app = App::new();
        for b in ["5", "+", "3"] {
            app.press_button(b);
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
        app.press_button(".");
        assert_eq!(app.current, "0.");
        app.press_button("5");
        assert_eq!(app.current, "0.5");

        // `.` then `=` now resolves to 0, not a blank display.
        let mut app = App::new();
        app.press_button(".");
        app.press_button("=");
        assert_eq!(app.expr, vec![Token::Number(0.0)]);
        assert_eq!(app.display_lines().1, "0");

        // `.` after an operator still works.
        let mut app = App::new();
        for b in ["1", "+", ".", "5", "="] {
            app.press_button(b);
        }
        assert_eq!(app.expr, vec![Token::Number(1.5)]);
    }

    #[test]
    fn second_dot_is_rejected() {
        let mut app = App::new();
        for b in ["1", ".", "5", ".", "2"] {
            app.press_button(b);
        }
        assert_eq!(app.current, "1.52"); // the second '.' is ignored
    }

    // --- evaluation ---

    #[test]
    fn evaluate_collapses_to_number() {
        let mut app = App::new();
        for b in ["5", "+", "3", "="] {
            app.press_button(b);
        }
        assert_eq!(app.expr, vec![Token::Number(8.0)]);
        assert_eq!(app.display_lines().1, "8");
        assert!(matches!(app.mode, Mode::Evaluated(_)));
    }

    #[test]
    fn evaluated_keeps_expression_on_top_line() {
        let mut app = App::new();
        for b in ["7", "8", "-", "6", "5", "×", "5", "="] {
            app.press_button(b);
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
            app.press_button(b);
        }
        assert!(app.expr.is_empty());
        assert_eq!(app.current, "2");
        assert!(matches!(app.mode, Mode::Editing));
    }

    #[test]
    fn operator_after_result_continues_from_value() {
        let mut app = App::new();
        for b in ["5", "+", "3", "=", "+"] {
            app.press_button(b);
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
            app.press_button(b);
        }
        assert_eq!(app.expr, vec![Token::Number(1.0)]);
        assert_eq!(app.display_lines().1, "1");
    }

    #[test]
    fn parens_evaluate_correctly() {
        let mut app = App::new();
        for b in ["(", "1", "+", "2", ")", "×", "3", "="] {
            app.press_button(b);
        }
        assert_eq!(app.expr, vec![Token::Number(9.0)]);
        assert_eq!(app.display_lines().1, "9");
    }

    #[test]
    fn division_by_zero_sets_error() {
        let mut app = App::new();
        for b in ["1", "÷", "0", "="] {
            app.press_button(b);
        }
        assert!(matches!(app.mode, Mode::Error(_)));
        assert_eq!(app.display_lines().1, "division by zero");
    }

    // --- clear / backspace ---

    #[test]
    fn clear_resets_all() {
        let mut app = App::new();
        for b in ["5", "+", "3", "="] {
            app.press_button(b);
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
            app.press_button(b);
        }
        app.press_button("⌫");
        assert_eq!(app.current, "6");
        assert_eq!(app.expr, vec![Token::Number(78.0), Token::Op('-')]);
    }

    #[test]
    fn backspace_trace_78_minus_65() {
        let mut app = App::new();
        for b in ["7", "8", "-", "6", "5"] {
            app.press_button(b);
        }
        assert_eq!(app.display_lines().1, "78-65");
        app.press_button("⌫");
        assert_eq!(app.display_lines().1, "78-6");
        app.press_button("⌫");
        assert_eq!(app.display_lines().1, "78-");
        app.press_button("⌫");
        assert_eq!(app.display_lines().1, "78"); // popped the Op token
        app.press_button("⌫");
        assert_eq!(app.display_lines().1, "7"); // pulled 78, dropped the 8
        app.press_button("⌫");
        assert_eq!(app.display_lines().1, "");
    }

    #[test]
    fn backspace_after_result_clears() {
        let mut app = App::new();
        for b in ["5", "+", "3", "="] {
            app.press_button(b);
        }
        app.press_button("⌫");
        assert!(app.expr.is_empty());
        assert!(app.current.is_empty());
        assert!(matches!(app.mode, Mode::Editing));
    }

    // --- focus (unchanged) ---

    #[test]
    fn move_focus_clamps() {
        let mut app = App::new();
        app.focus = (0, 0);
        app.move_focus(-5, -5);
        assert_eq!(app.focus, (0, 0));
        app.move_focus(99, 99);
        assert_eq!(app.focus, (4, 3));
    }

    #[test]
    fn focused_label_default() {
        assert_eq!(App::new().focused_label(), "=");
    }

    #[test]
    fn position_of_finds_labels_and_misses() {
        assert_eq!(position_of("C"), Some((0, 0)));
        assert_eq!(position_of("="), Some((4, 3)));
        assert_eq!(position_of("5"), Some((2, 1)));
        assert_eq!(position_of("⌫"), Some((4, 0)));
        assert_eq!(position_of("?"), None);
    }

    #[test]
    fn register_press_moves_focus_and_flashes() {
        let mut app = App::new(); // focus starts on "=" at (4, 3)
        app.register_press("5");
        assert_eq!(app.focus, (2, 1)); // focus followed the input
        assert!(app.is_pressed((2, 1))); // and that cell is flashing
        assert!(!app.is_pressed((4, 3))); // the old focus is not
    }

    #[test]
    fn register_press_ignores_unknown_label() {
        let mut app = App::new();
        app.register_press("?");
        assert_eq!(app.focus, (4, 3)); // unchanged
        assert!(!app.is_pressed((4, 3)));
    }

    #[test]
    fn tick_keeps_fresh_flash() {
        // A flash set this instant is well within FLASH_DURATION, so tick must
        // leave it visible. (Expiry after the duration is paced by the run loop
        // and exercised manually rather than with a sleep here.)
        let mut app = App::new();
        app.register_press("5");
        app.tick();
        assert!(app.is_pressed((2, 1)));
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
            app.press_button(b);
        }
        assert_eq!(app.display_lines().1, "0");
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
