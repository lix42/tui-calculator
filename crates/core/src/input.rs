//! The backend-neutral input layer: what each key *means*, shared by the native
//! and web entry points.
//!
//! Each entry point translates its own key event (crossterm's on native, the
//! browser's `KeyboardEvent` on the web) into a [`Key`], asks [`key_to_msg`] what
//! it means, and hands the resulting [`Msg`] to [`apply_msg`]. So the rules — the
//! quick-mode reassignments, the Ctrl/Alt gates, Esc-never-quits — have exactly
//! one definition, and a backend only owns the translation surface.
//!
//! [`Action`] stays the pure `App`-only alphabet; a [`Msg`] spans all three
//! subsystems (`App`, `UiState`, and the entry point's own lifecycle/clipboard).

use crate::action::{Action, quick_map};
use crate::app::App;
use crate::layout::Dir;
use crate::ui_state::UiState;

/// A key, as far as the calculator cares. Deliberately smaller than either
/// backend's key type: anything without a meaning here arrives as
/// [`KeyCode::Other`] and resolves to no [`Msg`].
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum KeyCode {
    /// A printable character. Shifted letters arrive already uppercased, which
    /// is why there's no `shift` flag on [`Key`].
    Char(char),
    Enter,
    Backspace,
    Esc,
    Tab,
    Left,
    Right,
    Up,
    Down,
    /// Any key the calculator has no use for (F-keys, Home, …).
    Other,
}

/// A key press with the two modifiers the input rules gate on.
///
/// No `meta`: the web build drops Cmd chords before building a `Key` (they're
/// the browser's — Cmd-C must not clear the expression), and terminals don't
/// report Cmd at all.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct Key {
    pub code: KeyCode,
    pub ctrl: bool,
    pub alt: bool,
}

impl Key {
    /// An unmodified press.
    pub fn new(code: KeyCode) -> Self {
        Self {
            code,
            ctrl: false,
            alt: false,
        }
    }
}

/// Everything an input can mean.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Msg {
    /// A calculator action, run through [`activate`] (focus-follow, flash,
    /// fever climb, and on a successful `=` the drift + reading grace).
    Apply(Action),
    /// Move grid focus one button.
    MoveFocus(Dir),
    /// Activate whatever button has focus (Space), leaving focus put.
    ActivateFocused,
    /// Enter quick-input mode (`i`).
    EnterQuickMode,
    /// Leave quick-input mode (`Esc`).
    LeaveQuickMode,
    /// Switch to the next pad and pin it against resizes (`Tab`).
    CycleLayout,
    /// Drop the pin and resume shape-based pad selection (`a`).
    ResumeAuto,
    /// Flip the palette between dark and light tunings (`t`).
    ToggleTheme,
    /// Copy the result (`y`). **Handled by the entry point**, not [`apply_msg`]:
    /// the clipboard is platform-specific (arboard natively,
    /// `navigator.clipboard` on the web).
    Copy,
    /// Quit (`q`, Ctrl-C). Sets `App::should_quit`, which only the native loop
    /// reads — a browser tab has nothing to quit.
    Quit,
}

