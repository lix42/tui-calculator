//! The calculator in the browser: Ratzilla's `DomBackend` + the shared core.
//!
//! Like `src/main.rs`, this owns only what is platform-specific: translating DOM
//! events into the core's [`Key`], the clipboard, and the frame loop. What each
//! key *means* is [`key_to_msg`], shared with the native build.
//!
//! Input does **not** go through Ratzilla's `on_key_event` / `on_mouse_event`.
//! Those attach to DomBackend's grid element, which is replaced on every window
//! resize, so both go dead after the first one (see `web-spike` in
//! `docs/progress.md`). Our listeners live on `document`, which never changes.

use std::{cell::RefCell, rc::Rc};

use calculator_core::action::Action;
use calculator_core::app::App;
use calculator_core::input::{Key, KeyCode, Msg, activate, apply_msg, key_to_msg};
use calculator_core::ui;
use calculator_core::ui_state::{Theme, UiState};
use ratzilla::ratatui::Terminal;
use ratzilla::ratatui::buffer::Buffer;
use ratzilla::ratatui::style::Color;
use ratzilla::{DomBackend, WebRenderer};
use web_sys::wasm_bindgen::convert::FromWasmAbi;
use web_sys::wasm_bindgen::{JsCast, JsValue, closure::Closure};
use web_sys::{AddEventListenerOptions, DomException, KeyboardEvent, MouseEvent};

/// Everything the listeners and the frame loop share. `Rc<RefCell<…>>`, not
/// `Arc<Mutex<…>>`: wasm is single-threaded, and no callback can run while
/// another holds the borrow (events and rAF frames are separate tasks).
struct Web {
    app: App,
    ui: UiState,
    /// The frame size `auto_select` last saw, so it runs only on a change.
    frame_size: (u16, u16),
    /// The theme `<body>` was last painted for.
    painted_theme: Option<Theme>,
}

type Shared = Rc<RefCell<Web>>;

fn main() -> std::io::Result<()> {
    std::panic::set_hook(Box::new(|info| {
        web_sys::console::error_1(&info.to_string().into());
    }));
    let mut ui = UiState::new();
    if prefers_light() {
        ui.toggle_theme();
    }
    let state: Shared = Rc::new(RefCell::new(Web {
        app: App::new(),
        ui,
        frame_size: (0, 0),
        painted_theme: None,
    }));
    let terminal = Terminal::new(DomBackend::new().map_err(std::io::Error::other)?)?;
    // Capture phase, so keys arrive whatever has focus; there's no `tabindex`
    // to keep alive across the grid being replaced.
    listen("keydown", true, {
        let state = state.clone();
        move |e: KeyboardEvent| on_key(&state, &e)
    });
    listen("mousedown", false, {
        let state = state.clone();
        move |e: MouseEvent| on_mouse_down(&state, &e)
    });
    terminal.draw_web(move |frame| {
        let web = &mut *state.borrow_mut();
        let area = frame.area();
        if (area.width, area.height) != web.frame_size {
            web.frame_size = (area.width, area.height);
            web.ui.auto_select(area.width, area.height);
        }
        let theme = web.ui.theme();
        if web.painted_theme != Some(theme) {
            paint_body(theme);
            web.painted_theme = Some(theme);
        }
        web.ui.tick();
        ui::draw(frame, &web.app, &mut web.ui);
        resolve_default_colors(frame.buffer_mut(), theme);
    });
    Ok(())
}

/// Attach a `document` listener for the app's lifetime.
fn listen<E, F>(event: &str, capture: bool, handler: F)
where
    E: FromWasmAbi + 'static,
    F: FnMut(E) + 'static,
{
    let closure = Closure::<dyn FnMut(E)>::new(handler);
    let opts = AddEventListenerOptions::new();
    opts.set_capture(capture);
    document()
        .add_event_listener_with_callback_and_add_event_listener_options(
            event,
            closure.as_ref().unchecked_ref(),
            &opts,
        )
        .expect("addEventListener on document");
    // The JS side holds the only reference from here on. Dropping the handle at
    // the end of this function would free the Rust closure, and the next event
    // would throw.
    closure.forget();
}

