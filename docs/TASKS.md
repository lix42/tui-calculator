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
[ ] rainbow-animation: Event-driven effect model for rainbow mode — generalize the press flash into `Effect { kind, origin, started, duration }`, then ripple / hue-drift / display-breath on top; two focus-triggered effects held for a later trial. Design lives in the "Animation" section of tasks/rainbow-mode.md. Depends: none outstanding (rainbow-mode, layout-config both done).
[ ] web-ratzilla: Ratzilla WASM web build + Cloudflare Pages deploy (large; platform port). Gaps: event-loop inversion → Msg enum, arboard→navigator.clipboard, crate split. Splits naturally into (1) extract core + Msg, (2) web entry + cfg-gated clipboard, (3) Trunk + deploy — hold the split until the crate-shape question is settled. Sequence last.

<!-- Not a task: the `std::time::Instant` → `web-time` swap (a shared prerequisite
     of the two above) landed 2026-07-31 as a standalone 2-line change, so neither
     task carries a clock gap and they no longer need to be coordinated. -->

