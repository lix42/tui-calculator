mod action;
mod app;
mod eval;
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

use action::Action;
use app::App;
use ui_state::{BUTTONS, UiState};

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
            } else if let Some((r, c)) = ui.button_at(mouse.column, mouse.row)
                && let Some(action) = Action::from_label(BUTTONS[r][c])
            {
                activate(app, ui, action);
            }
        }
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
        // HJKL / arrows move focus only — no activation, no flash. Gated on no
        // Ctrl/Alt so terminal control chords (Ctrl-H = Backspace, Ctrl-L =
        // redraw, …) aren't swallowed as navigation. Shift is allowed — that's
        // how the uppercase HJKL variants arrive.
        if !key
            .modifiers
            .intersects(KeyModifiers::CONTROL | KeyModifiers::ALT)
            && let Some((dr, dc)) = focus_delta(key.code)
        {
            ui.move_focus(dr, dc);
            return;
        }
        match key.code {
            KeyCode::Char('q') | KeyCode::Esc => app.should_quit = true,
            // Copy the result to the clipboard (vim-style yank; Ctrl-C is taken
            // by quit in raw mode). A no-op unless a result is on screen.
            KeyCode::Char('y') | KeyCode::Char('Y') => do_copy(app, ui),
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

/// Maps a navigation key to a `(row_delta, col_delta)` focus move. Accepts both
/// vim HJKL (either case) and the arrow keys; everything else is `None`.
fn focus_delta(code: KeyCode) -> Option<(i32, i32)> {
    match code {
        KeyCode::Left | KeyCode::Char('h') | KeyCode::Char('H') => Some((0, -1)),
        KeyCode::Down | KeyCode::Char('j') | KeyCode::Char('J') => Some((1, 0)),
        KeyCode::Up | KeyCode::Char('k') | KeyCode::Char('K') => Some((-1, 0)),
        KeyCode::Right | KeyCode::Char('l') | KeyCode::Char('L') => Some((0, 1)),
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
    fn nav_keys_map_to_focus_deltas() {
        // Left/H, Down/J, Up/K, Right/L — vim and arrows, both cases.
        assert_eq!(focus_delta(KeyCode::Left), Some((0, -1)));
        assert_eq!(focus_delta(KeyCode::Char('h')), Some((0, -1)));
        assert_eq!(focus_delta(KeyCode::Char('H')), Some((0, -1)));
        assert_eq!(focus_delta(KeyCode::Down), Some((1, 0)));
        assert_eq!(focus_delta(KeyCode::Char('j')), Some((1, 0)));
        assert_eq!(focus_delta(KeyCode::Up), Some((-1, 0)));
        assert_eq!(focus_delta(KeyCode::Char('k')), Some((-1, 0)));
        assert_eq!(focus_delta(KeyCode::Right), Some((0, 1)));
        assert_eq!(focus_delta(KeyCode::Char('l')), Some((0, 1)));
    }

    #[test]
    fn non_nav_keys_have_no_delta() {
        // Digits, operators, and other keys must fall through to activation,
        // not be swallowed as navigation.
        assert_eq!(focus_delta(KeyCode::Char('5')), None);
        assert_eq!(focus_delta(KeyCode::Char('+')), None);
        assert_eq!(focus_delta(KeyCode::Enter), None);
        assert_eq!(focus_delta(KeyCode::Char(' ')), None);
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
        assert!(ui.is_focused((4, 2))); // moved left
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
        assert!(ui.is_focused((4, 3))); // unchanged
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
}