fn on_key(state: &Shared, e: &KeyboardEvent) {
    // Cmd (macOS) and Ctrl (Windows/Linux) chords are the browser's shortcuts:
    // Cmd-C / Ctrl-C copy, Ctrl-− zooms out. Without this, Cmd-C would arrive as
    // a bare `c` and clear the calculator.
    if e.meta_key() || e.ctrl_key() {
        return;
    }
    // Firefox opens quick-find on `'`. It isn't a calculator key, but it would
    // steal focus, so it never keeps its default.
    if e.key() == "'" {
        e.prevent_default();
    }
    let key = to_key(&e.key(), e.alt_key());
    let web = &mut *state.borrow_mut();
    let Some(msg) = key_to_msg(key, web.ui.quick_mode()) else {
        return;
    };
    // Nothing quits a browser tab, so `q` is a key the calculator doesn't
    // handle, and keeps its default.
    if msg == Msg::Quit {
        return;
    }
    // Every key the calculator handles cancels its browser default: Tab
    // (focus), Space / arrows (scroll), `/` (Firefox quick-find), Backspace.
    // Deriving this from `key_to_msg` means there's no separate list to drift.
    e.prevent_default();
    match msg {
        Msg::Copy => copy(state, web),
        msg => apply_msg(&mut web.app, &mut web.ui, msg),
    }
}

/// Translate a DOM `KeyboardEvent.key` into the core's [`Key`]. The web's
/// counterpart of the native `to_key`: Shift is already baked into the
/// character, and the caller has dropped Ctrl/Cmd chords.
fn to_key(key: &str, alt: bool) -> Key {
    let code = match key {
        "Enter" => KeyCode::Enter,
        "Backspace" => KeyCode::Backspace,
        "Escape" => KeyCode::Esc,
        "Tab" => KeyCode::Tab,
        "ArrowLeft" => KeyCode::Left,
        "ArrowRight" => KeyCode::Right,
        "ArrowUp" => KeyCode::Up,
        "ArrowDown" => KeyCode::Down,
        // A printable key is exactly one char; named keys ("Shift", "F5",
        // "Dead") are longer.
        _ => {
            let mut chars = key.chars();
            match (chars.next(), chars.next()) {
                (Some(c), None) => KeyCode::Char(c),
                _ => KeyCode::Other,
            }
        }
    };
    Key {
        code,
        ctrl: false,
        alt,
    }
}

/// A left press on the copy affordance or a button. `mousedown` rather than
/// `click`, matching the native `MouseEventKind::Down`; it counts as a user
/// gesture for the clipboard just the same.
fn on_mouse_down(state: &Shared, e: &MouseEvent) {
    if e.button() != 0 {
        return;
    }
    // Look the grid up on every press: a resize replaces the element.
    let Some(grid) = document().get_element_by_id("grid") else {
        return;
    };
    // Count the cells in the DOM rather than trusting the frame size: Ratzilla
    // sizes the grid from `<body>` but the frame from the window, and between
    // a resize and the next frame the two are briefly different grids anyway.
    let rows = grid.child_element_count();
    let cols = grid
        .first_element_child()
        .map_or(0, |line| line.child_element_count());
    let rect = grid.get_bounding_client_rect();
    let Some((col, row)) = cell_at(
        (f64::from(e.client_x()), f64::from(e.client_y())),
        (rect.left(), rect.top(), rect.width(), rect.height()),
        (cols, rows),
    ) else {
        return;
    };
    let web = &mut *state.borrow_mut();
    if web.ui.copy_hit(col, row) {
        copy(state, web);
    } else if let Some(i) = web.ui.button_at(col, row)
        && let Some(action) = Action::from_label(web.ui.button_label(i))
    {
        activate(&mut web.app, &mut web.ui, action);
    }
}

