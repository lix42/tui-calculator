# Tasks

[x] eval-parser: Expression parser and evaluator
[x] eval-cleanup: Delete the unreachable &str eval/Parser and its tests
[x] app-state: Application state and core logic
[x] app-result-state: absorbed by app-display-split — verified 2026-07-31. Every goal met (value vs error typed by variant, raw f64 in the model, format_number at display time, zero result-parsing call sites), but via `Mode { Editing, Evaluated, Error }` rather than the specced `EvalResult`; the result lives in `expr` as `Token::Number(f64)`. See tasks/app-result-state.md.
[x] app-display-split: Tokenize the expression; separate display from internal state
[x] app-ui-state: Extract UI state from App into its own struct/file
[x] tui-skeleton: Terminal setup and event loop
[x] ui-display: Render display box
[x] ui-buttons: Render button grid with focus
[x] key-input: Direct keyboard input handling
[x] button-nav: Button navigation with HJKL/arrows
[x] mouse-input: Mouse click support
[x] paste-input: Paste a whole expression via bracketed paste
[x] copy-clipboard: Copy result to system clipboard
[x] layout-config: De-hardcode the button grid (array→Vec/slice; the const-generic 5×4 is the hard part) + cell-spanning buttons (wide 0, tall =). Ships one standard pad; no new keys/functions, no switching/auto-select (see follow-ups). Sequence first — rainbow-mode and quick-input build on its render path. (shipped #17)
[x] layout-registry: Multiple named pads + a manual switch key. Adds a Vec<Keypad> registry, active-index + override state, and the switch trigger routed in main.rs (not an Action); each pad carries a default_focus and a switch clamps focus into the new pad. Pure addition on layout-config's model. Depends (hard): layout-config.

[x] layout-auto: Auto-select the pad that best fits the terminal shape (narrow-tall vs wide-short) on resize, with the manual override taking precedence. Per-pad shape hint / fits(w,h) score. Depends (hard): layout-registry.

[x] focus-per-button: Make grid navigation step one button per key press instead of one lattice cell, so crossing a spanning button (wide 0, tall =) takes a single press. Focus stays a lattice cell but steps over the current button's covered cells via the pad's occupancy map (a `Dir` enum + `Keypad::step`). Depends (hard): layout-config.

## Planned
[x] rainbow-mode: Per-digit rainbow color mode for display + buttons. Depends (soft): layout-config. (static pass shipped as #21; the animation follow-up is now its own task, rainbow-animation)
[x] quick-input: Home-row quick keyboard map with on-button tips. Shipped as a **sticky mode** (`i` enters, `Esc` leaves), not the planned Alt-held modifier — default macOS Terminal.app composes Option+key into a dead char and sends no ALT modifier. Map is a numpad in place (u i o / j k l / m → 456/123/0, a s d f → operators, [ ] → parens), not h/j/k/l→4/5/6/-. Tips render in each button's top border, only while the mode is on. `Esc` dropped as a quit key as a consequence. Depends (soft): layout-config. (shipped #22)
[x] rainbow-animation: Event-driven effect model for rainbow mode — generalize the press flash into `Effect { kind, origin, started, duration }`, then ripple / hue-drift / display-breath on top; two focus-triggered effects held for a later trial. Design lives in the "Animation" section of tasks/rainbow-mode.md. Depends: none outstanding (rainbow-mode, layout-config both done).
[x] fever-mode: Typing-driven four-stage visual ladder. Score on `0..4` (one unit per stage) climbs `+0.15/press`, decays `-0.05/s`, with a 2.5s decay-pause after a successful `=`. Thresholds at 1/2/3 with ±0.08 hysteresis snap through plain → colored highlights → +animation → +rainbow. Replaces `r`; the meter *is* the display box's bottom border, filling right-to-left, color per stage. Design in docs/tasks/fever-mode.md. Depends: rainbow-animation.
[-] web-ratzilla: split 2026-10-06 into the web-* tasks below; tasks/web-ratzilla.md is now background (its banner lists what's superseded). Crate shape settled: workspace split.
[x] web-spike: Throwaway Ratzilla counter + Cloudflare Pages deploy; answers browser-default keys (Tab/Space/`/`), resize, renderer choice, core-without-crossterm ratatui features, mouse coords, Cmd modifiers, wasm size. Never merged. Depends: none.
[x] web-core-split: Cargo workspace — backend-free `calculator-core` lib (action/app/eval/layout/ui_state/ui) + thin native `calculator` bin. Pure relocation, same tests green. Depends: none (soft: web-spike Q4 for ratatui features).
[x] web-msg: Neutral `Key` + `Msg` + `key_to_msg(Key, quick_mode)` + `apply_msg` in core; `activate` moves to core as the single funnel; Copy/Quit stay per-entry effects. Native `handle_event` becomes translate-and-dispatch. Depends (hard): web-core-split.
[x] web-entry: `calculator-web` Ratzilla bin (DomBackend) — Rc<RefCell> state, own document-level key + mouse listeners (Ratzilla's die on resize; see web-spike), tick+draw in draw_web, auto_select on size change, navigator.clipboard copy. Works under `trunk serve`. Depends (hard): web-msg, web-spike.
[x] web-deploy: Trunk release build (`wasm-release` profile, wasm-opt, `--public-url ./`) + GitHub Action → GitHub Pages at lix42.github.io/tui-calculator, favicon, README web section. Rescoped 2026-10-08: Cloudflare moved to web-deploy-cf. Depends (hard): web-entry.
[x] web-deploy-cf: Second job in deploy-web.yml publishing the same dist/ to Cloudflare Workers Static Assets (assets-only `wrangler.jsonc`, worker `tui-calculator`, wrangler-action + repo secrets); delete the spike Worker. Custom Domains calc.xuli.dev + calc.lix42.com added 2026-10-08. Depends (hard): web-deploy.
[x] web-paste: (optional) DOM `paste` listener → shared core `input::paste` (also used by native bracketed paste) → `App::apply_str`, native paste parity. Depends (hard): web-entry.

<!-- Not a task: the `std::time::Instant` → `web-time` swap (a shared prerequisite
     of the two above) landed 2026-07-31 as a standalone 2-line change, so neither
     task carries a clock gap and they no longer need to be coordinated. -->

