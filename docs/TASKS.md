# Tasks

[x] eval-parser: Expression parser and evaluator
[x] eval-cleanup: Delete the unreachable &str eval/Parser and its tests
[x] app-state: Application state and core logic
[~] app-result-state: superseded by app-display-split (the new Mode enum replaces result: Option<String>)
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
[x] rainbow-mode: Per-digit rainbow color mode for display + buttons, optional animation (shares the web-time clock gap with web-ratzilla). Depends (soft): layout-config. (static pass shipped; animation deferred — see design doc + progress.md)
[x] quick-input: Home-row quick keyboard map with on-button tips. Shipped as a **sticky mode** (`i` enters, `Esc` leaves), not the planned Alt-held modifier — default macOS Terminal.app composes Option+key into a dead char and sends no ALT modifier. Map is a numpad in place (u i o / j k l / m → 456/123/0, a s d f → operators, [ ] → parens), not h/j/k/l→4/5/6/-. Tips render in each button's top border, only while the mode is on. `Esc` dropped as a quit key as a consequence. Depends (soft): layout-config. (shipped #22)
[ ] web-ratzilla: Ratzilla WASM web build + Cloudflare Pages deploy (large; platform port). Gaps: event-loop inversion → Msg enum, arboard→navigator.clipboard, Instant→web-time, crate split. Sequence last.
