//! Keypad layout: a lattice of equal cells with buttons that may span several
//! cells. A pad is authored as an *occupancy grid* of label tokens and compiled
//! into this trusted form at startup — the span/coverage invariants are checked
//! in `compile`, so the rest of the UI can treat a `Keypad` as valid geometry.

use std::collections::HashMap;

/// Fixed on-screen size of one lattice cell (columns × rows) and the height of
/// the display box above the grid. Both the renderer ([`crate::ui`]) and the
/// shape-fit heuristic ([`Keypad::fit_score`]) size pads from these, so the
/// panel geometry has a single source of truth.
pub const CELL_W: u16 = 7;
pub const CELL_H: u16 = 5;
pub const DISPLAY_H: u16 = 4;

/// A button occupying a rectangular region of the lattice. `(row, col)` is its
/// top-left (anchor) cell; a plain key is `1×1`, a wide `0` is `1×2`, a tall `=`
/// is `2×1`.
pub struct Button {
    pub label: &'static str,
    pub row: u16,
    pub col: u16,
    pub row_span: u16,
    pub col_span: u16,
}

/// A compiled keypad: the lattice dimensions, its buttons (in reading order),
/// and a `cell -> button index` map so focus and hit-testing resolve a cell to
/// the button covering it in O(1).
pub struct Keypad {
    rows: usize,
    cols: usize,
    buttons: Vec<Button>,
    occupancy: Vec<Vec<usize>>, // [row][col] -> index into `buttons`
    label_pos: HashMap<&'static str, (usize, usize)>, // label -> anchor cell
    default_focus: (usize, usize), // the pad's home cell (a button anchor)
}

/// The standard macOS-style pad. Every key is `1×1`; spanning exists in the
/// model (see `compile`) but this pad doesn't use it.
const STANDARD: &[&[&str]] = &[
    &["C", "(", ")", "÷"],
    &["7", "8", "9", "×"],
    &["4", "5", "6", "-"],
    &["1", "2", "3", "+"],
    &["⌫", "0", ".", "="],
];

/// A **tall, narrow** pad (7 rows × 3 cols): the standard key set stacked into
/// three columns, with a wide `=` (`1×2`) on the bottom row. Its portrait aspect
/// ratio makes it the best fit for a narrow-tall terminal (see
/// [`Keypad::fit_score`]); it also exercises a horizontal span on a real pad.
const TALL: &[&[&str]] = &[
    &["C", "⌫", "÷"],
    &["(", ")", "×"],
    &["7", "8", "9"],
    &["4", "5", "6"],
    &["1", "2", "3"],
    &["0", ".", "-"],
    &["=", "=", "+"],
];

/// A **wide, short** pad (3 rows × 7 cols): the numeric keypad as a 3×3 block on
/// the left, operators and editing keys to the right, and a tall `=` (`2×1`)
/// anchoring the right edge. Its landscape aspect ratio makes it the best fit for
/// a wide-short terminal (see [`Keypad::fit_score`]); it exercises a vertical
/// span, the row-span counterpart to `TALL`'s wide `=`.
const WIDE: &[&[&str]] = &[
    &["7", "8", "9", "÷", "C", "(", ")"],
    &["4", "5", "6", "×", "-", "⌫", "="],
    &["1", "2", "3", "0", ".", "+", "="],
];

impl Keypad {
    /// The standard pad, compiled once by the caller (e.g. `UiState::new`).
    /// Homes focus on `"="`.
    pub fn standard() -> Self {
        compile(STANDARD, "=")
    }

    /// The tall, narrow pad (see [`TALL`]). Homes focus on `"="`.
    pub fn tall() -> Self {
        compile(TALL, "=")
    }

    /// The wide, short pad (see [`WIDE`]). Homes focus on `"="`.
    pub fn wide() -> Self {
        compile(WIDE, "=")
    }

    pub fn rows(&self) -> usize {
        self.rows
    }

    pub fn cols(&self) -> usize {
        self.cols
    }

    pub fn button_count(&self) -> usize {
        self.buttons.len()
    }

    pub fn buttons(&self) -> &[Button] {
        &self.buttons
    }

    pub fn button(&self, idx: usize) -> &Button {
        &self.buttons[idx]
    }

    /// The index of the button covering lattice cell `(row, col)`. Every cell is
    /// covered (checked in `compile`), so this never goes out of range for an
    /// in-bounds cell.
    pub fn button_index_at(&self, row: usize, col: usize) -> usize {
        self.occupancy[row][col]
    }

    /// The anchor (top-left) cell of the button carrying `label`, or `None` if
    /// no button does. The inverse of a button's `(row, col)`.
    pub fn position_of(&self, label: &str) -> Option<(usize, usize)> {
        self.label_pos.get(label).copied()
    }

    /// The pad's home cell — where focus lands when switching to this pad can't
    /// carry the old cell over (it's out of bounds). Always a button anchor,
    /// resolved from a label at compile time.
    pub fn default_focus(&self) -> (usize, usize) {
        self.default_focus
    }

