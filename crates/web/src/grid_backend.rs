//! [`GridBackend`]: Ratzilla's `DomBackend` with a frame size that matches the
//! DOM grid it actually builds.
//!
//! Ratzilla 0.3.1 sizes the two from different sources. The grid of `<span>`
//! cells is `<body>`'s rect divided by a measured cell, but `size()`, which
//! sets the ratatui frame, is `window / (10, 20)` on desktop and the whole
//! *physical screen* (`screen / (10, 19)`) when `is_mobile()`. On an iPhone
//! that's a 42×48 frame over a 42×37 grid: the first draw indexes past the
//! last cell and the wasm module panics, so nothing on the page responds
//! (`web-touch` in `docs/progress.md`). Desktop only got away with it because
//! the window-based guess undershoots the grid.
//!
//! Everything passes straight through except [`Backend::size`], which returns
//! the grid size computed the way `DomBackend` computes it (`measure_grid`
//! mirrors its private `measure_cell_size` + `calculate_size`). Ratzilla
//! measures at construction, and again in the first draw after a window
//! `resize`, but only once its grid is in the document, i.e. after its first
//! draw. So this measures in [`GridBackend::new`] too, and a `resize` clears
//! the cached size only once the first draw has happened: a `resize` before
//! it would have us re-measure while Ratzilla populates the grid at its
//! construction size, and a larger frame than grid panics. Nothing checks the result
//! against Ratzilla's private size (it isn't exposed, and is recomputed only
//! inside its `draw`), so the two agree only while `measure_grid` mirrors it:
//! a Ratzilla upgrade that changes its measuring must change `measure_grid`.
//!
//! The same `resize` listener also raises [`GridBackend::take_resized`], for
//! the second bug. Every `resize` makes Ratzilla throw its grid away and
//! rebuild it blank, but ratatui repaints in full only when the frame *size*
//! changes and otherwise sends just the changed cells. A resize that keeps the
//! cell count (or a second event for one resize) left a blank grid that only
//! animated cells repainted: static labels went missing. The frame loop in
//! `main.rs` answers the flag with `Terminal::clear`, which makes the next
//! draw send every cell.

use std::{cell::Cell as StdCell, io, rc::Rc};

use ratzilla::DomBackend;
use ratzilla::error::Error;
use ratzilla::ratatui::backend::{Backend, ClearType, WindowSize};
use ratzilla::ratatui::buffer::Cell;
use ratzilla::ratatui::layout::{Position, Size};
use web_sys::wasm_bindgen::{JsCast, JsValue, closure::Closure};

/// `DomBackend`'s fallback when the probe measures nothing.
const DEFAULT_CELL_SIZE: (f64, f64) = (10.0, 20.0);
/// `DomBackend`'s fallback when there's no window size to read.
const DEFAULT_WINDOW_SIZE: (f64, f64) = (120.0, 120.0);

pub struct GridBackend {
    inner: DomBackend,
    /// The grid size, or `None` after a resize until the next `size()`.
    /// `Rc<Cell>` because `size()` takes `&self` and the resize listener
    /// clears it.
    size: Rc<StdCell<Option<Size>>>,
    /// Whether `inner` has drawn, which is when its grid joins the document
    /// and a `resize` starts making it re-measure.
    drawn: Rc<StdCell<bool>>,
    /// Set by a `resize`, taken by [`Self::take_resized`].
    resized: Rc<StdCell<bool>>,
}

impl GridBackend {
    /// Wrap a `DomBackend` built with the default options (grid in `<body>`),
    /// which is what `measure_grid` assumes.
    pub fn new() -> Result<Self, Error> {
        let inner = DomBackend::new()?;
        // Same moment as Ratzilla's own measurement, so the same answer.
        let size = Rc::new(StdCell::new(Some(measure_grid())));
        let drawn = Rc::new(StdCell::new(false));
        let resized = Rc::new(StdCell::new(false));
        let on_resize = Closure::<dyn FnMut()>::new({
            let (size, drawn, resized) = (size.clone(), drawn.clone(), resized.clone());
            move || {
                // Before the first draw Ratzilla keeps its construction size
                // (and that draw sends every cell anyway).
                if drawn.get() {
                    size.set(None);
                    resized.set(true);
                }
            }
        });
        web_sys::window()
            .ok_or(Error::UnableToRetrieveWindow)?
            .add_event_listener_with_callback("resize", on_resize.as_ref().unchecked_ref())?;
        on_resize.forget();
        Ok(Self {
            inner,
            size,
            drawn,
            resized,
        })
    }

    /// Whether a `resize` happened since the last call. Ratzilla has rebuilt
    /// (or is about to rebuild) its grid blank, so the next draw must send
    /// every cell, not a diff.
    pub fn take_resized(&self) -> bool {
        self.resized.replace(false)
    }
}

