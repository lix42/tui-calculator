use ratatui::Frame;
use ratatui::layout::{Constraint, Layout, Rect};
use ratatui::palette::Hsluv;
use ratatui::style::{Color, Style, Stylize};
use ratatui::text::{Line, Span};
use ratatui::widgets::{Block, BorderType, Padding, Paragraph};

use crate::app::App;
use crate::layout::{CELL_H, CELL_W, DISPLAY_H};
use crate::ui_state::{ColorMode, Theme, UiState};

/// The colored-glyph → color map: the **single source of truth** for both the
/// button grid and the display, so a glyph reads the same hue wherever it
/// appears. `None` means "stay neutral" — `=`, `C`, `⌫`, the decimal point, and
/// anything unrecognized — so the color reads as *the expression*, not noise.
///
/// Colors are built in **HSLuv** (perceptually-uniform lightness) via
/// [`ratatui::palette`], so the ten digit hues come out evenly bright rather than
/// "yellow glaring, blue muddy" — the trap a naive HSL/HSV palette hits on a dark
/// background. `theme` sets the lightness/saturation (see [`theme_sl`]) so the
/// hues stay legible on the chosen background.
fn glyph_color(c: char, theme: Theme) -> Option<Color> {
    let hue = glyph_hue(c)?;
    let (saturation, lightness) = theme_sl(theme);
    Some(Color::from_hsluv(Hsluv::new(hue, saturation, lightness)))
}

/// The hue (degrees) a glyph is colored with, or `None` if it stays neutral.
///
/// Digits `0`–`9` take ten evenly-spaced hues (`d * 36°`) — the rainbow. The
/// operators and parens take hand-picked hues *offset* from that grid so an
/// operator sitting between two digits doesn't blend into either neighbor. `=`,
/// `C`, `⌫` and the decimal point return `None`: the action/structure keys stay
/// neutral so focus and grouping still read.
///
/// These six operator/paren hues are the main palette knob to tune by eye.
fn glyph_hue(c: char) -> Option<f32> {
    if let Some(d) = c.to_digit(10) {
        return Some(d as f32 * 36.0);
    }
    Some(match c {
        '+' => 18.0,
        '-' => 90.0,
        '×' => 162.0,
        '÷' => 234.0,
        '(' => 306.0,
        ')' => 342.0,
        _ => return None,
    })
}

/// The HSLuv `(saturation, lightness)` the palette is built at for `theme`, both
/// on palette's `0..=100` scale. A dark background wants brighter hues (high
/// lightness reads on black); a light one wants darker, more saturated hues
/// (readable on white). The two tunings are the other palette knob. `f32` to
/// match palette's default component type (what `Color::from_hsluv` expects).
fn theme_sl(theme: Theme) -> (f32, f32) {
    match theme {
        Theme::Dark => (85.0, 70.0),
        Theme::Light => (90.0, 45.0),
    }
}

// --- Rainbow focus / press highlight ------------------------------------------
//
// In rainbow mode an outline focus cue is hard to pick out among the already-colored
// keys, so focus *fills* the cell with the key's own color: a colored border on the
// default background, the glyph knocked out in the background color. Pressing keeps
// that filled shape but swaps the fill to the theme's `loud` color (bright white on
// dark), so the two states read as colored chip → loud flash.

/// The fill color for the *focused* cell: a colored key fills with its own resting
/// hue (so the chip matches its glyph); a neutral key (`=`, `C`, `⌫`) has no hue,
/// so it fills with a subdued neutral instead.
fn focus_color(label: &str, theme: Theme) -> Color {
    label_color(label, theme).unwrap_or_else(|| neutral_focus(theme))
}

/// The focus-fill color for the hueless keys — a gray that stays distinct from the
/// default background without competing with the colored keys.
fn neutral_focus(theme: Theme) -> Color {
    match theme {
        Theme::Dark => Color::Gray,
        Theme::Light => Color::DarkGray,
    }
}

/// The glyph color painted over a filled chip so the label reads as a cut-out of
/// it — the theme's background color, i.e. the `fg: background` half of the fill
/// spec. The exact opposite of [`loud`], so a `loud`-filled chip always keeps a
/// legible knocked-out glyph.
fn knockout(theme: Theme) -> Color {
    match theme {
        Theme::Dark => Color::Black,
        Theme::Light => Color::White,
    }
}