    /// A shape-fit score for a terminal of `w`×`h` cells: **higher means a
    /// better fit**, and the scores across pads must be **totally ordered** so
    /// [`crate::ui_state::UiState`]'s selector can take a unique maximum.
    ///
    /// Contract for whatever scoring you choose:
    /// - A pad that **can't physically fit** — the terminal is narrower than
    ///   `cols * CELL_W` or shorter than `rows * CELL_H + DISPLAY_H` — must score
    ///   *below every pad that fits*, so a fitting pad is always preferred. When
    ///   *nothing* fits, the least-overflowing pad should still come out on top.
    /// - Among pads that fit, prefer the one whose **shape best matches** the
    ///   terminal: a portrait (narrow-tall) terminal should favour the tall pad,
    ///   a landscape (wide-short) one the wide pad, and a squarish terminal the
    ///   standard pad. Comparing the pad's aspect ratio to the terminal's — e.g.
    ///   `need_h * w` vs `h * need_w`, to stay in integers — is one way.
    ///
    /// The selector breaks ties toward the earliest pad (standard), so an exact
    /// tie is safe.
    pub fn fit_score(&self, w: u16, h: u16) -> i32 {
        let (w, h) = (w as i32, h as i32);
        let need_w = self.cols as i32 * CELL_W as i32;
        let need_h = self.rows as i32 * CELL_H as i32 + DISPLAY_H as i32;
        if w < need_w || h < need_h {
            let overflow = (need_w - w).max(0) + (need_h - h).max(0);
            return -1_000_000_000 - overflow;
        }
        -(need_h * w - h * need_w).abs()
    }
}

