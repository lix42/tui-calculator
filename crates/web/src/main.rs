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

mod grid_backend;

use std::{cell::RefCell, rc::Rc};

use calculator_core::action::Action;
use calculator_core::app::App;
use calculator_core::input::{Key, KeyCode, Msg, activate, apply_msg, key_to_msg, paste};
use calculator_core::ui;
use calculator_core::ui_state::{Theme, UiState};
use grid_backend::GridBackend;
use ratzilla::ratatui::buffer::Buffer;
use ratzilla::ratatui::style::Color;
use ratzilla::ratatui::{Frame, Terminal};
use web_sys::wasm_bindgen::convert::FromWasmAbi;
use web_sys::wasm_bindgen::{JsCast, JsValue, closure::Closure};
use web_sys::{AddEventListenerOptions, ClipboardEvent, DomException, KeyboardEvent, PointerEvent};

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
    // Always starts Dark (the core's default), whatever the OS prefers, like
    // the native build; `t` toggles it.
    let state: Shared = Rc::new(RefCell::new(Web {
        app: App::new(),
        ui: UiState::new(),
        frame_size: (0, 0),
        painted_theme: None,
    }));
    let terminal = Terminal::new(GridBackend::new().map_err(std::io::Error::other)?)?;
    // Capture phase, so keys arrive whatever has focus; there's no `tabindex`
    // to keep alive across the grid being replaced.
    listen("keydown", true, {
        let state = state.clone();
        move |e: KeyboardEvent| on_key(&state, &e)
    });
    // Pointer events, not mouse events: iOS Safari doesn't turn a tap into
    // `mousedown` on an element that doesn't look clickable, and the grid is
    // plain text with the listener on `document`.
    listen("pointerdown", false, {
        let state = state.clone();
        move |e: PointerEvent| on_pointer(&state, &e, true)
    });
    listen("pointerup", false, {
        let state = state.clone();
        move |e: PointerEvent| on_pointer(&state, &e, false)
    });
    listen("paste", false, {
        let state = state.clone();
        move |e: ClipboardEvent| on_paste(&state, &e)
    });
    run_frames(terminal, move |frame| {
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

/// The frame callback, filled in once it exists to refer to itself.
type FrameSlot = Rc<RefCell<Option<Closure<dyn FnMut()>>>>;

/// Draw with `render` on every animation frame, for the page's lifetime.
///
/// Ratzilla's `draw_web`, with two changes. After a `resize` the backend's
/// grid is blank (see `grid_backend.rs`), so `Terminal::clear` resets
/// ratatui's back buffer first and the draw sends every cell (strictly, every
/// cell that isn't `Cell::default()`, which is what the rebuilt grid already
/// holds; in practice `resolve_default_colors` leaves none). And a failed
/// draw is logged, ending the loop, instead of `draw_web`'s `unwrap` panic.
fn run_frames(mut terminal: Terminal<GridBackend>, mut render: impl FnMut(&mut Frame) + 'static) {
    // The closure re-requests itself, so it holds a handle to its own slot.
    // The cycle is the point: it keeps the loop alive.
    let slot: FrameSlot = Rc::new(RefCell::new(None));
    let next = slot.clone();
    *slot.borrow_mut() = Some(Closure::new(move || {
        let mut frame = || {
            if terminal.backend().take_resized() {
                terminal.clear()?;
            }
            terminal.draw(&mut render).map(drop)
        };
        if let Err(err) = frame() {
            web_sys::console::error_1(&format!("draw failed, stopping: {err}").into());
            return;
        }
        request_frame(next.borrow().as_ref().expect("set before the first frame"));
    }));
    request_frame(slot.borrow().as_ref().expect("just set"));
}

fn request_frame(callback: &Closure<dyn FnMut()>) {
    window()
        .request_animation_frame(callback.as_ref().unchecked_ref())
        .expect("requestAnimationFrame");
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

/// A primary press (`down`) or release on the copy affordance or a button,
/// from a mouse, finger or pen. Buttons fire on the press, matching the native
/// `MouseEventKind::Down`, so a tap feels as immediate as a click. The copy
/// affordance fires on whichever edge [`copies_on`] says counts as a user
/// gesture for the clipboard.
fn on_pointer(state: &Shared, e: &PointerEvent, down: bool) {
    // Only the first finger down is primary: a pinch's second finger would
    // otherwise press a second key.
    if e.button() != 0 || !e.is_primary() {
        return;
    }
    // A release only ever copies, so skip the layout read when it can't.
    if !down && !copies_on(&e.pointer_type(), false) {
        return;
    }
    let Some((col, row)) = grid_cell(e.client_x(), e.client_y()) else {
        return;
    };
    let web = &mut *state.borrow_mut();
    if web.ui.copy_hit(col, row) {
        if copies_on(&e.pointer_type(), down) {
            copy(state, web);
        }
    } else if down
        && let Some(i) = web.ui.button_at(col, row)
        && let Some(action) = Action::from_label(web.ui.button_label(i))
    {
        activate(&mut web.app, &mut web.ui, action);
    }
}

/// Whether a press on the copy affordance copies on this edge of it.
///
/// `writeText` needs a user gesture, and browsers grant one for a mouse on
/// `pointerdown` but for touch and pen only on `pointerup`: a finger landing
/// might still turn into a scroll. So a mouse copies on the press (like every
/// other click) and everything else on the release.
fn copies_on(pointer_type: &str, down: bool) -> bool {
    down == (pointer_type == "mouse")
}

/// The grid cell under a client point, or `None` outside the grid.
fn grid_cell(x: i32, y: i32) -> Option<(u16, u16)> {
    // Look the grid up on every press: a resize replaces the element.
    let grid = document().get_element_by_id("grid")?;
    // Count the cells in the DOM rather than trusting the frame size.
    // `GridBackend` makes the two agree, but between a resize and the next
    // frame they are briefly different grids anyway.
    let rows = grid.child_element_count();
    let cols = grid
        .first_element_child()
        .map_or(0, |line| line.child_element_count());
    let rect = grid.get_bounding_client_rect();
    cell_at(
        (f64::from(x), f64::from(y)),
        (rect.left(), rect.top(), rect.width(), rect.height()),
        (cols, rows),
    )
}

/// The browser's paste (Cmd-V / Ctrl-V, or the Edit menu) into the core's
/// shared [`paste`], the same path as native bracketed paste.
///
/// The `paste` event rather than `navigator.clipboard.readText()`: the event
/// carries the text with no permission prompt. Cmd-V / Ctrl-V can't also type
/// a `v`, because [`on_key`] leaves Cmd/Ctrl chords to the browser, and that
/// browser default is what fires this event.
fn on_paste(state: &Shared, e: &ClipboardEvent) {
    let Some(text) = e
        .clipboard_data()
        .and_then(|data| data.get_data("text").ok())
    else {
        return;
    };
    e.prevent_default();
    let web = &mut *state.borrow_mut();
    paste(&mut web.app, &mut web.ui, &text);
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
/// there's no result. `writeText` is async and needs a user gesture (keydown,
/// or the pointer edge [`copies_on`] picks) and a secure context (HTTPS or localhost). The
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
    fn copy_fires_on_the_edge_that_grants_a_user_gesture() {
        // A mouse copies on the press, exactly once.
        assert!(copies_on("mouse", true));
        assert!(!copies_on("mouse", false));
        // Touch and pen copy on the release, exactly once.
        for pointer in ["touch", "pen"] {
            assert!(!copies_on(pointer, true));
            assert!(copies_on(pointer, false));
        }
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
