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

## Planned

[ ] layout-config: Configurable, runtime-switchable button layouts (array→Vec refactor; the const-generic grid is the hard part). Sequence first — rainbow-mode and quick-input build on its render path.
[ ] rainbow-mode: Per-digit rainbow color mode for display + buttons, optional animation (shares the web-time clock gap with web-ratzilla). Depends (soft): layout-config.
[ ] quick-input: Modifier-held (Alt, not Ctrl) quick keyboard map h/j/k/l→4/5/6/- with on-cell tips. Depends (soft): layout-config.
[ ] web-ratzilla: Ratzilla WASM web build + Cloudflare Pages deploy (large; platform port). Gaps: event-loop inversion → Msg enum, arboard→navigator.clipboard, Instant→web-time, crate split. Sequence last.
