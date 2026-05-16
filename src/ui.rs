use ratatui::Frame;
use ratatui::layout::{Constraint, Layout};
use ratatui::style::Stylize;
use ratatui::text::Line;
use ratatui::widgets::{Block, Padding};

use crate::app::{App, expr_to_display};

pub fn draw(frame: &mut Frame, app: &App) {
    let [display_area, _button_area] =
        Layout::vertical([Constraint::Length(4), Constraint::Fill(1)]).areas(frame.area());

    let display_block = Block::bordered().padding(Padding::horizontal(1));
    let inner = display_block.inner(display_area);
    frame.render_widget(display_block, display_area);

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
