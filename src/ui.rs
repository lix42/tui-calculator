use ratatui::Frame;
use ratatui::layout::{Constraint, Layout, Rect};
use ratatui::style::{Color, Style, Stylize};
use ratatui::text::Line;
use ratatui::widgets::{Block, BorderType, Padding, Paragraph};

use crate::app::App;
use crate::ui_state::{BUTTONS, UiState};

pub fn draw(frame: &mut Frame, app: &App, ui: &mut UiState) {
    let panel = centered_panel(frame.area(), 28, 29);
    let [display_area, button_area] =
        Layout::vertical([Constraint::Length(4), Constraint::Length(25)]).areas(panel);

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
    // long right-aligned expression can't render over the persistent hint.
    let reserved = draw_copy_affordance(frame, app, ui, top_area);
    let expr_area = Rect {
        x: top_area.x + reserved,
        width: top_area.width.saturating_sub(reserved),
        ..top_area
    };
    frame.render_widget(Line::from(top).right_aligned().dim(), expr_area);
    frame.render_widget(Line::from(bottom).right_aligned().bold(), bottom_area);
}

/// The label shown when a result is copyable. The leading `y` mirrors the key
/// that triggers the copy; its width sets the clickable hit-area. ASCII, so
/// `len()` equals its rendered column width.
const COPY_HINT: &str = "[y Copy]";

/// Renders the copy affordance (or the transient status message) left-aligned in
/// the top-left of the display, and returns the column width the caller must keep
/// clear of the right-aligned expression.
///
/// Three states:
/// - a live status ("Copied!"/"Copy failed: …") wins while it lasts. It returns
///   `0` (no reservation): it's momentary feedback right after the user acted, so
///   a brief overlap with the dim expression is acceptable, and a long error
///   message must be free to use the whole row rather than shrink the result.
/// - else the `[y Copy]` hint shows whenever the result is copyable, and its
///   width *is* reserved so the expression never overlaps it. Only the hint is
///   clickable, so it's the only state that records a non-zero `copy_rect`.
/// - else nothing; rect cleared, `0` reserved.
fn draw_copy_affordance(frame: &mut Frame, app: &App, ui: &mut UiState, top_area: Rect) -> u16 {
    if let Some(status) = ui.status_text() {
        frame.render_widget(Line::from(status).left_aligned().cyan(), top_area);
        ui.set_copy_rect(Rect::ZERO);
        0
    } else if app.copy_text().is_some() {
        let rect = left_rect(top_area, COPY_HINT.len());
        frame.render_widget(Line::from(COPY_HINT).left_aligned().dim(), rect);
        ui.set_copy_rect(rect);
        rect.width
    } else {
        ui.set_copy_rect(Rect::ZERO);
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
    let row_constraints = [Constraint::Max(5); 5];
    let col_constraints = [Constraint::Length(7); 4];
    let rows = Layout::vertical(row_constraints).areas::<5>(area);

    let mut rects = [[Rect::ZERO; 4]; 5];
    for (r, row_area) in rows.iter().enumerate() {
        let cells = Layout::horizontal(col_constraints).areas::<4>(*row_area);
        for (c, cell_area) in cells.iter().enumerate() {
            let label = BUTTONS[r][c];
            let focused = ui.is_focused((r, c));
            let pressed = ui.is_pressed((r, c));
            draw_button(frame, label, focused, pressed, *cell_area);
            rects[r][c] = *cell_area;
        }
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
