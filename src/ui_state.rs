//! UI state: button-grid focus, the momentary press flash, the on-screen
//! geometry used for mouse hit-testing, and the copy affordance + its status.
//!
//! This is the rendering/input-routing half of what used to live in `App`. It
//! owns the active [`Keypad`], where focus currently sits (as a lattice cell),
//! which button is flashing, and the screen rect of every button. `App` keeps
//! only the calculator state (`expr` / `current` / `mode`); the two have
//! different lifecycles and concerns.

use std::time::{Duration, Instant};

use ratatui::layout::{Position, Rect};

use crate::layout::{Dir, Keypad};

/// How long a button stays in its "pressed" look after activation. Terminals
/// have no key-release event, so the press is shown as a brief flash that the
/// run loop's `tick` clears once this much time has passed.
const FLASH_DURATION: Duration = Duration::from_millis(120);

/// How long the copy status message ("Copied!" / "Copy failed") stays on screen.
/// Longer than `FLASH_DURATION` because this is text the user needs to *read*,
/// not a momentary blink. Cleared by the same `tick` that expires the flash.
const STATUS_DURATION: Duration = Duration::from_millis(1500);

/// How the digits are colored. A **presentation-only** toggle — it changes no
/// calculator state, so it lives on `UiState` (the rendering half), not `App`.
/// `Rainbow` (each digit `0`–`9` its own hue on both the button grid and the
/// display) is the default look; `Mono` is the plain fallback (see
/// [`crate::ui::glyph_color`]).
#[derive(Debug, Clone, Copy, PartialEq, Eq, Default)]
pub enum ColorMode {
    Mono,
    #[default]
    Rainbow,
}

/// Which background the palette is tuned for. It sets the HSLuv
/// lightness/saturation the hues are built at (see [`crate::ui::glyph_color`]), so
/// colors stay legible on the chosen background. Affects **both** color modes: the
/// digit hues in `Rainbow`, and — since mono's focus/press accents are drawn from
/// the same palette — the highlight colors in `Mono` too. `Dark` is the default
/// (the common TUI case); toggled at runtime by the `t` key. A plain data enum —
/// the color math lives in `ui.rs`, not here.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Default)]
pub enum Theme {
    #[default]
    Dark,
    Light,
}

pub struct UiState {
    // The pads the user can switch between, and which one is active. Built at
    // startup (a `Keypad` allocates, so it can't be a `static`). `keypad()`
    // returns `&layouts[layout]`; everything downstream reads the active pad
    // through that one accessor, so multiplying pads didn't re-open the model.
    layouts: Vec<Keypad>,
    layout: usize,
    // `Some(i)` => the user pinned pad `i` (via the switch key), so resize leaves
    // it put; `None` => follow automatic shape-based selection. Cleared by the
    // resume-auto key.
    override_layout: Option<usize>,
    // The last terminal size auto-selection saw, cached so `resume_auto` can
    // re-pick for the current size without the size being threaded through the
    // event handler. Updated by `auto_select` (even while pinned).
    term_size: (u16, u16),
    focus: (usize, usize),         // lattice cell holding focus
    flash: Option<(usize, usize)>, // lattice cell of the button showing its press look
    flash_at: Instant,             // when the current flash began (see FLASH_DURATION)
    // Screen rect of each button (indexed like `keypad.buttons()`), captured by
    // the UI each draw. Mouse hit-testing reads these (see `button_at`).
    button_rects: Vec<Rect>,
    // Screen rect of the copy affordance, captured by the UI each draw (or
    // `Rect::ZERO` when it isn't shown). `copy_hit` clicks against it.
    copy_rect: Rect,
    // The transient copy status message and when it was set. Owned `String` (not
    // `&'static str`) so a failure can carry the actual `arboard` error detail —
    // a TUI has no log, so this status line is the only place it can surface.
    // `None` when nothing is being shown; expired by `tick` after `STATUS_DURATION`.
    status: Option<(String, Instant)>,
    // Per-digit rainbow (default) vs mono. Read by the renderer each draw; toggled
    // by the `r` key, routed at the I/O boundary in `main.rs` like the pad switch.
    color_mode: ColorMode,
    // Which background the palette is tuned for (dark by default). Affects both
    // modes — rainbow digit hues and mono highlight accents; toggled by the `t` key.
    theme: Theme,
}

