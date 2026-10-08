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

use calculator_core::action::Action;
use calculator_core::app::App;
use calculator_core::input::{Key, KeyCode as CoreKeyCode, Msg, activate, apply_msg, key_to_msg};
use calculator_core::ui;
use calculator_core::ui_state::UiState;

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
/// Keys are translated to the core's neutral [`Key`] and resolved by
/// [`key_to_msg`] — the one definition of what each key means, shared with the
/// web build — so this function only owns what is genuinely native: crossterm's
/// event shapes, the arboard clipboard, terminal resize, and bracketed paste.
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
    // `Press` only: on Windows crossterm also reports key *releases*, which
    // would double every key. (Held-key repeats arrive as `Press` too, since we
    // never opt into `REPORT_EVENT_TYPES` — so holding ⌫ deletes repeatedly.)
    if let Event::Key(key) = event
        && key.kind == KeyEventKind::Press
    {
        match key_to_msg(to_key(key), ui.quick_mode()) {
            // Copy is platform code, so it's intercepted here rather than in the
            // core's `apply_msg`.
            Some(Msg::Copy) => do_copy(app, ui),
            Some(msg) => apply_msg(app, ui, msg),
            None => {}
        }
    }
}

/// Translate a crossterm key event into the core's neutral [`Key`]. The only
/// crossterm-shaped key code in the program; everything past it is shared.
fn to_key(key: crossterm::event::KeyEvent) -> Key {
    let code = match key.code {
        KeyCode::Char(c) => CoreKeyCode::Char(c),
        KeyCode::Enter => CoreKeyCode::Enter,
        KeyCode::Backspace => CoreKeyCode::Backspace,
        KeyCode::Esc => CoreKeyCode::Esc,
        KeyCode::Tab => CoreKeyCode::Tab,
        KeyCode::Left => CoreKeyCode::Left,
        KeyCode::Right => CoreKeyCode::Right,
        KeyCode::Up => CoreKeyCode::Up,
        KeyCode::Down => CoreKeyCode::Down,
        _ => CoreKeyCode::Other,
    };
    Key {
        code,
        ctrl: key.modifiers.contains(KeyModifiers::CONTROL),
        alt: key.modifiers.contains(KeyModifiers::ALT),
    }
}

/// Copy the current result to the system clipboard, then show a status message.
///
/// Copy is *not* an [`Action`]: it's a side-effecting command on the result, not
/// a calculator state transition, so it stays out of `App::apply`'s pure, total
/// match. It's a [`Msg`], but [`handle_event`] intercepts it before the core's
/// `apply_msg`, because writing the clipboard is platform code.
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
    //! Only what's native lives here: crossterm translation and the event
    //! routing `handle_event` owns (resize, the copy intercept). The key rules
    //! themselves are tested in `calculator_core::input`.
    use super::*;
    use crossterm::event::KeyEvent;

    #[test]
    fn to_key_carries_ctrl_and_alt_but_not_shift() {
        let k = to_key(KeyEvent::new(KeyCode::Char('u'), KeyModifiers::CONTROL));
        assert_eq!(
            k,
            Key {
                code: CoreKeyCode::Char('u'),
                ctrl: true,
                alt: false
            }
        );
        let k = to_key(KeyEvent::new(KeyCode::Char('d'), KeyModifiers::ALT));
        assert_eq!(
            k,
            Key {
                code: CoreKeyCode::Char('d'),
                ctrl: false,
                alt: true
            }
        );
        // Shift arrives baked into the char, so it's dropped, not lost.
        let k = to_key(KeyEvent::new(KeyCode::Char('H'), KeyModifiers::SHIFT));
        assert_eq!(k, Key::new(CoreKeyCode::Char('H')));
    }

    #[test]
    fn to_key_maps_named_keys_and_folds_the_rest_into_other() {
        for (from, to) in [
            (KeyCode::Enter, CoreKeyCode::Enter),
            (KeyCode::Backspace, CoreKeyCode::Backspace),
            (KeyCode::Esc, CoreKeyCode::Esc),
            (KeyCode::Tab, CoreKeyCode::Tab),
            (KeyCode::Left, CoreKeyCode::Left),
            (KeyCode::Right, CoreKeyCode::Right),
            (KeyCode::Up, CoreKeyCode::Up),
            (KeyCode::Down, CoreKeyCode::Down),
            (KeyCode::F(5), CoreKeyCode::Other),
            (KeyCode::Home, CoreKeyCode::Other),
        ] {
            assert_eq!(to_key(KeyEvent::new(from, KeyModifiers::NONE)).code, to);
        }
    }

    #[test]
    fn crossterm_keys_reach_the_shared_rules() {
        // End to end through handle_event: Tab cycles the pad and Ctrl-C quits,
        // so translation + key_to_msg + apply_msg are wired up.
        let (mut app, mut ui) = (App::new(), UiState::new());
        handle_event(
            Event::Key(KeyEvent::new(KeyCode::Tab, KeyModifiers::NONE)),
            &mut app,
            &mut ui,
        );
        assert_eq!(ui.layout_index(), 1);
        handle_event(
            Event::Key(KeyEvent::new(KeyCode::Char('c'), KeyModifiers::CONTROL)),
            &mut app,
            &mut ui,
        );
        assert!(app.should_quit);
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
}
