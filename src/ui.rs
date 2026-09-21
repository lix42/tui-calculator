use ratatui::Frame;
use ratatui::layout::{Constraint, Layout, Rect};
use ratatui::palette::Hsluv;
use ratatui::style::{Color, Style, Stylize};
use ratatui::text::{Line, Span};
use ratatui::widgets::{Block, BorderType, Padding, Paragraph};

use crate::action::quick_key;
use crate::app::App;
use crate::layout::{CELL_H, CELL_W, DISPLAY_H};
use crate::ui_state::{ColorMode, EffectKind, Theme, UiState};

/// Everything the color functions need to build a color: which background the
/// palette is tuned for, and how far the hues are currently rotated by a drift
/// effect.
///
/// Carried as one value rather than as two parameters because `drift` would
/// otherwise have to be threaded through every signature that already takes a
/// `Theme` — and every *future* palette-wide modulation would widen them all
/// again. [`Palette::new`] is the undrifted case, which is what the resting UI
/// and every test that isn't about drift uses.
#[derive(Debug, Clone, Copy, PartialEq)]
struct Palette {
    theme: Theme,
    /// Hue rotation in degrees, applied to every hued glyph. `0.0` at rest.
    drift: f32,
}

impl Palette {
    /// The resting palette for `theme` — no drift.
    fn new(theme: Theme) -> Self {
        Self { theme, drift: 0.0 }
    }

    /// The palette for `theme` with hues rotated `drift` degrees.
    fn drifted(theme: Theme, drift: f32) -> Self {
        Self { theme, drift }
    }
}

