//! UI state: button-grid focus, the momentary press flash, the on-screen
//! geometry used for mouse hit-testing, and the copy affordance + its status.
//!
//! This is the rendering/input-routing half of what used to live in `App`. It
//! owns the grid (`BUTTONS`), where focus currently sits, which cell is flashing,
//! and the screen rect of every cell. `App` keeps only the calculator state
//! (`expr` / `current` / `mode`); the two have different lifecycles and concerns.

use std::collections::HashMap;
use std::sync::LazyLock;
use std::time::{Duration, Instant};

use ratatui::layout::{Position, Rect};

/// How long a button stays in its "pressed" look after activation. Terminals
/// have no key-release event, so the press is shown as a brief flash that the
/// run loop's `tick` clears once this much time has passed.
const FLASH_DURATION: Duration = Duration::from_millis(120);

/// How long the copy status message ("Copied!" / "Copy failed") stays on screen.
/// Longer than `FLASH_DURATION` because this is text the user needs to *read*,
/// not a momentary blink. Cleared by the same `tick` that expires the flash.
const STATUS_DURATION: Duration = Duration::from_millis(1500);

pub const BUTTONS: [[&str; 4]; 5] = [
    ["C", "(", ")", "÷"],
    ["7", "8", "9", "×"],
    ["4", "5", "6", "-"],
    ["1", "2", "3", "+"],
    ["⌫", "0", ".", "="],
];

pub struct UiState {
    focus: (usize, usize),
    flash: Option<(usize, usize)>, // button showing its momentary "pressed" look
    flash_at: Instant,             // when the current flash began (see FLASH_DURATION)
    // Screen rect of each grid cell, captured by the UI each draw. Mouse
    // hit-testing reads these (see `button_at`).
    button_rects: [[Rect; 4]; 5],
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
        Self {
            focus: (4, 3),
            flash: None,
            flash_at: Instant::now(),
            button_rects: [[Rect::ZERO; 4]; 5],
            copy_rect: Rect::ZERO,
            status: None,
        }
    }

    pub fn move_focus(&mut self, dr: i32, dc: i32) {
        let rows = BUTTONS.len() as i32;
        let cols = BUTTONS[0].len() as i32;
        self.focus.0 = (self.focus.0 as i32 + dr).clamp(0, rows - 1) as usize;
        self.focus.1 = (self.focus.1 as i32 + dc).clamp(0, cols - 1) as usize;
    }

    /// `&'static` because `BUTTONS` is a `'static` const — returning it untied
    /// from `&self` lets the caller hold the label while mutably borrowing
    /// elsewhere (e.g. `let l = ui.focused_label(); ui.register_press(l);`).
    pub fn focused_label(&self) -> &'static str {
        BUTTONS[self.focus.0][self.focus.1]
    }

    /// Whether the button at `pos` currently holds focus. Mirrors `is_pressed`;
    /// keeps `focus` private so it can only move through `move_focus` /
    /// `register_press` (both bounds-safe) — a raw write could put it
    /// out of range and panic `focused_label`.
    pub fn is_focused(&self, pos: (usize, usize)) -> bool {
        self.focus == pos
    }

    /// Record that `label` was just activated: focus follows it and its press
    /// flash starts. No-op if the label isn't on the grid. The run loop's
    /// `tick` clears the flash after `FLASH_DURATION`.
    pub fn register_press(&mut self, label: &str) {
        if let Some(pos) = position_of(label) {
            self.focus = pos;
            self.flash = Some(pos);
            self.flash_at = Instant::now();
        }
    }

    /// Whether the button at `pos` is currently showing its pressed flash.
    pub fn is_pressed(&self, pos: (usize, usize)) -> bool {
        self.flash == Some(pos)
    }

    /// Record the screen rect of every grid cell. Called by the UI once per
    /// draw so `button_at` can hit-test the *current* layout (the panel is
    /// re-centered on resize, so last frame's rects are the truth for the next
    /// mouse event).
    pub fn set_button_rects(&mut self, rects: [[Rect; 4]; 5]) {
        self.button_rects = rects;
    }

    /// Resolve a click at terminal coordinates `(col, row)` to the grid cell it
    /// landed on, or `None` if it missed every button.
    ///
    /// `self.button_rects[r][c]` holds the screen `Rect` of each cell as of the
    /// last draw. Each rect spans its cell *including* the border, so a click on
    /// a button's frame still counts as a hit — the generous behavior we want,
    /// no inset math needed. The layout tiles without overlap, so the first
    /// containing cell is the only one.
    pub fn button_at(&self, col: u16, row: u16) -> Option<(usize, usize)> {
        let pos = Position { x: col, y: row };
        for (r, grid_row) in self.button_rects.iter().enumerate() {
            for (c, row_cell) in grid_row.iter().enumerate() {
                if row_cell.contains(pos) {
                    return Some((r, c));
                }
            }
        }
        None
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
}