impl UiState {
    pub fn new() -> Self {
        // The registry: the standard pad first (the startup default), then the
        // tall-narrow and wide-short pads. Index 0 is active, so behavior is
        // unchanged until the user switches (or `layout-auto` picks by shape).
        let layouts = vec![Keypad::standard(), Keypad::tall(), Keypad::wide()];
        let focus = layouts[0].default_focus();
        let button_rects = vec![Rect::ZERO; layouts[0].button_count()];
        Self {
            layouts,
            layout: 0,
            override_layout: None,
            term_size: (0, 0),
            focus,
            flash: None,
            flash_at: Instant::now(),
            button_rects,
            copy_rect: Rect::ZERO,
            status: None,
            color_mode: ColorMode::default(),
            theme: Theme::default(),
        }
    }

    /// Flip between mono and rainbow coloring. Routed from the `r` key in
    /// `main.rs` (like the pad switch and copy), *not* through the `Action` enum:
    /// coloring is a rendering concern that transforms no calculator state.
    pub fn toggle_color_mode(&mut self) {
        self.color_mode = match self.color_mode {
            ColorMode::Mono => ColorMode::Rainbow,
            ColorMode::Rainbow => ColorMode::Mono,
        };
    }

    /// The active color mode; the renderer reads it to decide whether to color
    /// digits. `Copy`, so callers hold a value rather than borrowing `self`.
    pub fn color_mode(&self) -> ColorMode {
        self.color_mode
    }

    /// Flip the palette between its dark- and light-background tunings. Routed from
    /// the `t` key in `main.rs`. Affects both modes: the rainbow digit hues and the
    /// mono focus/press accents (both drawn from the themed palette).
    pub fn toggle_theme(&mut self) {
        self.theme = match self.theme {
            Theme::Dark => Theme::Light,
            Theme::Light => Theme::Dark,
        };
    }

    /// The active palette theme; the renderer reads it to build hues at the right
    /// lightness for the background. `Copy`.
    pub fn theme(&self) -> Theme {
        self.theme
    }

    /// The active keypad; the UI reads its dimensions and buttons to render.
    pub fn keypad(&self) -> &Keypad {
        &self.layouts[self.layout]
    }

    /// Switch to the next pad in the registry, wrapping around, and **pin** it:
    /// the manual switch sets the override, so a later resize won't move off the
    /// pad the user chose. Routed from the I/O boundary in `main.rs` (like copy
    /// and focus moves), *not* through the `Action` enum: switching transforms no
    /// calculator state. Cleared by [`resume_auto`](Self::resume_auto).
    pub fn cycle_layout(&mut self) {
        let next = (self.layout + 1) % self.layouts.len();
        self.override_layout = Some(next);
        self.set_layout(next);
    }

    /// Pick and activate the pad that best fits a `w`×`h` terminal, unless the
    /// user has pinned one. Called on launch and on every resize.
    ///
    /// The size is cached (even while pinned) so [`resume_auto`](Self::resume_auto)
    /// can re-pick for the current terminal. While pinned this is otherwise a
    /// no-op — the override wins. Otherwise it switches only when the best pad
    /// actually *changes*, so a resize that doesn't cross a shape boundary leaves
    /// the user's focus and any in-progress press flash untouched.
    pub fn auto_select(&mut self, w: u16, h: u16) {
        self.term_size = (w, h);
        if self.override_layout.is_some() {
            return;
        }
        let best = self.select_for(w, h);
        if best != self.layout {
            self.set_layout(best);
        }
    }

    /// Clear a manual override and resume automatic selection, re-picking for the
    /// terminal size last seen. Routed from the resume-auto key in `main.rs`.
    pub fn resume_auto(&mut self) {
        self.override_layout = None;
        let (w, h) = self.term_size;
        self.auto_select(w, h);
    }

    /// The index of the pad that best fits a `w`×`h` terminal, by each pad's
    /// [`Keypad::fit_score`]. Ties resolve to the earliest pad (the standard pad
    /// at index 0): the scan keeps the incumbent unless a later pad *strictly*
    /// beats it.
    fn select_for(&self, w: u16, h: u16) -> usize {
        let mut best = 0;
        let mut best_score = self.layouts[0].fit_score(w, h);
        for i in 1..self.layouts.len() {
            let score = self.layouts[i].fit_score(w, h);
            if score > best_score {
                best = i;
                best_score = score;
            }
        }
        best
    }

