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

/// The quick-input map: a keyboard character → the grid label it enters while
/// quick-mode is on. One table read in *both* directions ([`quick_map`] and
/// [`quick_key`]), so the routing and the on-screen tips cannot drift apart.
///
/// The right hand is a **numpad in place**: on a QWERTY keyboard `u i o` sit
/// directly below `7 8 9`, and `j k l` directly below those, so the rows descend
/// `789 / 456 / 123` exactly like a physical keypad — with `m`, one row further
/// down again, as the `0` beneath them. The number row itself needs no entry (a
/// digit key already types its digit), and neither does `.`: it is *already* the
/// decimal point and already sits bottom-right, where a numpad puts it. The left
/// hand takes the four operators, and `[` / `]` reach the parens without the
/// Shift that `(` / `)` normally cost.
///
/// Values are *labels*, not [`Action`]s, so callers resolve through
/// [`Action::from_label`] — the same display-glyph boundary the button grid and
/// paste already use — and the tips know which cell to mark. Keeping this a pure
/// `char → label` table (no crossterm types) means the web port can reuse it
/// verbatim against ratzilla's key events.
#[rustfmt::skip]
const QUICK_MAP: &[(char, &str)] = &[
    // Right hand: the numpad, descending, with `0` under it.
    ('u', "4"), ('i', "5"), ('o', "6"),
    ('j', "1"), ('k', "2"), ('l', "3"),
                ('m', "0"),
    // Left hand: the four operators.
    ('a', "+"), ('s', "-"), ('d', "×"), ('f', "÷"),
    // Parens, without the Shift they normally need.
    ('[', "("), (']', ")"),
];

/// The grid label `ch` enters in quick-mode, or `None` if it has no mapping.
/// See [`QUICK_MAP`] for the layout and why the digit row and `.` are absent.
pub fn quick_map(ch: char) -> Option<&'static str> {
    QUICK_MAP
        .iter()
        .find(|(key, _)| *key == ch)
        .map(|(_, label)| *label)
}

/// The inverse of [`quick_map`]: the key that enters `label`, or `None` if that
/// button has no quick key. Drives the on-cell tips, so it has to agree with
/// `quick_map` — reading the one [`QUICK_MAP`] table is what guarantees it.
pub fn quick_key(label: &str) -> Option<char> {
    QUICK_MAP
        .iter()
        .find(|(_, mapped)| *mapped == label)
        .map(|(key, _)| *key)
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::layout::Keypad;
    use std::collections::HashSet;

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

    #[test]
    fn quick_map_lays_out_a_numpad_under_the_right_hand() {
        // The shape is the point: `u i o` / `j k l` / `m` descend 456 / 123 / 0,
        // mirroring the physical keys they sit under.
        assert_eq!(
            ["u", "i", "o"].map(|k| quick_map(k.parse().unwrap())),
            [Some("4"), Some("5"), Some("6")]
        );
        assert_eq!(
            ["j", "k", "l"].map(|k| quick_map(k.parse().unwrap())),
            [Some("1"), Some("2"), Some("3")]
        );
        assert_eq!(quick_map('m'), Some("0"));
        // Left hand: operators, as the *display glyphs* (so `from_label` resolves
        // them to the eval operators, not as literal `*`/`/` keystrokes).
        assert_eq!(
            ['a', 's', 'd', 'f'].map(quick_map),
            [Some("+"), Some("-"), Some("×"), Some("÷")]
        );
        assert_eq!(['[', ']'].map(quick_map), [Some("("), Some(")")]);
    }

    #[test]
    fn quick_map_leaves_the_digit_row_and_dot_alone() {
        // Deliberate absences, not oversights: a digit key already types its
        // digit, and `.` is already the decimal point — mapping either would be a
        // no-op entry that also earned a pointless on-screen tip.
        for ch in ['7', '8', '9', '0', '.'] {
            assert_eq!(quick_map(ch), None, "{ch:?} needs no quick mapping");
        }
        // `h` is genuinely unmapped — the numpad has nothing left of `1`.
        for ch in ['h', 'q', 'z', 'y', ' '] {
            assert_eq!(quick_map(ch), None, "{ch:?} should not be mapped");
        }
    }

    #[test]
    fn every_quick_label_is_a_real_button_on_every_pad() {
        // The contract that keeps quick-input honest: a mapped label must resolve
        // to an Action *and* exist on whichever pad is active, or a key would
        // silently do nothing (or flash a cell that isn't there).
        for (key, label) in QUICK_MAP {
            assert!(
                Action::from_label(label).is_some(),
                "{key:?} maps to {label:?}, which is not an Action"
            );
            for (name, pad) in [
                ("standard", Keypad::standard()),
                ("tall", Keypad::tall()),
                ("wide", Keypad::wide()),
            ] {
                assert!(
                    pad.position_of(label).is_some(),
                    "{label:?} (key {key:?}) is missing from the {name} pad"
                );
            }
        }
    }

    #[test]
    fn quick_key_is_the_inverse_of_quick_map() {
        // The tips read this direction; if the two disagreed, a cell would
        // advertise a key that doesn't drive it.
        for (key, label) in QUICK_MAP {
            assert_eq!(quick_key(label), Some(*key));
            assert_eq!(quick_map(*key), Some(*label));
        }
        // A button with no quick key gets no tip.
        assert_eq!(quick_key("="), None);
        assert_eq!(quick_key("7"), None);
    }

    #[test]
    fn quick_map_has_no_duplicate_keys_or_labels() {
        // Both lookups take the *first* match, so a duplicate entry would be
        // silently shadowed — and a label mapped twice would draw two tips.
        let keys: HashSet<char> = QUICK_MAP.iter().map(|(key, _)| *key).collect();
        assert_eq!(keys.len(), QUICK_MAP.len(), "duplicate key in QUICK_MAP");
        let labels: HashSet<&str> = QUICK_MAP.iter().map(|(_, label)| *label).collect();
        assert_eq!(
            labels.len(),
            QUICK_MAP.len(),
            "duplicate label in QUICK_MAP"
        );
    }
}
