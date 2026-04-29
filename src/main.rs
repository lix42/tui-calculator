mod app;
mod eval;

use app::App;
use eval::eval;

fn main() {
    let mut app = App::new();
    let _ = app.should_quit;
    app.move_focus(1, 1);
    let _ = app.focused_label();
    for ch in "1+1=".chars() {
        app.press_button(&ch.to_string());
    }

    app.evaluate();
    match app.result {
        Some(ref res) => println!("Result: {res}"),
        None => println!("No result"),
    }
}
