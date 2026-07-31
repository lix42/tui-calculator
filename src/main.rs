mod action;
mod app;
mod eval;
mod layout;
mod ui;
mod ui_state;

use std::io::{self, Result, Stdout};
use std::time::Duration;

use crossterm::event::{
    self, DisableBracketedPaste, DisableMouseCapture, EnableBracketedPaste, EnableMouseCapture,
    Event, KeyCode, KeyEventKind, KeyModifiers, MouseButton, MouseEventKind,
};
use crossterm::execute;
use crossterm::terminal::{
    EnterAlternateScreen, LeaveAlternateScreen, disable_raw_mode, enable_raw_mode,
};
use ratatui::Terminal;
use ratatui::backend::CrosstermBackend;

use action::{Action, quick_map};
use app::App;
use layout::Dir;
use ui_state::UiState;

type Tui = Terminal<CrosstermBackend<Stdout>>;

fn setup_terminal() -> Result<Tui> {
    enable_raw_mode()?;
    let mut stdout = io::stdout();
    execute!(
        stdout,
        EnterAlternateScreen,
        EnableMouseCapture,
        EnableBracketedPaste
    )?;
    Terminal::new(CrosstermBackend::new(stdout))
}

fn restore_terminal(terminal: &mut Tui) -> Result<()> {
    // Reverse of setup: drop mouse capture and bracketed paste *before* leaving
    // alt screen.
    execute!(
        terminal.backend_mut(),
        DisableBracketedPaste,
        DisableMouseCapture,
        LeaveAlternateScreen
    )?;
    disable_raw_mode()?;
    terminal.show_cursor()?;
    Ok(())
}

/// Restore the terminal on panic so the user lands back in a cooked shell
/// instead of a frozen raw-mode terminal.
fn install_panic_hook() {
    let original = std::panic::take_hook();
    std::panic::set_hook(Box::new(move |info| {
        let _ = execute!(
            io::stdout(),
            DisableBracketedPaste,
            DisableMouseCapture,
            LeaveAlternateScreen
        );
        let _ = disable_raw_mode();
        original(info);
    }));
}

fn run(terminal: &mut Tui, app: &mut App, ui: &mut UiState) -> Result<()> {
    // Launch on the default pad (the 5×4 standard) rather than seeding a
    // shape-appropriate one from the initial terminal size. Auto-selection still
    // adapts the pad on `Event::Resize`; the standard pad is just the launch default.
    while !app.should_quit {
        // Expire any press flash before drawing; the 100ms poll below paces
        // this, so a flash clears ~1-2 ticks after the key (a brief blink).
        ui.tick();
        terminal.draw(|frame| ui::draw(frame, app, ui))?;
        if event::poll(Duration::from_millis(100))? {
            handle_event(event::read()?, app, ui);
        }
    }
    Ok(())
}

