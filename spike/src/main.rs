//! Throwaway probe for docs/tasks/web-spike.md. Answers, in the browser:
//! Q1 browser-default keys, Q2 resize, Q3 renderer, Q5 mouse coords, Q6 modifiers.
//! Backend via `?backend=dom|canvas|webgl2`; `p` toggles our own preventDefault;
//! `y` tries navigator.clipboard.writeText.

use std::{cell::RefCell, rc::Rc};

use ratzilla::{
    DomBackend, WebEventHandler, WebRenderer,
    event::{KeyCode, KeyEvent, MouseEvent, MouseEventKind},
    ratatui::{
        Frame, Terminal,
        backend::Backend,
        layout::{Constraint, Layout, Rect},
        style::{Color, Modifier, Style},
        text::{Line, Span},
        widgets::{Block, BorderType, Borders, Paragraph},
    },
    web_sys::{self, wasm_bindgen::JsCast, wasm_bindgen::closure::Closure},
};

#[derive(Default)]
struct Probe {
    backend: String,
    frames: u64,
    fps: f64,
    fps_window: (f64, u64),
    last_size: (u16, u16),
    resizes: u32,
    last_key: String,
    keys_seen: u32,
    last_mouse: String,
    hit: String,
    button_rects: Vec<(Rect, &'static str)>,
    prevent_default: bool,
    clip: String,
}

const LABELS: [&str; 8] = ["7", "8", "÷", "×", "−", "⌫", "=", "("];

fn now_ms() -> f64 {
    web_sys::window().unwrap().performance().unwrap().now()
}

#[cfg(feature = "all-backends")]
use ratzilla::{CanvasBackend, WebGl2Backend};

fn main() -> std::io::Result<()> {
    console_error_panic_hook_set();
    let backend = web_sys::window()
        .unwrap()
        .location()
        .search()
        .unwrap_or_default()
        .trim_start_matches("?backend=")
        .to_string();
    let state = Rc::new(RefCell::new(Probe {
        prevent_default: true,
        ..Default::default()
    }));
    install_prevent_default(state.clone());
    match backend.as_str() {
        #[cfg(feature = "all-backends")]
        "canvas" => run(Terminal::new(CanvasBackend::new()?)?, "canvas", state),
        #[cfg(feature = "all-backends")]
        "webgl2" => run(Terminal::new(WebGl2Backend::new()?)?, "webgl2", state),
        _ => run(Terminal::new(DomBackend::new()?)?, "dom", state),
    }
}

fn console_error_panic_hook_set() {
    std::panic::set_hook(Box::new(|info| {
        web_sys::console::error_1(&info.to_string().into());
    }));
}

/// Q1: a document-level *capture* keydown listener that suppresses the browser
/// defaults for Tab / Space / `/` / `'` — runs before Ratzilla's grid listener and
/// doesn't stop propagation, so Ratzilla still sees the key.
fn install_prevent_default(state: Rc<RefCell<Probe>>) {
    let cb = Closure::<dyn FnMut(web_sys::KeyboardEvent)>::new(move |e: web_sys::KeyboardEvent| {
        if state.borrow().prevent_default
            && matches!(e.key().as_str(), "Tab" | " " | "/" | "'" | "Backspace")
        {
            e.prevent_default();
        }
    });
    let opts = web_sys::AddEventListenerOptions::new();
    opts.set_capture(true);
    web_sys::window()
        .unwrap()
        .document()
        .unwrap()
        .add_event_listener_with_callback_and_add_event_listener_options(
            "keydown",
            cb.as_ref().unchecked_ref(),
            &opts,
        )
        .unwrap();
    cb.forget();
}

fn run<B>(mut terminal: Terminal<B>, name: &str, state: Rc<RefCell<Probe>>) -> std::io::Result<()>
where
    B: Backend + WebEventHandler + 'static,
{
    state.borrow_mut().backend = name.into();
    terminal
        .on_key_event({
            let state = state.clone();
            move |k: KeyEvent| on_key(&state, k)
        })
        .map_err(std::io::Error::other)?;
    terminal
        .on_mouse_event({
            let state = state.clone();
            move |m: MouseEvent| on_mouse(&state, m)
        })
        .map_err(std::io::Error::other)?;
    terminal.draw_web(move |frame| draw(frame, &mut state.borrow_mut()));
    Ok(())
}

fn on_key(state: &Rc<RefCell<Probe>>, k: KeyEvent) {
    let mut s = state.borrow_mut();
    s.keys_seen += 1;
    s.last_key = format!("{:?} ctrl={} alt={} shift={}", k.code, k.ctrl, k.alt, k.shift);
    match k.code {
        KeyCode::Char('p') => s.prevent_default = !s.prevent_default,
        KeyCode::Char('y') => {
            s.clip = "pending…".into();
            drop(s);
            copy(state.clone(), "spike 42");
        }
        _ => {}
    }
}

fn on_mouse(state: &Rc<RefCell<Probe>>, m: MouseEvent) {
    let mut s = state.borrow_mut();
    if matches!(m.kind, MouseEventKind::Moved) {
        return;
    }
    s.last_mouse = format!("{:?} col={} row={}", m.kind, m.col, m.row);
    if let MouseEventKind::SingleClick(_) = m.kind {
        // Q5: same hit-test shape as UiState::button_at.
        s.hit = s
            .button_rects
            .iter()
            .find(|(r, _)| r.contains((m.col, m.row).into()))
            .map_or("miss".into(), |(_, l)| format!("hit {l}"));
    }
}

/// Fire-and-report: the write is async and gesture-gated.
fn copy(state: Rc<RefCell<Probe>>, text: &str) {
    let clip = web_sys::window().unwrap().navigator().clipboard();
    let promise = clip.write_text(text);
    wasm_bindgen_futures::spawn_local(async move {
        let r = wasm_bindgen_futures::JsFuture::from(promise).await;
        state.borrow_mut().clip = match r {
            Ok(_) => "Copied!".into(),
            Err(e) => format!("failed: {e:?}"),
        };
    });
}

fn draw(frame: &mut Frame, s: &mut Probe) {
    s.frames += 1;
    let t = now_ms();
    if t - s.fps_window.0 > 1000.0 {
        s.fps = (s.frames - s.fps_window.1) as f64 * 1000.0 / (t - s.fps_window.0);
        s.fps_window = (t, s.frames);
    }
    let area = frame.area();
    if (area.width, area.height) != s.last_size {
        if s.last_size != (0, 0) {
            s.resizes += 1;
        }
        s.last_size = (area.width, area.height);
    }

    let [info, glyphs, grid] = Layout::vertical([
        Constraint::Length(10),
        Constraint::Length(3),
        Constraint::Length(6),
    ])
    .areas(area.intersection(Rect::new(0, 0, 60, 19)));

    let lines = vec![
        Line::from(format!("backend={}  area={}x{}  resizes={}", s.backend, area.width, area.height, s.resizes)),
        Line::from(format!("frames={}  fps={:.0}", s.frames, s.fps)),
        Line::from(format!("keys_seen={}  last: {}", s.keys_seen, s.last_key)),
        Line::from(format!("mouse: {}", s.last_mouse)),
        Line::from(format!("click: {}", s.hit)),
        Line::from(format!("preventDefault(p)={}  clipboard(y): {}", s.prevent_default, s.clip)),
        Line::from("Try: Tab Space / ' Backspace, Cmd-C/V, resize, click"),
    ];
    frame.render_widget(
        Paragraph::new(lines).block(Block::bordered().title(" ratzilla spike ")),
        info,
    );

    // Q3: box-drawing, multibyte glyphs, truecolor fg/bg, REVERSED, bold.
    let hue = (s.frames % 360) as f64;
    let spans: Vec<Span> = LABELS
        .iter()
        .enumerate()
        .map(|(i, l)| {
            let h = (hue + i as f64 * 45.0) % 360.0;
            let style = match i % 3 {
                0 => Style::new().fg(hsv(h)),
                1 => Style::new().fg(Color::Black).bg(hsv(h)),
                _ => Style::new().add_modifier(Modifier::REVERSED | Modifier::BOLD),
            };
            Span::styled(format!(" {l} "), style)
        })
        .collect();
    frame.render_widget(
        Paragraph::new(Line::from(spans)).block(Block::bordered().border_type(BorderType::Thick)),
        glyphs,
    );

    // Q5: a row of bordered buttons with titled top borders (quick-tip style).
    let cells = Layout::horizontal([Constraint::Length(7); 4]).split(grid);
    s.button_rects.clear();
    for (i, cell) in cells.iter().enumerate() {
        let rect = Rect { height: 3, ..*cell };
        let label = LABELS[i];
        frame.render_widget(
            Paragraph::new(label).centered().block(
                Block::new()
                    .borders(Borders::ALL)
                    .border_type(BorderType::Rounded)
                    .title(Line::from("u").right_aligned()),
            ),
            rect,
        );
        s.button_rects.push((rect, label));
    }
}

fn hsv(h: f64) -> Color {
    let x = 1.0 - ((h / 60.0) % 2.0 - 1.0).abs();
    let (r, g, b) = match (h / 60.0) as u32 {
        0 => (1.0, x, 0.0),
        1 => (x, 1.0, 0.0),
        2 => (0.0, 1.0, x),
        3 => (0.0, x, 1.0),
        4 => (x, 0.0, 1.0),
        _ => (1.0, 0.0, x),
    };
    Color::Rgb((r * 255.0) as u8, (g * 255.0) as u8, (b * 255.0) as u8)
}
