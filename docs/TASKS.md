# Tasks

[x] eval-parser: Expression parser and evaluator
[x] eval-cleanup: Delete the unreachable &str eval/Parser and its tests
[x] app-state: Application state and core logic
[~] app-result-state: superseded by app-display-split (the new Mode enum replaces result: Option<String>)
[x] app-display-split: Tokenize the expression; separate display from internal state
[ ] app-ui-state: Extract UI state from App into its own struct/file
[x] tui-skeleton: Terminal setup and event loop
[x] ui-display: Render display box
[x] ui-buttons: Render button grid with focus
[x] key-input: Direct keyboard input handling
[ ] button-nav: Button navigation with HJKL/arrows
[ ] mouse-input: Mouse click support
[ ] copy-clipboard: Copy result to system clipboard