/// Reverse index of `BUTTONS`: label → grid position. Built once on first
/// lookup (`LazyLock`) and derived from `BUTTONS`, so it stays in sync with the
/// grid — `BUTTONS` is the single source of truth — at no startup cost.
static LABEL_POS: LazyLock<HashMap<&'static str, (usize, usize)>> = LazyLock::new(|| {
    let mut map = HashMap::new();
    for (r, row) in BUTTONS.iter().enumerate() {
        for (c, label) in row.iter().enumerate() {
            map.insert(*label, (r, c));
        }
    }
    map
});

/// Grid position of `label`, or `None` if no button carries it. The inverse of
/// `BUTTONS[r][c]`; used to make focus follow keyboard input and to locate the
/// cell to flash.
pub fn position_of(label: &str) -> Option<(usize, usize)> {
    LABEL_POS.get(label).copied()
}

#[cfg(test)]
mod tests {
    use super::*;

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
    fn position_of_finds_labels_and_misses() {
        assert_eq!(position_of("C"), Some((0, 0)));
        assert_eq!(position_of("="), Some((4, 3)));
        assert_eq!(position_of("5"), Some((2, 1)));
        assert_eq!(position_of("⌫"), Some((4, 0)));
        assert_eq!(position_of("?"), None);
    }

    #[test]
    fn register_press_moves_focus_and_flashes() {
        let mut ui = UiState::new(); // focus starts on "=" at (4, 3)
        ui.register_press("5");
        assert_eq!(ui.focus, (2, 1)); // focus followed the input
        assert!(ui.is_pressed((2, 1))); // and that cell is flashing
        assert!(!ui.is_pressed((4, 3))); // the old focus is not
    }

    #[test]
    fn button_at_resolves_clicks_to_cells() {
        // Synthetic grid: cell (r, c) is a 7×5 rect at (c*7, r*5). This mirrors
        // the real layout's fixed cell size but is independent of it, so the
        // test pins down `button_at`'s hit-test logic, not the UI geometry.
        let mut ui = UiState::new();
        let mut rects = [[Rect::ZERO; 4]; 5];
        for (r, row) in rects.iter_mut().enumerate() {
            for (c, cell) in row.iter_mut().enumerate() {
                *cell = Rect::new(c as u16 * 7, r as u16 * 5, 7, 5);
            }
        }
        ui.set_button_rects(rects);

        // A point inside the "7" cell (row 1, col 0 → x∈[0,7), y∈[5,10)).
        assert_eq!(ui.button_at(3, 7), Some((1, 0)));
        assert_eq!(BUTTONS[1][0], "7");
        // The "=" cell (row 4, col 3).
        assert_eq!(ui.button_at(23, 22), Some((4, 3)));
        // A click well outside every cell hits nothing.
        assert_eq!(ui.button_at(200, 200), None);
    }

    #[test]
    fn register_press_ignores_unknown_label() {
        let mut ui = UiState::new();
        ui.register_press("?");
        assert_eq!(ui.focus, (4, 3)); // unchanged
        assert!(!ui.is_pressed((4, 3)));
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
        assert!(ui.is_pressed((2, 1)));
    }
}
