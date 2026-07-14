use ratatui::Frame;
use ratatui::layout::{Constraint, Layout, Rect};
use ratatui::style::{Color, Style, Stylize};
use ratatui::text::Line;
use ratatui::widgets::{Block, BorderType, Padding, Paragraph};

use crate::app::App;
use crate::ui_state::UiState;

/// Fixed on-screen size of one lattice cell (columns × rows) and the height of
/// the display box above the grid. The panel is sized from these and the active
/// keypad's dimensions, so a differently-shaped pad still centers correctly —
/// there are no baked-in grid dimensions here.
const CELL_W: u16 = 7;
const CELL_H: u16 = 5;
const DISPLAY_H: u16 = 4;

pub fn draw(frame: &mut Frame, app: &App, ui: &mut UiState) {
    let grid_w = ui.keypad().cols() as u16 * CELL_W;
    let grid_h = ui.keypad().rows() as u16 * CELL_H;
    let panel = centered_panel(frame.area(), grid_w, DISPLAY_H + grid_h);
    let [display_area, button_area] =
        Layout::vertical([Constraint::Length(DISPLAY_H), Constraint::Length(grid_h)]).areas(panel);

    draw_display(frame, app, ui, display_area);
    draw_buttons(frame, ui, button_area);
}

fn centered_panel(area: Rect, width: u16, height: u16) -> Rect {
    let [_, vert, _] = Layout::vertical([
        Constraint::Fill(1),
        Constraint::Length(height),
        Constraint::Fill(1),
    ])
    .areas(area);
    let [_, panel, _] = Layout::horizontal([
        Constraint::Fill(1),
        Constraint::Length(width),
        Constraint::Fill(1),
    ])
    .areas(vert);
    panel
}

fn draw_display(frame: &mut Frame, app: &App, ui: &mut UiState, area: Rect) {
    let display_block = Block::bordered().padding(Padding::horizontal(1));
    let inner = display_block.inner(area);
    frame.render_widget(display_block, area);

    let [top_area, bottom_area] =
        Layout::vertical([Constraint::Fill(1), Constraint::Fill(1)]).areas(inner);

    let (top, bottom) = app.display_lines();

    // The affordance occupies the top row's left edge; reserve those columns so a
    // long right-aligned expression can't render over the persistent hint. The
    // expression is drawn *first*, then the affordance on top, so a momentary
    // status ("Copied!") — which reserves nothing — always wins over the
    // expression instead of being overwritten by a long one.
    let reserved = copy_affordance_width(app, ui, top_area);
    let expr_area = Rect {
        x: top_area.x + reserved,
        width: top_area.width.saturating_sub(reserved),
        ..top_area
    };
    frame.render_widget(Line::from(top).right_aligned().dim(), expr_area);
    draw_copy_affordance(frame, app, ui, top_area);
    frame.render_widget(Line::from(bottom).right_aligned().bold(), bottom_area);
}

/// The label shown when a result is copyable. The leading `y` mirrors the key
/// that triggers the copy; its width sets the clickable hit-area. ASCII, so
/// `len()` equals its rendered column width.
const COPY_HINT: &str = "[y Copy]";

/// Renders the copy affordance (or the transient status message) left-aligned in
/// the top-left of the display. Drawn *after* the expression (see `draw_display`)
/// so a live status paints on top of it.
///
/// Three states:
/// - a live status ("Copied!"/"Copy failed: …") wins while it lasts. It reserves
///   no columns (see `copy_affordance_width`) but is drawn last, so it overlays
///   the dim expression — momentary feedback right after the user acted, and a
///   long error message is free to use the whole row rather than shrink the
///   result.
/// - else the `[y Copy]` hint shows whenever the result is copyable, and its
///   width *is* reserved so the expression never overlaps it. Only the hint is
///   clickable, so it's the only state that records a non-zero `copy_rect`.
/// - else nothing; rect cleared.
fn draw_copy_affordance(frame: &mut Frame, app: &App, ui: &mut UiState, top_area: Rect) {
    if let Some(status) = ui.status_text() {
        frame.render_widget(Line::from(status).left_aligned().cyan(), top_area);
        ui.set_copy_rect(Rect::ZERO);
    } else if app.copy_text().is_some() {
        let rect = left_rect(top_area, COPY_HINT.len());
        frame.render_widget(Line::from(COPY_HINT).left_aligned().dim(), rect);
        ui.set_copy_rect(rect);
    } else {
        ui.set_copy_rect(Rect::ZERO);
    }
}