/// The loudest fill against the theme's background — **bright white on dark,
/// near-black on light**. Used where a highlight must shout regardless of hue: the
/// rainbow press flash and the mono neutral-key focus/press cue. It's the opposite
/// end of the scale from [`knockout`], so a `loud`-filled chip (`block: bg = loud`,
/// `fg = knockout`) always has a contrasting glyph — a hardcoded `Color::White`
/// here would knock out white-on-white and vanish under the Light theme.
fn loud(theme: Theme) -> Color {
    match theme {
        Theme::Dark => Color::White,
        Theme::Light => Color::Black,
    }
}

/// Build the display line for `s` under `mode`. Mono keeps the borrow-only path
/// (`Line::from(&str)`); rainbow colors each glyph (see [`rainbow_spans`]).
/// The returned `Line` borrows from `s`, so `s` must outlive the render call.
fn styled_line(s: &str, mode: ColorMode, theme: Theme) -> Line<'_> {
    match mode {
        ColorMode::Mono => Line::from(s),
        ColorMode::Rainbow => Line::from(rainbow_spans(s, theme)),
    }
}

/// Split `s` into per-character `Span`s, coloring each glyph via [`glyph_color`]
/// and leaving the rest neutral (`Span::raw`, no `fg`). Every span borrows a
/// slice of `s`, so there's no per-character allocation; iterating by
/// `char_indices` keeps multibyte glyphs (`×`, `÷`) whole, which a byte-wise walk
/// would split mid-character.
fn rainbow_spans(s: &str, theme: Theme) -> Vec<Span<'_>> {
    s.char_indices()
        .map(|(i, ch)| {
            let slice = &s[i..i + ch.len_utf8()];
            match glyph_color(ch, theme) {
                Some(color) => Span::styled(slice, Style::new().fg(color)),
                None => Span::raw(slice),
            }
        })
        .collect()
}

pub fn draw(frame: &mut Frame, app: &App, ui: &mut UiState) {
    let grid_w = ui.keypad().cols() as u16 * CELL_W;
    let grid_h = ui.keypad().rows() as u16 * CELL_H;
    let panel = centered_panel(frame.area(), grid_w, DISPLAY_H + grid_h);
    let [display_area, button_area] =
        Layout::vertical([Constraint::Length(DISPLAY_H), Constraint::Length(grid_h)]).areas(panel);

    draw_display(frame, app, ui, display_area);
    draw_buttons(frame, ui, button_area);
}

fn centered_panel(area: Rect, width: u16, height: u16) -> Rect {
    let [_, vert, _] = Layout::vertical([
        Constraint::Fill(1),
        Constraint::Length(height),
        Constraint::Fill(1),
    ])
    .areas(area);
    let [_, panel, _] = Layout::horizontal([
        Constraint::Fill(1),
        Constraint::Length(width),
        Constraint::Fill(1),
    ])
    .areas(vert);
    panel
}

fn draw_display(frame: &mut Frame, app: &App, ui: &mut UiState, area: Rect) {
    let display_block = Block::bordered().padding(Padding::horizontal(1));
    let inner = display_block.inner(area);
    frame.render_widget(display_block, area);

    let [top_area, bottom_area] =
        Layout::vertical([Constraint::Fill(1), Constraint::Fill(1)]).areas(inner);

    let (top, bottom) = app.display_lines();

    // The affordance occupies the top row's left edge; reserve those columns so a
    // long right-aligned expression can't render over the persistent hint. The
    // expression is drawn *first*, then the affordance on top, so a momentary
    // status ("Copied!") — which reserves nothing — always wins over the
    // expression instead of being overwritten by a long one.
    let reserved = copy_affordance_width(app, ui, top_area);
    let expr_area = Rect {
        x: top_area.x + reserved,
        width: top_area.width.saturating_sub(reserved),
        ..top_area
    };
    let (mode, theme) = (ui.color_mode(), ui.theme());
    frame.render_widget(
        styled_line(&top, mode, theme).right_aligned().dim(),
        expr_area,
    );
    draw_copy_affordance(frame, app, ui, top_area);
    frame.render_widget(
        styled_line(&bottom, mode, theme).right_aligned().bold(),
        bottom_area,
    );
}

