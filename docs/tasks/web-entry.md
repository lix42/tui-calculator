# web-entry: `calculator-web` Ratzilla Entry Point

> Sub-task of [web-ratzilla](web-ratzilla.md) (split 2026-10-06).

## Goal

A `calculator-web` workspace member that runs the calculator in the browser via
Ratzilla: keyboard, mouse, copy, animations and fever all working, served
locally with `trunk serve`.

## Design

Shaped by `web-spike` (2026-10-07) — read its progress section first.
**DomBackend**, and the entry point **owns its input**. Ratzilla's
`on_key_event` / `on_mouse_event` are deliberately **not used**: they attach to
DomBackend's grid element, which is replaced on every window resize, so both go
dead after the first resize.

```rust
fn main() -> io::Result<()> {
    let state = Rc::new(RefCell::new(Web { app: App::new(), ui: UiState::new(), grid: (0, 0) }));
    let terminal = Terminal::new(DomBackend::new()?)?;
    // Listeners live on `document`, which survives resizes (the grid doesn't).
    listen(&document, "keydown", capture = true, { let s = state.clone(); move |e: KeyboardEvent| on_key(&s, e) });
    listen(&document, "click", capture = false, { let s = state.clone(); move |e: MouseEvent| on_click(&s, e) });
    terminal.draw_web(move |frame| {
        let web = &mut *state.borrow_mut();
        let area = frame.area();
        if (area.width, area.height) != web.grid { web.grid = (area.width, area.height); web.ui.auto_select(area.width, area.height); }
        web.ui.tick();
        ui::draw(frame, &web.app, &mut web.ui);
    });
    Ok(())
}

fn on_key(state: &Rc<RefCell<Web>>, e: KeyboardEvent) {
    if e.meta_key() { return; }                       // Cmd chords stay the browser's: Cmd-C must not clear
    if matches!(e.key().as_str(), "Tab" | " " | "/" | "'" | "Backspace") { e.prevent_default(); }
    let Some(key) = to_core_key(&e) else { return };  // KeyboardEvent → core `Key` (from web-msg)
    let web = &mut *state.borrow_mut();
    match key_to_msg(key, web.ui.quick_mode()) {
        Some(Msg::Copy) => web_copy(state, &web.app, &mut web.ui),   // navigator.clipboard
        Some(Msg::Quit) | None => {}
        Some(msg) => apply_msg(&mut web.app, &mut web.ui, msg),
    }
}

fn on_click(state: &Rc<RefCell<Web>>, e: MouseEvent) {
    // Look the grid up by id on every click: a resize replaces the element.
    let rect = document.get_element_by_id("grid")?.get_bounding_client_rect();
    let web = &mut *state.borrow_mut();
    let (cols, rows) = web.grid;
    let col = ((e.client_x() as f64 - rect.left()) / (rect.width() / cols as f64)) as u16;
    let row = ((e.client_y() as f64 - rect.top()) / (rect.height() / rows as f64)) as u16;
    /* copy_hit(col, row) → web_copy; else button_at → from_label → activate */
}
```

- **Key handling order matters:** the meta check and `preventDefault` come
  first, on the raw `KeyboardEvent`. An unhandled Tab moves focus to `<body>`,
  and Space and `/` trigger browser actions (page scroll, Firefox quick-find).
  Listening on `document` in the capture phase means keys arrive whatever has
  focus, so there is no `tabindex` to keep alive across resizes.
- **Check the click math against `web.grid`.** The spike showed Ratzilla's own
  coordinates were exact *before* a resize. The hand-rolled version above must
  match them, including after one. Re-run the spike's corner-cell click test.
- **Dependencies:** `critical-section = { version = "1", features = ["std"] }`,
  or the bin fails to link (`_critical_section_1_0_acquire`).
- **`REVERSED` on DomBackend draws white on white** when fg and bg are both the
  default. The stage-1 press flash in `plain_style` is exactly that case. Give
  it explicit colors on the web, or fix Ratzilla's color mapping upstream.
- `Rc<RefCell<…>>`, not `Arc<Mutex<…>>` — wasm is single-threaded; the
  listener and draw closures are all `'static`.
- **Clipboard:** `navigator.clipboard.writeText` via `web-sys`. Async and
  gesture-gated (keydown/click handlers qualify); requires a secure context
  (HTTPS or localhost — `*.workers.dev` is HTTPS). Fire the write and set `Copied!`
  optimistically, or spawn the promise and report failure into the status line.
- **`Msg::Quit`** is ignored on the web.
- **Resize:** handled in the `draw_web` closure above. `frame.area()` reflects
  the new size (`web-spike` Q2), and `auto_select` runs only when the size
  changes.
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

Regression checks for the spike's findings:
- **Resize the window, then type and click.** Both still work, and a click on a
  button's corner cell still hits that button.
- Press **Tab** several times, then type. Keys still arrive, and the page
  doesn't scroll on Space.
- **Cmd-C** with an expression on screen leaves it unchanged.
- At **fever stage 1**, the press flash is visible, not white on white.

## Dependencies

- [web-msg](web-msg.md)
- [web-spike](web-spike.md) — backend choice, browser-default handling, resize