/// The column width the right-aligned expression must keep clear at the top-left,
/// mirroring the states in `draw_copy_affordance`. Only the persistent `[y Copy]`
/// hint reserves space; a live status reserves nothing (it overlays the
/// expression), and so does the empty state. Read by `draw_display` *before* the
/// expression is rendered, so it can't borrow `ui` mutably — hence a separate
/// read-only pass rather than a value returned from the draw.
fn copy_affordance_width(app: &App, ui: &UiState, top_area: Rect) -> u16 {
    if ui.status_text().is_some() {
        0
    } else if app.copy_text().is_some() {
        left_rect(top_area, COPY_HINT.len()).width
    } else {
        0
    }
}

/// A `width`-wide, single-row rect anchored at the left of `area`, clamped to
/// `area`'s width so it never overflows the display box.
fn left_rect(area: Rect, width: usize) -> Rect {
    Rect {
        width: (width as u16).min(area.width),
        height: 1,
        ..area
    }
}

fn draw_buttons(frame: &mut Frame, ui: &mut UiState, area: Rect) {
    let keypad = ui.keypad();
    // Split once per axis into the coordinate lattice; each button's rect is the
    // bounding box of the cells it spans (see `layout::Button`). `split` is
    // runtime-sized (`Rc<[Rect]>`), so no grid dimension is a const generic.
    let col_x = Layout::horizontal(vec![Constraint::Length(CELL_W); keypad.cols()]).split(area);
    let row_y = Layout::vertical(vec![Constraint::Length(CELL_H); keypad.rows()]).split(area);

    let mut rects = vec![Rect::ZERO; keypad.button_count()];
    for (i, b) in keypad.buttons().iter().enumerate() {
        let left = col_x[b.col as usize];
        let top = row_y[b.row as usize];
        let right = col_x[(b.col + b.col_span - 1) as usize];
        let bottom = row_y[(b.row + b.row_span - 1) as usize];
        let rect = Rect {
            x: left.x,
            y: top.y,
            width: right.x + right.width - left.x,
            height: bottom.y + bottom.height - top.y,
        };
        draw_button(
            frame,
            b.label,
            ui.is_button_focused(i),
            ui.is_button_pressed(i),
            rect,
        );
        rects[i] = rect;
    }
    // Hand the just-rendered geometry to the UI state so the next mouse event
    // can hit-test against exactly what's on screen.
    ui.set_button_rects(rects);
}

fn draw_button(frame: &mut Frame, label: &str, focused: bool, pressed: bool, area: Rect) {
    let style = button_styles(focused, pressed);
    let block = Block::bordered()
        .border_type(style.border_type)
        .border_style(style.border_style)
        .style(style.block_style)
        .padding(Padding::symmetric(2, 1));
    let paragraph = Paragraph::new(label)
        .centered()
        .style(style.text_style)
        .block(block);
    frame.render_widget(paragraph, area);
}

/// The full visual description of a button in one state.
///
/// Splitting the border out from the block lets a state recolor the frame
/// (`border_style`) or swap the line characters (`border_type`, e.g. a `Thick`
/// or `Double` frame to read as "pushed in") independently of the cell fill
/// (`block_style`) and the label (`text_style`).
struct ButtonStyle {
    /// Base style for the cell — primarily its background fill.
    block_style: Style,
    /// Style applied to the label text.
    text_style: Style,
    /// Color/weight of the border characters.
    border_style: Style,
    /// Which line-drawing set the border uses.
    border_type: BorderType,
}

static PRESSED_STYLE: ButtonStyle = ButtonStyle {
    block_style: Style::new().on_light_cyan(),
    text_style: Style::new().dark_gray().bold(),
    border_style: Style::new().cyan().bg(Color::Reset),
    border_type: BorderType::Rounded,
};
static FOCUSED_STYLE: ButtonStyle = ButtonStyle {
    block_style: Style::new(),
    text_style: Style::new().cyan().bold(),
    border_style: Style::new().cyan(),
    border_type: BorderType::Rounded,
};
static REGULAR_STYLE: ButtonStyle = ButtonStyle {
    block_style: Style::new(),
    text_style: Style::new(),
    border_style: Style::new(),
    border_type: BorderType::Rounded,
};

/// Returns the [`ButtonStyle`] for a button across its three states.
///
/// A pressed button is *always* also the focused one (you can only activate the
/// focused cell), so `pressed` is checked first and takes precedence over
/// `focused`. The flash is momentary — `App::tick` clears it after
/// `FLASH_DURATION` — so this style is what the user sees "on key down".
fn button_styles(focused: bool, pressed: bool) -> &'static ButtonStyle {
    if pressed {
        &PRESSED_STYLE
    } else if focused {
        &FOCUSED_STYLE
    } else {
        &REGULAR_STYLE
    }
}