/// The label shown when a result is copyable. The leading `y` mirrors the key
/// that triggers the copy; its width sets the clickable hit-area. ASCII, so
/// `len()` equals its rendered column width.
const COPY_HINT: &str = "[y Copy]";

/// Renders the copy affordance (or the transient status message) left-aligned in
/// the top-left of the display. Drawn *after* the expression (see `draw_display`)
/// so a live status paints on top of it.
///
/// Three states:
/// - a live status ("Copied!"/"Copy failed: …") wins while it lasts. It reserves
///   no columns (see `copy_affordance_width`) but is drawn last, so it overlays
///   the dim expression — momentary feedback right after the user acted, and a
///   long error message is free to use the whole row rather than shrink the
///   result.
/// - else the `[y Copy]` hint shows whenever the result is copyable, and its
///   width *is* reserved so the expression never overlaps it. Only the hint is
///   clickable, so it's the only state that records a non-zero `copy_rect`.
/// - else nothing; rect cleared.
fn draw_copy_affordance(frame: &mut Frame, app: &App, ui: &mut UiState, top_area: Rect) {
    if let Some(status) = ui.status_text() {
        frame.render_widget(Line::from(status).left_aligned().cyan(), top_area);
        ui.set_copy_rect(Rect::ZERO);
    } else if app.copy_text().is_some() {
        let rect = left_rect(top_area, COPY_HINT.len());
        frame.render_widget(Line::from(COPY_HINT).left_aligned().dim(), rect);
        ui.set_copy_rect(rect);
    } else {
        ui.set_copy_rect(Rect::ZERO);
    }
}

/// The column width the right-aligned expression must keep clear at the top-left,
/// mirroring the states in `draw_copy_affordance`. Only the persistent `[y Copy]`
/// hint reserves space; a live status reserves nothing (it overlays the
/// expression), and so does the empty state. Read by `draw_display` *before* the
/// expression is rendered, so it can't borrow `ui` mutably — hence a separate
/// read-only pass rather than a value returned from the draw.
fn copy_affordance_width(app: &App, ui: &UiState, top_area: Rect) -> u16 {
    if ui.status_text().is_some() {
        0
    } else if app.copy_text().is_some() {
        left_rect(top_area, COPY_HINT.len()).width
    } else {
        0
    }
}

/// A `width`-wide, single-row rect anchored at the left of `area`, clamped to
/// `area`'s width so it never overflows the display box.
fn left_rect(area: Rect, width: usize) -> Rect {
    Rect {
        width: (width as u16).min(area.width),
        height: 1,
        ..area
    }
}

fn draw_buttons(frame: &mut Frame, ui: &mut UiState, area: Rect) {
    let keypad = ui.keypad();
    // Split once per axis into the coordinate lattice; each button's rect is the
    // bounding box of the cells it spans (see `layout::Button`). `split` is
    // runtime-sized (`Rc<[Rect]>`), so no grid dimension is a const generic.
    let col_x = Layout::horizontal(std::iter::repeat_n(
        Constraint::Length(CELL_W),
        keypad.cols(),
    ))
    .split(area);
    let row_y = Layout::vertical(std::iter::repeat_n(
        Constraint::Length(CELL_H),
        keypad.rows(),
    ))
    .split(area);

    let (mode, theme) = (ui.color_mode(), ui.theme());
    let mut rects = vec![Rect::ZERO; keypad.button_count()];
    for (i, b) in keypad.buttons().iter().enumerate() {
        let left = col_x[b.col as usize];
        let top = row_y[b.row as usize];
        let right = col_x[(b.col + b.col_span - 1) as usize];
        let bottom = row_y[(b.row + b.row_span - 1) as usize];
        let rect = Rect {
            x: left.x,
            y: top.y,
            width: right.x + right.width - left.x,
            height: bottom.y + bottom.height - top.y,
        };
        draw_button(
            frame,
            b.label,
            ui.is_button_focused(i),
            ui.is_button_pressed(i),
            mode,
            theme,
            rect,
        );
        rects[i] = rect;
    }
    // Hand the just-rendered geometry to the UI state so the next mouse event
    // can hit-test against exactly what's on screen.
    ui.set_button_rects(rects);
}