/// The grid `DomBackend` builds in `<body>`, measured the way it measures it:
/// a `<pre><span>█</span></pre>` probe for the cell, and `<body>`'s rect
/// (or the window, if `<body>` has no size) for the area. Every part that
/// can't be read falls back as Ratzilla's does, so this always has an answer.
fn measure_grid() -> Size {
    let window = web_sys::window();
    let window_px = window
        .as_ref()
        .and_then(|w| {
            let px = |v: Result<JsValue, JsValue>| v.ok().and_then(|v| v.as_f64());
            Some((px(w.inner_width())?, px(w.inner_height())?))
        })
        .unwrap_or(DEFAULT_WINDOW_SIZE);
    let body = window.and_then(|w| w.document()).and_then(|d| d.body());
    let area = body.as_ref().map_or((0.0, 0.0), |body| {
        let rect = body.get_bounding_client_rect();
        (rect.width(), rect.height())
    });
    let cell = body
        .and_then(|body| measure_cell(&body))
        .unwrap_or((0.0, 0.0));
    grid_size(area, window_px, cell)
}

/// The size of one cell, from a probe appended to `body` and removed again.
fn measure_cell(body: &web_sys::HtmlElement) -> Option<(f64, f64)> {
    let document = body.owner_document()?;
    let pre = document.create_element("pre").ok()?;
    pre.set_attribute(
        "style",
        "margin: 0; padding: 0; border: 0; line-height: normal;",
    )
    .ok()?;
    let span = document.create_element("span").ok()?;
    span.set_inner_html("\u{2588}");
    span.set_attribute("style", "display: inline-block; width: 1ch;")
        .ok()?;
    pre.append_child(&span).ok()?;
    body.append_child(&pre).ok()?;
    let cell = span.get_bounding_client_rect();
    body.remove_child(&pre).ok()?;
    Some((cell.width(), cell.height()))
}

/// `DomBackend::calculate_size`, given its inputs: the parent's size (or the
/// window's when the parent has none) divided by the cell size (or
/// [`DEFAULT_CELL_SIZE`] when the probe measured nothing), truncated.
fn grid_size(parent: (f64, f64), window: (f64, f64), cell: (f64, f64)) -> Size {
    let (cw, ch) = if cell.0 > 0.0 && cell.1 > 0.0 {
        cell
    } else {
        DEFAULT_CELL_SIZE
    };
    let (w, h) = if parent.0 > 0.0 && parent.1 > 0.0 {
        parent
    } else {
        window
    };
    // `as` saturates, as in Ratzilla.
    Size::new((w / cw) as u16, (h / ch) as u16)
}

impl Backend for GridBackend {
    type Error = io::Error;

    fn draw<'a, I>(&mut self, content: I) -> io::Result<()>
    where
        I: Iterator<Item = (u16, u16, &'a Cell)>,
    {
        self.inner.draw(content)?;
        self.drawn.set(true);
        Ok(())
    }

    fn hide_cursor(&mut self) -> io::Result<()> {
        self.inner.hide_cursor()
    }

    fn show_cursor(&mut self) -> io::Result<()> {
        self.inner.show_cursor()
    }

    fn get_cursor_position(&mut self) -> io::Result<Position> {
        self.inner.get_cursor_position()
    }

    fn set_cursor_position<P: Into<Position>>(&mut self, position: P) -> io::Result<()> {
        self.inner.set_cursor_position(position)
    }

    fn clear(&mut self) -> io::Result<()> {
        self.inner.clear()
    }

    fn clear_region(&mut self, clear_type: ClearType) -> io::Result<()> {
        self.inner.clear_region(clear_type)
    }

    /// The one override: the grid's size, not Ratzilla's window/screen guess.
    /// Never an error: Ratzilla's own loop unwrapped it, and a measurement
    /// that fails falls back to Ratzilla's defaults, as `DomBackend`'s does.
    fn size(&self) -> io::Result<Size> {
        let size = self.size.get().unwrap_or_else(measure_grid);
        self.size.set(Some(size));
        Ok(size)
    }

    fn window_size(&mut self) -> io::Result<WindowSize> {
        self.inner.window_size()
    }

    fn flush(&mut self) -> io::Result<()> {
        self.inner.flush()
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn grid_size_matches_the_iphone_that_crashed() {
        // 420×746 body, 10×20 cells: the 42×37 grid (1554 cells) the panic
        // reported, where Ratzilla's own size() said 42×48.
        let size = grid_size((420.0, 746.0), (420.0, 746.0), (10.0, 20.0));
        assert_eq!(size, Size::new(42, 37));
    }

    #[test]
    fn grid_size_truncates_and_falls_back() {
        // Partial cells don't count.
        assert_eq!(
            grid_size((99.9, 59.9), (0.0, 0.0), (9.6, 19.2)),
            Size::new(10, 3)
        );
        // A parent with no size falls back to the window.
        assert_eq!(
            grid_size((0.0, 500.0), (200.0, 100.0), (10.0, 20.0)),
            Size::new(20, 5)
        );
        // A probe that measured nothing falls back to 10×20.
        assert_eq!(
            grid_size((200.0, 100.0), (0.0, 0.0), (0.0, 0.0)),
            Size::new(20, 5)
        );
    }
}
