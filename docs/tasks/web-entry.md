# web-entry: `calculator-web` Ratzilla Entry Point

> Sub-task of [web-ratzilla](web-ratzilla.md) (split 2026-10-06).

## Goal

A `calculator-web` workspace member that runs the calculator in the browser via
Ratzilla: keyboard, mouse, copy, animations and fever all working, served
locally with `trunk serve`.

## Design

> **Revised by `web-spike` (2026-10-07) — read its progress section first.**
> Use **DomBackend**. Do **not** rely on `on_key_event` / `on_mouse_event`: after
> any window resize DomBackend swaps in a new grid element and both go dead.
> Instead, own the input: one document-level **capture** `keydown` listener that
> drops Cmd/meta chords (Ratzilla can't see Cmd, so Cmd-C would arrive as a bare
> `c` = clear), `preventDefault`s Tab/Space/`/`/`'`/Backspace (an unhandled Tab
> strands focus and loses every later key), and builds the core `Key` from
> `KeyboardEvent` directly. Handle mouse with a click listener: `clientX/Y` →
> cell via the grid element's rect / size. Also: add
> `critical-section = { version = "1", features = ["std"] }` (link error
> otherwise), and fix `REVERSED`-with-default-colors rendering white-on-white
> (stage-1 press flash). The sketch below predates these findings.

```rust
fn main() -> io::Result<()> {
    let state = Rc::new(RefCell::new((App::new(), UiState::new())));
    let mut terminal = Terminal::new(/* backend chosen in web-spike */)?;
    terminal.on_key_event({ let s = state.clone(); move |k| { /* ratzilla Key → core Key → key_to_msg → apply_msg / web copy */ } })?;
    terminal.on_mouse_event({ let s = state.clone(); move |m| { /* SingleClick → copy_hit / button_at → activate */ } })?;
    terminal.draw_web(move |frame| { let (app, ui) = &mut *state.borrow_mut(); /* auto_select on size change; ui.tick(); ui::draw(frame, app, ui) */ });
    Ok(())
}
```

- `Rc<RefCell<…>>`, not `Arc<Mutex<…>>` — wasm is single-threaded; both closures
  are `'static`.
- **Clipboard:** `navigator.clipboard.writeText` via `web-sys`. Async and
  gesture-gated (keydown/click handlers qualify); requires a secure context
  (HTTPS or localhost — Pages is HTTPS). Fire the write and set `Copied!`
  optimistically, or spawn the promise and report failure into the status line.
- **`Msg::Quit`** is ignored on the web.
- **Resize:** call `ui.auto_select(w, h)` from `draw_web` when `frame.area()`
  size changes (per `web-spike` Q2).
- **Browser defaults:** suppress `Tab`/`Space`/`/` defaults per `web-spike` Q1.
- **Pacing:** rAF runs ~60 fps vs the native ~10 fps loop. Animations are
  phase-based, so they get smoother, not faster. The breath and fever decay mean
  there's always something to repaint; gating redraws is an optimization, not
  required for correctness.
- Optional: seed `Theme` from `prefers-color-scheme`.

## How to Verify

`trunk serve`, then in the browser: type an expression, `=`, `y` copies (paste
into another tab), click buttons and the copy affordance, HJKL/arrows navigate,
`Tab`/`a`/`t`/`i`/`Esc` behave as in the README, fever climbs to rainbow, no
`Instant` panic or other errors in the console. Native `cargo test --workspace`
still green.

## Dependencies

- [web-msg](web-msg.md)
- [web-spike](web-spike.md) — backend choice, browser-default handling, resize
