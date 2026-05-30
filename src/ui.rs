use ratatui::Frame;
use ratatui::layout::{Constraint, Layout, Rect};
use ratatui::style::{Style, Stylize};
use ratatui::text::Line;
use ratatui::widgets::{Block, Padding, Paragraph};

use crate::app::{App, BUTTONS, expr_to_display};

pub fn draw(frame: &mut Frame, app: &App) {
    let panel = centered_panel(frame.area(), 28, 29);
    let [display_area, button_area] =
        Layout::vertical([Constraint::Length(4), Constraint::Length(25)]).areas(panel);

    draw_display(frame, app, display_area);
    draw_buttons(frame, app, button_area);
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

fn draw_display(frame: &mut Frame, app: &App, area: Rect) {
    let display_block = Block::bordered().padding(Padding::horizontal(1));
    let inner = display_block.inner(area);
    frame.render_widget(display_block, area);

    let [top_area, bottom_area] =
        Layout::vertical([Constraint::Fill(1), Constraint::Fill(1)]).areas(inner);

    let display_expr = expr_to_display(&app.expression);

    let (top, bottom) = match &app.result {
        Some(result) => (display_expr.as_str(), result.as_str()),
        None => ("", display_expr.as_str()),
    };
    frame.render_widget(Line::from(top).right_aligned().dim(), top_area);
    frame.render_widget(Line::from(bottom).right_aligned().bold(), bottom_area);
}

fn draw_buttons(frame: &mut Frame, app: &App, area: Rect) {
    let row_constraints = [Constraint::Max(5); 5];
    let col_constraints = [Constraint::Length(7); 4];
    let rows = Layout::vertical(row_constraints).areas::<5>(area);

    for (r, row_area) in rows.iter().enumerate() {
        let cells = Layout::horizontal(col_constraints).areas::<4>(*row_area);
        for (c, cell_area) in cells.iter().enumerate() {
            let label = BUTTONS[r][c];
            let focused = app.focus == (r, c);
            draw_button(frame, label, focused, *cell_area);
        }
    }
}

fn draw_button(frame: &mut Frame, label: &str, focused: bool, area: Rect) {
    let (block_style, text_style) = button_styles(focused);
    let block = Block::bordered()
        .style(block_style)
        .padding(Padding::symmetric(2, 1));
    let paragraph = Paragraph::new(label)
        .centered()
        .style(text_style)
        .block(block);
    frame.render_widget(paragraph, area);
}

/// Returns `(block_style, text_style)` for a button based on whether it is focused.
fn button_styles(focused: bool) -> (Style, Style) {
    if focused {
        (Style::new().cyan(), Style::new().cyan().bold())
    } else {
        (Style::new(), Style::new())
    }
}