/// The colored-glyph → color map: the **single source of truth** for both the
/// button grid and the display, so a glyph reads the same hue wherever it
/// appears. `None` means "stay neutral" — `=`, `C`, `⌫`, the decimal point, and
/// anything unrecognized — so the color reads as *the expression*, not noise.
///
/// Colors are built in **HSLuv** (perceptually-uniform lightness) via
/// [`ratatui::palette`], so the ten digit hues come out evenly bright rather than
/// "yellow glaring, blue muddy" — the trap a naive HSL/HSV palette hits on a dark
/// background. The palette's theme sets the lightness/saturation (see
/// [`theme_sl`]) so the hues stay legible on the chosen background, and its drift
/// rotates every hue together — which is why the drift belongs *here*, at the one
/// place a hue is chosen, rather than being re-applied at each call site.
fn glyph_color(c: char, palette: Palette) -> Option<Color> {
    let hue = (glyph_hue(c)? + palette.drift).rem_euclid(360.0);
    let (saturation, lightness) = theme_sl(palette.theme);
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
fn focus_color(label: &str, palette: Palette) -> Color {
    label_color(label, palette).unwrap_or_else(|| neutral_focus(palette.theme))
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
fn styled_line(s: &str, mode: ColorMode, palette: Palette) -> Line<'_> {
    match mode {
        ColorMode::Mono => Line::from(s),
        ColorMode::Rainbow => Line::from(rainbow_spans(s, palette)),
    }
}

/// Split `s` into per-character `Span`s, coloring each glyph via [`glyph_color`]
/// and leaving the rest neutral (`Span::raw`, no `fg`). Every span borrows a
/// slice of `s`, so there's no per-character allocation; iterating by
/// `char_indices` keeps multibyte glyphs (`×`, `÷`) whole, which a byte-wise walk
/// would split mid-character.
fn rainbow_spans(s: &str, palette: Palette) -> Vec<Span<'_>> {
    s.char_indices()
        .map(|(i, ch)| {
            let slice = &s[i..i + ch.len_utf8()];
            match glyph_color(ch, palette) {
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

    let palette = frame_palette(ui);
    draw_display(frame, app, ui, palette, display_area);
    draw_buttons(frame, ui, palette, button_area);
}

/// The palette this frame is drawn with: the user's theme, with every hue rotated
/// by however far a drift effect has turned them.
///
/// Resolved **once per frame** rather than per glyph, so the whole UI is
/// guaranteed to agree — a drift that reached the buttons but not the display
/// would read as a rendering bug rather than an effect.
///
/// The drift is **rainbow-only**: mono has no hues to rotate, so rotating them is
/// a no-op that would still cost the work. This is the one effect the two color
/// modes genuinely differ on — the ripple and the breath both ride on lightness,
/// which means the same thing either way.
fn frame_palette(ui: &UiState) -> Palette {
    let drift_phase = ui.effects().iter().find_map(|e| match e.kind() {
        EffectKind::Drift => Some(e.progress()),
        EffectKind::Press { .. } | EffectKind::Ripple { .. } => None,
    });
    palette_for(ui.color_mode(), ui.theme(), drift_phase)
}

/// The pure half of [`frame_palette`]: which palette a frame is drawn with, given
/// the mode, the theme, and how far through a drift the frame lands (`None` when
/// nothing is drifting).
///
/// Split out so the mode gate is testable **at a chosen phase**. Asserting it
/// through `frame_palette` alone can only sample a drift that started
/// microseconds ago, where `drift_offset` is legitimately ~`0.0` — indistinguishable
/// from the mono path, and exactly `0.0` on a coarse clock (`web-time`'s
/// `performance.now()` is deliberately quantized on wasm). Same reason
/// [`crate::ui_state::Effect::progress`] hands the renderer a normalized phase and
/// never an `Instant`.
fn palette_for(mode: ColorMode, theme: Theme, drift_phase: Option<f32>) -> Palette {
    match (mode, drift_phase) {
        (ColorMode::Rainbow, Some(phase)) => Palette::drifted(theme, drift_offset(phase)),
        _ => Palette::new(theme),
    }
}

/// How far the palette's hues are rotated at `phase` through a drift effect.
///
/// A half sine: `0°` at both ends, peaking at [`DRIFT_SPAN`] in the middle. Zero
/// at *both* ends is the point — the palette leaves and returns to its resting
/// hues continuously, so a completed calculation washes color across the UI and
/// settles, with no jump at either the trigger or the expiry. (The ripple learned
/// the same lesson the hard way; see `RIPPLE_RING_FADE`.)
fn drift_offset(phase: f32) -> f32 {
    (phase * std::f32::consts::PI).sin() * DRIFT_SPAN
}

/// How far the hue drift rotates the palette at its peak, in degrees.
///
/// The digit hues sit on a 36° grid, so this is a deliberate ~1.5 steps: enough
/// that the whole palette visibly moves, while a digit lands between its
/// neighbours' resting hues rather than squarely on one — which would read as
/// "the 3 key turned into the 4 key" instead of "the colors swept".
const DRIFT_SPAN: f32 = 54.0;

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

fn draw_display(frame: &mut Frame, app: &App, ui: &mut UiState, palette: Palette, area: Rect) {
    // The one always-on effect: the display's frame breathes. It rides on the
    // border rather than the text so it can never make the expression itself
    // harder to read, and on lightness rather than hue so it means the same thing
    // in both color modes.
    let display_block = Block::bordered()
        .border_style(Style::new().fg(breath_color(ui.breath_phase(), palette.theme)))
        .padding(Padding::horizontal(1));
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
    let mode = ui.color_mode();
    frame.render_widget(
        styled_line(&top, mode, palette).right_aligned().dim(),
        expr_area,
    );
    draw_copy_affordance(frame, app, ui, top_area);
    frame.render_widget(
        styled_line(&bottom, mode, palette).right_aligned().bold(),
        bottom_area,
    );
}

/// The display border's color at `phase` through a breath cycle.
///
/// A full sine over the cycle, swinging between two lightnesses that both sit
/// near the terminal's default border brightness — the amplitude is small on
/// purpose. This is the only thing on screen that moves without the user having
/// done anything, so it has to stay at the edge of perception; a wide swing would
/// turn a calm frame into a pulsing one.
///
/// Hueless (saturation `0`), so it is identical in both color modes, and it
/// swings *away from the background* per theme like [`loud`] and the ripple.
fn breath_color(phase: f32, theme: Theme) -> Color {
    // sin over the whole cycle, remapped from -1..1 to 0..1, so the value returns
    // continuously to where it began and the loop point is invisible.
    let wave = ((phase * std::f32::consts::TAU).sin() + 1.0) / 2.0;
    let (low, high) = match theme {
        Theme::Dark => (52.0, 72.0),
        Theme::Light => (48.0, 28.0),
    };
    Color::from_hsluv(Hsluv::new(0.0, 0.0, low + (high - low) * wave))
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

fn draw_buttons(frame: &mut Frame, ui: &mut UiState, palette: Palette, area: Rect) {
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

    let mode = ui.color_mode();
    // Tips are drawn only while quick-mode is on, which makes them the mode
    // indicator as well as the key legend: the mode is never silently active.
    let quick = ui.quick_mode();
    // The ripple in flight, resolved once per frame to (origin button, phase)
    // rather than per button — the origin cell is the same for every cell we're
    // about to measure against it.
    let ripple = ui.effects().iter().find_map(|e| match e.kind() {
        EffectKind::Ripple { cell } => Some((keypad.button_index_at(cell.0, cell.1), e.progress())),
        // The drift is global — it recolors the palette rather than radiating
        // from a cell, so it is resolved in `frame_palette`, not here.
        EffectKind::Press { .. } | EffectKind::Drift => None,
    });
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
            ButtonView {
                label: b.label,
                focused: ui.is_button_focused(i),
                pressed: ui.is_button_pressed(i),
                tip: if quick { quick_key(b.label) } else { None },
                ripple: ripple.map_or(0.0, |(origin, phase)| {
                    ripple_intensity(keypad.button_distance(origin, i), phase)
                }),
            },
            mode,
            palette,
            rect,
        );
        rects[i] = rect;
    }
    // Hand the just-rendered geometry to the UI state so the next mouse event
    // can hit-test against exactly what's on screen.
    ui.set_button_rects(rects);
}

/// Everything about one button's appearance that isn't its geometry or the
/// global palette. Grouping these keeps `draw_button` at five arguments and
/// names the two bare `bool`s at the call site.
struct ButtonView<'a> {
    label: &'a str,
    focused: bool,
    pressed: bool,
    /// The quick-input key that enters this button, shown in its top border while
    /// quick-mode is on; `None` when the mode is off or the button has no mapping.
    tip: Option<char>,
    /// How strongly the press ripple is lighting this button right now, `0.0`
    /// (untouched — the overwhelming common case) to `1.0` (full). See
    /// [`ripple_intensity`].
    ripple: f32,
}

fn draw_button(frame: &mut Frame, view: ButtonView, mode: ColorMode, palette: Palette, area: Rect) {
    let base = button_style(view.label, view.focused, view.pressed, mode, palette);
    let active = view.focused || view.pressed;
    let style = apply_ripple(base, active, view.ripple, view.label, mode, palette);
    let mut block = Block::bordered()
        .border_type(style.border_type)
        .border_style(style.border_style)
        .style(style.block_style)
        .padding(Padding::symmetric(2, 1));
    // The tip rides *in the top border*, not inside the cell: the label is
    // centered in the padded interior, so a second glyph in there would either
    // shift it off-center or collide with it. The border is otherwise empty. It
    // inherits `border_style` so it tracks the cell's focus/press state, dimmed so
    // the button's own glyph stays the thing you read first.
    if let Some(key) = view.tip {
        block = block.title(Span::styled(key.to_string(), style.border_style.dim()));
    }
    let paragraph = Paragraph::new(view.label)
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
    palette: Palette,
) -> ButtonStyle {
    if mode == ColorMode::Mono {
        return match label_color(label, palette) {
            // Colored keys: outline on focus, fill on press.
            Some(color) if pressed => filled_style(color, palette.theme),
            Some(color) if focused => outline_style(color),
            // Neutral keys: the shapes are swapped, so the focused state is the loud
            // fill (a faint outline reads too close to the plain resting cell). `loud`
            // (not a hardcoded white) keeps it visible on the Light theme too.
            None if pressed => outline_style(loud(palette.theme)),
            None if focused => filled_style(loud(palette.theme), palette.theme),
            // Resting (either kind): the plain preset.
            _ => REGULAR_STYLE,
        };
    }
    if pressed {
        return filled_style(loud(palette.theme), palette.theme);
    }
    if focused {
        return filled_style(focus_color(label, palette), palette.theme);
    }
    // Resting: color only the glyph; neutral keys keep the plain preset.
    match label_color(label, palette) {
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

// --- Press ripple ---------------------------------------------------------------
//
// A press starts a wave that crosses the pad and fades (see
// `ui_state::EffectKind::Ripple`). It lights each button's *border* rather than
// filling the cell, so it layers under the focus/press highlights instead of
// fighting them — and because it is carried by lightness, not hue, it reads in
// **both** color modes.

/// Intensities at or below this don't paint at all, so a resting button is drawn
/// with exactly the style it had before this effect existed — no "almost zero"
/// recoloring of the whole pad on every frame of every ripple.
const RIPPLE_FLOOR: f32 = 0.02;

/// How much later each successive ring of keys begins to light, as a fraction of
/// the ripple's lifetime. This is what makes the wave *travel* rather than the
/// whole pad flashing at once. At the current `RIPPLE_DURATION` it works out to
/// ~60 ms per ring.
const RIPPLE_RING_DELAY: f32 = 0.075;

/// How long one ring takes to fade from full to dark, as a fraction of the
/// ripple's lifetime (~200 ms). Rings overlap — several are lit at once — which
/// is deliberate: a band this wide survives being sampled at ~10 fps, where a
/// one-ring-thin front would skip rings between frames.
///
/// **Invariant:** `RIPPLE_RING_DELAY * max_distance + RIPPLE_RING_FADE <= 1.0`,
/// or the outermost ring is still lit when `phase` hits `1.0` and the effect is
/// dropped mid-fade — a visible pop at the pad's edge. Guarded for every shipped
/// pad by `ripple_completes_before_expiring_on_every_pad`.
const RIPPLE_RING_FADE: f32 = 0.25;

/// How strongly the ripple lights a button `distance` cells from where the press
/// landed, at `phase` through the effect's lifetime (`0.0` at the press, `1.0`
/// as it expires).
///
/// **This is the feel of the whole feature**, and the shape is a judgment call
/// rather than a formula — see the three candidates discussed alongside it. The
/// hard constraint is pacing: the run loop repaints at ~10 fps, so the 800 ms
/// `RIPPLE_DURATION` is sampled about **eight times**. A curve that is smooth in
/// the limit can strobe at eight samples, and a wavefront thin enough to look
/// crisp can skip a ring entirely between frames.
///
/// Pure, and both inputs are normalized (no clock, no `Instant`), so it is
/// directly unit-testable at chosen distances and phases.
fn ripple_intensity(distance: usize, phase: f32) -> f32 {
    let start = distance as f32 * RIPPLE_RING_DELAY;
    if phase < start {
        0.0
    } else {
        (1.0 - (phase - start) / RIPPLE_RING_FADE).max(0.0)
    }
}

/// Overlay the ripple on a button's resting/focused/pressed style.
///
/// Returns `base` **untouched** below [`RIPPLE_FLOOR`], which is what keeps a
/// quiet pad byte-identical to the pre-animation rendering. Above it, the border
/// takes a lightness-scaled color: in rainbow that's the key's own hue, and in
/// mono it's the same construction at zero saturation — HSLuv with no saturation
/// *is* a gray, so one code path serves both modes rather than a mono special
/// case.
fn apply_ripple(
    base: ButtonStyle,
    active: bool,
    intensity: f32,
    label: &str,
    mode: ColorMode,
    palette: Palette,
) -> ButtonStyle {
    // A focused or pressed key keeps its highlight untouched. The ripple is
    // ambient feedback and rides on the border, which is exactly where those two
    // states put their strongest cue — and the pressed key is always at distance
    // 0, so without this guard every press would overwrite its own `loud` chip
    // border with a mid-lightness hue at the very instant it fires.
    if active {
        return base;
    }
    let intensity = intensity.clamp(0.0, 1.0);
    if intensity <= RIPPLE_FLOOR {
        return base;
    }
    ButtonStyle {
        border_style: base
            .border_style
            .fg(ripple_color(label, intensity, mode, palette)),
        ..base
    }
}

/// The border color for a button the ripple is lighting at `intensity`.
///
/// Built in HSLuv like the rest of the palette, but with **lightness** as the
/// varying term — the ripple is a brightness wave, not a hue change, which is
/// what lets it mean the same thing in mono. It moves *away from the background*
/// in both themes: brighter on dark, darker on light, mirroring the reasoning
/// behind [`loud`] and [`knockout`].
fn ripple_color(label: &str, intensity: f32, mode: ColorMode, palette: Palette) -> Color {
    // Mono has no hues to borrow, so it rides at zero saturation — the same
    // construction, landing on a gray.
    let (hue, full_saturation) = match mode {
        ColorMode::Rainbow => label
            .chars()
            .next()
            .and_then(glyph_hue)
            .map_or((0.0, 0.0), |h| {
                (
                    (h + palette.drift).rem_euclid(360.0),
                    theme_sl(palette.theme).0,
                )
            }),
        ColorMode::Mono => (0.0, 0.0),
    };
    // Saturation ramps with intensity too, so the border grows *into* its hue
    // from the resting gray rather than switching to it. This is what carries the
    // ripple in rainbow mode: the resting border is already bright on a dark
    // theme, leaving little lightness headroom before a hue washes out to white
    // (HSLuv at L 100 is white at *any* saturation — the same "no contrast left
    // at the extreme" trap as `loud`/`knockout`). Mono has no hue to grow into,
    // so its `full_saturation` is 0 and lightness carries the whole signal — one
    // code path, each mode using the channel it actually has.
    Color::from_hsluv(Hsluv::new(
        hue,
        full_saturation * intensity,
        ripple_lightness(intensity, palette.theme),
    ))
}

/// The HSLuv lightness a rippling border sits at, ramping from the button's
/// **resting** appearance at zero intensity to [`RIPPLE_PEAK_L`] at full.
///
/// The resting anchor is load-bearing, and getting it wrong is subtle: a resting
/// button's `border_style` is `Style::new()` — *no* `fg` — so it renders in the
/// terminal's default foreground, which is already bright on a dark theme
/// (~L 80) and already dark on a light one (~L 22). Anchoring the ramp anywhere
/// else means the ripple starts by jumping the border to an unrelated lightness,
/// and — because [`apply_ripple`] stops painting below [`RIPPLE_FLOOR`] and hands
/// back the *unstyled* base — snaps back to the default at each ring's trailing
/// edge. Starting *at* the resting value makes that hand-off continuous, the same
/// property `drift_offset`'s half-sine and `breath_color`'s full sine are shaped
/// for.
///
/// From there it moves **away from the background** per theme, like [`loud`] and
/// [`knockout`]: brighter on dark, darker on light.
fn ripple_lightness(intensity: f32, theme: Theme) -> f32 {
    let (rest, peak) = match theme {
        Theme::Dark => (RESTING_BORDER_L_DARK, RIPPLE_PEAK_L_DARK),
        Theme::Light => (RESTING_BORDER_L_LIGHT, RIPPLE_PEAK_L_LIGHT),
    };
    rest + (peak - rest) * intensity
}

/// Approximate HSLuv lightness of the terminal's default foreground — the color a
/// resting button's border is actually drawn in, since it carries no `fg`. Only
/// an approximation is possible (the real value is whatever the user's terminal
/// theme says), but anchoring near it is what keeps the ripple continuous where
/// it stops painting.
const RESTING_BORDER_L_DARK: f32 = 80.0;
const RESTING_BORDER_L_LIGHT: f32 = 22.0;

/// Where a fully-lit ripple ring lands. A modest swing from the resting anchor on
/// purpose: the ripple is ambient feedback spreading across the whole pad, so it
/// has to stay well under the press flash, which is a `loud` *fill* rather than a
/// border tint and is therefore in no danger of being confused with it.
/// Held short of the extremes (`100`/`0`) deliberately: HSLuv at either end is
/// pure white or pure black at *any* saturation, so peaking there would erase the
/// hue the rainbow ripple is carrying at exactly its most visible moment.
const RIPPLE_PEAK_L_DARK: f32 = 93.0;
const RIPPLE_PEAK_L_LIGHT: f32 = 9.0;

/// The palette color for a button label, or `None` if it stays neutral. Every
/// button label is a single glyph, so this defers to [`glyph_color`] on that
/// char — digits and operators/parens get a hue, `=`/`C`/`⌫` stay neutral.
fn label_color(label: &str, palette: Palette) -> Option<Color> {
    let mut chars = label.chars();
    match (chars.next(), chars.next()) {
        (Some(c), None) => glyph_color(c, palette),
        _ => None,
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::layout::Keypad;
    use ratatui::Terminal;
    use ratatui::backend::TestBackend;
    use std::collections::HashSet;

    /// Render the whole UI onto a `28×29` test terminal — exactly the standard
    /// pad's panel size (`4×7` cells wide, `5×5` cells plus the display tall), so
    /// the panel fills the buffer and cell coordinates are predictable.
    fn render(ui: &mut UiState) -> ratatui::buffer::Buffer {
        let mut terminal = Terminal::new(TestBackend::new(28, 29)).expect("test terminal");
        let app = App::new();
        terminal
            .draw(|frame| draw(frame, &app, ui))
            .expect("draw succeeds");
        terminal.backend().buffer().clone()
    }

    #[test]
    fn quick_tips_render_in_the_button_border_only_while_the_mode_is_on() {
        // The "5" button sits at pad cell (row 2, col 1) => x = 1*CELL_W = 7,
        // y = DISPLAY_H + 2*CELL_H = 14. Its top border is that row, and a title
        // starts just past the corner, at x = 8. The label itself is centered two
        // rows lower, so finding `i` at (8, 14) proves the tip is in the *border*
        // and not displacing the glyph.
        let mut ui = UiState::new();
        ui.set_quick_mode(true);
        let buf = render(&mut ui);
        assert_eq!(buf[(8, 14)].symbol(), "i", "tip missing from the 5 button");
        // The glyph it hints at is still centered, untouched, below it.
        assert_eq!(buf[(10, 16)].symbol(), "5");

        // With the mode off the border is plain again — no lowercase tip letters
        // anywhere (every pad label is a digit, operator, or uppercase `C`).
        ui.set_quick_mode(false);
        let plain = render(&mut ui);
        assert_eq!(plain[(8, 14)].symbol(), "─");
        for y in 0..plain.area.height {
            for x in 0..plain.area.width {
                let s = plain[(x, y)].symbol();
                assert!(
                    !s.chars().any(|c| QUICK_TIP_CHARS.contains(c)),
                    "found tip {s:?} at ({x}, {y}) with quick-mode off"
                );
            }
        }
    }

    /// Every character `QUICK_MAP` uses as a tip, for the "no tips when off" sweep.
    const QUICK_TIP_CHARS: &str = "uiojklmasdf[]";

    // Every glyph the palette colors — the ten digits plus the operators and
    // parens. `=`, `C`, `⌫`, `.` are deliberately absent (they stay neutral).
    const COLORED: &str = "0123456789+-×÷()";

    #[test]
    fn glyph_color_is_distinct_across_the_palette() {
        // Contract: every colored glyph gets a color (no `None` for these), and
        // no two share one — so all sixteen are tellable apart on screen.
        let colors: HashSet<Color> = COLORED
            .chars()
            .map(|c| glyph_color(c, Palette::new(Theme::Dark)).expect("colored glyph"))
            .collect();
        assert_eq!(colors.len(), COLORED.chars().count());
    }

    #[test]
    fn glyph_color_leaves_action_keys_neutral() {
        // The structure/action keys and the decimal point stay uncolored so
        // focus and grouping still read; unknown chars too.
        for c in ['=', 'C', '⌫', '.', ' '] {
            assert_eq!(
                glyph_color(c, Palette::new(Theme::Dark)),
                None,
                "{c:?} should be neutral"
            );
        }
    }

    #[test]
    fn theme_changes_the_colors() {
        // The same glyph is built at a different lightness per theme, so the two
        // themes must actually differ (the runtime `t` toggle has a visible effect).
        assert_ne!(
            glyph_color('5', Palette::new(Theme::Dark)),
            glyph_color('5', Palette::new(Theme::Light))
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
        let spans = rainbow_spans("1.5+2", Palette::new(Theme::Dark));
        let by_char: Vec<_> = spans
            .iter()
            .map(|s| (s.content.as_ref(), s.style.fg))
            .collect();
        assert_eq!(
            by_char[0],
            ("1", glyph_color('1', Palette::new(Theme::Dark)))
        );
        assert_eq!(by_char[1], (".", None)); // decimal point neutral
        assert_eq!(
            by_char[2],
            ("5", glyph_color('5', Palette::new(Theme::Dark)))
        );
        assert_eq!(
            by_char[3],
            ("+", glyph_color('+', Palette::new(Theme::Dark)))
        );
        assert_eq!(
            by_char[4],
            ("2", glyph_color('2', Palette::new(Theme::Dark)))
        );
    }

    #[test]
    fn rainbow_spans_handles_multibyte_operators() {
        // `×`/`÷` are multibyte UTF-8; slicing by `char_indices` must keep the
        // whole glyph in one span, not split it mid-byte.
        let spans = rainbow_spans("6×2", Palette::new(Theme::Dark));
        assert_eq!(spans.len(), 3);
        assert_eq!(spans[1].content.as_ref(), "×");
        assert_eq!(
            spans[1].style.fg,
            glyph_color('×', Palette::new(Theme::Dark))
        );
        assert_eq!(
            spans[0].style.fg,
            glyph_color('6', Palette::new(Theme::Dark))
        );
    }

    #[test]
    fn button_style_overlays_glyph_color_only_in_rainbow() {
        // Mono leaves a digit's text neutral (REGULAR has no `fg`); rainbow
        // overlays its color — and the same holds for an operator button.
        assert_eq!(
            button_style(
                "5",
                false,
                false,
                ColorMode::Mono,
                Palette::new(Theme::Dark)
            )
            .text_style
            .fg,
            None
        );
        assert_eq!(
            button_style(
                "5",
                false,
                false,
                ColorMode::Rainbow,
                Palette::new(Theme::Dark)
            )
            .text_style
            .fg,
            glyph_color('5', Palette::new(Theme::Dark))
        );
        assert_eq!(
            button_style(
                "×",
                false,
                false,
                ColorMode::Rainbow,
                Palette::new(Theme::Dark)
            )
            .text_style
            .fg,
            glyph_color('×', Palette::new(Theme::Dark))
        );
    }

    #[test]
    fn button_style_leaves_action_keys_neutral_in_rainbow() {
        // `=` keeps its preset even in rainbow mode, so action keys stay distinct.
        assert_eq!(
            button_style(
                "=",
                false,
                false,
                ColorMode::Rainbow,
                Palette::new(Theme::Dark)
            )
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
        let s = button_style(
            "5",
            true,
            false,
            ColorMode::Rainbow,
            Palette::new(Theme::Dark),
        );
        assert_eq!(
            s.block_style.bg,
            glyph_color('5', Palette::new(Theme::Dark))
        );
        assert_eq!(
            s.border_style.fg,
            glyph_color('5', Palette::new(Theme::Dark))
        );
        assert_eq!(s.border_style.bg, Some(Color::Reset)); // ring on the background
        assert_eq!(s.text_style.fg, Some(Color::Black)); // knockout on the dark theme
        // A neutral key (no hue) fills with a neutral color, distinct from a hue.
        let eq = button_style(
            "=",
            true,
            false,
            ColorMode::Rainbow,
            Palette::new(Theme::Dark),
        );
        assert_ne!(eq.block_style.bg, s.block_style.bg);
    }

    #[test]
    fn mono_focus_uses_the_rainbow_hue_as_its_accent() {
        // Normal mode borrows the palette for its accent — no static cyan left. A
        // focused digit is an *outline* in its glyph hue (colored border + text,
        // background untouched), and pressing *fills* that same hue.
        let f = button_style("5", true, false, ColorMode::Mono, Palette::new(Theme::Dark));
        assert_eq!(
            f.border_style.fg,
            glyph_color('5', Palette::new(Theme::Dark))
        );
        assert_eq!(f.text_style.fg, glyph_color('5', Palette::new(Theme::Dark)));
        assert_eq!(f.block_style.bg, None); // outline only

        let p = button_style("5", true, true, ColorMode::Mono, Palette::new(Theme::Dark));
        assert_eq!(
            p.block_style.bg,
            glyph_color('5', Palette::new(Theme::Dark))
        ); // fill in the hue
        assert_eq!(p.text_style.fg, Some(Color::Black)); // knocked out on dark

        // A hueless key uses bright white — and its two shapes are *swapped* vs. a
        // colored key: focus is the loud white *fill* (so it can't be mistaken for
        // the plain resting cell), press is the white *outline*.
        let eq_focus = button_style("=", true, false, ColorMode::Mono, Palette::new(Theme::Dark));
        assert_eq!(eq_focus.block_style.bg, Some(Color::White)); // filled on focus
        let eq_press = button_style("=", true, true, ColorMode::Mono, Palette::new(Theme::Dark));
        assert_eq!(eq_press.block_style.bg, None); // outline on press
        assert_eq!(eq_press.border_style.fg, Some(Color::White));

        // Resting stays the plain preset for both kinds.
        assert_eq!(
            button_style(
                "5",
                false,
                false,
                ColorMode::Mono,
                Palette::new(Theme::Dark)
            )
            .border_style,
            REGULAR_STYLE.border_style
        );
        assert_eq!(
            button_style(
                "=",
                false,
                false,
                ColorMode::Mono,
                Palette::new(Theme::Dark)
            )
            .block_style,
            REGULAR_STYLE.block_style
        );
    }

    #[test]
    fn button_style_pressed_floods_the_cell_in_rainbow() {
        // Pressed: the same filled chip flooded bright white, glyph knocked out
        // dark — the loud flash, one step up from the colored focus chip.
        let s = button_style(
            "5",
            true,
            true,
            ColorMode::Rainbow,
            Palette::new(Theme::Dark),
        );
        assert_eq!(s.block_style.bg, Some(Color::White));
        assert_eq!(s.border_style.fg, Some(Color::White));
        assert_eq!(s.text_style.fg, Some(Color::Black));
        // Mono presses in the key's *own* hue instead, not white.
        assert_eq!(
            button_style("5", true, true, ColorMode::Mono, Palette::new(Theme::Dark))
                .block_style
                .bg,
            glyph_color('5', Palette::new(Theme::Dark))
        );
    }

    #[test]
    fn drift_starts_and_ends_at_the_resting_palette() {
        // Zero at *both* ends is the contract: the palette leaves and returns to
        // its resting hues continuously, so there is no jump when the effect
        // fires and none when it expires. (The ripple's tail-truncation bug was
        // exactly this property being violated at the far end.)
        assert_eq!(drift_offset(0.0), 0.0);
        assert!(drift_offset(1.0).abs() < 0.001); // sin(PI), modulo float error
        // And it actually goes somewhere in between, peaking at the midpoint.
        assert_eq!(drift_offset(0.5), DRIFT_SPAN);
        assert!(drift_offset(0.25) > 0.0);
        assert!(drift_offset(0.25) < DRIFT_SPAN);
    }

    #[test]
    fn drift_rotates_every_hue_together() {
        // The drift is a rotation of the whole palette, not a recolor of one
        // glyph: two different glyphs must move by the same amount, so their
        // relative spacing — the thing that makes digits distinguishable — is
        // preserved through the sweep.
        let rest = Palette::new(Theme::Dark);
        let swept = Palette::drifted(Theme::Dark, DRIFT_SPAN);
        for c in ['3', '7', '+'] {
            assert_ne!(glyph_color(c, rest), glyph_color(c, swept), "{c:?} moved");
        }
        // A rotation by a full turn is the identity, which is what `rem_euclid`
        // guarantees — the hue circle wraps rather than clamping at 360.
        assert_eq!(
            glyph_color('3', Palette::drifted(Theme::Dark, 360.0)),
            glyph_color('3', rest)
        );
        // Neutral keys have no hue to rotate, so they are untouched by any drift.
        assert_eq!(glyph_color('=', swept), None);
    }

    #[test]
    fn drift_is_rainbow_only_but_the_theme_still_applies() {
        // Mono has no hues to rotate, so the frame palette must not be a drifted
        // one — the drift is the one effect the two color modes genuinely differ
        // on (ripple and breath both ride on lightness). Asserted at the drift's
        // *peak* through the pure `palette_for`: a drift read live through
        // `frame_palette` has only just started, and `drift_offset(≈0.0)` is
        // legitimately `0.0`, so it can't tell the two modes apart.
        let peak = Some(0.5);
        assert_eq!(
            palette_for(ColorMode::Rainbow, Theme::Dark, peak).drift,
            DRIFT_SPAN
        );
        assert_eq!(palette_for(ColorMode::Mono, Theme::Dark, peak).drift, 0.0);
        // …but mono still tracks the theme, so the palette isn't simply ignored.
        assert_eq!(
            palette_for(ColorMode::Mono, Theme::Dark, peak).theme,
            Theme::Dark
        );
        assert_eq!(
            palette_for(ColorMode::Mono, Theme::Light, peak).theme,
            Theme::Light
        );

        // And the live wiring reaches it: a drift in flight under mono still
        // resolves to the resting palette.
        let mut ui = UiState::new();
        ui.register_press("=");
        ui.register_drift();
        ui.toggle_color_mode(); // → mono
        assert_eq!(frame_palette(&ui), Palette::new(Theme::Dark));
    }

    #[test]
    fn frame_palette_rests_when_nothing_is_drifting() {
        // The common case: no effect in flight, and a press alone (flash +
        // ripple, no drift) must not rotate the palette either.
        let mut ui = UiState::new();
        assert_eq!(frame_palette(&ui), Palette::new(Theme::Dark));
        ui.register_press("5");
        assert_eq!(frame_palette(&ui).drift, 0.0);
    }

    #[test]
    fn breath_is_continuous_across_the_loop_point() {
        // The breath runs forever, so its one hard requirement is that the end of
        // a cycle matches the start — otherwise the display border visibly jumps
        // once every period, which is worse than not breathing at all.
        assert_eq!(
            breath_color(0.0, Theme::Dark),
            breath_color(1.0, Theme::Dark)
        );
        assert_eq!(
            breath_color(0.0, Theme::Light),
            breath_color(1.0, Theme::Light)
        );
        // It does move, and both themes swing (in opposite directions, away from
        // their own background).
        assert_ne!(
            breath_color(0.25, Theme::Dark),
            breath_color(0.75, Theme::Dark)
        );
        assert_ne!(
            breath_color(0.25, Theme::Dark),
            breath_color(0.25, Theme::Light)
        );
    }

    #[test]
    fn breath_phase_advances_and_stays_normalized() {
        // A free-running clock phase, so it is always in range and independent of
        // any effect being in flight.
        let ui = UiState::new();
        let phase = ui.breath_phase();
        assert!((0.0..1.0).contains(&phase), "phase {phase} out of range");
    }

    #[test]
    fn ripple_is_full_at_the_press_and_fades() {
        // The pressed key itself (distance 0) lights immediately and decays to
        // nothing over one fade window — the anchor the travelling rings follow.
        assert_eq!(ripple_intensity(0, 0.0), 1.0);
        assert!(ripple_intensity(0, RIPPLE_RING_FADE / 2.0) < 1.0);
        assert!(ripple_intensity(0, RIPPLE_RING_FADE / 2.0) > 0.0);
        assert_eq!(ripple_intensity(0, RIPPLE_RING_FADE), 0.0);
        // And stays dark rather than going negative or re-lighting.
        assert_eq!(ripple_intensity(0, 1.0), 0.0);
    }

    #[test]
    fn ripple_reaches_far_keys_later_than_near_ones() {
        // The property that makes it a *wave* rather than a flash: at any instant
        // mid-flight, the front has passed the near key and not yet reached the
        // far one. A distance-independent curve (a plain decaying glow) would
        // fail this.
        let phase = 3.0 * RIPPLE_RING_DELAY;
        assert_eq!(ripple_intensity(6, phase), 0.0); // front hasn't arrived
        assert!(ripple_intensity(3, phase) > 0.0); // front is here now
        // Each ring peaks exactly when the front reaches it, so a later ring is
        // still at full while an earlier one has begun to fade.
        assert_eq!(ripple_intensity(3, phase), 1.0);
        assert!(ripple_intensity(1, phase) < 1.0);
    }

    #[test]
    fn ripple_completes_before_expiring_on_every_pad() {
        // The tail-truncation guard. `phase` is clamped at 1.0, so if the
        // outermost ring's fade window runs past it the effect is dropped while
        // that ring is still lit — a pop at the pad's edge, worst exactly where
        // the wave should be gentlest. Lengthening RIPPLE_DURATION does *not* fix
        // it (phase is normalized); only the constants summing under 1.0 does.
        //
        // Measured against the real pads rather than a hardcoded max distance, so
        // adding a bigger pad fails here instead of shipping the pop.
        for pad in [Keypad::standard(), Keypad::tall(), Keypad::wide()] {
            let max = (0..pad.button_count())
                .flat_map(|a| (0..pad.button_count()).map(move |b| (a, b)))
                .map(|(a, b)| pad.button_distance(a, b))
                .max()
                .expect("a pad has buttons");
            assert_eq!(
                ripple_intensity(max, 1.0),
                0.0,
                "the farthest ring is still lit when the ripple expires \
                 (max distance {max}); RIPPLE_RING_DELAY * {max} + RIPPLE_RING_FADE \
                 must not exceed 1.0"
            );
        }
    }

    #[test]
    fn ripple_leaves_a_resting_button_untouched() {
        // Below RIPPLE_FLOOR the base style is returned *identically*, so a quiet
        // pad renders exactly as it did before effects existed — no whole-pad
        // recolor riding along at imperceptible intensity.
        let base = button_style(
            "5",
            false,
            false,
            ColorMode::Rainbow,
            Palette::new(Theme::Dark),
        );
        let rippled = apply_ripple(
            base,
            false,
            0.0,
            "5",
            ColorMode::Rainbow,
            Palette::new(Theme::Dark),
        );
        assert_eq!(rippled.border_style, base.border_style);
        assert_eq!(rippled.block_style, base.block_style);
        assert_eq!(rippled.text_style, base.text_style);
    }

    #[test]
    fn ripple_never_disturbs_a_focused_or_pressed_key() {
        // The ripple is ambient and rides on the border — which is where focus and
        // press put their strongest cue, so it yields to both. This matters most
        // for the pressed key: it sits at distance 0, i.e. full ripple intensity
        // at the exact instant its own flash fires, so without the guard every
        // press would overwrite its `loud` chip border.
        for (focused, pressed) in [(true, false), (true, true)] {
            let base = button_style(
                "5",
                focused,
                pressed,
                ColorMode::Rainbow,
                Palette::new(Theme::Dark),
            );
            let rippled = apply_ripple(
                base,
                true,
                1.0,
                "5",
                ColorMode::Rainbow,
                Palette::new(Theme::Dark),
            );
            assert_eq!(rippled.border_style, base.border_style);
            assert_eq!(rippled.block_style, base.block_style);
        }
    }

    #[test]
    fn ripple_lights_the_border_in_both_color_modes() {
        // The user-facing half of the Mono decision: the ripple is carried by
        // *lightness*, not hue, so it reads in mono too — mono just rides the same
        // HSLuv construction at zero saturation, landing on a gray rather than the
        // key's hue. Both modes must recolor the border, and differently.
        let base = button_style(
            "5",
            false,
            false,
            ColorMode::Mono,
            Palette::new(Theme::Dark),
        );
        let mono = apply_ripple(
            base,
            false,
            1.0,
            "5",
            ColorMode::Mono,
            Palette::new(Theme::Dark),
        );
        assert_ne!(mono.border_style.fg, base.border_style.fg);

        let rainbow_base = button_style(
            "5",
            false,
            false,
            ColorMode::Rainbow,
            Palette::new(Theme::Dark),
        );
        let rainbow = apply_ripple(
            rainbow_base,
            false,
            1.0,
            "5",
            ColorMode::Rainbow,
            Palette::new(Theme::Dark),
        );
        assert_ne!(rainbow.border_style.fg, rainbow_base.border_style.fg);
        // Rainbow borrows the key's hue; mono is hueless — so they differ.
        assert_ne!(mono.border_style.fg, rainbow.border_style.fg);
    }

    #[test]
    fn ripple_brightens_on_dark_and_darkens_on_light() {
        // The theme rule the palette already follows (`loud` / `knockout`): the
        // ripple moves *away from the background*, so it stays visible on both.
        //
        // Asserted on the lightness *numbers*, not by comparing two ripple colors
        // to each other. An earlier version of this test did the latter and passed
        // vacuously while the ripple was in fact darkening the border on the Dark
        // theme — two colors differing tells you nothing about which direction
        // they moved, or about where they sit relative to the resting border.
        for (theme, rest) in [
            (Theme::Dark, RESTING_BORDER_L_DARK),
            (Theme::Light, RESTING_BORDER_L_LIGHT),
        ] {
            // Zero intensity lands exactly on the resting border, so the moment
            // `apply_ripple` stops painting is invisible rather than a snap.
            assert_eq!(ripple_lightness(0.0, theme), rest, "{theme:?} anchor");
            // And every lit level moves away from that background, monotonically.
            let (dim, lit) = (ripple_lightness(0.2, theme), ripple_lightness(1.0, theme));
            match theme {
                Theme::Dark => {
                    assert!(
                        rest < dim && dim < lit,
                        "dark must brighten: {rest} {dim} {lit}"
                    )
                }
                Theme::Light => {
                    assert!(
                        rest > dim && dim > lit,
                        "light must darken: {rest} {dim} {lit}"
                    )
                }
            }
        }
    }

    #[test]
    fn ripple_hands_off_to_the_resting_border_without_a_jump() {
        // The continuity contract between `ripple_lightness` and `apply_ripple`'s
        // floor: just above the floor the painted border must already be
        // indistinguishable from the unpainted one, or every ring pops as it
        // fades out. Guards the two halves being re-tuned independently.
        for theme in [Theme::Dark, Theme::Light] {
            let at_floor = ripple_lightness(RIPPLE_FLOOR, theme);
            let resting = ripple_lightness(0.0, theme);
            assert!(
                (at_floor - resting).abs() < 1.0,
                "{theme:?}: border jumps {} lightness on hand-off",
                (at_floor - resting).abs()
            );
        }
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
        let press = button_style(
            "5",
            true,
            true,
            ColorMode::Rainbow,
            Palette::new(Theme::Light),
        );
        assert_eq!(press.block_style.bg, Some(Color::Black)); // loud on light
        assert_eq!(press.text_style.fg, Some(Color::White)); // knockout contrasts
        assert_ne!(press.block_style.bg, press.text_style.fg);
        // Mono neutral-key focus fill (`=`, `C`, `⌫`):
        let eq_focus = button_style(
            "=",
            true,
            false,
            ColorMode::Mono,
            Palette::new(Theme::Light),
        );
        assert_eq!(eq_focus.block_style.bg, Some(Color::Black));
        assert_ne!(eq_focus.block_style.bg, eq_focus.text_style.fg);
        // Mono neutral-key press outline is loud too, not an invisible white.
        let eq_press = button_style("=", true, true, ColorMode::Mono, Palette::new(Theme::Light));
        assert_eq!(eq_press.border_style.fg, Some(Color::Black));
    }

    #[test]
    fn rainbow_neutral_focus_fill_is_the_theme_neutral() {
        // The hueless-key focus fill in rainbow mode is the per-theme neutral gray,
        // pinned to an actual color (not just "different from a hue") on both themes.
        assert_eq!(
            button_style(
                "=",
                true,
                false,
                ColorMode::Rainbow,
                Palette::new(Theme::Dark)
            )
            .block_style
            .bg,
            Some(Color::Gray)
        );
        assert_eq!(
            button_style(
                "=",
                true,
                false,
                ColorMode::Rainbow,
                Palette::new(Theme::Light)
            )
            .block_style
            .bg,
            Some(Color::DarkGray)
        );
    }

    #[test]
    fn styled_line_dispatches_by_mode() {
        // Mono is a single uncolored span borrowing the whole string; rainbow routes
        // to per-glyph coloring (same result as `rainbow_spans`).
        let mono = styled_line("1+2", ColorMode::Mono, Palette::new(Theme::Dark));
        assert_eq!(mono.spans.len(), 1);
        assert_eq!(mono.spans[0].content.as_ref(), "1+2");
        assert_eq!(mono.spans[0].style.fg, None);

        let rainbow = styled_line("1+2", ColorMode::Rainbow, Palette::new(Theme::Dark));
        let fgs: Vec<_> = rainbow.spans.iter().map(|s| s.style.fg).collect();
        let expected: Vec<_> = rainbow_spans("1+2", Palette::new(Theme::Dark))
            .iter()
            .map(|s| s.style.fg)
            .collect();
        assert_eq!(fgs, expected);
        assert_eq!(
            rainbow.spans[1].style.fg,
            glyph_color('+', Palette::new(Theme::Dark))
        );
    }
}