/// What a key means, given whether quick-input mode is on.
///
/// The rules, in the order they must be checked (each is a regression test in
/// this module's `tests`):
///
/// 1. **Ctrl-C quits** — before the bare-`c` mapping below, which clears.
/// 2. **Quick-mode** (only when `quick_mode` is on), checked *before* navigation
///    because it reassigns the nav letters:
///    - `Esc` leaves the mode, whatever the modifiers.
///    - With no Ctrl/Alt: a char in [`quick_map`] types that button (resolve the
///      label with [`Action::from_label`]); an *unmapped* nav letter (`h`) is
///      inert — return `None` right there, so it can't fall through to
///      navigation. Arrows are not chars, so they fall through and navigate.
///    - Ctrl/Alt chords fall through, so Ctrl-U never types `4`.
/// 3. **Navigation**: with no Ctrl/Alt, a [`focus_dir`] key moves focus.
/// 4. **Commands and actions** (not modifier-gated): `q` quits, `Tab` cycles the
///    pad, `a`/`A` resumes auto, `t`/`T` toggles the theme, `y`/`Y` copies,
///    `i`/`I` enters quick-mode, Space activates the focused button; anything
///    else goes through [`key_to_action`].
///
/// Returns `None` for a key with no meaning, including the inert in-mode nav
/// letter.
pub fn key_to_msg(key: Key, quick_mode: bool) -> Option<Msg> {
    let Key { code, ctrl, alt } = key;
    // Ctrl/Alt chords are terminal control keys (Ctrl-H = Backspace, Ctrl-U =
    // kill line, Alt-D = kill word, …): they must never type or navigate.
    let bare = !ctrl && !alt;

    // 1. Ctrl-C quits — ahead of rule 4, where a bare `c` clears.
    if ctrl && code == KeyCode::Char('c') {
        return Some(Msg::Quit);
    }

    // 2. Quick-mode reassigns the nav letters, so it runs before navigation.
    if quick_mode {
        // Esc ignores modifiers: it's the one way out of the mode, and some
        // terminals send Alt-Esc for a fast Esc-then-key.
        if code == KeyCode::Esc {
            return Some(Msg::LeaveQuickMode);
        }
        if bare && let KeyCode::Char(ch) = code {
            if let Some(action) = quick_map(ch).and_then(Action::from_label) {
                return Some(Msg::Apply(action));
            }
            // An unmapped nav *letter* (`h`) goes inert: `j k l` type digits
            // here, so letting `h` still move focus would make one row of keys
            // behave two ways. Arrows aren't chars, so they reach rule 3 and keep
            // navigating, as in vim's insert mode.
            if focus_dir(code).is_some() {
                return None;
            }
        }
        // Everything else falls through with its normal meaning.
    }

    // 3. Navigation, gated on no Ctrl/Alt so control chords aren't swallowed.
    // Shift is fine: that's how uppercase HJKL arrive.
    if bare && let Some(dir) = focus_dir(code) {
        return Some(Msg::MoveFocus(dir));
    }

    // 4. Commands, then calculator actions. Not modifier-gated, as before.
    match code {
        KeyCode::Char('q') => Some(Msg::Quit),
        KeyCode::Tab => Some(Msg::CycleLayout),
        KeyCode::Char('a' | 'A') => Some(Msg::ResumeAuto),
        KeyCode::Char('t' | 'T') => Some(Msg::ToggleTheme),
        KeyCode::Char('y' | 'Y') => Some(Msg::Copy),
        // Only reachable with the mode off: in-mode, rule 2 claims `i` as `5`.
        KeyCode::Char('i' | 'I') => Some(Msg::EnterQuickMode),
        KeyCode::Char(' ') => Some(Msg::ActivateFocused),
        _ => key_to_action(code).map(Msg::Apply),
    }
}

/// Run a [`Msg`] against the calculator and UI state.
///
/// Total over [`Msg`], no catch-all. [`Msg::Copy`] is a deliberate no-op here:
/// each entry point intercepts it first, because writing the clipboard is
/// platform code that can't live in this crate.
pub fn apply_msg(app: &mut App, ui: &mut UiState, msg: Msg) {
    match msg {
        Msg::Apply(action) => activate(app, ui, action),
        Msg::MoveFocus(dir) => ui.move_focus(dir),
        // The focused cell is always a real grid label, so `from_label`
        // resolves it.
        Msg::ActivateFocused => {
            if let Some(action) = Action::from_label(ui.focused_label()) {
                activate(app, ui, action);
            }
        }
        Msg::EnterQuickMode => ui.set_quick_mode(true),
        Msg::LeaveQuickMode => ui.set_quick_mode(false),
        Msg::CycleLayout => ui.cycle_layout(),
        Msg::ResumeAuto => ui.resume_auto(),
        Msg::ToggleTheme => ui.toggle_theme(),
        Msg::Copy => {}
        Msg::Quit => app.should_quit = true,
    }
}

