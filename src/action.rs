//! The typed input boundary.
//!
//! Every input event — a keypress, a grid button, a mouse click — is resolved to
//! an [`Action`] *before* it reaches `App`. `App::apply` then matches an enum with
//! no catch-all arm, so an illegal input is rejected here (a `None` from the
//! resolvers) instead of being silently mishandled downstream. This replaces the
//! old stringly-typed `press_button(&str)` path, where `press_button("a")` fell
//! into the digit catch-all and pushed `"a"` onto the display.
//!
//! `Digit` lives in this module specifically so its field stays private: enum
//! variant fields inherit the enum's visibility and can't be made private, so
//! `Action::Digit(u8)` could be built with any `u8`. Wrapping the value in a
//! newtype whose only constructor is [`Digit::new`] makes an out-of-range digit
//! unrepresentable by type. The module boundary is the enforcement.

/// A single decimal digit, `0..=9`. The private field means the only way to get
/// one is [`Digit::new`], which rejects everything outside the range — so an
/// `Action::Digit` always holds a valid digit.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct Digit(u8);

impl Digit {
    /// Construct a `Digit`, or `None` if `n` is not in `0..=9`.
    pub fn new(n: u8) -> Option<Digit> {
        if n <= 9 { Some(Digit(n)) } else { None }
    }

    /// The underlying digit value, always `0..=9`.
    pub fn get(self) -> u8 {
        self.0
    }
}

/// A resolved input action — the only thing `App::apply` consumes.
///
/// `Op` holds the *evaluation* operator (`'+' '-' '*' '/'`), not the display
/// glyph: both `from_key('*')` and `from_label("×")` normalize to `Op('*')`, so
/// `App` deals in one alphabet.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Action {
    Digit(Digit),
    Dot,
    Op(char),
    LParen,
    RParen,
    Clear,
    Backspace,
    Equals,
}

impl Action {
    /// Resolve a typed keyboard character to an `Action`, or `None` for keys with
    /// no calculator meaning.
    ///
    /// This is the keyboard's ASCII alphabet: `*` and `/` map to `Op('*')` /
    /// `Op('/')` (the eval operators), `c`/`C` to `Clear`, digits via
    /// [`Digit::new`], etc. It subsumes the old `key_char_to_label`.
    pub fn from_key(ch: char) -> Option<Action> {
        let action = match ch {
            '0'..='9' => Action::Digit(Digit::new(ch as u8 - b'0')?),
            '.' => Action::Dot,
            '+' | '-' | '*' | '/' => Action::Op(ch),
            '(' => Action::LParen,
            ')' => Action::RParen,
            '=' => Action::Equals,
            'c' | 'C' => Action::Clear,
            _ => return None,
        };
        Some(action)
    }

    /// Resolve a button-grid label (display glyph) to an `Action`, or `None` if
    /// the label isn't a real button.
    ///
    /// The grid speaks glyphs: `"×" "÷" "⌫"`. Operators normalize to their eval
    /// char (`"×" -> Op('*')`, `"÷" -> Op('/')`), so this and [`from_key`] agree.
    ///
    /// [`from_key`]: Action::from_key
    pub fn from_label(label: &str) -> Option<Action> {
        match label {
            "×" => Some(Action::Op('*')),
            "÷" => Some(Action::Op('/')),
            "⌫" => Some(Action::Backspace),
            _ => label.parse::<char>().ok().and_then(Action::from_key),
        }
    }

    /// The grid label (display glyph) this action corresponds to. The inverse of
    /// [`from_label`]; used to drive focus-follow and the press flash after a
    /// keyboard activation, where the originating cell isn't otherwise known.
    ///
    /// [`from_label`]: Action::from_label
    pub fn label(&self) -> &'static str {
        match self {
            Action::Digit(d) => match d.get() {
                0 => "0",
                1 => "1",
                2 => "2",
                3 => "3",
                4 => "4",
                5 => "5",
                6 => "6",
                7 => "7",
                8 => "8",
                _ => "9",
            },
            Action::Dot => ".",
            Action::Op('*') => "×",
            Action::Op('/') => "÷",
            Action::Op('+') => "+",
            Action::Op('-') => "-",
            Action::Op(ch) => unreachable!("unexpected operator: {}", ch),
            Action::LParen => "(",
            Action::RParen => ")",
            Action::Clear => "C",
            Action::Backspace => "⌫",
            Action::Equals => "=",
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn digit_new_accepts_0_through_9() {
        for n in 0..=9 {
            assert_eq!(Digit::new(n).map(Digit::get), Some(n));
        }
    }

    #[test]
    fn digit_new_rejects_out_of_range() {
        assert_eq!(Digit::new(10), None);
        assert_eq!(Digit::new(42), None);
        assert_eq!(Digit::new(255), None);
    }

    #[test]
    fn from_key_maps_ascii_operators_to_eval_chars() {
        // The crux: keyboard ASCII `*`/`/` become the eval operators, not glyphs.
        assert_eq!(Action::from_key('*'), Some(Action::Op('*')));
        assert_eq!(Action::from_key('/'), Some(Action::Op('/')));
        assert_eq!(Action::from_key('+'), Some(Action::Op('+')));
        assert_eq!(Action::from_key('-'), Some(Action::Op('-')));
    }

    #[test]
    fn from_key_maps_digits_dot_parens_and_clear() {
        assert_eq!(
            Action::from_key('7'),
            Some(Action::Digit(Digit::new(7).unwrap()))
        );
        assert_eq!(
            Action::from_key('0'),
            Some(Action::Digit(Digit::new(0).unwrap()))
        );
        assert_eq!(Action::from_key('.'), Some(Action::Dot));
        assert_eq!(Action::from_key('('), Some(Action::LParen));
        assert_eq!(Action::from_key(')'), Some(Action::RParen));
        assert_eq!(Action::from_key('='), Some(Action::Equals));
        // Clear is case-insensitive so Shift doesn't matter.
        assert_eq!(Action::from_key('c'), Some(Action::Clear));
        assert_eq!(Action::from_key('C'), Some(Action::Clear));
    }

    #[test]
    fn from_key_rejects_unmapped() {
        assert_eq!(Action::from_key('z'), None);
        assert_eq!(Action::from_key('q'), None); // quit is handled in handle_event
        assert_eq!(Action::from_key(' '), None);
    }

    #[test]
    fn from_label_maps_glyphs_to_eval_chars() {
        assert_eq!(Action::from_label("×"), Some(Action::Op('*')));
        assert_eq!(Action::from_label("÷"), Some(Action::Op('/')));
        assert_eq!(Action::from_label("+"), Some(Action::Op('+')));
        assert_eq!(Action::from_label("⌫"), Some(Action::Backspace));
        assert_eq!(Action::from_label("C"), Some(Action::Clear));
        assert_eq!(Action::from_label("="), Some(Action::Equals));
        assert_eq!(
            Action::from_label("5"),
            Some(Action::Digit(Digit::new(5).unwrap()))
        );
        assert_eq!(Action::from_label("."), Some(Action::Dot));
    }

    #[test]
    fn from_label_rejects_non_buttons() {
        assert_eq!(Action::from_label("a"), None);
        assert_eq!(Action::from_label(""), None);
        assert_eq!(Action::from_label("foo"), None);
    }

    #[test]
    fn label_is_inverse_of_from_label() {
        // Every label that resolves to an Action must round-trip back to itself.
        for label in ["C", "(", ")", "÷", "×", "-", "+", "=", "⌫", ".", "0", "9"] {
            let action = Action::from_label(label).expect("known label");
            assert_eq!(action.label(), label);
        }
    }
}