/// Compile an occupancy grid into a [`Keypad`], validating the invariants the
/// rest of the UI relies on. **Panics** on a malformed pad — pads are static
/// data, so a violation is a programming error to catch at startup, not a
/// runtime condition to handle.
///
/// A token that repeats across adjacent cells *is* a spanning button; its region
/// is the bounding box of its cells. The checks reject anything that would make
/// a button's bounding box lie about its region: a ragged grid, and a token
/// whose cells don't fill their bounding box (an L-shape, or the same label
/// reused in two disjoint places).
///
/// `default_focus_label` names the pad's home button and must appear on the pad;
/// an unknown label panics like the static-data violations above.
fn compile(grid: &[&[&'static str]], default_focus_label: &'static str) -> Keypad {
    assert!(!grid.is_empty(), "keypad has no rows");
    let rows = grid.len();
    let cols = grid[0].len();
    assert!(cols > 0, "keypad has no columns");

    // Gather each token's cells in first-appearance (reading) order, so button
    // indices run top-to-bottom, left-to-right.
    let mut order: Vec<&'static str> = Vec::new();
    let mut cells: HashMap<&'static str, Vec<(u16, u16)>> = HashMap::new();
    for (r, row) in grid.iter().enumerate() {
        assert_eq!(row.len(), cols, "keypad is not rectangular (row {r})");
        for (c, &label) in row.iter().enumerate() {
            cells
                .entry(label)
                .or_insert_with(|| {
                    order.push(label);
                    Vec::new()
                })
                .push((r as u16, c as u16));
        }
    }

    let mut buttons = Vec::with_capacity(order.len());
    let mut index: HashMap<&'static str, usize> = HashMap::new();
    let mut label_pos: HashMap<&'static str, (usize, usize)> = HashMap::new();
    for (i, &label) in order.iter().enumerate() {
        let cs = &cells[label];
        let min_r = cs.iter().map(|&(r, _)| r).min().unwrap();
        let max_r = cs.iter().map(|&(r, _)| r).max().unwrap();
        let min_c = cs.iter().map(|&(_, c)| c).min().unwrap();
        let max_c = cs.iter().map(|&(_, c)| c).max().unwrap();
        let row_span = max_r - min_r + 1;
        let col_span = max_c - min_c + 1;
        // Every cell of the bounding box must belong to this token; otherwise the
        // span is ragged or the label is reused in two disjoint places.
        assert_eq!(
            cs.len(),
            row_span as usize * col_span as usize,
            "button '{label}' does not form a filled rectangle"
        );
        buttons.push(Button {
            label,
            row: min_r,
            col: min_c,
            row_span,
            col_span,
        });
        index.insert(label, i);
        label_pos.insert(label, (min_r as usize, min_c as usize));
    }

    let mut occupancy = vec![vec![0usize; cols]; rows];
    for (r, row) in grid.iter().enumerate() {
        for (c, &label) in row.iter().enumerate() {
            occupancy[r][c] = index[label];
        }
    }

    // The home cell must name a real button; a typo is a programming error in
    // static data, caught here like the span invariants above.
    let default_focus = *label_pos
        .get(default_focus_label)
        .unwrap_or_else(|| panic!("default-focus label '{default_focus_label}' is not on the pad"));

    Keypad {
        rows,
        cols,
        buttons,
        occupancy,
        label_pos,
        default_focus,
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn standard_is_all_1x1() {
        let k = Keypad::standard();
        assert_eq!((k.rows(), k.cols()), (5, 4));
        assert_eq!(k.button_count(), 20);
        assert!(
            k.buttons()
                .iter()
                .all(|b| b.row_span == 1 && b.col_span == 1)
        );
    }

    #[test]
    fn reading_order_and_occupancy() {
        let k = Keypad::standard();
        // Buttons run in reading order: first is top-left "C", last is "=".
        assert_eq!(k.button(0).label, "C");
        assert_eq!(k.button(k.button_count() - 1).label, "=");
        // occupancy resolves a cell to the button whose label sits there.
        let i = k.button_index_at(2, 1);
        assert_eq!(k.button(i).label, "5");
        assert_eq!(k.position_of("5"), Some((2, 1)));
        assert_eq!(k.position_of("⌫"), Some((4, 0)));
        assert_eq!(k.position_of("?"), None);
    }

    #[test]
    fn standard_default_focus_is_equals() {
        let k = Keypad::standard();
        assert_eq!(k.default_focus(), k.position_of("=").unwrap());
    }

    #[test]
    fn tall_pad_spans_and_covers() {
        let k = Keypad::tall();
        assert_eq!((k.rows(), k.cols()), (7, 3));
        // wide = (1×2) on the bottom row — a horizontal span on a real pad.
        let eq = k.button(k.button_index_at(6, 0));
        assert_eq!((eq.label, eq.row_span, eq.col_span), ("=", 1, 2));
        assert_eq!(k.button_index_at(6, 0), k.button_index_at(6, 1));
        assert_eq!(k.default_focus(), k.position_of("=").unwrap());
    }

    #[test]
    fn wide_pad_spans_and_covers() {
        let k = Keypad::wide();
        assert_eq!((k.rows(), k.cols()), (3, 7));
        // tall = (2×1) anchoring the right edge — a vertical span on a real pad,
        // the row-span counterpart to the tall pad's wide `=`.
        let eq = k.button(k.button_index_at(1, 6));
        assert_eq!((eq.label, eq.row_span, eq.col_span), ("=", 2, 1));
        assert_eq!(k.button_index_at(1, 6), k.button_index_at(2, 6));
        assert_eq!(k.default_focus(), k.position_of("=").unwrap());
    }

    #[test]
    fn fit_score_ranks_by_shape() {
        let (std, tall, wide) = (Keypad::standard(), Keypad::tall(), Keypad::wide());
        // Narrow-tall terminal: the tall pad fits best; the wide pad can't fit at
        // this width, so it must rank below the standard pad (which does fit).
        assert!(tall.fit_score(30, 45) > std.fit_score(30, 45));
        assert!(std.fit_score(30, 45) > wide.fit_score(30, 45));
        // Wide-short terminal: the wide pad is the best shape match.
        assert!(wide.fit_score(70, 40) > std.fit_score(70, 40));
        assert!(wide.fit_score(70, 40) > tall.fit_score(70, 40));
        // Squarish terminal: the standard pad matches best.
        assert!(std.fit_score(40, 40) > tall.fit_score(40, 40));
        assert!(std.fit_score(40, 40) > wide.fit_score(40, 40));
    }

    #[test]
    #[should_panic(expected = "default-focus label")]
    fn rejects_unknown_default_focus() {
        compile(&[&["a", "b"]], "z");
    }

    #[test]
    fn compiles_wide_and_tall_spans() {
        let grid: &[&[&str]] = &[
            &["a", "wide", "wide"],
            &["tall", "b", "c"],
            &["tall", "d", "e"],
        ];
        let k = compile(grid, "a");

        let wide = k.button(k.button_index_at(0, 1));
        assert_eq!((wide.label, wide.row_span, wide.col_span), ("wide", 1, 2));
        // both cells of the wide button resolve to the same button
        assert_eq!(k.button_index_at(0, 1), k.button_index_at(0, 2));

        let tall = k.button(k.button_index_at(1, 0));
        assert_eq!((tall.label, tall.row_span, tall.col_span), ("tall", 2, 1));
        assert_eq!(k.button_index_at(1, 0), k.button_index_at(2, 0));

        // the anchor is the top-left cell of the region
        assert_eq!(k.position_of("wide"), Some((0, 1)));
        assert_eq!(k.position_of("tall"), Some((1, 0)));
    }

    #[test]
    #[should_panic(expected = "filled rectangle")]
    fn rejects_disjoint_label() {
        // "x" appears in two non-adjacent cells: bounding box is 2×2 (4 cells) but
        // only 2 belong to it.
        let grid: &[&[&str]] = &[&["x", "a"], &["b", "x"]];
        compile(grid, "a");
    }

    #[test]
    #[should_panic(expected = "filled rectangle")]
    fn rejects_l_shaped_span() {
        let grid: &[&[&str]] = &[&["x", "x"], &["x", "a"]];
        compile(grid, "a");
    }

    #[test]
    #[should_panic(expected = "rectangular")]
    fn rejects_ragged_grid() {
        let grid: &[&[&str]] = &[&["a", "b"], &["c"]];
        compile(grid, "a");
    }
}
