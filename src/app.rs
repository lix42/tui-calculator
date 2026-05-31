use crate::eval::{self, Token};

pub const BUTTONS: [[&str; 4]; 5] = [
    ["C", "(", ")", "÷"],
    ["7", "8", "9", "×"],
    ["4", "5", "6", "-"],
    ["1", "2", "3", "+"],
    ["⌫", "0", ".", "="],
];

pub struct App {
    pub expression: String,
    pub result: Option<String>,
    pub focus: (usize, usize),
    pub should_quit: bool,
}

impl App {
    pub fn new() -> Self {
        Self {
            expression: String::new(),
            result: None,
            focus: (4, 3),
            should_quit: false,
        }
    }

    pub fn press_button(&mut self, label: &str) {
        match label {
            "C" => self.clear(),
            "⌫" => self.backspace(),
            "=" => self.evaluate(),
            _ => self.append(label),
        }
    }

    fn append(&mut self, label: &str) {
        let ch = match label {
            "÷" => "/",
            "×" => "*",
            other => other,
        };
        let is_operator = matches!(ch, "+" | "-" | "*" | "/");
        if let Some(res) = self.result.take() {
            if is_operator {
                // Continue calculation: operator after result uses result as left operand.
                // If the result was an error string, fall back to a fresh expression.
                if res.parse::<f64>().is_ok() {
                    self.expression = format!("{}{}", res, ch);
                } else {
                    self.expression.clear();
                }
            } else {
                // Digit / paren / dot after result: start a fresh expression.
                self.expression = ch.to_string();
            }
        } else {
            self.expression.push_str(ch);
        }
    }

    pub fn evaluate(&mut self) {
        if self.expression.is_empty() {
            return;
        }
        self.result = Some(match eval::eval(&self.expression) {
            Ok(val) => format_number(val),
            Err(msg) => msg,
        });
    }

    pub fn clear(&mut self) {
        self.expression.clear();
        self.result = None;
    }

    pub fn backspace(&mut self) {
        if self.result.is_some() {
            // Backspace after evaluation: discard the result and resume editing the expression.
            self.result = None;
            return;
        }
        self.expression.pop();
    }

    pub fn move_focus(&mut self, dr: i32, dc: i32) {
        let rows = BUTTONS.len() as i32;
        let cols = BUTTONS[0].len() as i32;
        self.focus.0 = (self.focus.0 as i32 + dr).clamp(0, rows - 1) as usize;
        self.focus.1 = (self.focus.1 as i32 + dc).clamp(0, cols - 1) as usize;
    }

    pub fn focused_label(&self) -> &str {
        BUTTONS[self.focus.0][self.focus.1]
    }
}

pub fn expr_to_display(expr: &str) -> String {
    expr.replace('*', "×").replace('/', "÷")
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
    format!("{:.10}", val)
        .trim_end_matches('0')
        .trim_end_matches('.')
        .to_string()
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn sequence_digit_plus_digit() {
        let mut app = App::new();
        app.press_button("5");
        app.press_button("+");
        app.press_button("3");
        assert_eq!(app.expression, "5+3");
        assert!(app.result.is_none());
    }

    #[test]
    fn evaluate_simple() {
        let mut app = App::new();
        app.press_button("5");
        app.press_button("+");
        app.press_button("3");
        app.press_button("=");
        assert_eq!(app.result.as_deref(), Some("8"));
    }

    #[test]
    fn digit_after_result_starts_fresh() {
        let mut app = App::new();
        app.press_button("5");
        app.press_button("+");
        app.press_button("3");
        app.press_button("=");
        app.press_button("2");
        assert_eq!(app.expression, "2");
        assert!(app.result.is_none());
    }

    #[test]
    fn operator_after_result_continues() {
        let mut app = App::new();
        app.press_button("5");
        app.press_button("+");
        app.press_button("3");
        app.press_button("=");
        app.press_button("+");
        assert_eq!(app.expression, "8+");
        assert!(app.result.is_none());
    }

    #[test]
    fn clear_resets_all() {
        let mut app = App::new();
        app.press_button("5");
        app.press_button("=");
        app.clear();
        assert!(app.expression.is_empty());
        assert!(app.result.is_none());
    }

    #[test]
    fn backspace_removes_last_char() {
        let mut app = App::new();
        app.press_button("5");
        app.press_button("+");
        app.press_button("3");
        app.backspace();
        assert_eq!(app.expression, "5+");
    }

    #[test]
    fn backspace_after_result_restores_editing() {
        let mut app = App::new();
        app.press_button("5");
        app.press_button("+");
        app.press_button("3");
        app.press_button("=");
        app.backspace();
        assert!(app.result.is_none());
        assert_eq!(app.expression, "5+3");
    }

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
    fn display_string_renders_parens_and_single_number() {
        let parens = [
            Token::LParen,
            Token::Number(1.0),
            Token::Op('+'),
            Token::Number(2.0),
            Token::RParen,
        ];
        assert_eq!(display_string(&parens, ""), "(1+2)");
        // Post-`=` collapsed state: a lone Number renders as its formatted value.
        assert_eq!(display_string(&[Token::Number(-247.0)], ""), "-247");
    }

    #[test]
    fn display_chars_map_to_expression_chars() {
        let mut app = App::new();
        app.press_button("6");
        app.press_button("÷");
        app.press_button("2");
        assert_eq!(app.expression, "6/2");
        let mut app2 = App::new();
        app2.press_button("3");
        app2.press_button("×");
        app2.press_button("4");
        assert_eq!(app2.expression, "3*4");
    }
}