fn draw_button(
    frame: &mut Frame,
    label: &str,
    focused: bool,
    pressed: bool,
    mode: ColorMode,
    theme: Theme,
    area: Rect,
) {
    let style = button_style(label, focused, pressed, mode, theme);
    let block = Block::bordered()
        .border_type(style.border_type)
        .border_style(style.border_style)
        .style(style.block_style)
        .padding(Padding::symmetric(2, 1));
    let paragraph = Paragraph::new(label)
        .centered()
        .style(style.text_style)
        .block(block);
    frame.render_widget(paragraph, area);
}

/// The full visual description of a button in one state.
///
/// Splitting the border out from the block lets a state recolor the frame
/// (`border_style`) or swap the line characters (`border_type`, e.g. a `Thick`
/// or `Double` frame to read as "pushed in") independently of the cell fill
/// (`block_style`) and the label (`text_style`).
#[derive(Clone, Copy)]
struct ButtonStyle {
    /// Base style for the cell — primarily its background fill.
    block_style: Style,
    /// Style applied to the label text.
    text_style: Style,
    /// Color/weight of the border characters.
    border_style: Style,
    /// Which line-drawing set the border uses.
    border_type: BorderType,
}

static REGULAR_STYLE: ButtonStyle = ButtonStyle {
    block_style: Style::new(),
    text_style: Style::new(),
    border_style: Style::new(),
    border_type: BorderType::Rounded,
};

/// The [`ButtonStyle`] a button is actually drawn with. Both modes derive every
/// highlight from the palette — there is no static accent color left.
///
/// **Mono** keeps the plain resting look and shows the two active states by *shape*,
/// colored from the key's palette hue — or **bright white** for the hueless keys
/// (`=`, `C`, `⌫`). Colored and neutral keys use *opposite* shapes:
/// - **colored keys**: an *outline* in the hue on focus, a *fill* on press.
/// - **neutral keys**: *swapped* — a white *fill* on focus, a white *outline* on
///   press, because a white outline is too faint to tell from the resting cell, so
///   the loud fill has to be the resting/focused distinction.
///
/// **Rainbow** fills on both active states instead, since an outline is hard to pick
/// out among the already-colored keys:
/// - **resting**: just the glyph text takes its [`glyph_color`]; neutral keys stay
///   uncolored.
/// - **focused**: the cell fills with the key's own hue.
/// - **pressed**: the same filled chip, but flooded bright white — a loud flash.
///
/// `pressed` implies `focused` (only the focused cell can activate), so it's
/// checked first.
fn button_style(
    label: &str,
    focused: bool,
    pressed: bool,
    mode: ColorMode,
    theme: Theme,
) -> ButtonStyle {
    if mode == ColorMode::Mono {
        return match label_color(label, theme) {
            // Colored keys: outline on focus, fill on press.
            Some(color) if pressed => filled_style(color, theme),
            Some(color) if focused => outline_style(color),
            // Neutral keys: the shapes are swapped, so the focused state is the loud
            // fill (a faint outline reads too close to the plain resting cell). `loud`
            // (not a hardcoded white) keeps it visible on the Light theme too.
            None if pressed => outline_style(loud(theme)),
            None if focused => filled_style(loud(theme), theme),
            // Resting (either kind): the plain preset.
            _ => REGULAR_STYLE,
        };
    }
    if pressed {
        return filled_style(loud(theme), theme);
    }
    if focused {
        return filled_style(focus_color(label, theme), theme);
    }
    // Resting: color only the glyph; neutral keys keep the plain preset.
    match label_color(label, theme) {
        Some(color) => ButtonStyle {
            text_style: REGULAR_STYLE.text_style.fg(color),
            ..REGULAR_STYLE
        },
        None => REGULAR_STYLE,
    }
}

/// A filled-cell [`ButtonStyle`]: `chip` floods the block (`block: bg = chip`) and
/// colors the border, but the border keeps the *default* background (`border: bg =
/// background, fg = chip`) so it reads as a ring around the fill rather than melting
/// into it. The bold glyph is knocked out on top in the theme's background color
/// ([`knockout`], the `block: fg = background` half of the spec). Rainbow's focus
/// chip (`chip` = the key's hue) and press flash (`chip` = [`loud`]) are both this.
fn filled_style(chip: Color, theme: Theme) -> ButtonStyle {
    ButtonStyle {
        block_style: Style::new().bg(chip),
        text_style: Style::new().fg(knockout(theme)).bold(),
        border_style: Style::new().fg(chip).bg(Color::Reset),
        border_type: BorderType::Rounded,
    }
}