/// Dispatches a single terminal event to the app.
///
/// Navigation (HJKL / arrows) moves the grid focus. Every key that *activates*
/// a button goes through `activate`, so focus follows the input and the button
/// flashes — keyboard, the button grid, and (later) the mouse share one path.
fn handle_event(event: Event, app: &mut App, ui: &mut UiState) {
    // A left-click resolves to a grid cell (if any) and activates it through the
    // same funnel as the keyboard, so the click gets focus-follow and the press
    // flash. Clicks that miss every button are ignored.
    if let Event::Mouse(mouse) = event {
        if let MouseEventKind::Down(MouseButton::Left) = mouse.kind {
            // The copy affordance sits in the display area, outside the grid, so
            // it's checked before the button hit-test.
            if ui.copy_hit(mouse.column, mouse.row) {
                do_copy(app, ui);
            } else if let Some(i) = ui.button_at(mouse.column, mouse.row)
                && let Some(action) = Action::from_label(ui.button_label(i))
            {
                activate(app, ui, action);
            }
        }
        return;
    }
    // A terminal resize re-picks the shape-appropriate pad (unless the user has
    // pinned one with Tab). Like copy and focus moves, it's a UI-only side effect
    // routed here at the I/O boundary, not an `Action`. crossterm reports the new
    // size as (columns, rows).
    if let Event::Resize(cols, rows) = event {
        ui.auto_select(cols, rows);
        return;
    }
    // A bracketed paste arrives as one (or, for large pastes, more than one)
    // `Event::Paste` carrying the pasted text. It routes through
    // `App::apply_str`, not `activate`, so the paste is one logical edit — no
    // per-character focus move or press flash.
    if let Event::Paste(text) = event {
        // A paste is a fresh edit, so drop any lingering "Copied!" from the last
        // result before it's applied.
        ui.clear_status();
        app.apply_str(&text);
        return;
    }
    if let Event::Key(key) = event
        && key.kind == KeyEventKind::Press
    {
        // Ctrl-C quits — checked before the bare-`c` mapping below, which clears.
        if key.code == KeyCode::Char('c') && key.modifiers.contains(KeyModifiers::CONTROL) {
            app.should_quit = true;
            return;
        }
        // Quick-input mode, checked before navigation because it *reassigns* the
        // nav letters: while it's on, `j k l` type digits (see `action::QUICK_MAP`).
        // A mode rather than a held modifier because terminals emit no
        // "modifier went down" event, and macOS terminals may swallow Alt-chords
        // into composed characters before the app ever sees a modifier.
        if ui.quick_mode() {
            // Esc leaves the mode. It no longer quits anywhere (see the match
            // below): entering on `i` invites the vim reflex of tapping Esc twice
            // to be sure, and a second Esc that quit would discard the expression
            // — the exact mishap this mode's key choice courts.
            if key.code == KeyCode::Esc {
                ui.set_quick_mode(false);
                return;
            }
            // Gated on no Ctrl/Alt for the same reason the nav block below is:
            // terminal control chords (Ctrl-U = kill line, Ctrl-L = redraw, Alt-D
            // = kill word, …) arrive as the plain char plus a modifier, and a
            // quick key that ignored the modifier would turn Ctrl-U into a `4`.
            if !key
                .modifiers
                .intersects(KeyModifiers::CONTROL | KeyModifiers::ALT)
            {
                if let KeyCode::Char(ch) = key.code
                    && let Some(action) = quick_map(ch).and_then(Action::from_label)
                {
                    activate(app, ui, action);
                    return;
                }
                // The vim nav *letters* go inert here: `j k l` already type digits
                // above, so letting `h` still move focus would make one row of keys
                // behave two different ways. The arrow keys fall through and keep
                // navigating (exactly as they do in vim's insert mode), so focus is
                // never stranded. Every other unmapped key keeps its normal meaning.
                if matches!(key.code, KeyCode::Char(_)) && focus_dir(key.code).is_some() {
                    return;
                }
            }
        }
        // HJKL / arrows move focus only — no activation, no flash. Gated on no
        // Ctrl/Alt so terminal control chords (Ctrl-H = Backspace, Ctrl-L =
        // redraw, …) aren't swallowed as navigation. Shift is allowed — that's
        // how the uppercase HJKL variants arrive.
        if !key
            .modifiers
            .intersects(KeyModifiers::CONTROL | KeyModifiers::ALT)
            && let Some(dir) = focus_dir(key.code)
        {
            ui.move_focus(dir);
            return;
        }
        match key.code {
            // `q` and Ctrl-C are the only ways out. Esc is deliberately *not* a
            // quit key: it means "leave quick-mode", and one key that sometimes
            // exits the app and sometimes exits a mode is a trap when the app is
            // modal — the same one-job-each rule Tab (pin) and `a` (un-pin) follow.
            // Outside quick-mode Esc falls through here and is simply inert.
            KeyCode::Char('q') => app.should_quit = true,
            // Tab switches to the next keypad. Like copy and focus moves, it's a
            // UI-only side effect (no calculator state changes), so it's routed
            // here at the I/O boundary rather than through an `Action`.
            KeyCode::Tab => ui.cycle_layout(),
            // Clear a manual pad override and resume automatic shape-based
            // selection for the current terminal size. The counterpart to Tab:
            // Tab pins, `a` un-pins.
            KeyCode::Char('a') | KeyCode::Char('A') => ui.resume_auto(),
            // Toggle per-digit rainbow coloring. Like the pad switch, it's a
            // rendering-only side effect (no calculator state changes), so it's
            // routed here at the I/O boundary rather than through an `Action`.
            KeyCode::Char('r') | KeyCode::Char('R') => ui.toggle_color_mode(),
            // Switch the rainbow palette between dark- and light-background
            // tunings. Rendering-only, like the rainbow toggle above.
            KeyCode::Char('t') | KeyCode::Char('T') => ui.toggle_theme(),
            // Copy the result to the clipboard (vim-style yank; Ctrl-C is taken
            // by quit in raw mode). A no-op unless a result is on screen.
            KeyCode::Char('y') | KeyCode::Char('Y') => do_copy(app, ui),
            // Enter quick-input mode (vim's "insert"), leaving `hjkl` free to type
            // rather than navigate. Only reachable when the mode is *off* — the
            // block above claims `i` for the digit `5` once it's on.
            KeyCode::Char('i') | KeyCode::Char('I') => ui.set_quick_mode(true),
            // Space activates whatever is focused, leaving focus put so it can
            // be re-pressed in place. The focused cell is always a real grid
            // label, so `from_label` resolves it.
            KeyCode::Char(' ') => {
                if let Some(action) = Action::from_label(ui.focused_label()) {
                    activate(app, ui, action);
                }
            }
            // Everything else that maps to a calculator action — digits/operators,
            // plus Enter and Backspace — goes through the single keyboard map.
            _ => {
                if let Some(action) = key_to_action(key.code) {
                    activate(app, ui, action);
                }
            }
        }
    }
}