/// The grid cell under a point, given the grid's client rect
/// `(left, top, width, height)` and its size in cells. `None` outside the grid.
fn cell_at(point: (f64, f64), rect: (f64, f64, f64, f64), cells: (u32, u32)) -> Option<(u16, u16)> {
    let (x, y) = point;
    let (left, top, width, height) = rect;
    let (cols, rows) = cells;
    if cols == 0 || rows == 0 || width <= 0.0 || height <= 0.0 {
        return None;
    }
    let col = ((x - left) / (width / f64::from(cols))).floor();
    let row = ((y - top) / (height / f64::from(rows))).floor();
    let inside = (0.0..f64::from(cols)).contains(&col) && (0.0..f64::from(rows)).contains(&row);
    // In range, so the casts can't truncate.
    inside.then_some((col as u16, row as u16))
}

/// Write the current result to the clipboard and report into the status line.
///
/// The browser's counterpart of native `do_copy`, and like it a no-op when
/// there's no result. `writeText` is async and needs a user gesture (keydown
/// and mousedown both count) and a secure context (HTTPS or localhost). The
/// status is set when the promise settles, so a refusal is reported, not
/// papered over with an optimistic "Copied!".
fn copy(state: &Shared, web: &Web) {
    let Some(text) = web.app.copy_text() else {
        return;
    };
    let promise = window().navigator().clipboard().write_text(&text);
    let state = state.clone();
    // `spawn_local` polls on a later microtask, after the caller's borrow of
    // `state` has ended.
    wasm_bindgen_futures::spawn_local(async move {
        let status = match wasm_bindgen_futures::JsFuture::from(promise).await {
            Ok(_) => "Copied!".to_string(),
            Err(err) => format!("Copy failed: {}", js_error_message(&err)),
        };
        state.borrow_mut().ui.set_status(status);
    });
}

fn js_error_message(err: &JsValue) -> String {
    err.dyn_ref::<DomException>()
        .map(DomException::message)
        .or_else(|| err.as_string())
        .unwrap_or_else(|| "unknown error".to_string())
}

/// The colors this "terminal" uses for `Color::Reset`, per theme: `(fg, bg)`.
///
/// The core leaves many styles at the terminal default (resting borders, the
/// display text, the stage-1 `REVERSED` press flash) and lets the terminal pick.
/// DomBackend maps `Reset` to hard-coded white, for the background of a
/// `REVERSED` cell too, which gives white-on-white flashes and an unreadable
/// Light theme. In the browser this program *is* the terminal, so it supplies
/// the defaults itself. The foregrounds match `RESTING_BORDER_L_DARK` (80) and
/// `RESTING_BORDER_L_LIGHT` (22) in `ui.rs` as HSLuv grays, which the ripple
/// ramp is anchored to, so on the web that assumption holds exactly.
fn default_colors(theme: Theme) -> (Color, Color) {
    match theme {
        Theme::Dark => (Color::Rgb(0xc6, 0xc6, 0xc6), Color::Rgb(0x12, 0x12, 0x12)),
        Theme::Light => (Color::Rgb(0x35, 0x35, 0x35), Color::Rgb(0xfa, 0xfa, 0xfa)),
    }
}

/// Replace every `Color::Reset` in the frame with the theme's default, so no
/// `Reset` ever reaches DomBackend (see [`default_colors`]). Runs after the core
/// draws, before Ratzilla diffs the buffer.
fn resolve_default_colors(buf: &mut Buffer, theme: Theme) {
    let (fg, bg) = default_colors(theme);
    for cell in &mut buf.content {
        if cell.fg == Color::Reset {
            cell.fg = fg;
        }
        if cell.bg == Color::Reset {
            cell.bg = bg;
        }
    }
}