/// An outlined-cell [`ButtonStyle`]: `color` paints the border and the bold glyph
/// while the background stays the terminal default — a ring, not a fill. Mono's
/// focus cue for colored keys and its press cue for neutral keys are both this
/// (the counterpart to [`filled_style`]).
fn outline_style(color: Color) -> ButtonStyle {
    ButtonStyle {
        block_style: Style::new(),
        text_style: Style::new().fg(color).bold(),
        border_style: Style::new().fg(color),
        border_type: BorderType::Rounded,
    }
}

/// The palette color for a button label, or `None` if it stays neutral. Every
/// button label is a single glyph, so this defers to [`glyph_color`] on that
/// char — digits and operators/parens get a hue, `=`/`C`/`⌫` stay neutral.
fn label_color(label: &str, theme: Theme) -> Option<Color> {
    let mut chars = label.chars();
    match (chars.next(), chars.next()) {
        (Some(c), None) => glyph_color(c, theme),
        _ => None,
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::collections::HashSet;

    // Every glyph the palette colors — the ten digits plus the operators and
    // parens. `=`, `C`, `⌫`, `.` are deliberately absent (they stay neutral).
    const COLORED: &str = "0123456789+-×÷()";

    #[test]
    fn glyph_color_is_distinct_across_the_palette() {
        // Contract: every colored glyph gets a color (no `None` for these), and
        // no two share one — so all sixteen are tellable apart on screen.
        let colors: HashSet<Color> = COLORED
            .chars()
            .map(|c| glyph_color(c, Theme::Dark).expect("colored glyph"))
            .collect();
        assert_eq!(colors.len(), COLORED.chars().count());
    }

    #[test]
    fn glyph_color_leaves_action_keys_neutral() {
        // The structure/action keys and the decimal point stay uncolored so
        // focus and grouping still read; unknown chars too.
        for c in ['=', 'C', '⌫', '.', ' '] {
            assert_eq!(glyph_color(c, Theme::Dark), None, "{c:?} should be neutral");
        }
    }

    #[test]
    fn theme_changes_the_colors() {
        // The same glyph is built at a different lightness per theme, so the two
        // themes must actually differ (the runtime `t` toggle has a visible effect).
        assert_ne!(
            glyph_color('5', Theme::Dark),
            glyph_color('5', Theme::Light)
        );
    }

    #[test]
    fn glyph_hue_places_operators_off_the_digit_grid() {
        // Digits land on the 36° grid; operators/parens are offset so one sitting
        // between two digits can't share a neighbor's hue.
        assert_eq!(glyph_hue('0'), Some(0.0));
        assert_eq!(glyph_hue('5'), Some(180.0));
        let ops = ['+', '-', '×', '÷', '(', ')'];
        for op in ops {
            let h = glyph_hue(op).expect("operator has a hue");
            assert!(
                h % 36.0 != 0.0,
                "{op:?} hue {h} collides with the digit grid"
            );
        }
    }

    #[test]
    fn rainbow_spans_colors_digits_and_operators_but_not_neutrals() {
        // Digits and operators are colored; the decimal point stays neutral.
        let spans = rainbow_spans("1.5+2", Theme::Dark);
        let by_char: Vec<_> = spans
            .iter()
            .map(|s| (s.content.as_ref(), s.style.fg))
            .collect();
        assert_eq!(by_char[0], ("1", glyph_color('1', Theme::Dark)));
        assert_eq!(by_char[1], (".", None)); // decimal point neutral
        assert_eq!(by_char[2], ("5", glyph_color('5', Theme::Dark)));
        assert_eq!(by_char[3], ("+", glyph_color('+', Theme::Dark)));
        assert_eq!(by_char[4], ("2", glyph_color('2', Theme::Dark)));
    }

    #[test]
    fn rainbow_spans_handles_multibyte_operators() {
        // `×`/`÷` are multibyte UTF-8; slicing by `char_indices` must keep the
        // whole glyph in one span, not split it mid-byte.
        let spans = rainbow_spans("6×2", Theme::Dark);
        assert_eq!(spans.len(), 3);
        assert_eq!(spans[1].content.as_ref(), "×");
        assert_eq!(spans[1].style.fg, glyph_color('×', Theme::Dark));
        assert_eq!(spans[0].style.fg, glyph_color('6', Theme::Dark));
    }

    #[test]
    fn button_style_overlays_glyph_color_only_in_rainbow() {
        // Mono leaves a digit's text neutral (REGULAR has no `fg`); rainbow
        // overlays its color — and the same holds for an operator button.
        assert_eq!(
            button_style("5", false, false, ColorMode::Mono, Theme::Dark)
                .text_style
                .fg,
            None
        );
        assert_eq!(
            button_style("5", false, false, ColorMode::Rainbow, Theme::Dark)
                .text_style
                .fg,
            glyph_color('5', Theme::Dark)
        );
        assert_eq!(
            button_style("×", false, false, ColorMode::Rainbow, Theme::Dark)
                .text_style
                .fg,
            glyph_color('×', Theme::Dark)
        );
    }

    #[test]
    fn button_style_leaves_action_keys_neutral_in_rainbow() {
        // `=` keeps its preset even in rainbow mode, so action keys stay distinct.
        assert_eq!(
            button_style("=", false, false, ColorMode::Rainbow, Theme::Dark)
                .text_style
                .fg,
            None
        );
    }

    #[test]
    fn button_style_focused_fills_the_cell_in_rainbow() {
        // Focused: the cell fills with the key's own hue — block bg and border fg
        // both the hue, the glyph knocked out in the background color. The border
        // keeps the default background so it rings the fill.
        let s = button_style("5", true, false, ColorMode::Rainbow, Theme::Dark);
        assert_eq!(s.block_style.bg, glyph_color('5', Theme::Dark));
        assert_eq!(s.border_style.fg, glyph_color('5', Theme::Dark));
        assert_eq!(s.border_style.bg, Some(Color::Reset)); // ring on the background
        assert_eq!(s.text_style.fg, Some(Color::Black)); // knockout on the dark theme
        // A neutral key (no hue) fills with a neutral color, distinct from a hue.
        let eq = button_style("=", true, false, ColorMode::Rainbow, Theme::Dark);
        assert_ne!(eq.block_style.bg, s.block_style.bg);
    }

    #[test]
    fn mono_focus_uses_the_rainbow_hue_as_its_accent() {
        // Normal mode borrows the palette for its accent — no static cyan left. A
        // focused digit is an *outline* in its glyph hue (colored border + text,
        // background untouched), and pressing *fills* that same hue.
        let f = button_style("5", true, false, ColorMode::Mono, Theme::Dark);
        assert_eq!(f.border_style.fg, glyph_color('5', Theme::Dark));
        assert_eq!(f.text_style.fg, glyph_color('5', Theme::Dark));
        assert_eq!(f.block_style.bg, None); // outline only

        let p = button_style("5", true, true, ColorMode::Mono, Theme::Dark);
        assert_eq!(p.block_style.bg, glyph_color('5', Theme::Dark)); // fill in the hue
        assert_eq!(p.text_style.fg, Some(Color::Black)); // knocked out on dark

        // A hueless key uses bright white — and its two shapes are *swapped* vs. a
        // colored key: focus is the loud white *fill* (so it can't be mistaken for
        // the plain resting cell), press is the white *outline*.
        let eq_focus = button_style("=", true, false, ColorMode::Mono, Theme::Dark);
        assert_eq!(eq_focus.block_style.bg, Some(Color::White)); // filled on focus
        let eq_press = button_style("=", true, true, ColorMode::Mono, Theme::Dark);
        assert_eq!(eq_press.block_style.bg, None); // outline on press
        assert_eq!(eq_press.border_style.fg, Some(Color::White));

        // Resting stays the plain preset for both kinds.
        assert_eq!(
            button_style("5", false, false, ColorMode::Mono, Theme::Dark).border_style,
            REGULAR_STYLE.border_style
        );
        assert_eq!(
            button_style("=", false, false, ColorMode::Mono, Theme::Dark).block_style,
            REGULAR_STYLE.block_style
        );
    }

    #[test]
    fn button_style_pressed_floods_the_cell_in_rainbow() {
        // Pressed: the same filled chip flooded bright white, glyph knocked out
        // dark — the loud flash, one step up from the colored focus chip.
        let s = button_style("5", true, true, ColorMode::Rainbow, Theme::Dark);
        assert_eq!(s.block_style.bg, Some(Color::White));
        assert_eq!(s.border_style.fg, Some(Color::White));
        assert_eq!(s.text_style.fg, Some(Color::Black));
        // Mono presses in the key's *own* hue instead, not white.
        assert_eq!(
            button_style("5", true, true, ColorMode::Mono, Theme::Dark)
                .block_style
                .bg,
            glyph_color('5', Theme::Dark)
        );
    }

    #[test]
    fn loud_and_knockout_are_opposite_per_theme() {
        // The contract that keeps a loud-filled chip legible: the fill (`loud`) and
        // the knocked-out glyph (`knockout`) sit at opposite ends of the scale on
        // *both* themes, so neither state is ever same-on-same.
        for theme in [Theme::Dark, Theme::Light] {
            assert_ne!(loud(theme), knockout(theme), "{theme:?} loud == knockout");
        }
        // And specifically: white on dark, black on light (the reverse of knockout).
        assert_eq!(loud(Theme::Dark), Color::White);
        assert_eq!(loud(Theme::Light), Color::Black);
    }

    #[test]
    fn loud_filled_highlights_stay_legible_on_the_light_theme() {
        // Regression: a hardcoded white fill knocked out white-on-white under Light
        // (invisible glyph + chip lost in the background). Every `loud`-filled
        // highlight must keep its glyph contrasting the fill on the Light theme.
        // Rainbow press flash (any key):
        let press = button_style("5", true, true, ColorMode::Rainbow, Theme::Light);
        assert_eq!(press.block_style.bg, Some(Color::Black)); // loud on light
        assert_eq!(press.text_style.fg, Some(Color::White)); // knockout contrasts
        assert_ne!(press.block_style.bg, press.text_style.fg);
        // Mono neutral-key focus fill (`=`, `C`, `⌫`):
        let eq_focus = button_style("=", true, false, ColorMode::Mono, Theme::Light);
        assert_eq!(eq_focus.block_style.bg, Some(Color::Black));
        assert_ne!(eq_focus.block_style.bg, eq_focus.text_style.fg);
        // Mono neutral-key press outline is loud too, not an invisible white.
        let eq_press = button_style("=", true, true, ColorMode::Mono, Theme::Light);
        assert_eq!(eq_press.border_style.fg, Some(Color::Black));
    }

    #[test]
    fn rainbow_neutral_focus_fill_is_the_theme_neutral() {
        // The hueless-key focus fill in rainbow mode is the per-theme neutral gray,
        // pinned to an actual color (not just "different from a hue") on both themes.
        assert_eq!(
            button_style("=", true, false, ColorMode::Rainbow, Theme::Dark)
                .block_style
                .bg,
            Some(Color::Gray)
        );
        assert_eq!(
            button_style("=", true, false, ColorMode::Rainbow, Theme::Light)
                .block_style
                .bg,
            Some(Color::DarkGray)
        );
    }

    #[test]
    fn styled_line_dispatches_by_mode() {
        // Mono is a single uncolored span borrowing the whole string; rainbow routes
        // to per-glyph coloring (same result as `rainbow_spans`).
        let mono = styled_line("1+2", ColorMode::Mono, Theme::Dark);
        assert_eq!(mono.spans.len(), 1);
        assert_eq!(mono.spans[0].content.as_ref(), "1+2");
        assert_eq!(mono.spans[0].style.fg, None);

        let rainbow = styled_line("1+2", ColorMode::Rainbow, Theme::Dark);
        let fgs: Vec<_> = rainbow.spans.iter().map(|s| s.style.fg).collect();
        let expected: Vec<_> = rainbow_spans("1+2", Theme::Dark)
            .iter()
            .map(|s| s.style.fg)
            .collect();
        assert_eq!(fgs, expected);
        assert_eq!(rainbow.spans[1].style.fg, glyph_color('+', Theme::Dark));
    }
}
