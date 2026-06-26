# web-ratzilla: Render the Calculator on the Web (Ratzilla) + Deploy to Cloudflare

## Requirement

Run the same calculator in a browser by adding a [Ratzilla](https://github.com/ratatui/ratzilla)
target — Ratatui rendered to the DOM via WebAssembly — and deploy the static
build to Cloudflare. The native TUI keeps working unchanged; the web build shares
the calculator core.

This is the largest task: it's a **platform port**, not a feature. Most of the
calculator logic is already platform-agnostic; the work is isolating the parts
that aren't (the event loop, the clipboard, the clock) and giving the web build
its own thin entry point.

## How Ratzilla works (from the official docs)

The web model is **inverted** from the native one:

```rust
use ratzilla::{event::KeyCode, DomBackend, WebRenderer};
use ratzilla::ratatui::Terminal;

fn main() -> std::io::Result<()> {
    let state = Rc::new(RefCell::new(/* App + UiState */));
    let backend = DomBackend::new()?;
    let mut terminal = Terminal::new(backend)?;

    terminal.on_key_event({
        let state = state.clone();
        move |key_event| { /* map key_event -> intent, mutate state */ }
    })?;

    terminal.draw_web(move |frame| { /* render from state */ });
    Ok(())
}
```

Key differences from `main.rs` today:

- **No blocking loop.** There is no `while !should_quit { poll; read; draw }`.
  `draw_web` registers a render closure that Ratzilla drives (via
  `requestAnimationFrame`); `on_key_event` registers an input callback. Both
  closures are `'static`, so shared mutable state must be `Rc<RefCell<…>>`
  (WASM is single-threaded, so `Rc`/`RefCell`, not `Arc`/`Mutex`).
- **Its own event types.** `ratzilla::event::{KeyEvent, KeyCode}`, **not**
  crossterm's. The good news: `action.rs` is already crossterm-free and resolves
  from a `char` / `&str`, so the typed `Action` boundary ports as-is.
- **Built with Trunk**, target `wasm32-unknown-unknown`; the output is a folder of
  static assets (HTML + JS glue + `.wasm`) — no server process.

## The four gaps to close

### 1. Event-loop inversion → factor a pure "intent" mapper

`handle_event` / `key_to_action` / `focus_delta` in `main.rs` consume crossterm
`KeyCode`/`KeyModifiers`. Rather than duplicate that logic for ratzilla, push the
**decision** into a pure, backend-agnostic mapper and let each entry point do
only the thin "this backend's event → neutral input" translation.

Today `Action` (in `action.rs`) is the pure calculator alphabet, but several
inputs aren't `Action`s: focus moves, quit, copy, and (from the new tasks) layout
switch / mode toggle / quick-input. This is exactly the deferred **`Msg` enum**
already documented in `progress.md` ("Known Issues / Deferred → Unified `Msg`
enum"). The web port is the forcing function to do it:

```rust
enum Msg { Apply(Action), MoveFocus(i32, i32), ActivateFocused, Copy, Quit, /* ... */ }
```

- Native `main.rs`: `crossterm::KeyEvent -> Option<Msg>`.
- Web entry: `ratzilla::KeyEvent -> Option<Msg>`.
- A shared `apply_msg(&mut App, &mut UiState, Msg)` runs the effect.

This keeps one definition of *what each input does* and confines per-backend code
to the translation surface.

### 2. Clipboard: `arboard` is native-only

`arboard` links against X11/AppKit/Win32 and **does not build for
`wasm32-unknown-unknown`**. The web clipboard is the browser API
`navigator.clipboard.writeText(text)` via `web-sys`/`wasm-bindgen`. Differences
that matter:

- It's **async** (returns a `Promise`) and **must be called from a user gesture**
  (the key/click handler qualifies). So web `do_copy` fires the write and
  optimistically sets the `Copied!` status (or awaits the promise to report
  failure) — it can't be the synchronous `set_text` the native path uses.
- `copy_to_clipboard` / the `CLIPBOARD` `thread_local` in `main.rs` are native
  concerns. Gate them: a `clipboard` module with `#[cfg(not(wasm))]` (arboard)
  and `#[cfg(wasm)]` (web-sys) implementations behind one signature, so
  `do_copy`/`apply_msg(Copy)` call the same function name on both targets.
- The X11-lifetime caveat documented for the native build is irrelevant on the
  web; the web has its own constraint (gesture requirement) instead.

### 3. Time source: `std::time::Instant` panics on WASM

The press-flash and copy-status timers use `std::time::Instant`
(`flash_at`, `status`'s `Instant`, and `tick`'s `elapsed()` checks in
`ui_state.rs`). `Instant::now()` **panics on `wasm32-unknown-unknown`** (no
monotonic clock without a shim). Swap to the [`web-time`](https://crates.io/crates/web-time)
crate, whose `Instant` is a drop-in that uses `performance.now()` on the web and
re-exports `std`'s on native. One import change in `ui_state.rs` covers flash and
status; it also unblocks `rainbow-mode`'s animation clock (same gap — coordinate
so the swap happens once).

### 4. Cargo / crate structure

The native build needs `crossterm` + `arboard`; the web build needs `ratzilla` +
`web-sys` + `web-time`, on a different target. Two viable shapes:

- **Target-gated deps in one crate** (lighter): keep one crate; put crossterm +
  arboard under `[target.'cfg(not(target_arch = "wasm32"))'.dependencies]` and
  ratzilla + web-sys under `[target.'cfg(target_arch = "wasm32")'.dependencies]`,
  with `main.rs` (native) vs a `lib.rs`/`#[wasm_bindgen(start)]` web entry chosen
  by cfg.
- **Workspace split** (cleaner, recommended): a `calculator-core` lib
  (`action`, `app`, `eval`, `ui_state`, and the backend-agnostic parts of `ui`)
  plus two thin entry crates — `calculator` (native bin, crossterm) and
  `calculator-web` (cdylib, ratzilla). This matches the existing note in
  `progress.md` ("If a non-terminal backend is ever needed, the right factoring
  is a separate binary, not generics here"). The `ui::draw` signature already
  takes a `Frame`, so rendering is backend-agnostic and moves into core mostly
  intact.

Recommend the workspace split: it makes the shared core explicit and keeps each
entry point honest about its dependencies.

### Likely smaller gaps (verify during spike)

- **Mouse**: Ratzilla's pointer-event support and the cell coordinates it
  reports must be confirmed; `button_at` hit-testing relies on terminal-cell
  coordinates. If web pointer events don't map cleanly to cells, mouse may be
  keyboard-first on web initially.
- **Bracketed paste**: the native `Event::Paste` path has no direct ratzilla
  equivalent; a web paste would come through a DOM `paste` event /
  `navigator.clipboard.readText`. `App::apply_str` (the ingest entry point) is
  already backend-agnostic, so only the event wiring differs.
- **Sizing / fonts**: the HTML uses a monospace web font (Fira Code in the
  docs' template); the fixed 28×29 panel assumes terminal cells. Confirm the
  centered panel renders sensibly in the DOM grid.

## Deployment to Cloudflare

Trunk produces a static `dist/` (HTML + JS + `.wasm`) — pure client-side, so this
is **Cloudflare Pages** (static hosting), not a Worker:

1. `rustup target add wasm32-unknown-unknown` and `cargo install --locked trunk`.
2. An `index.html` with `<link data-trunk rel="rust"/>` (per the Ratzilla
   template) and the monospace font.
3. `trunk build --release` → `dist/`.
4. Deploy: `wrangler pages deploy dist` (or connect the repo to Cloudflare Pages
   with build command `trunk build --release` and output dir `dist`). No
   `wrangler.toml` Worker config needed for a static deploy; a Pages project
   suffices. Watch the `.wasm` payload size (release + `wasm-opt`, which Trunk can
   run, keeps it reasonable).

## Implementation Notes

- **Spike first.** Stand up the minimal Ratzilla counter from the docs, confirm
  the toolchain and a Cloudflare Pages deploy end-to-end, *then* wire the
  calculator core in. The unknowns are environmental (toolchain, clock panic,
  clipboard gesture), not algorithmic — surface them on a throwaway before the
  real port.
- Sequence the refactors so native stays green at each step: (1) `web-time` swap
  (native-neutral), (2) extract `calculator-core` + `Msg`/`apply_msg` with the
  native bin still passing all tests, (3) add the web entry + cfg-gated clipboard,
  (4) Trunk + deploy.
- Best done **after** `layout-config` / `rainbow-mode` / `quick-input` settle, so
  the core being extracted is stable — but the `web-time` swap is shared with
  `rainbow-mode` and can land early either way.

## How to Test

- Native: the full existing `cargo test` suite stays green throughout (the core
  is unchanged behavior, just relocated).
- Core: `Msg` mapping is pure and unit-testable without either backend.
- Web (manual): `trunk serve`, then in the browser — type an expression, `=`,
  copy (paste into another tab to confirm `navigator.clipboard`), navigate the
  grid, switch layout / toggle rainbow if those have landed. Confirm no
  `Instant` panic in the console.
- Deploy (manual): `wrangler pages deploy dist`; load the Pages URL; repeat the
  smoke test against the deployed build.

## Dependencies

- **Whole app** — this ports the finished calculator; it cross-cuts every module.
- **app-ui-state** — the `Action` boundary and App/UiState split that make the
  core backend-agnostic; the `Msg` enum is the deferred follow-up named there and
  in `progress.md`.
- **rainbow-mode** (shared) — both need the `web-time` clock swap; coordinate so
  it happens once.
- New crates: `ratzilla`, `web-sys`, `wasm-bindgen`, `web-time`; tooling: `trunk`,
  `wrangler`.

## Open Questions

- **Crate shape**: workspace split (recommended) vs target-gated single crate.
- **Mouse on web**: full pointer hit-testing vs keyboard-first v1, pending the
  spike.
- **Deploy path**: `wrangler pages deploy` from CI vs Cloudflare Pages Git
  integration (build-on-push). Either works; pick per how the repo is hosted.
