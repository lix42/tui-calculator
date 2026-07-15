//! Keypad layout: a lattice of equal cells with buttons that may span several
//! cells. A pad is authored as an *occupancy grid* of label tokens and compiled
//! into this trusted form at startup — the span/coverage invariants are checked
//! in `compile`, so the rest of the UI can treat a `Keypad` as valid geometry.

use std::collections::HashMap;

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
}

/// The standard macOS-style pad. Every key is `1×1`; spanning exists in the
/// model (see `compile`) but the shipped pad doesn't use it yet.
const STANDARD: &[&[&str]] = &[
    &["C", "(", ")", "÷"],
    &["7", "8", "9", "×"],
    &["4", "5", "6", "-"],
    &["1", "2", "3", "+"],
    &["⌫", "0", ".", "="],
];

impl Keypad {
    /// The standard pad, compiled once by the caller (e.g. `UiState::new`).
    pub fn standard() -> Self {
        compile(STANDARD)
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
fn compile(grid: &[&[&'static str]]) -> Keypad {
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

    Keypad {
        rows,
        cols,
        buttons,
        occupancy,
        label_pos,
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
    fn compiles_wide_and_tall_spans() {
        let grid: &[&[&str]] = &[
            &["a", "wide", "wide"],
            &["tall", "b", "c"],
            &["tall", "d", "e"],
        ];
        let k = compile(grid);

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
        compile(grid);
    }

    #[test]
    #[should_panic(expected = "filled rectangle")]
    fn rejects_l_shaped_span() {
        let grid: &[&[&str]] = &[&["x", "x"], &["x", "a"]];
        compile(grid);
    }

    #[test]
    #[should_panic(expected = "rectangular")]
    fn rejects_ragged_grid() {
        let grid: &[&[&str]] = &[&["a", "b"], &["c"]];
        compile(grid);
    }
}