/// Paint `<body>` in the theme's background, so the strip of page outside the
/// grid matches the cells.
fn paint_body(theme: Theme) {
    let Color::Rgb(r, g, b) = default_colors(theme).1 else {
        unreachable!("default_colors returns RGB");
    };
    if let Some(body) = document().body() {
        let _ = body
            .style()
            .set_property("background-color", &format!("rgb({r}, {g}, {b})"));
    }
}

/// Seed the theme from the OS setting. Dark is the core's default, so only a
/// light preference changes anything.
fn prefers_light() -> bool {
    window()
        .match_media("(prefers-color-scheme: light)")
        .ok()
        .flatten()
        .is_some_and(|query| query.matches())
}

fn window() -> web_sys::Window {
    web_sys::window().expect("running in a browser window")
}

fn document() -> web_sys::Document {
    window().document().expect("window has a document")
}

#[cfg(test)]
mod tests {
    //! The pure translation pieces, run natively. The listeners and clipboard
    //! need a browser and are checked by hand (see `docs/tasks/web-entry.md`).
    use super::*;
    use ratzilla::ratatui::layout::Rect;
    use ratzilla::ratatui::style::{Modifier, Style};

    #[test]
    fn to_key_maps_named_keys_and_single_chars() {
        for (dom, code) in [
            ("Enter", KeyCode::Enter),
            ("Backspace", KeyCode::Backspace),
            ("Escape", KeyCode::Esc),
            ("Tab", KeyCode::Tab),
            ("ArrowLeft", KeyCode::Left),
            ("ArrowRight", KeyCode::Right),
            ("ArrowUp", KeyCode::Up),
            ("ArrowDown", KeyCode::Down),
            ("7", KeyCode::Char('7')),
            ("H", KeyCode::Char('H')),
            (" ", KeyCode::Char(' ')),
            ("×", KeyCode::Char('×')),
            ("Shift", KeyCode::Other),
            ("F5", KeyCode::Other),
            ("Dead", KeyCode::Other),
        ] {
            assert_eq!(to_key(dom, false), Key::new(code), "{dom:?}");
        }
    }

    #[test]
    fn to_key_carries_alt() {
        let k = to_key("h", true);
        assert!(k.alt && !k.ctrl);
    }

    #[test]
    fn cell_at_maps_corners_and_rejects_outside() {
        // A 10×4 grid of 8×20 px cells at (100, 50).
        let rect = (100.0, 50.0, 80.0, 80.0);
        let cells = (10, 4);
        assert_eq!(cell_at((100.0, 50.0), rect, cells), Some((0, 0)));
        assert_eq!(cell_at((179.9, 129.9), rect, cells), Some((9, 3)));
        assert_eq!(cell_at((108.0, 70.0), rect, cells), Some((1, 1)));
        assert_eq!(cell_at((99.9, 60.0), rect, cells), None);
        assert_eq!(cell_at((180.0, 60.0), rect, cells), None);
        assert_eq!(cell_at((120.0, 130.0), rect, cells), None);
        assert_eq!(cell_at((120.0, 60.0), rect, (0, 0)), None);
    }

    #[test]
    fn resolve_replaces_only_reset_colors() {
        let mut buf = Buffer::empty(Rect::new(0, 0, 3, 1));
        buf[(1, 0)].set_style(Style::new().fg(Color::Red));
        buf[(2, 0)].set_style(Style::new().add_modifier(Modifier::REVERSED));
        resolve_default_colors(&mut buf, Theme::Light);
        let (fg, bg) = default_colors(Theme::Light);
        assert_eq!((buf[(0, 0)].fg, buf[(0, 0)].bg), (fg, bg));
        assert_eq!((buf[(1, 0)].fg, buf[(1, 0)].bg), (Color::Red, bg));
        // The REVERSED cell now has two distinct explicit colors to swap,
        // instead of DomBackend's white and white.
        assert_eq!((buf[(2, 0)].fg, buf[(2, 0)].bg), (fg, bg));
        assert_ne!(fg, bg);
    }
}
