mod eval;

use eval::eval;

fn main() {
    // Placeholder: the TUI event loop will replace this.
    // Wired up now so the eval module is reachable and clippy stays clean.
    match eval("1+1") {
        Ok(v) => println!("eval: {v}"),
        Err(e) => eprintln!("error: {e}"),
    }
}
