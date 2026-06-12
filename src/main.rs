mod app;
mod eval;
mod ui;

use std::io::{self, Result, Stdout};
use std::time::Duration;

use crossterm::event::{
    self, DisableMouseCapture, EnableMouseCapture, Event, KeyCode, KeyEventKind, KeyModifiers,
};
use crossterm::execute;
use crossterm::terminal::{
    EnterAlternateScreen, LeaveAlternateScreen, disable_raw_mode, enable_raw_mode,
};
use ratatui::Terminal;
use ratatui::backend::CrosstermBackend;

use app::App;

type Tui = Terminal<CrosstermBackend<Stdout>>;

fn setup_terminal() -> Result<Tui> {
    enable_raw_mode()?;
    let mut stdout = io::stdout();
    execute!(stdout, EnterAlternateScreen, EnableMouseCapture)?;
    Terminal::new(CrosstermBackend::new(stdout))
}

fn restore_terminal(terminal: &mut Tui) -> Result<()> {
    // Reverse of setup: drop mouse capture *before* leaving alt screen.
    execute!(
        terminal.backend_mut(),
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
        let _ = execute!(io::stdout(), DisableMouseCapture, LeaveAlternateScreen);
        let _ = disable_raw_mode();
        original(info);
    }));
}

fn run(terminal: &mut Tui, app: &mut App) -> Result<()> {
    while !app.should_quit {
        // Expire any press flash before drawing; the 100ms poll below paces
        // this, so a flash clears ~1-2 ticks after the key (a brief blink).
        app.tick();
        terminal.draw(|frame| ui::draw(frame, app))?;
        if event::poll(Duration::from_millis(100))? {
            handle_event(event::read()?, app);
        }
    }
    Ok(())
}

/// Dispatches a single terminal event to the app.
///
/// Navigation (HJKL / arrows) moves the grid focus. Every key that *activates*
/// a button goes through `activate`, so focus follows the input and the button
/// flashes — keyboard, the button grid, and (later) the mouse share one path.
fn handle_event(event: Event, app: &mut App) {
    if let Event::Key(key) = event
        && key.kind == KeyEventKind::Press
    {
        // Ctrl-C quits — checked before the bare-`c` mapping below, which clears.
        if key.code == KeyCode::Char('c') && key.modifiers.contains(KeyModifiers::CONTROL) {
            app.should_quit = true;
            return;
        }
        // HJKL / arrows move focus only — no activation, no flash.
        if let Some((dr, dc)) = focus_delta(key.code) {
            app.move_focus(dr, dc);
            return;
        }
        match key.code {
            KeyCode::Char('q') | KeyCode::Esc => app.should_quit = true,
            // Space activates whatever is focused, leaving focus put so it can
            // be re-pressed in place.
            KeyCode::Char(' ') => activate(app, app.focused_label()),
            // Enter always evaluates (pressing "=" *is* evaluate); focus snaps
            // to "=" to match. Backspace likewise routes through its label.
            KeyCode::Enter => activate(app, "="),
            KeyCode::Backspace => activate(app, "⌫"),
            KeyCode::Char(ch) => {
                if let Some(label) = key_char_to_label(ch) {
                    activate(app, label);
                }
            }
            _ => {}
        }
    }
}

/// Press a button by `label`, then make focus follow it and flash it. The
/// single funnel for every activation so feedback is uniform across inputs.
fn activate(app: &mut App, label: &str) {
    app.press_button(label);
    app.register_press(label);
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

/// Translates a typed character into the button-grid label `press_button`
/// expects, or `None` for keys with no calculator action.
///
/// Most keys map to their own label 1:1. The interesting cases are where the
/// keyboard's ASCII alphabet diverges from the grid's display glyphs.
fn key_char_to_label(ch: char) -> Option<&'static str> {
    match ch {
        '0' => Some("0"),
        '1' => Some("1"),
        '2' => Some("2"),
        '3' => Some("3"),
        '4' => Some("4"),
        '5' => Some("5"),
        '6' => Some("6"),
        '7' => Some("7"),
        '8' => Some("8"),
        '9' => Some("9"),
        '.' => Some("."),
        '*' => Some("×"),
        '/' => Some("÷"),
        '+' => Some("+"),
        '-' => Some("-"),
        '(' => Some("("),
        ')' => Some(")"),
        '=' => Some("="),
        'c' | 'C' => Some("C"),
        _ => None,
    }
}

fn main() -> Result<()> {
    install_panic_hook();
    let mut terminal = setup_terminal()?;
    let mut app = App::new();
    let result = run(&mut terminal, &mut app);
    restore_terminal(&mut terminal)?;
    result
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn key_maps_ascii_operators_to_glyphs() {
        // The crux: keyboard ASCII `*`/`/` become the grid's display glyphs,
        // which are the only labels press_button recognizes as multiply/divide.
        assert_eq!(key_char_to_label('*'), Some("×"));
        assert_eq!(key_char_to_label('/'), Some("÷"));
    }

    #[test]
    fn key_maps_passthrough_and_control_chars() {
        assert_eq!(key_char_to_label('7'), Some("7"));
        assert_eq!(key_char_to_label('.'), Some("."));
        assert_eq!(key_char_to_label('+'), Some("+"));
        assert_eq!(key_char_to_label('('), Some("("));
        assert_eq!(key_char_to_label('='), Some("="));
        // Clear is case-insensitive so Shift doesn't matter.
        assert_eq!(key_char_to_label('c'), Some("C"));
        assert_eq!(key_char_to_label('C'), Some("C"));
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
    fn key_with_no_action_returns_none() {
        // Guards the catch-all: unmapped keys must be inert, not panic or
        // accidentally fall into another arm.
        assert_eq!(key_char_to_label('z'), None);
        assert_eq!(key_char_to_label('q'), None); // quit is handled in handle_event, not here
    }
}