    /// Make pad `i` (mod the registry size) active and fix up the per-pad UI state
    /// for it: the old lattice cell may not exist on the new pad (a `(4, 3)` focus
    /// is invalid on a 3×4 pad), so focus is re-resolved against the new pad; the
    /// press flash belongs to the pad we're leaving, so it's dropped; and the
    /// hit-test rects are resized to the new pad's button count so a click landing
    /// before the next draw can't reference the old pad's buttons.
    pub fn set_layout(&mut self, i: usize) {
        self.layout = i % self.layouts.len();
        self.focus = resolve_focus(self.focus, &self.layouts[self.layout]);
        // The press flash belongs to the pad we're leaving; drop it.
        self.flash = None;
        // `button_rects` is per-pad; resize to the new pad so hit-testing can't
        // reference the old pad's buttons before the next draw refills them.
        self.button_rects = vec![Rect::ZERO; self.layouts[self.layout].button_count()];
    }

    /// Move focus one **button** in `dir`, not one lattice cell.
    ///
    /// The distinction only shows up on a spanning button traversed along its
    /// own span axis: a per-cell step from the tall pad's wide `=` (cells
    /// `(6,0)`–`(6,1)`) lands back on `=`, so crossing it costs two presses. This
    /// skips every cell the current button owns, so it costs one.
    ///
    /// Focus is left where it is when the lattice edge is reached without finding
    /// another button — moving right from the rightmost column is a no-op, not a
    /// wrap.
    pub fn move_focus(&mut self, dir: Dir) {
        if let Some(cell) = next_button_cell(self.keypad(), self.focus, dir) {
            self.focus = cell;
        }
    }

