# web-spike: Throwaway Ratzilla + Cloudflare Pages Spike

> Sub-task of [web-ratzilla](web-ratzilla.md) (split 2026-10-06). **Throwaway** —
> lives on a scratch branch and is never merged. Its output is answers, recorded
> in the `web-spike` section of `docs/progress.md`.

## Goal

Prove the toolchain and the deploy path end-to-end on the minimal Ratzilla
counter, and answer the environmental unknowns *before* the real port depends on
them. The risks here are environmental (toolchain, browser defaults, renderer),
not algorithmic.

## Design

A standalone Ratzilla counter app (README template: `index.html` with
`<link data-trunk rel="rust"/>`, `fn main()` → `Terminal::new(DomBackend::new()?)`
→ `on_key_event` + `draw_web`). Ratzilla apps are plain `fn main()` binaries built
by Trunk for `wasm32-unknown-unknown` — no `cdylib`, no `#[wasm_bindgen(start)]`.
Ratzilla pins `ratatui 0.30.1` with `default-features = false`, matching ours.

## Questions to answer

1. **Browser default actions.** Does Ratzilla `preventDefault` on keydown? If not,
   `Tab` steals browser focus, `Space` scrolls, `/` opens Firefox quick-find —
   each breaks a key. Find the hook if we have to suppress them ourselves.
2. **Resize.** Does a window resize surface as a changed `frame.area()` inside
   `draw_web`? If so, `ui.auto_select(w, h)` can run there on change.
3. **Renderer.** `DomBackend` vs `CanvasBackend` vs `WebGl2Backend`: box-drawing
   glyphs, per-cell fg/bg, `×`/`÷`/`⌫` rendering, and redraw cost with the
   always-on breath at rAF rate (the panel is only ~28×29 cells).
4. **Core without crossterm.** Do the `ui.rs` tests (`TestBackend`) still compile
   with `ratatui = { default-features = false, features = ["palette", …] }`?
   This decides the core crate's ratatui features in `web-core-split`.
5. **Mouse.** `on_mouse_event` → `SingleClick` with `event.col/row` in cells —
   confirm the coordinates line up with what `button_at` expects.
6. **Modifiers.** Ratzilla's `KeyEvent { code, ctrl, alt, shift }` has no `meta`:
   what does Cmd-V / Cmd-C arrive as on macOS? (Matters for `y` copy and for
   `web-paste`.)
7. **Deploy.** `trunk build --release` → `wrangler pages deploy dist` works, and
   note the `.wasm` size with and without `wasm-opt`.

## How to Verify

Every question above has a written answer (with the evidence) in `progress.md`,
and the counter is reachable at a `*.pages.dev` URL.

## Dependencies

None — standalone throwaway. Can run in parallel with
[web-core-split](web-core-split.md).
