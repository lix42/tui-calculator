mod action;
mod app;
mod eval;
mod ui;
mod ui_state;

use std::io::{self, Result, Stdout};
use std::time::Duration;

use crossterm::event::{
    self, DisableMouseCapture, EnableMouseCapture, Event, KeyCode, KeyEventKind, KeyModifiers,
    MouseButton, MouseEventKind,
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
        if let MouseEventKind::Down(MouseButton::Left) = mouse.kind
            && let Some((r, c)) = ui.button_at(mouse.column, mouse.row)
            && let Some(action) = Action::from_label(BUTTONS[r][c])
        {
            activate(app, ui, action);
        }
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
    app.apply(action);
    ui.register_press(action.label());
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
    fn key_to_action_ignores_non_action_keys() {
        // Navigation, Space, and quit keys have no calculator action — they're
        // routed before key_to_action is reached, so it returns None for them.
        assert_eq!(key_to_action(KeyCode::Left), None);
        assert_eq!(key_to_action(KeyCode::Char(' ')), None);
        assert_eq!(key_to_action(KeyCode::Char('q')), None);
        assert_eq!(key_to_action(KeyCode::Esc), None);
    }
}