    /// The label of the focused button. `&'static` because labels are `'static`,
    /// so the caller can hold it while mutably borrowing `self` elsewhere (e.g.
    /// `let l = ui.focused_label(); ui.register_press(l);`).
    pub fn focused_label(&self) -> &'static str {
        let idx = self.keypad().button_index_at(self.focus.0, self.focus.1);
        self.keypad().button(idx).label
    }

    /// The label of button `idx`. Used by the mouse path after `button_at`
    /// resolves a click to a button.
    pub fn button_label(&self, idx: usize) -> &'static str {
        self.keypad().button(idx).label
    }

    /// Whether button `idx` currently holds focus — i.e. the focused cell is one
    /// it covers. Resolved through the keypad's occupancy map, so a spanning
    /// button reads as focused from any of its cells. Read by the UI per button
    /// each draw.
    pub fn is_button_focused(&self, idx: usize) -> bool {
        self.keypad().button_index_at(self.focus.0, self.focus.1) == idx
    }

    /// Whether button `idx` is currently showing its pressed flash.
    pub fn is_button_pressed(&self, idx: usize) -> bool {
        self.flash
            .is_some_and(|(r, c)| self.keypad().button_index_at(r, c) == idx)
    }

    /// Record that `label` was just activated: focus follows it and its press
    /// flash starts. No-op if the label isn't on the grid. The run loop's `tick`
    /// clears the flash after `FLASH_DURATION`.
    pub fn register_press(&mut self, label: &str) {
        if let Some(pos) = self.keypad().position_of(label) {
            self.focus = pos;
            self.flash = Some(pos);
            self.flash_at = Instant::now();
        }
    }

    /// Record the screen rect of every button. Called by the UI once per draw so
    /// `button_at` can hit-test the *current* layout (the panel is re-centered on
    /// resize, so last frame's rects are the truth for the next mouse event).
    pub fn set_button_rects(&mut self, rects: Vec<Rect>) {
        self.button_rects = rects;
    }

    /// Resolve a click at terminal coordinates `(col, row)` to the button it
    /// landed on, or `None` if it missed every button.
    ///
    /// `button_rects[i]` is button `i`'s whole region as of the last draw (its
    /// union rect, including the border), so a spanning button is a single rect:
    /// a click anywhere on it — internal seams included — hits it, and only the
    /// gutters between distinct buttons miss. The layout tiles without overlap,
    /// so the first containing rect is the only one.
    pub fn button_at(&self, col: u16, row: u16) -> Option<usize> {
        let pos = Position { x: col, y: row };
        self.button_rects.iter().position(|rect| rect.contains(pos))
    }

    /// Record the screen rect of the copy affordance, or `Rect::ZERO` when it
    /// isn't shown. Called by the UI once per draw, mirroring `set_button_rects`,
    /// so `copy_hit` tests against the current layout.
    pub fn set_copy_rect(&mut self, rect: Rect) {
        self.copy_rect = rect;
    }

    /// Whether a click at `(col, row)` landed on the copy affordance. Always
    /// `false` when the affordance isn't shown, since its rect is then
    /// `Rect::ZERO` (zero-area rects contain no point).
    pub fn copy_hit(&self, col: u16, row: u16) -> bool {
        self.copy_rect.contains(Position { x: col, y: row })
    }

    /// Show a transient status message (e.g. "Copied!"). Replaces any current
    /// one and restarts its timer; `tick` clears it after `STATUS_DURATION`.
    pub fn set_status(&mut self, message: String) {
        self.status = Some((message, Instant::now()));
    }

    /// The status message currently on screen, or `None` if none is showing.
    pub fn status_text(&self) -> Option<&str> {
        self.status.as_ref().map(|(msg, _)| msg.as_str())
    }

    /// Dismiss the transient status message immediately, rather than waiting for
    /// `tick` to expire it after `STATUS_DURATION`. Called when the user starts a
    /// new edit (a digit, an operator, a paste): the "Copied!" line refers to the
    /// previous result, so it shouldn't linger over a fresh expression.
    pub fn clear_status(&mut self) {
        self.status = None;
    }

    /// Expire the press flash and the status message once each has been visible
    /// for its duration. Called once per run-loop iteration before drawing.
    pub fn tick(&mut self) {
        if self.flash.is_some() && self.flash_at.elapsed() >= FLASH_DURATION {
            self.flash = None;
        }
        if let Some((_, at)) = self.status
            && at.elapsed() >= STATUS_DURATION
        {
            self.status = None;
        }
    }

    /// The focused lattice cell. Test-only accessor for the input-routing tests
    /// in `main.rs`, which assert focus moved without reaching into the private
    /// field.
    #[cfg(test)]
    pub fn focus(&self) -> (usize, usize) {
        self.focus
    }

    /// The active pad's index in the registry. Test-only, for the switch tests.
    #[cfg(test)]
    pub fn layout_index(&self) -> usize {
        self.layout
    }

    /// The pinned-pad override, or `None` when following auto-selection.
    /// Test-only, for the override/resume tests.
    #[cfg(test)]
    pub fn override_layout(&self) -> Option<usize> {
        self.override_layout
    }
}

/// Walk from `from` in direction `dir` until reaching a cell owned by a
/// *different* button than the one covering `from`, and return that cell.
/// `None` if the lattice edge arrives first — i.e. there is no next button that
/// way.
///
/// This is what makes navigation per-button rather than per-cell: the cells a
/// spanning button owns are skipped in a single move. The cell returned is the
/// one focus *entered* through, deliberately **not** the destination button's
/// anchor — resting on the entry cell is what makes the move reversible. On the
/// wide pad, `"+"` at `(2,5)` → Right lands on `=`'s lower cell `(2,6)`, so Left
/// steps back to `(2,5)`; snapping to `=`'s anchor `(1,6)` instead would return
/// the user to `"⌫"`, a different key than they came from.
fn next_button_cell(pad: &Keypad, from: (usize, usize), dir: Dir) -> Option<(usize, usize)> {
    let start = pad.button_index_at(from.0, from.1);
    let mut cell = from;
    while let Some(next) = pad.step(cell.0, cell.1, dir) {
        if pad.button_index_at(next.0, next.1) != start {
            return Some(next);
        }
        cell = next;
    }
    None
}