/// Apply an `action`, then make focus follow it and flash its cell. The single
/// funnel for every activation so feedback is uniform across keyboard, grid,
/// and mouse. `action.label()` names the grid cell to flash.
fn activate(app: &mut App, ui: &mut UiState, action: Action) {
    // A new activation is a fresh edit, so drop any lingering "Copied!" status
    // before applying it — that line refers to the previous result.
    ui.clear_status();
    app.apply(action);
    ui.register_press(action.label());
}

/// Copy the current result to the system clipboard, then show a status message.
///
/// Copy is *not* an [`Action`]: it's a side-effecting command on the result, not
/// a calculator state transition, so it stays out of `App::apply`'s pure, total
/// match (and out of the crossterm-free `action.rs`). Like quit and focus moves,
/// it's routed here, at the I/O boundary that already owns the terminal.
///
/// A no-op (no status) when there's nothing to copy — `app.copy_text()` is
/// `None` while editing or after an error, so pressing `y` then does nothing.
fn do_copy(app: &App, ui: &mut UiState) {
    let Some(text) = app.copy_text() else {
        return;
    };
    // Carry the real error into the status: a TUI has no log, so this line is the
    // only place the cause can surface. "no clipboard" (headless/SSH, permanent)
    // and "clipboard busy" (transient) ask for different responses, and
    // `arboard::Error`'s `Display` distinguishes them.
    let status = match copy_to_clipboard(&text) {
        Ok(()) => "Copied!".to_string(),
        Err(e) => format!("Copy failed: {e}"),
    };
    ui.set_status(status);
}

thread_local! {
    /// A clipboard handle reused for the whole session.
    ///
    /// On Linux (X11 and Wayland) arboard serves the copied text *from the live
    /// `Clipboard` instance* — drop it and the contents can vanish before another
    /// app reads them, so a fresh-per-copy handle would let `set_text` report
    /// success while the paste silently fails. Holding one instance for the
    /// process lifetime keeps the text available while the app runs. macOS and
    /// Windows hand the text to the OS, so reusing the handle is simply cheaper.
    ///
    /// The TUI is single-threaded, so a `thread_local` is effectively a
    /// process-global without needing `Clipboard: Sync`. Lazily built on first
    /// copy; a failed build leaves the slot empty so the next copy retries.
    static CLIPBOARD: std::cell::RefCell<Option<arboard::Clipboard>> =
        const { std::cell::RefCell::new(None) };
}

/// Place `text` on the system clipboard, using the session-long handle above.
///
/// NOTE: even with a persistent handle, on Linux the text is served by this
/// process, so it may not survive the app exiting unless a clipboard manager is
/// running to take ownership. macOS and Windows persist it after exit.
fn copy_to_clipboard(text: &str) -> std::result::Result<(), arboard::Error> {
    CLIPBOARD.with_borrow_mut(|slot| {
        if slot.is_none() {
            *slot = Some(arboard::Clipboard::new()?);
        }
        // Just populated above on the `None` path, so the handle is present.
        slot.as_mut().expect("clipboard initialized").set_text(text)
    })
}

