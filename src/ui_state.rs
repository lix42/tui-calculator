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

use crate::layout::Keypad;

/// How long a button stays in its "pressed" look after activation. Terminals
/// have no key-release event, so the press is shown as a brief flash that the
/// run loop's `tick` clears once this much time has passed.
const FLASH_DURATION: Duration = Duration::from_millis(120);

/// How long the copy status message ("Copied!" / "Copy failed") stays on screen.
/// Longer than `FLASH_DURATION` because this is text the user needs to *read*,
/// not a momentary blink. Cleared by the same `tick` that expires the flash.
const STATUS_DURATION: Duration = Duration::from_millis(1500);

pub struct UiState {
    keypad: Keypad,
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
}

impl UiState {
    pub fn new() -> Self {
        let keypad = Keypad::standard();
        // Home the cursor on "=" (its anchor cell), matching the old (4, 3) seed.
        let focus = keypad.position_of("=").unwrap_or((0, 0));
        let button_rects = vec![Rect::ZERO; keypad.button_count()];
        Self {
            keypad,
            focus,
            flash: None,
            flash_at: Instant::now(),
            button_rects,
            copy_rect: Rect::ZERO,
            status: None,
        }
    }

    /// The active keypad; the UI reads its dimensions and buttons to render.
    pub fn keypad(&self) -> &Keypad {
        &self.keypad
    }

    pub fn move_focus(&mut self, dr: i32, dc: i32) {
        let rows = self.keypad.rows() as i32;
        let cols = self.keypad.cols() as i32;
        self.focus.0 = (self.focus.0 as i32 + dr).clamp(0, rows - 1) as usize;
        self.focus.1 = (self.focus.1 as i32 + dc).clamp(0, cols - 1) as usize;
    }

    /// The label of the focused button. `&'static` because labels are `'static`,
    /// so the caller can hold it while mutably borrowing `self` elsewhere (e.g.
    /// `let l = ui.focused_label(); ui.register_press(l);`).
    pub fn focused_label(&self) -> &'static str {
        let idx = self.keypad.button_index_at(self.focus.0, self.focus.1);
        self.keypad.button(idx).label
    }

    /// The label of button `idx`. Used by the mouse path after `button_at`
    /// resolves a click to a button.
    pub fn button_label(&self, idx: usize) -> &'static str {
        self.keypad.button(idx).label
    }

    /// Whether button `idx` currently holds focus — i.e. the focused cell is one
    /// it covers. Resolved through the keypad's occupancy map, so a spanning
    /// button reads as focused from any of its cells. Read by the UI per button
    /// each draw.
    pub fn is_button_focused(&self, idx: usize) -> bool {
        self.keypad.button_index_at(self.focus.0, self.focus.1) == idx
    }

    /// Whether button `idx` is currently showing its pressed flash.
    pub fn is_button_pressed(&self, idx: usize) -> bool {
        self.flash
            .is_some_and(|(r, c)| self.keypad.button_index_at(r, c) == idx)
    }

    /// Record that `label` was just activated: focus follows it and its press
    /// flash starts. No-op if the label isn't on the grid. The run loop's `tick`
    /// clears the flash after `FLASH_DURATION`.
    pub fn register_press(&mut self, label: &str) {
        if let Some(pos) = self.keypad.position_of(label) {
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
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::layout::Keypad;

    #[test]
    fn move_focus_clamps() {
        let mut ui = UiState::new();
        ui.focus = (0, 0);
        ui.move_focus(-5, -5);
        assert_eq!(ui.focus, (0, 0));
        ui.move_focus(99, 99);
        assert_eq!(ui.focus, (4, 3));
    }

    #[test]
    fn focused_label_default() {
        assert_eq!(UiState::new().focused_label(), "=");
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
