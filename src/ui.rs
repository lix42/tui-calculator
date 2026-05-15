use ratatui::Frame;
use ratatui::widgets::Block;

use crate::app::App;

/// Stub renderer. Real layout (display + button grid) lands in `ui-display`
/// and `ui-buttons`.
pub fn draw(frame: &mut Frame, _app: &App) {
    let block = Block::bordered().title("Calculator");
    frame.render_widget(block, frame.area());
}