/// The single keyboard → [`Action`] map. Printable characters resolve via
/// [`Action::from_key`]; Enter and Backspace are handled here because they
/// arrive as their own `KeyCode`s, not as chars. Returns `None` for keys with
/// no calculator action (navigation, Space, quit) — those are routed before
/// this is reached.
fn key_to_action(code: KeyCode) -> Option<Action> {
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
fn focus_dir(code: KeyCode) -> Option<Dir> {
    match code {
        KeyCode::Left | KeyCode::Char('h') | KeyCode::Char('H') => Some(Dir::Left),
        KeyCode::Down | KeyCode::Char('j') | KeyCode::Char('J') => Some(Dir::Down),
        KeyCode::Up | KeyCode::Char('k') | KeyCode::Char('K') => Some(Dir::Up),
        KeyCode::Right | KeyCode::Char('l') | KeyCode::Char('L') => Some(Dir::Right),
        _ => None,
    }
}

fn main() -> Result<()> {
    install_panic_hook();
    let mut terminal = setup_terminal()?;
    let mut app = App::new();
    let mut ui = UiState::new();
    let result = run(&mut terminal, &mut app, &mut ui);
    restore_terminal(&mut terminal)?;
    result
}

#[cfg(test)]
mod tests {
    use super::*;
    use crossterm::event::KeyEvent;

    #[test]
    fn key_to_action_maps_enter_and_backspace() {
        // Enter and Backspace arrive as their own KeyCodes (not chars), so the
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

    #[test]
    fn bare_nav_key_moves_focus() {
        // Sanity baseline for the modifier gate below: an unmodified nav key
        // still navigates.
        let mut app = App::new();
        let mut ui = UiState::new(); // focus starts on "=" at (4, 3)
        handle_event(
            Event::Key(KeyEvent::new(KeyCode::Char('h'), KeyModifiers::NONE)),
            &mut app,
            &mut ui,
        );
        assert_eq!(ui.focus(), (4, 2)); // moved left
    }

    #[test]
    fn tab_cycles_layout() {
        // Tab is routed to the pad switch here, not through an Action.
        let mut app = App::new();
        let mut ui = UiState::new();
        assert_eq!(ui.layout_index(), 0);
        handle_event(
            Event::Key(KeyEvent::new(KeyCode::Tab, KeyModifiers::NONE)),
            &mut app,
            &mut ui,
        );
        assert_eq!(ui.layout_index(), 1);
    }

    #[test]
    fn resize_respects_pinned_override() {
        // Tab pins a pad; a subsequent resize must not move off it. (The auto path
        // itself is unit-tested in ui_state; here we check the Resize event is
        // routed and the override honored.)
        let mut app = App::new();
        let mut ui = UiState::new();
        handle_event(
            Event::Key(KeyEvent::new(KeyCode::Tab, KeyModifiers::NONE)),
            &mut app,
            &mut ui,
        );
        assert_eq!(ui.layout_index(), 1); // pinned to tall
        handle_event(Event::Resize(200, 200), &mut app, &mut ui);
        assert_eq!(ui.layout_index(), 1); // unchanged
        assert_eq!(ui.override_layout(), Some(1));
    }

    #[test]
    fn r_key_toggles_color_mode() {
        // `r` is routed to the rainbow toggle here, not through an Action.
        use ui_state::ColorMode;
        let mut app = App::new();
        let mut ui = UiState::new();
        assert_eq!(ui.color_mode(), ColorMode::Rainbow); // rainbow is the default
        handle_event(
            Event::Key(KeyEvent::new(KeyCode::Char('r'), KeyModifiers::NONE)),
            &mut app,
            &mut ui,
        );
        assert_eq!(ui.color_mode(), ColorMode::Mono);
    }

    #[test]
    fn t_key_toggles_theme() {
        // `t` flips the rainbow palette theme, routed here like `r`.
        use ui_state::Theme;
        let mut app = App::new();
        let mut ui = UiState::new();
        assert_eq!(ui.theme(), Theme::Dark);
        handle_event(
            Event::Key(KeyEvent::new(KeyCode::Char('t'), KeyModifiers::NONE)),
            &mut app,
            &mut ui,
        );
        assert_eq!(ui.theme(), Theme::Light);
    }

    #[test]
    fn a_key_resumes_auto() {
        // `a` is the counterpart to Tab: it clears the manual override.
        let mut app = App::new();
        let mut ui = UiState::new();
        handle_event(
            Event::Key(KeyEvent::new(KeyCode::Tab, KeyModifiers::NONE)),
            &mut app,
            &mut ui,
        );
        assert_eq!(ui.override_layout(), Some(1));
        handle_event(
            Event::Key(KeyEvent::new(KeyCode::Char('a'), KeyModifiers::NONE)),
            &mut app,
            &mut ui,
        );
        assert_eq!(ui.override_layout(), None);
    }

    #[test]
    fn ctrl_nav_key_is_not_navigation() {
        // Ctrl-H (and friends) must not be swallowed as "move focus left" — the
        // Ctrl/Alt gate lets control chords keep their terminal meaning. Here
        // Ctrl-H has no calculator action, so focus must stay put.
        let mut app = App::new();
        let mut ui = UiState::new(); // focus at (4, 3)
        handle_event(
            Event::Key(KeyEvent::new(KeyCode::Char('h'), KeyModifiers::CONTROL)),
            &mut app,
            &mut ui,
        );
        assert_eq!(ui.focus(), (4, 3)); // unchanged
    }

    #[test]
    fn do_copy_is_noop_without_a_result() {
        // While editing there's no result, so `copy_text` is None and `do_copy`
        // returns before touching the clipboard — no status is set. (The success
        // path sets a status but writes to the system clipboard, so it's verified
        // manually rather than here.)
        let mut app = App::new();
        for ch in ['2', '+', '3'] {
            app.apply(Action::from_key(ch).expect("mapped key"));
        }
        assert_eq!(app.copy_text(), None);
        let mut ui = UiState::new();
        do_copy(&app, &mut ui);
        assert_eq!(ui.status_text(), None);
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

    /// Feed one unmodified key press through the event handler.
    fn press(app: &mut App, ui: &mut UiState, code: KeyCode) {
        handle_event(Event::Key(KeyEvent::new(code, KeyModifiers::NONE)), app, ui);
    }

    #[test]
    fn i_enters_quick_mode_and_esc_leaves_it() {
        let mut app = App::new();
        let mut ui = UiState::new();
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
        let mut app = App::new();
        let mut ui = UiState::new();
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
        let mut app = App::new();
        let mut ui = UiState::new();
        press(&mut app, &mut ui, KeyCode::Char('i'));
        for code in ['j', 'k', 'l', 'i'] {
            press(&mut app, &mut ui, KeyCode::Char(code));
        }
        assert_eq!(app.display_lines().1, "1235");
        assert!(ui.quick_mode()); // still on — only Esc leaves
    }

    #[test]
    fn quick_mode_enters_operators_as_display_glyphs() {
        // `a s d f` route through `from_label`, so `d` must apply the *eval*
        // multiply while displaying `×` — the same round-trip paste relies on.
        let mut app = App::new();
        let mut ui = UiState::new();
        press(&mut app, &mut ui, KeyCode::Char('i'));
        for code in ['k', 'd', 'j', 'm'] {
            press(&mut app, &mut ui, KeyCode::Char(code));
        }
        assert_eq!(app.display_lines().1, "2×10");
        press(&mut app, &mut ui, KeyCode::Enter);
        assert_eq!(app.display_lines().1, "20");
    }

    #[test]
    fn nav_letters_still_navigate_outside_quick_mode() {
        // The two interpretations stay disjoint: the same `j` that types 1 in-mode
        // moves focus down when the mode is off.
        let mut app = App::new();
        let mut ui = UiState::new(); // focus starts on "=" at (4, 3)
        press(&mut app, &mut ui, KeyCode::Char('k'));
        assert_eq!(ui.focus(), (3, 3)); // moved up
        assert_eq!(app.display_lines().1, ""); // typed nothing
    }

    #[test]
    fn quick_mode_silences_nav_letters_but_not_arrows() {
        // `h` has no quick mapping (nothing sits left of `1` on a numpad). It must
        // go inert rather than navigate, or its row would behave two ways at once —
        // while the arrow keys keep working, as they do in vim's insert mode.
        let mut app = App::new();
        let mut ui = UiState::new();
        press(&mut app, &mut ui, KeyCode::Char('i'));
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
        let mut app = App::new();
        let mut ui = UiState::new();
        press(&mut app, &mut ui, KeyCode::Char('i'));
        for (code, mods) in [
            (KeyCode::Char('u'), KeyModifiers::CONTROL),
            (KeyCode::Char('l'), KeyModifiers::CONTROL),
            (KeyCode::Char('d'), KeyModifiers::ALT),
        ] {
            handle_event(Event::Key(KeyEvent::new(code, mods)), &mut app, &mut ui);
        }
        assert_eq!(app.display_lines().1, "");
        assert!(ui.quick_mode()); // and the mode is untouched
    }

    #[test]
    fn unmapped_keys_keep_their_normal_meaning_in_quick_mode() {
        // Quick-mode only reassigns the keys in the map; everything else falls
        // through. `c` still clears and `q` still quits.
        let mut app = App::new();
        let mut ui = UiState::new();
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
        let mut app = App::new();
        let mut ui = UiState::new();
        press(&mut app, &mut ui, KeyCode::Char('i'));
        press(&mut app, &mut ui, KeyCode::Char('o')); // the "6" button
        let six = ui.keypad().position_of("6").expect("6 is on the pad");
        assert_eq!(ui.focus(), six);
        let idx = ui.keypad().button_index_at(six.0, six.1);
        assert!(ui.is_button_pressed(idx));
    }
}