/// Apply an `action`, then make focus follow it and flash its cell. The single
/// funnel for every activation so feedback is uniform across keyboard, grid,
/// and mouse. `action.label()` names the grid cell to flash.
pub fn activate(app: &mut App, ui: &mut UiState, action: Action) {
    // A new activation is a fresh edit, so drop any lingering "Copied!" status
    // before applying it — that line refers to the previous result.
    ui.clear_status();
    app.apply(action);
    ui.register_press(action.label());
    // Fever climb: every activation bumps the meter. `register_press_fever`
    // catches up any outstanding decay first, so an occasional press after a
    // long idle doesn't eat its own climb into the decay interval. Ordered
    // after `register_press` for symmetry with `register_drift` below (both
    // are side effects of the same trigger).
    ui.register_press_fever();
    // A *successful* evaluation additionally sweeps the palette and starts a
    // reading-grace on the fever meter. `copy_text` is the existing "is there
    // a result?" question — `Some` only in `Mode::Evaluated`, so a syntax error
    // or an `=` on an empty expression leaves everything alone, and neither
    // effect can claim a success that didn't happen. Read-only, so nothing
    // leaks back into `App`.
    //
    // Ordered *after* `register_press`, which clears the previous trigger's
    // effects: the flash, the ripple and the drift all belong to this one press.
    if matches!(action, Action::Equals) && app.copy_text().is_some() {
        ui.register_drift();
        ui.register_grace();
    }
}

/// Enter pasted text: the native bracketed paste and the web `paste` event both
/// land here, so the two can't drift apart.
///
/// Deliberately *not* routed through [`activate`]: a paste is one logical edit,
/// so there's no per-char focus move or press flash, and fever is tied to
/// typing *pace*, not input volume, so it neither climbs the meter nor starts
/// the post-`=` reading grace. The status line is cleared first, like any new
/// edit: a lingering "Copied!" refers to the previous result.
pub fn paste(app: &mut App, ui: &mut UiState, text: &str) {
    ui.clear_status();
    app.apply_str(text);
}

/// The single keyboard → [`Action`] map. Printable characters resolve via
/// [`Action::from_key`]; Enter and Backspace are handled here because they
/// arrive as their own key codes, not as chars. Returns `None` for keys with
/// no calculator action (navigation, Space, quit) — [`key_to_msg`] routes those
/// before this is reached.
pub fn key_to_action(code: KeyCode) -> Option<Action> {
    match code {
        KeyCode::Enter => Some(Action::Equals),
        KeyCode::Backspace => Some(Action::Backspace),
        KeyCode::Char(ch) => Action::from_key(ch),
        _ => None,
    }
}

