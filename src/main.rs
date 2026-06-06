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
        terminal.draw(|frame| ui::draw(frame, app))?;
        if event::poll(Duration::from_millis(100))? {
            handle_event(event::read()?, app);
        }
    }
    Ok(())
}

/// Dispatches a single terminal event to the app.
///
/// Direct keyboard shortcuts work regardless of button focus. Printable
/// characters are routed through `press_button` so keys and the button grid
/// share one definition of input behavior; control keys map to App methods.
fn handle_event(event: Event, app: &mut App) {
    if let Event::Key(key) = event
        && key.kind == KeyEventKind::Press
    {
        // Ctrl-C quits — checked before the bare-`c` mapping below, which clears.
        if key.code == KeyCode::Char('c') && key.modifiers.contains(KeyModifiers::CONTROL) {
            app.should_quit = true;
            return;
        }
        match key.code {
            KeyCode::Char('q') | KeyCode::Esc => app.should_quit = true,
            KeyCode::Backspace => app.backspace(),
            KeyCode::Enter => app.evaluate(),
            KeyCode::Char(ch) => {
                if let Some(label) = key_char_to_label(ch) {
                    app.press_button(label);
                }
            }
            _ => {}
        }
    }
}

/// Translates a typed character into the button-grid label `press_button`
/// expects, or `None` for keys with no calculator action.
///
/// Most keys map to their own label 1:1. The interesting cases are where the
/// keyboard's ASCII alphabet diverges from the grid's display glyphs.
fn key_char_to_label(ch: char) -> Option<&'static str> {
    Some(match ch {
        '0' => "0",
        '1' => "1",
        '2' => "2",
        '3' => "3",
        '4' => "4",
        '5' => "5",
        '6' => "6",
        '7' => "7",
        '8' => "8",
        '9' => "9",
        '.' => ".",
        '*' => "×",
        '/' => "÷",
        '+' => "+",
        '-' => "-",
        '(' => "(",
        ')' => ")",
        '=' => "=",
        'c' => "C",
        'C' => "C",
        _ => return None,
    })
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
    fn key_with_no_action_returns_none() {
        // Guards the catch-all: unmapped keys must be inert, not panic or
        // accidentally fall into another arm.
        assert_eq!(key_char_to_label('z'), None);
        assert_eq!(key_char_to_label('q'), None); // quit is handled in handle_event, not here
    }
}