/// Choose the focus cell for `pad` when switching to it, carrying the old cell
/// `(row, col)` over when possible.
///
/// Policy ("preserve, else default"): if `(row, col)` is a valid cell on `pad`,
/// keep the user roughly where they were — but snap to the **anchor** of the
/// button covering that cell. If the old cell is out of `pad`'s bounds, fall
/// back to `pad.default_focus()`.
///
/// The anchor snap applies to a *pad switch* only. Within a pad, focus may rest
/// on any cell of a spanning button — [`next_button_cell`] leaves it on the cell
/// it entered through, which is what keeps navigation reversible.
fn resolve_focus(old: (usize, usize), pad: &Keypad) -> (usize, usize) {
    let (row, col) = old;
    if row >= pad.rows() || col >= pad.cols() {
        return pad.default_focus();
    }
    let idx = pad.button_index_at(row, col);
    let b = pad.button(idx);
    (b.row as usize, b.col as usize)
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::layout::Keypad;

    #[test]
    fn move_focus_stops_at_the_edges() {
        // Off the top-left corner and off the bottom-right corner: no wrap, no
        // panic — focus just stays put.
        let mut ui = UiState::new(); // standard, 5×4
        ui.focus = (0, 0);
        ui.move_focus(Dir::Up);
        assert_eq!(ui.focus, (0, 0));
        ui.move_focus(Dir::Left);
        assert_eq!(ui.focus, (0, 0));
        ui.focus = (4, 3);
        ui.move_focus(Dir::Down);
        assert_eq!(ui.focus, (4, 3));
        ui.move_focus(Dir::Right);
        assert_eq!(ui.focus, (4, 3));
    }

    #[test]
    fn move_focus_steps_one_button_on_a_plain_pad() {
        // Every standard-pad key is 1×1, so per-button and per-cell coincide —
        // the baseline the spanning cases below deviate from.
        let mut ui = UiState::new();
        ui.focus = (2, 1); // "5"
        ui.move_focus(Dir::Left);
        assert_eq!(ui.focused_label(), "4");
        ui.move_focus(Dir::Up);
        assert_eq!(ui.focused_label(), "7");
        ui.move_focus(Dir::Right);
        assert_eq!(ui.focused_label(), "8");
        ui.move_focus(Dir::Down);
        assert_eq!(ui.focused_label(), "5");
    }

    #[test]
    fn crossing_a_horizontal_span_takes_one_press() {
        // The regression this task exists for. On the tall pad the bottom row is
        // ["=", "=", "+"], so a per-*cell* step right from ="s anchor (6, 0)
        // lands on (6, 1) — still "=" — and reaching "+" costs two presses.
        let mut ui = UiState::new();
        ui.set_layout(1); // tall
        ui.focus = (6, 0); // "=" anchor
        ui.move_focus(Dir::Right);
        assert_eq!(ui.focused_label(), "+"); // skipped ="s second cell
        assert_eq!(ui.focus, (6, 2)); // and landed on "+"'s own cell
    }

    #[test]
    fn crossing_a_horizontal_span_from_its_far_cell_takes_one_press() {
        // The other start-cell for the horizontal span: entering "=" from the
        // left leaves focus on its *far* cell (6, 1), and one more Right must
        // clear the whole button to "+". Guards the skip loop against an
        // off-by-one that only shows from a non-anchor start.
        let mut ui = UiState::new();
        ui.set_layout(1); // tall
        ui.focus = (6, 1); // ="s second (non-anchor) cell
        ui.move_focus(Dir::Right);
        assert_eq!(ui.focused_label(), "+");
        assert_eq!(ui.focus, (6, 2));
    }

    #[test]
    fn entering_a_horizontal_span_is_reversible() {
        // The row-span counterpart to `entering_a_span_is_reversible`: dropping
        // onto the tall pad's wide "=" from above lands on the entry cell (6, 1),
        // not the anchor (6, 0), so Up returns to "." rather than veering to "0".
        let mut ui = UiState::new();
        ui.set_layout(1); // tall
        ui.focus = (5, 1); // "." (row ["0", ".", "-"])
        ui.move_focus(Dir::Down);
        assert_eq!(ui.focused_label(), "=");
        assert_eq!(ui.focus, (6, 1)); // entry cell, not the anchor (6, 0)
        ui.move_focus(Dir::Up);
        assert_eq!(ui.focused_label(), "."); // back where we started
    }

    #[test]
    fn crossing_a_vertical_span_takes_one_press() {
        // The row-span counterpart: on the wide pad "=" is 2×1 at col 6 spanning
        // rows 1–2. Stepping up from its anchor must clear the whole button.
        let mut ui = UiState::new();
        ui.set_layout(2); // wide
        ui.focus = (2, 6); // ="s lower cell
        ui.move_focus(Dir::Up);
        assert_eq!(ui.focused_label(), ")"); // (0, 6), skipping (1, 6)
    }

    #[test]
    fn entering_a_span_is_reversible() {
        // Focus rests on the cell it entered a spanning button through, not the
        // button's anchor, so the move undoes exactly. Snapping to ="s anchor
        // (1, 6) here would send the return trip to "⌫" instead of "+".
        let mut ui = UiState::new();
        ui.set_layout(2); // wide
        ui.focus = (2, 5); // "+"
        ui.move_focus(Dir::Right);
        assert_eq!(ui.focused_label(), "=");
        assert_eq!(ui.focus, (2, 6)); // the entry cell, not the anchor (1, 6)
        ui.move_focus(Dir::Left);
        assert_eq!(ui.focused_label(), "+"); // back where we started
    }

    #[test]
    fn move_focus_within_a_span_is_a_noop_at_the_edge() {
        // From ="s lower cell on the wide pad, Down would only reach ="s own
        // cells before running off the lattice — so focus stays, rather than the
        // walk falling off the end or spinning.
        let mut ui = UiState::new();
        ui.set_layout(2); // wide
        ui.focus = (1, 6); // ="s anchor; (2, 6) below is the same button
        ui.move_focus(Dir::Down);
        assert_eq!(ui.focus, (1, 6));
    }

    #[test]
    fn focused_label_default() {
        assert_eq!(UiState::new().focused_label(), "=");
    }

    #[test]
    fn cycle_layout_advances_and_wraps() {
        let mut ui = UiState::new();
        assert_eq!(ui.layout_index(), 0); // standard active at startup
        ui.cycle_layout();
        assert_eq!(ui.layout_index(), 1); // tall
        ui.cycle_layout();
        assert_eq!(ui.layout_index(), 2); // wide
        ui.cycle_layout();
        assert_eq!(ui.layout_index(), 0); // wrapped back to standard
    }

    #[test]
    fn cycle_pins_the_override() {
        // The manual switch key pins the chosen pad so a later resize won't move
        // off it — Tab sets the override, `a` (resume_auto) clears it.
        let mut ui = UiState::new();
        assert_eq!(ui.override_layout(), None); // auto at startup
        ui.cycle_layout();
        assert_eq!(ui.override_layout(), Some(1));
        ui.cycle_layout();
        assert_eq!(ui.override_layout(), Some(2));
    }

    #[test]
    fn auto_select_is_noop_while_pinned() {
        // With a pad pinned, auto_select (fired on resize) must leave it put — the
        // override wins regardless of what shape the terminal became. Independent
        // of the fit heuristic: the override short-circuits before scoring.
        let mut ui = UiState::new();
        ui.cycle_layout(); // pin pad 1
        assert_eq!(ui.layout_index(), 1);
        ui.auto_select(200, 60); // a wide-short terminal
        assert_eq!(ui.layout_index(), 1); // still pinned
        assert_eq!(ui.override_layout(), Some(1));
    }

    #[test]
    fn resume_auto_clears_the_override() {
        // `a` un-pins and returns to automatic selection. (Which pad it lands on
        // depends on the fit heuristic; here we only assert the override cleared,
        // so this stays green before fit_score is implemented.)
        let mut ui = UiState::new();
        ui.cycle_layout(); // pin
        assert_eq!(ui.override_layout(), Some(1));
        ui.resume_auto();
        assert_eq!(ui.override_layout(), None);
    }

    #[test]
    fn select_for_picks_shape_appropriate_pad() {
        // Representative shapes → the pad whose aspect ratio matches. Ties resolve
        // to standard (index 0).
        let ui = UiState::new();
        assert_eq!(ui.select_for(30, 45), 1); // narrow-tall → tall pad
        assert_eq!(ui.select_for(70, 40), 2); // wide-short → wide pad
        assert_eq!(ui.select_for(40, 40), 0); // squarish → standard pad
        // The wide pad best matches this landscape shape but is 1 column too wide
        // to fit (needs 49); the overflow gate must disqualify it so the fitting
        // standard pad wins. Regression guard for the "best aspect but doesn't
        // fit" path.
        assert_eq!(ui.select_for(48, 29), 0);
        // Every pad fits here, so the choice rests purely on the ratio distance —
        // which only ranks correctly once it's normalised by each pad's own width.
        assert_eq!(ui.select_for(60, 40), 2);
    }

    #[test]
    fn startup_state_is_standard_pad_unpinned() {
        // The launch default: a freshly constructed UiState sits on the standard 5×4
        // pad, unpinned, *before* any resize — `run()` no longer seeds a
        // shape-appropriate pad from the initial terminal size. Even a tall terminal
        // (which `select_for` would map to the tall pad) must not have been applied
        // yet, so re-adding the startup auto_select seed would fail this guard.
        let ui = UiState::new();
        assert_eq!(ui.layout_index(), 0); // standard
        assert_eq!(ui.override_layout(), None); // auto, not pinned
        assert_eq!(ui.select_for(30, 45), 1); // a tall shape *would* pick tall…
        assert_eq!(ui.layout_index(), 0); // …but startup ignored shape
    }

    #[test]
    fn auto_select_follows_shape_when_auto() {
        // In auto mode a resize switches to the best-fit pad.
        let mut ui = UiState::new();
        ui.auto_select(30, 45);
        assert_eq!(ui.layout_index(), 1); // tall
        ui.auto_select(70, 40);
        assert_eq!(ui.layout_index(), 2); // wide
    }

    #[test]
    fn auto_select_preserves_flash_when_pad_unchanged() {
        // A resize that doesn't cross a shape boundary picks the same pad, so the
        // guard skips set_layout and an in-progress press flash survives.
        let mut ui = UiState::new();
        ui.auto_select(40, 40); // standard
        ui.register_press("7"); // start a flash
        let flash = ui.flash;
        assert!(flash.is_some());
        ui.auto_select(40, 40); // same shape → same pad → no churn
        assert_eq!(ui.flash, flash); // flash not dropped
    }

    #[test]
    fn switch_falls_back_to_default_when_cell_gone() {
        // tall (7 rows) → standard (5 rows): a focus on tall's bottom rows (5+)
        // doesn't exist on standard, so focus falls back to standard's home.
        let mut ui = UiState::new();
        ui.set_layout(1); // tall
        ui.focus = (5, 0); // tall-only row ("0")
        ui.set_layout(0); // standard
        assert_eq!(ui.focus(), ui.keypad().default_focus());
    }

    #[test]
    fn switch_preserves_in_bounds_focus() {
        // (2, 1) is a plain 1×1 digit cell in bounds on both standard ("5") and
        // tall ("8"), so switching keeps focus there rather than resetting to the
        // new pad's home.
        let mut ui = UiState::new(); // standard, focus on "="
        ui.focus = (2, 1);
        ui.set_layout(1); // tall
        assert_eq!(ui.focus(), (2, 1));
    }

    #[test]
    fn switch_snaps_onto_span_anchor() {
        // On the tall pad the bottom-row "=" is wide (1×2, anchor (6, 0)); (6, 1)
        // is its second cell. Preserving focus must snap to the covering button's
        // anchor, never a non-anchor cell of a span.
        let mut ui = UiState::new();
        ui.focus = (6, 1);
        ui.set_layout(1); // tall
        assert_eq!(ui.focus(), (6, 0));
    }

    #[test]
    fn switch_snaps_onto_vertical_span_anchor() {
        // On the wide pad the right-edge "=" is tall (2×1, anchor (1, 6)); (2, 6)
        // is its lower cell — the row-span counterpart of the wide-"=" case above.
        // Preserving must snap up to the anchor, never rest on the covered cell.
        let mut ui = UiState::new();
        ui.set_layout(2); // wide
        ui.focus = (2, 6);
        ui.set_layout(2); // re-resolve against the wide pad itself
        assert_eq!(ui.focus(), (1, 6));
    }

    #[test]
    fn switch_clears_stale_flash() {
        // The press flash names a cell on the pad we're leaving; carrying it over
        // would flash an unrelated button on the new pad (or, for a cell the new pad
        // doesn't have, index its occupancy map out of bounds). set_layout drops it.
        let mut ui = UiState::new();
        ui.register_press("5"); // flash on standard's (2, 1)
        assert!(ui.flash.is_some());
        ui.set_layout(1); // tall
        assert_eq!(ui.flash, None);
    }

    #[test]
    fn keypad_positions_labels_and_misses() {
        let k = Keypad::standard();
        assert_eq!(k.position_of("C"), Some((0, 0)));
        assert_eq!(k.position_of("="), Some((4, 3)));
        assert_eq!(k.position_of("5"), Some((2, 1)));
        assert_eq!(k.position_of("⌫"), Some((4, 0)));
        assert_eq!(k.position_of("?"), None);
    }

    #[test]
    fn register_press_moves_focus_and_flashes() {
        let mut ui = UiState::new(); // focus starts on "=" at (4, 3)
        ui.register_press("5");
        assert_eq!(ui.focus, (2, 1)); // focus followed the input
        assert_eq!(ui.flash, Some((2, 1))); // and that cell is flashing
    }

    #[test]
    fn button_at_resolves_clicks_to_buttons() {
        // Give each button a 7×5 rect at (col*7, row*5). This mirrors the real
        // cell size but is independent of the UI geometry, so the test pins down
        // `button_at`'s hit-test logic, not the layout.
        let mut ui = UiState::new();
        let mut rects = vec![Rect::ZERO; ui.keypad().button_count()];
        for (i, b) in ui.keypad().buttons().iter().enumerate() {
            rects[i] = Rect::new(b.col * 7, b.row * 5, 7, 5);
        }
        ui.set_button_rects(rects);

        // A point inside the "7" cell (row 1, col 0 → x∈[0,7), y∈[5,10)).
        let hit = ui.button_at(3, 7).expect("hit a button");
        assert_eq!(ui.button_label(hit), "7");
        // The "=" cell (row 4, col 3).
        let eq = ui.button_at(23, 22).expect("hit a button");
        assert_eq!(ui.button_label(eq), "=");
        // A click well outside every button hits nothing.
        assert_eq!(ui.button_at(200, 200), None);
    }

    #[test]
    fn register_press_ignores_unknown_label() {
        let mut ui = UiState::new();
        ui.register_press("?");
        assert_eq!(ui.focus, (4, 3)); // unchanged
        assert_eq!(ui.flash, None);
    }

    #[test]
    fn copy_hit_tests_against_the_stored_rect() {
        let mut ui = UiState::new();
        // No affordance shown yet → rect is ZERO, so nothing is a hit.
        assert!(!ui.copy_hit(0, 0));

        ui.set_copy_rect(Rect::new(2, 1, 8, 1)); // x∈[2,10), y == 1
        assert!(ui.copy_hit(2, 1)); // top-left corner is inside
        assert!(ui.copy_hit(9, 1)); // last column inside
        assert!(!ui.copy_hit(10, 1)); // just past the right edge
        assert!(!ui.copy_hit(5, 2)); // wrong row
    }

    #[test]
    fn status_set_and_read() {
        let mut ui = UiState::new();
        assert_eq!(ui.status_text(), None);
        ui.set_status("Copied!".to_string());
        assert_eq!(ui.status_text(), Some("Copied!"));
        // A fresh status is within STATUS_DURATION, so tick keeps it.
        ui.tick();
        assert_eq!(ui.status_text(), Some("Copied!"));
        // A new edit dismisses it immediately, without waiting for expiry.
        ui.clear_status();
        assert_eq!(ui.status_text(), None);
    }

    #[test]
    fn toggle_color_mode_flips_and_round_trips() {
        // Starts rainbow (the default); each press flips, so two presses return to it.
        let mut ui = UiState::new();
        assert_eq!(ui.color_mode(), ColorMode::Rainbow);
        ui.toggle_color_mode();
        assert_eq!(ui.color_mode(), ColorMode::Mono);
        ui.toggle_color_mode();
        assert_eq!(ui.color_mode(), ColorMode::Rainbow);
    }

    #[test]
    fn toggle_theme_flips_and_round_trips() {
        // Dark by default; each press flips, two presses return to dark.
        let mut ui = UiState::new();
        assert_eq!(ui.theme(), Theme::Dark);
        ui.toggle_theme();
        assert_eq!(ui.theme(), Theme::Light);
        ui.toggle_theme();
        assert_eq!(ui.theme(), Theme::Dark);
    }

    #[test]
    fn tick_keeps_fresh_flash() {
        // A flash set this instant is well within FLASH_DURATION, so tick must
        // leave it visible. (Expiry after the duration is paced by the run loop
        // and exercised manually rather than with a sleep here.)
        let mut ui = UiState::new();
        ui.register_press("5");
        ui.tick();
        assert_eq!(ui.flash, Some((2, 1)));
    }
}