/// Maps a navigation key to the direction it moves focus. Accepts both vim HJKL
/// (either case) and the arrow keys; everything else is `None`.
///
/// A [`Dir`] rather than a `(row, col)` delta pair: focus moves one *button* per
/// press, which [`UiState::move_focus`] resolves by walking the lattice a cell at
/// a time — so only unit, single-axis steps are meaningful.
pub fn focus_dir(code: KeyCode) -> Option<Dir> {
    match code {
        KeyCode::Left | KeyCode::Char('h') | KeyCode::Char('H') => Some(Dir::Left),
        KeyCode::Down | KeyCode::Char('j') | KeyCode::Char('J') => Some(Dir::Down),
        KeyCode::Up | KeyCode::Char('k') | KeyCode::Char('K') => Some(Dir::Up),
        KeyCode::Right | KeyCode::Char('l') | KeyCode::Char('L') => Some(Dir::Right),
        _ => None,
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::ui_state::{ColorMode, EffectKind, Theme};

    /// Whether a hue drift is currently in flight.
    fn drifting(ui: &UiState) -> bool {
        ui.effects()
            .iter()
            .any(|e| matches!(e.kind(), EffectKind::Drift))
    }

    /// Run one key through the shared path, exactly as an entry point would
    /// (minus the clipboard: `Copy` is the entry point's, so it's a no-op here).
    fn send(app: &mut App, ui: &mut UiState, key: Key) {
        if let Some(msg) = key_to_msg(key, ui.quick_mode()) {
            apply_msg(app, ui, msg);
        }
    }

    /// Feed one unmodified key press.
    fn press(app: &mut App, ui: &mut UiState, code: KeyCode) {
        send(app, ui, Key::new(code));
    }

    fn ctrl(code: KeyCode) -> Key {
        Key {
            ctrl: true,
            ..Key::new(code)
        }
    }

    fn alt(code: KeyCode) -> Key {
        Key {
            alt: true,
            ..Key::new(code)
        }
    }

    // ---- activate: the shared funnel -------------------------------------

    #[test]
    fn drift_fires_only_on_a_successful_evaluation() {
        // 2+2= evaluates, so the palette sweeps.
        let (mut app, mut ui) = (App::new(), UiState::new());
        app.apply_str("2+2");
        activate(&mut app, &mut ui, Action::Equals);
        assert!(drifting(&ui), "a successful = should sweep the palette");

        // `=` on an empty expression produces no result, so nothing sweeps —
        // the effect must not claim a success that didn't happen.
        let (mut app, mut ui) = (App::new(), UiState::new());
        activate(&mut app, &mut ui, Action::Equals);
        assert!(!drifting(&ui));

        // An incomplete expression is an error, not a result.
        let (mut app, mut ui) = (App::new(), UiState::new());
        app.apply_str("2+");
        activate(&mut app, &mut ui, Action::Equals);
        assert!(!drifting(&ui));
    }

    #[test]
    fn a_plain_keypress_flashes_without_drifting() {
        // Only `=` sweeps; an ordinary digit gets the flash and ripple alone.
        let (mut app, mut ui) = (App::new(), UiState::new());
        activate(&mut app, &mut ui, Action::from_label("5").expect("a digit"));
        assert!(!drifting(&ui));
        assert!(!ui.effects().is_empty(), "but it does flash");
    }

    #[test]
    fn drift_survives_the_press_that_started_it() {
        // Ordering guard: `register_press` clears the previous trigger's effects,
        // so registering the drift before it would silently throw the drift away.
        // All three effects belong to the one `=` press.
        let (mut app, mut ui) = (App::new(), UiState::new());
        app.apply_str("6×7");
        activate(&mut app, &mut ui, Action::Equals);
        assert!(drifting(&ui));
        assert!(
            ui.effects()
                .iter()
                .any(|e| matches!(e.kind(), EffectKind::Press { .. })),
            "the = key should still flash"
        );
    }

    // ---- key_to_action / focus_dir ---------------------------------------

    #[test]
    fn key_to_action_maps_enter_and_backspace() {
        // Enter and Backspace arrive as their own key codes (not chars), so the
        // keyboard map handles them directly: Enter evaluates, Backspace deletes.
        assert_eq!(key_to_action(KeyCode::Enter), Some(Action::Equals));
        assert_eq!(key_to_action(KeyCode::Backspace), Some(Action::Backspace));
    }

    #[test]
    fn key_to_action_delegates_chars_to_from_key() {
        // Printable chars defer to Action::from_key (covered exhaustively in
        // action.rs); this just checks the delegation is wired up.
        assert_eq!(key_to_action(KeyCode::Char('5')), Action::from_key('5'));
        assert_eq!(key_to_action(KeyCode::Char('*')), Some(Action::Op('*')));
    }

    #[test]
    fn key_to_action_ignores_non_action_keys() {
        // Navigation, Space, and quit keys have no calculator action — they're
        // routed before key_to_action is reached, so it returns None for them.
        assert_eq!(key_to_action(KeyCode::Left), None);
        assert_eq!(key_to_action(KeyCode::Char(' ')), None);
        assert_eq!(key_to_action(KeyCode::Char('q')), None);
        assert_eq!(key_to_action(KeyCode::Esc), None);
    }

    #[test]
    fn nav_keys_map_to_directions() {
        // Left/H, Down/J, Up/K, Right/L — vim and arrows, both cases.
        assert_eq!(focus_dir(KeyCode::Left), Some(Dir::Left));
        assert_eq!(focus_dir(KeyCode::Char('h')), Some(Dir::Left));
        assert_eq!(focus_dir(KeyCode::Char('H')), Some(Dir::Left));
        assert_eq!(focus_dir(KeyCode::Down), Some(Dir::Down));
        assert_eq!(focus_dir(KeyCode::Char('j')), Some(Dir::Down));
        assert_eq!(focus_dir(KeyCode::Up), Some(Dir::Up));
        assert_eq!(focus_dir(KeyCode::Char('k')), Some(Dir::Up));
        assert_eq!(focus_dir(KeyCode::Right), Some(Dir::Right));
        assert_eq!(focus_dir(KeyCode::Char('l')), Some(Dir::Right));
    }

    #[test]
    fn non_nav_keys_have_no_direction() {
        // Digits, operators, and other keys must fall through to activation,
        // not be swallowed as navigation.
        assert_eq!(focus_dir(KeyCode::Char('5')), None);
        assert_eq!(focus_dir(KeyCode::Char('+')), None);
        assert_eq!(focus_dir(KeyCode::Enter), None);
        assert_eq!(focus_dir(KeyCode::Char(' ')), None);
    }

    // ---- key_to_msg: the rules, one test each -----------------------------

    #[test]
    fn ctrl_c_quits_but_bare_c_clears() {
        // Rule 1 is checked first: the bare-`c` mapping below would clear.
        assert_eq!(key_to_msg(ctrl(KeyCode::Char('c')), false), Some(Msg::Quit));
        assert_eq!(key_to_msg(ctrl(KeyCode::Char('c')), true), Some(Msg::Quit));
        assert_eq!(
            key_to_msg(Key::new(KeyCode::Char('c')), false),
            Some(Msg::Apply(Action::Clear))
        );
    }

    #[test]
    fn command_keys_map_to_their_messages() {
        let k = |c| key_to_msg(Key::new(c), false);
        assert_eq!(k(KeyCode::Char('q')), Some(Msg::Quit));
        assert_eq!(k(KeyCode::Tab), Some(Msg::CycleLayout));
        assert_eq!(k(KeyCode::Char('a')), Some(Msg::ResumeAuto));
        assert_eq!(k(KeyCode::Char('A')), Some(Msg::ResumeAuto));
        assert_eq!(k(KeyCode::Char('t')), Some(Msg::ToggleTheme));
        assert_eq!(k(KeyCode::Char('T')), Some(Msg::ToggleTheme));
        assert_eq!(k(KeyCode::Char('y')), Some(Msg::Copy));
        assert_eq!(k(KeyCode::Char('Y')), Some(Msg::Copy));
        assert_eq!(k(KeyCode::Char('i')), Some(Msg::EnterQuickMode));
        assert_eq!(k(KeyCode::Char('I')), Some(Msg::EnterQuickMode));
        assert_eq!(k(KeyCode::Char(' ')), Some(Msg::ActivateFocused));
        assert_eq!(k(KeyCode::Enter), Some(Msg::Apply(Action::Equals)));
    }

    #[test]
    fn keys_without_a_meaning_map_to_nothing() {
        assert_eq!(key_to_msg(Key::new(KeyCode::Other), false), None);
        assert_eq!(key_to_msg(Key::new(KeyCode::Char('z')), false), None);
        // Esc means "leave quick-mode" and nothing else — outside the mode it's
        // inert, never a quit.
        assert_eq!(key_to_msg(Key::new(KeyCode::Esc), false), None);
    }

    #[test]
    fn bare_nav_key_moves_focus() {
        // Sanity baseline for the modifier gate below: an unmodified nav key
        // still navigates.
        let (mut app, mut ui) = (App::new(), UiState::new()); // focus on "=" at (4, 3)
        press(&mut app, &mut ui, KeyCode::Char('h'));
        assert_eq!(ui.focus(), (4, 2)); // moved left
    }

    #[test]
    fn ctrl_nav_key_is_not_navigation() {
        // Ctrl-H (and friends) must not be swallowed as "move focus left" — the
        // Ctrl/Alt gate lets control chords keep their terminal meaning. Here
        // Ctrl-H has no calculator action, so focus must stay put.
        let (mut app, mut ui) = (App::new(), UiState::new()); // focus at (4, 3)
        send(&mut app, &mut ui, ctrl(KeyCode::Char('h')));
        assert_eq!(ui.focus(), (4, 3)); // unchanged
    }

    #[test]
    fn tab_cycles_layout() {
        let (mut app, mut ui) = (App::new(), UiState::new());
        assert_eq!(ui.layout_index(), 0);
        press(&mut app, &mut ui, KeyCode::Tab);
        assert_eq!(ui.layout_index(), 1);
    }

    #[test]
    fn a_key_resumes_auto() {
        // `a` is the counterpart to Tab: it clears the manual override.
        let (mut app, mut ui) = (App::new(), UiState::new());
        press(&mut app, &mut ui, KeyCode::Tab);
        assert_eq!(ui.override_layout(), Some(1));
        press(&mut app, &mut ui, KeyCode::Char('a'));
        assert_eq!(ui.override_layout(), None);
    }

    #[test]
    fn r_key_is_inert_after_fever_took_over() {
        // The `r` toggle used to flip mono/rainbow. Fever stage drives that
        // now, so `r` should fall through as an unmapped key — not navigate,
        // not quit, not somehow still re-color.
        let (mut app, mut ui) = (App::new(), UiState::new());
        assert_eq!(ui.color_mode(), ColorMode::Mono); // startup: stage One = mono
        press(&mut app, &mut ui, KeyCode::Char('r'));
        assert_eq!(ui.color_mode(), ColorMode::Mono); // unchanged
        assert!(!app.should_quit); // and no stray quit
    }

    #[test]
    fn t_key_toggles_theme() {
        let (mut app, mut ui) = (App::new(), UiState::new());
        assert_eq!(ui.theme(), Theme::Dark);
        press(&mut app, &mut ui, KeyCode::Char('t'));
        assert_eq!(ui.theme(), Theme::Light);
    }

    #[test]
    fn copy_is_left_to_the_entry_point() {
        // `apply_msg` can't write a clipboard, so `Copy` must change nothing —
        // even with a result on screen. The entry point intercepts it.
        let (mut app, mut ui) = (App::new(), UiState::new());
        app.apply_str("2+2");
        activate(&mut app, &mut ui, Action::Equals);
        let before = app.display_lines();
        apply_msg(&mut app, &mut ui, Msg::Copy);
        assert_eq!(app.display_lines(), before);
        assert_eq!(ui.status_text(), None);
    }

    // ---- quick-input mode ------------------------------------------------

    #[test]
    fn i_enters_quick_mode_and_esc_leaves_it() {
        let (mut app, mut ui) = (App::new(), UiState::new());
        assert!(!ui.quick_mode()); // off at launch
        press(&mut app, &mut ui, KeyCode::Char('i'));
        assert!(ui.quick_mode());
        press(&mut app, &mut ui, KeyCode::Esc);
        assert!(!ui.quick_mode());
        assert!(!app.should_quit); // Esc left the mode, it did not quit
    }

    #[test]
    fn esc_never_quits_and_double_tapping_it_is_safe() {
        // Esc is not a quit key at all. The case that forced this: a vim user
        // taps Esc twice to be sure they've left insert — the first leaves
        // quick-mode, and the second must not take the expression down with it.
        let (mut app, mut ui) = (App::new(), UiState::new());
        press(&mut app, &mut ui, KeyCode::Esc); // bare Esc, mode never entered
        assert!(!app.should_quit);

        press(&mut app, &mut ui, KeyCode::Char('i'));
        press(&mut app, &mut ui, KeyCode::Char('k')); // types 2
        press(&mut app, &mut ui, KeyCode::Esc); // leaves the mode
        press(&mut app, &mut ui, KeyCode::Esc); // the reflex second tap
        assert!(!ui.quick_mode());
        assert!(!app.should_quit);
        assert_eq!(app.display_lines().1, "2"); // work intact
    }

    #[test]
    fn quick_mode_turns_the_nav_letters_into_digits() {
        // The crux of the feature: in-mode `j k l` type 1 2 3 instead of moving
        // focus, and `i` types 5 rather than re-entering the mode.
        let (mut app, mut ui) = (App::new(), UiState::new());
        press(&mut app, &mut ui, KeyCode::Char('i'));
        for c in ['j', 'k', 'l', 'i'] {
            press(&mut app, &mut ui, KeyCode::Char(c));
        }
        assert_eq!(app.display_lines().1, "1235");
        assert!(ui.quick_mode()); // still on — only Esc leaves
    }

    #[test]
    fn quick_mode_enters_operators_as_display_glyphs() {
        // `a s d f` route through `from_label`, so `d` must apply the *eval*
        // multiply while displaying `×` — the same round-trip paste relies on.
        let (mut app, mut ui) = (App::new(), UiState::new());
        press(&mut app, &mut ui, KeyCode::Char('i'));
        for c in ['k', 'd', 'j', 'm'] {
            press(&mut app, &mut ui, KeyCode::Char(c));
        }
        assert_eq!(app.display_lines().1, "2×10");
        press(&mut app, &mut ui, KeyCode::Enter);
        assert_eq!(app.display_lines().1, "20");
    }

    #[test]
    fn nav_letters_still_navigate_outside_quick_mode() {
        // The two interpretations stay disjoint: the same `k` that types 2
        // in-mode moves focus up when the mode is off.
        let (mut app, mut ui) = (App::new(), UiState::new()); // focus on "=" at (4, 3)
        press(&mut app, &mut ui, KeyCode::Char('k'));
        assert_eq!(ui.focus(), (3, 3)); // moved up
        assert_eq!(app.display_lines().1, ""); // typed nothing
    }

    #[test]
    fn quick_mode_silences_nav_letters_but_not_arrows() {
        // `h` has no quick mapping (nothing sits left of `1` on a numpad). It must
        // go inert rather than navigate, or its row would behave two ways at once —
        // while the arrow keys keep working, as they do in vim's insert mode.
        let (mut app, mut ui) = (App::new(), UiState::new());
        press(&mut app, &mut ui, KeyCode::Char('i'));
        assert_eq!(key_to_msg(Key::new(KeyCode::Char('h')), true), None);
        press(&mut app, &mut ui, KeyCode::Char('h'));
        assert_eq!(ui.focus(), (4, 3)); // unmoved
        assert_eq!(app.display_lines().1, ""); // and typed nothing
        press(&mut app, &mut ui, KeyCode::Left);
        assert_eq!(ui.focus(), (4, 2)); // arrows still navigate
    }

    #[test]
    fn quick_mode_ignores_ctrl_and_alt_chords() {
        // A quick key is the *bare* letter. Ctrl-U (kill line), Ctrl-L (redraw),
        // and Alt-D (kill word) are terminal chords a user fires by reflex; typing
        // `4`, `3`, and `×` for them would corrupt the expression, so the mode is
        // gated on no Ctrl/Alt exactly like the navigation block.
        let (mut app, mut ui) = (App::new(), UiState::new());
        press(&mut app, &mut ui, KeyCode::Char('i'));
        for key in [
            ctrl(KeyCode::Char('u')),
            ctrl(KeyCode::Char('l')),
            alt(KeyCode::Char('d')),
        ] {
            send(&mut app, &mut ui, key);
        }
        assert_eq!(app.display_lines().1, "");
        assert!(ui.quick_mode()); // and the mode is untouched
    }

    #[test]
    fn esc_leaves_quick_mode_even_with_a_modifier() {
        // Esc is checked before the Ctrl/Alt gate, so Alt-Esc (what some
        // terminals send for a quick Esc-then-key) still leaves the mode.
        assert_eq!(
            key_to_msg(alt(KeyCode::Esc), true),
            Some(Msg::LeaveQuickMode)
        );
    }

    #[test]
    fn unmapped_keys_keep_their_normal_meaning_in_quick_mode() {
        // Quick-mode only reassigns the keys in the map; everything else falls
        // through. `c` still clears and `q` still quits.
        let (mut app, mut ui) = (App::new(), UiState::new());
        press(&mut app, &mut ui, KeyCode::Char('i'));
        press(&mut app, &mut ui, KeyCode::Char('k')); // types 2
        press(&mut app, &mut ui, KeyCode::Char('c')); // clears
        assert_eq!(app.display_lines().1, "");
        press(&mut app, &mut ui, KeyCode::Char('q'));
        assert!(app.should_quit);
    }

    #[test]
    fn quick_input_follows_focus_and_flashes_like_any_activation() {
        // Quick keys go through the shared `activate` funnel, so they get the same
        // feedback a click or a normal keypress does.
        let (mut app, mut ui) = (App::new(), UiState::new());
        press(&mut app, &mut ui, KeyCode::Char('i'));
        press(&mut app, &mut ui, KeyCode::Char('o')); // the "6" button
        let six = ui.keypad().position_of("6").expect("6 is on the pad");
        assert_eq!(ui.focus(), six);
        let idx = ui.keypad().button_index_at(six.0, six.1);
        assert!(ui.is_button_pressed(idx));
    }

    #[test]
    fn paste_is_one_edit_without_fever_or_effects() {
        // The shared paste path for native and web: text lands as one edit, a
        // stale status is cleared, and nothing that belongs to *typing* fires —
        // no fever climb, no press flash or ripple, no focus move, and no
        // drift/grace even when the paste ends in a successful `=`.
        let (mut app, mut ui) = (App::new(), UiState::new());
        ui.set_status("Copied!".to_string());
        let focus = ui.focus();
        paste(&mut app, &mut ui, "(1+2)×3=");
        assert_eq!(app.display_lines().1, "9");
        assert_eq!(ui.status_text(), None);
        assert_eq!(ui.fever_score(), 0.0);
        assert!(ui.effects().is_empty());
        assert_eq!(ui.focus(), focus);
    }
}
