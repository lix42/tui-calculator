//! UI state: button-grid focus, the transient visual effects in flight, the
//! on-screen geometry used for mouse hit-testing, and the copy affordance + its
//! status.
//!
//! This is the rendering/input-routing half of what used to live in `App`. It
//! owns the active [`Keypad`], where focus currently sits (as a lattice cell),
//! which [`Effect`]s are running, and the screen rect of every button. `App` keeps
//! only the calculator state (`expr` / `current` / `mode`); the two have
//! different lifecycles and concerns.

// `web_time`, not `std::time`: `Instant::now()` panics on wasm32, and this module
// is shared by the (future) web build. Drop-in — it re-exports std's on native.
use web_time::{Duration, Instant};

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

/// How long a press ripple takes to cross the pad and fade. Longer than
/// `FLASH_DURATION` — the chip on the pressed key is a blink, the wave leaving it
/// has ground to cover.
///
/// **The run loop's 100 ms poll is the pacing budget**: an idle terminal repaints
/// at ~10 fps, so this duration buys the ripple about eight frames. That is the
/// constraint the intensity curve is tuned against — a curve that is smooth in
/// the limit can strobe at eight samples. It also sets the wall-clock meaning of
/// the curve's normalized constants (see `ui::RIPPLE_RING_DELAY`), so changing it
/// re-times the wave without changing its shape.
const RIPPLE_DURATION: Duration = Duration::from_millis(800);

/// How long one cycle of the always-on display breath takes.
///
/// Deliberately slow. Every other effect here is event-driven and quiescent by
/// default; this is the single exception that runs forever, so it has to sit far
/// below the threshold where motion draws the eye — long enough that you notice
/// it only when looking for it.
const BREATH_PERIOD: Duration = Duration::from_millis(4200);

/// How long the hue drift takes to sweep the palette and settle back.
///
/// Longer than the ripple: the ripple acknowledges a keystroke, while this marks
/// a finished calculation, and a rotation fast enough to catch the eye would read
/// as a glitch rather than a flourish.
const DRIFT_DURATION: Duration = Duration::from_millis(1400);

/// How much a single press adds to the fever meter, on the `0..=1` scale.
///
/// Climb beats decay at any sustained rate above `FEVER_DECAY / FEVER_CLIMB`
/// presses per second (≈ one every 3 s with the current constants), so slow
/// typing holds the meter, bursts climb it, and silence drains it. See
/// `docs/tasks/fever-mode.md` for the rate math and tuning options.
const FEVER_CLIMB: f64 = 0.15;

/// How fast the fever meter decays when the user isn't typing, in `score` units
/// per second. A full meter drops to zero in `1.0 / FEVER_DECAY` ≈ 20 seconds.
const FEVER_DECAY: f64 = 0.05;

/// How long decay is suppressed after a successful `=`, so a short reading pause
/// right after a result doesn't cost altitude. Only fires on a *successful* `=`
/// (the same `app.copy_text().is_some()` gate the hue drift uses).
const FEVER_GRACE: Duration = Duration::from_millis(2500);

/// Deadband around each stage threshold, to keep a score oscillating within a
/// hair of a boundary from flapping back and forth between stages every tick.
/// Promoting requires `score >= threshold + H`, demoting `score <= threshold - H`.
const FEVER_HYSTERESIS: f64 = 0.02;

/// What a transient visual effect *is*, plus whatever that kind of effect needs
/// to know where it happened.
///
/// Each variant carries **its own** origin data rather than the whole enum
/// sharing one `origin` field. A press is anchored to a lattice cell, but a
/// global effect has no cell at all — a shared field would need a "no origin"
/// case that every global variant has to remember to ignore. Per-variant
/// payloads make the meaningless combination unrepresentable instead, the same
/// reason [`crate::action::Digit`] is a newtype rather than a checked `u8`.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum EffectKind {
    /// The momentary "pressed" look on an activated button. Held as the lattice
    /// cell rather than a button index because a pad switch invalidates cells
    /// (see [`UiState::set_layout`]), and because a spatial effect needs a
    /// position on the lattice to measure from.
    Press { cell: (usize, usize) },
    /// A wave radiating outward from the pressed button across the rest of the
    /// pad. Separate from [`Press`](EffectKind::Press) — which stays the short,
    /// sharp chip on the key you actually hit — because the two run on different
    /// clocks: the chip is over in `FLASH_DURATION` while the wave needs long
    /// enough to cross the lattice.
    Ripple { cell: (usize, usize) },
    /// A rotation of the whole palette's hues, fired by a successful `=`. Global
    /// — it belongs to no cell, which is exactly why [`cell`](EffectKind::cell)
    /// returns `None` for it and a pad switch leaves it running.
    Drift,
}

impl EffectKind {
    /// How long this kind of effect stays visible.
    ///
    /// Derived from the kind rather than stored on each `Effect`: the duration
    /// is a property of *what the effect is*, so a per-instance copy would be a
    /// second source of truth free to disagree with the constant. A total match,
    /// so adding a variant is a compile error here rather than a silent default.
    fn duration(self) -> Duration {
        match self {
            EffectKind::Press { .. } => FLASH_DURATION,
            EffectKind::Ripple { .. } => RIPPLE_DURATION,
            EffectKind::Drift => DRIFT_DURATION,
        }
    }

    /// The lattice cell this effect is anchored to, or `None` if it's global.
    ///
    /// Drives what survives a pad switch: a cell-anchored effect names a cell on
    /// the pad being left (which may not even exist on the new one), while a
    /// global effect is unaffected by the change of lattice.
    fn cell(self) -> Option<(usize, usize)> {
        match self {
            EffectKind::Press { cell } | EffectKind::Ripple { cell } => Some(cell),
            EffectKind::Drift => None,
        }
    }
}

/// One transient visual effect in flight: what it is, and when it began.
///
/// The renderer derives intensity from how far through its lifetime the effect
/// is, so `started` plus the kind's [`duration`](EffectKind::duration) is the
/// whole of its state — there is no per-frame bookkeeping to keep in step.
#[derive(Debug, Clone, Copy)]
pub struct Effect {
    kind: EffectKind,
    started: Instant,
}

impl Effect {
    /// Begin an effect now.
    fn new(kind: EffectKind) -> Self {
        Self {
            kind,
            started: Instant::now(),
        }
    }

    /// What this effect is, and where it started from.
    pub fn kind(self) -> EffectKind {
        self.kind
    }

    /// How far through its lifetime the effect is: `0.0` the instant it begins,
    /// rising to `1.0` as it expires.
    ///
    /// This is the **only** time input the renderer gets — it never sees an
    /// `Instant`. That keeps every animation curve a pure function of a
    /// normalized phase, so the curves are unit-testable without a clock or a
    /// sleep, and the wall-clock pacing stays a concern of this module alone.
    pub fn progress(self) -> f32 {
        let total = self.kind.duration().as_secs_f32();
        (self.started.elapsed().as_secs_f32() / total).clamp(0.0, 1.0)
    }

    /// Whether the effect has outlived its kind's duration and should be dropped.
    fn is_expired(self) -> bool {
        self.started.elapsed() >= self.kind.duration()
    }
}

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

/// Which visual stage the fever meter is currently in. The user has no way to
/// pick a stage directly — it is a function of the meter's score (climbed by
/// typing, decayed by time) and of hysteresis around each threshold.
///
/// Each stage **adds** to the one below: One is plain-plain, Four is full
/// rainbow + animation. The three predicate methods are the gates the renderer
/// reads to decide what to show, and are named for what they enable so a
/// caller doesn't have to know which stage number does what.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Default)]
pub enum FeverStage {
    /// Plain — no palette color, no animation. The press flash still fires
    /// (that's input confirmation, not decoration). The startup default.
    #[default]
    One,
    /// Mono highlights come alive: focus/press use the per-key palette color,
    /// but animation is still off.
    Two,
    /// Animation unlocks: ripple on press and the always-on display breath.
    /// Still mono digit coloring.
    Three,
    /// Full rainbow + the hue drift on a successful `=`. The top of the ladder.
    Four,
}

impl FeverStage {
    /// Whether button focus/press should use the per-key palette color
    /// (today's mono styling). False only at stage One, where everything stays
    /// uncolored.
    pub fn colored_highlights(self) -> bool {
        !matches!(self, FeverStage::One)
    }

    /// Whether decorative animations run — the press ripple and the display
    /// breath. Press flash is **not** gated on this (it's input confirmation).
    pub fn animated(self) -> bool {
        matches!(self, FeverStage::Three | FeverStage::Four)
    }

    /// Whether per-digit rainbow coloring is on. Also gates the hue drift,
    /// indirectly: a non-rainbow palette makes `palette_for` ignore the drift.
    pub fn rainbow(self) -> bool {
        matches!(self, FeverStage::Four)
    }
}

/// Decay the fever meter by one interval of real time, clamped at zero.
///
/// Pure, so the exponent-free decay model and the floor are both testable
/// without a clock. The lazy decay in `UiState::apply_decay` wraps this with the
/// grace-remaining accounting; a caller that just wants "no grace active" calls
/// this directly.
fn score_after_decay(prev: f64, elapsed_secs: f64) -> f64 {
    (prev - FEVER_DECAY * elapsed_secs).max(0.0)
}

/// Decay `prev` over `elapsed_secs`, with up to `grace_remaining_secs` of that
/// interval covered by a reading-grace (no decay).
///
/// The grace is subtracted from the elapsed time *first*; the remainder decays
/// at the normal rate. So a 1 s interval with 0.3 s grace remaining decays for
/// 0.7 s; a 1 s interval with 2 s grace remaining decays for 0 s. Pure.
fn decayed_with_grace(prev: f64, elapsed_secs: f64, grace_remaining_secs: f64) -> f64 {
    let decay_time = (elapsed_secs - grace_remaining_secs).max(0.0);
    score_after_decay(prev, decay_time)
}

/// The stage `score` belongs to when the previous stage was `current`.
///
/// Hysteresis: once in a stage, you leave it only when the score has crossed
/// the boundary by `FEVER_HYSTERESIS`. Promoting requires the upper edge of the
/// deadband; demoting requires the lower edge. Right at a threshold nothing
/// moves. Pure and totally-matched over `FeverStage`.
fn next_stage(current: FeverStage, score: f64) -> FeverStage {
    use FeverStage::*;
    const H: f64 = FEVER_HYSTERESIS;
    match current {
        One => {
            if score >= 0.25 + H {
                Two
            } else {
                One
            }
        }
        Two => {
            if score >= 0.50 + H {
                Three
            } else if score <= 0.25 - H {
                One
            } else {
                Two
            }
        }
        Three => {
            if score >= 0.75 + H {
                Four
            } else if score <= 0.50 - H {
                Two
            } else {
                Three
            }
        }
        Four => {
            if score <= 0.75 - H {
                Three
            } else {
                Four
            }
        }
    }
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
    focus: (usize, usize), // lattice cell holding focus
    // The transient visual effects currently in flight — today just the press
    // flash. A bounded collection rather than a single slot so that letting
    // effects *compose* is a change to `insert_effect`'s policy alone, not a
    // change to how the state is stored. Expired entries are dropped by `tick`.
    effects: Vec<Effect>,
    // When the process started animating, i.e. the zero point of the always-on
    // breath's phase. Fixed for the lifetime of the app — it is a clock origin,
    // not a timer, so nothing ever resets it.
    animation_start: Instant,
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
    // Which background the palette is tuned for (dark by default). Affects both
    // modes — rainbow digit hues and mono highlight accents; toggled by the `t` key.
    theme: Theme,
    // --- Fever state -----------------------------------------------------------
    // A `0..=1` meter climbed by presses and decayed by time, with hysteresis
    // around each stage threshold. The user doesn't pick a visual mode — it is
    // *earned* by typing. See `register_press_fever` / `apply_decay` and the
    // free `score_after_decay` / `next_stage` for the math.
    fever_score: f64,
    // The band `fever_score` sits in, cached so renderer reads don't recompute
    // it on every call. Kept current by `tick` and by `register_press_fever`.
    fever_stage: FeverStage,
    // When the lazy decay was last applied. `Instant::now()` at startup; any
    // decay interval is measured from this to `now` and this is then set to
    // `now`. Private so a test can't forge a time delta.
    fever_last_tick: Instant,
    // `Some(t)` while a reading-grace (fired by a successful `=`) is still in
    // effect: decay skips any part of an interval up to `t`. `None` means no
    // active grace. Cleared by `apply_decay` once `t` is in the past.
    reading_grace_until: Option<Instant>,
    // Quick-input mode: while on, the home-row keys enter digits/operators (see
    // `action::QUICK_MAP`) instead of navigating, and each mapped button shows its
    // key in the border. Entered with `i` and left with `Esc`, both routed in
    // `main.rs` — it's an input-routing and rendering concern, so like `color_mode`
    // and `theme` it lives here rather than on `App`.
    quick_mode: bool,
}

impl UiState {
    pub fn new() -> Self {
        // The registry: the standard pad first (the startup default), then the
        // tall-narrow and wide-short pads. Index 0 is active, so behavior is
        // unchanged until the user switches (or `layout-auto` picks by shape).
        let layouts = vec![Keypad::standard(), Keypad::tall(), Keypad::wide()];
        let focus = layouts[0].default_focus();
        let button_rects = vec![Rect::ZERO; layouts[0].button_count()];
        // One `Instant::now()` call shared across every clock-origin field, so
        // tests that register a press immediately after construction see the
        // minimum possible decay interval rather than two independent calls'
        // worth of microseconds between them.
        let now = Instant::now();
        Self {
            layouts,
            layout: 0,
            override_layout: None,
            term_size: (0, 0),
            focus,
            effects: Vec::new(),
            animation_start: now,
            button_rects,
            copy_rect: Rect::ZERO,
            status: None,
            theme: Theme::default(),
            quick_mode: false,
            fever_score: 0.0,
            fever_stage: FeverStage::One,
            fever_last_tick: now,
            reading_grace_until: None,
        }
    }

    /// Turn quick-input mode on or off. A setter rather than a toggle because the
    /// two triggers are one-way and live in different states: `i` only *enters*
    /// (in-mode it types `5`, per the numpad map) and `Esc` only *leaves*.
    pub fn set_quick_mode(&mut self, on: bool) {
        self.quick_mode = on;
    }

    /// Whether quick-input mode is on. Read by the key router (which keys enter
    /// digits) and by the renderer (which buttons show a tip).
    pub fn quick_mode(&self) -> bool {
        self.quick_mode
    }

    /// The active color mode, **derived** from the fever stage: only stage Four
    /// is rainbow, every other stage is mono. There is no manual override —
    /// fever replaces what `r` used to do. The renderer reads this to pick
    /// between the mono and rainbow render paths.
    pub fn color_mode(&self) -> ColorMode {
        if self.fever_stage == FeverStage::Four {
            ColorMode::Rainbow
        } else {
            ColorMode::Mono
        }
    }

    /// The active fever stage, snapped through hysteresis around the thresholds.
    /// `Copy`, so callers can hold it while mutably borrowing `self` elsewhere.
    /// Read by the renderer to decide what visual elements to show (see
    /// [`FeverStage::colored_highlights`] / [`animated`][FeverStage::animated] /
    /// [`rainbow`][FeverStage::rainbow]).
    pub fn stage(&self) -> FeverStage {
        self.fever_stage
    }

    /// The current fever meter score, on `0..=1`. The renderer draws it as a
    /// right-to-left fill on the display's bottom border. Returned as the
    /// cached value — `tick` or a press bring it up to date.
    pub fn fever_score(&self) -> f64 {
        self.fever_score
    }

    /// Bump the fever meter for a successful input event. Called from
    /// `activate` *after* `register_press`, since the flash and ripple are
    /// their own concern. Catches up any outstanding decay first so an
    /// occasional press after a long idle doesn't eat its own climb.
    pub fn register_press_fever(&mut self) {
        let now = Instant::now();
        self.apply_decay(now);
        self.fever_score = (self.fever_score + FEVER_CLIMB).min(1.0);
        self.fever_stage = next_stage(self.fever_stage, self.fever_score);
    }

    /// Start a reading-grace window: for the next [`FEVER_GRACE`] no decay
    /// applies, so a short pause after seeing a result doesn't cost altitude.
    /// Called from `activate` on a *successful* `=` (the existing
    /// `app.copy_text().is_some()` gate — same as the hue drift).
    pub fn register_grace(&mut self) {
        self.reading_grace_until = Some(Instant::now() + FEVER_GRACE);
    }

    /// Apply any outstanding decay since the last tick, bringing `fever_score`
    /// (and the cached stage) up to `now`. The reading-grace, if active, is
    /// subtracted from the decay interval first — see
    /// [`decayed_with_grace`]. Called by `tick` once per draw iteration and by
    /// `register_press_fever` right before it climbs.
    fn apply_decay(&mut self, now: Instant) {
        // Grace remaining as of the previous tick: zero unless a grace was
        // active *then*. If `grace_until` has drifted into the past since the
        // last tick, the grace covered part of this interval, which the pure
        // helper handles via subtraction.
        let grace_remaining = match self.reading_grace_until {
            Some(g) if g > self.fever_last_tick => (g - self.fever_last_tick).as_secs_f64(),
            _ => 0.0,
        };
        let elapsed = now.duration_since(self.fever_last_tick).as_secs_f64();
        self.fever_score = decayed_with_grace(self.fever_score, elapsed, grace_remaining);
        self.fever_last_tick = now;
        if let Some(g) = self.reading_grace_until
            && g <= now
        {
            self.reading_grace_until = None;
        }
        self.fever_stage = next_stage(self.fever_stage, self.fever_score);
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
        // Cell-anchored effects name a cell on the pad we're leaving — which may
        // not exist on the new one — so drop them. Global effects have no cell to
        // invalidate and ride through the switch untouched.
        self.effects.retain(|e| e.kind.cell().is_none());
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

    /// Whether button `idx` is currently showing its pressed flash. Resolved
    /// through the keypad's occupancy map, so a spanning button flashes as one
    /// unit from whichever of its cells was pressed.
    pub fn is_button_pressed(&self, idx: usize) -> bool {
        self.effects.iter().any(|e| match e.kind {
            EffectKind::Press { cell } => self.keypad().button_index_at(cell.0, cell.1) == idx,
            // The ripple is drawn by modulating every button's color, not by the
            // pressed look — a rippled key is not a pressed key. The drift is
            // global and touches no single button at all.
            EffectKind::Ripple { .. } | EffectKind::Drift => false,
        })
    }

    /// Record that `label` was just activated: focus follows it and its press
    /// flash starts. No-op if the label isn't on the grid. The run loop's `tick`
    /// clears the flash after `FLASH_DURATION`.
    pub fn register_press(&mut self, label: &str) {
        if let Some(pos) = self.keypad().position_of(label) {
            self.focus = pos;
            // One press starts two effects on two clocks: the sharp chip on the
            // key itself, and the slower wave leaving it.
            self.start_effects([
                EffectKind::Press { cell: pos },
                EffectKind::Ripple { cell: pos },
            ]);
        }
    }

    /// Start `kinds` together as one trigger's worth of animation, superseding
    /// whatever was in flight.
    ///
    /// **Latest-wins**: a new trigger clears the collection rather than layering
    /// onto it, so fresh input cancels a running effect instead of compounding
    /// with it. Letting ripples *compose* — retain the live ones and append up to
    /// a cap, so rapid input leaves overlapping waves — is a change to this
    /// policy and nothing else. That is the whole reason `effects` is a
    /// collection rather than a single slot: the stretch goal is an insertion
    /// rule here, not a different shape of state everywhere else.
    fn start_effects(&mut self, kinds: impl IntoIterator<Item = EffectKind>) {
        self.effects.clear();
        self.effects.extend(kinds.into_iter().map(Effect::new));
    }

    /// The effects currently in flight, for the renderer to derive intensities
    /// from. Ordered by insertion, which is also priority order for a trigger
    /// that starts several.
    pub fn effects(&self) -> &[Effect] {
        &self.effects
    }

    /// Start the global hue drift that marks a successful evaluation.
    ///
    /// **Joins** the effects already in flight rather than replacing them, unlike
    /// [`start_effects`](Self::start_effects): pressing `=` is one trigger, and
    /// its press flash, ripple and drift all belong to it. Call it *after*
    /// `register_press` — that's the call that clears the previous trigger, so
    /// the reverse order would throw this away immediately.
    pub fn register_drift(&mut self) {
        self.effects.push(Effect::new(EffectKind::Drift));
    }

    /// A free-running `0.0..1.0` phase for the always-on display breath, cycling
    /// every [`BREATH_PERIOD`].
    ///
    /// Unlike every other effect this has no trigger and never expires, so it
    /// reads the wall clock directly rather than living in `effects` — modelling
    /// a thing that is always running as a thing that was just started would mean
    /// re-inserting it forever.
    pub fn breath_phase(&self) -> f32 {
        let period = BREATH_PERIOD.as_secs_f32();
        (self.animation_start.elapsed().as_secs_f32() / period).fract()
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

    /// Expire finished effects and the status message once each has been visible
    /// for its duration, and apply any outstanding fever-meter decay. Called
    /// once per run-loop iteration before drawing.
    pub fn tick(&mut self) {
        self.apply_decay(Instant::now());
        self.effects.retain(|e| !e.is_expired());
        if let Some((_, at)) = self.status
            && at.elapsed() >= STATUS_DURATION
        {
            self.status = None;
        }
    }

    /// The lattice cell currently showing the press flash, or `None`. Test-only:
    /// production code asks [`is_button_pressed`](Self::is_button_pressed) rather
    /// than reaching for the cell, so this exists to keep the flash tests
    /// asserting on the cell they always did.
    #[cfg(test)]
    fn flash_cell(&self) -> Option<(usize, usize)> {
        self.effects.iter().find_map(|e| match e.kind {
            EffectKind::Press { cell } => Some(cell),
            EffectKind::Ripple { .. } | EffectKind::Drift => None,
        })
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
        let flash = ui.flash_cell();
        assert!(flash.is_some());
        ui.auto_select(40, 40); // same shape → same pad → no churn
        assert_eq!(ui.flash_cell(), flash); // flash not dropped
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
        assert!(ui.flash_cell().is_some());
        ui.set_layout(1); // tall
        assert_eq!(ui.flash_cell(), None);
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
        assert_eq!(ui.flash_cell(), Some((2, 1))); // and that cell is flashing
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
        assert_eq!(ui.flash_cell(), None);
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
    fn color_mode_is_mono_until_fever_hits_stage_four() {
        // There is no manual `r` toggle any more: rainbow is reserved for the
        // top of the fever ladder. A fresh UiState sits at stage One (score 0),
        // so it reads as mono until the meter is climbed into stage Four.
        let mut ui = UiState::new();
        assert_eq!(ui.stage(), FeverStage::One);
        assert_eq!(ui.color_mode(), ColorMode::Mono);
        // Any stage below Four is still mono.
        for score in [0.3, 0.6] {
            ui.fever_score = score;
            ui.fever_stage = next_stage(ui.fever_stage, score);
            assert_eq!(ui.color_mode(), ColorMode::Mono, "stage {:?}", ui.stage());
        }
        // At stage Four the derivation flips to rainbow.
        ui.fever_score = 0.9;
        ui.fever_stage = next_stage(ui.fever_stage, 0.9);
        assert_eq!(ui.stage(), FeverStage::Four);
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
    fn quick_mode_is_off_at_launch_and_set_explicitly() {
        // A setter, not a toggle: `i` only enters and `Esc` only leaves, so
        // setting it twice the same way is idempotent rather than a flip.
        let mut ui = UiState::new();
        assert!(!ui.quick_mode());
        ui.set_quick_mode(true);
        assert!(ui.quick_mode());
        ui.set_quick_mode(true);
        assert!(ui.quick_mode());
        ui.set_quick_mode(false);
        assert!(!ui.quick_mode());
    }

    #[test]
    fn tick_keeps_fresh_flash() {
        // A flash set this instant is well within FLASH_DURATION, so tick must
        // leave it visible. (Expiry after the duration is paced by the run loop
        // and exercised manually rather than with a sleep here.)
        let mut ui = UiState::new();
        ui.register_press("5");
        ui.tick();
        assert_eq!(ui.flash_cell(), Some((2, 1)));
    }

    // --- Fever math ------------------------------------------------------------

    #[test]
    fn score_after_decay_falls_at_the_configured_rate() {
        // One second removes exactly `FEVER_DECAY` from the score — the
        // mechanism's one tunable rate.
        assert!((score_after_decay(1.0, 1.0) - (1.0 - FEVER_DECAY)).abs() < 1e-9);
        assert!((score_after_decay(0.5, 2.0) - (0.5 - 2.0 * FEVER_DECAY)).abs() < 1e-9);
    }

    #[test]
    fn score_after_decay_clamps_at_zero() {
        // A long enough interval doesn't drive the score negative — the meter
        // bottoms out at zero and stays there. This is the floor the
        // render-time geometry (bottom border fill width) relies on.
        assert_eq!(score_after_decay(0.1, 10.0), 0.0);
        assert_eq!(score_after_decay(0.0, 10.0), 0.0);
    }

    #[test]
    fn decayed_with_grace_covers_the_whole_interval() {
        // Grace that spans the whole elapsed window means no decay: an idle
        // 2 s immediately after `=` with ≥ 2 s of grace left is a free pause.
        assert_eq!(decayed_with_grace(1.0, 2.0, 2.5), 1.0);
        assert_eq!(decayed_with_grace(0.5, 1.0, 10.0), 0.5);
    }

    #[test]
    fn decayed_with_grace_decays_only_the_post_grace_portion() {
        // Grace that expires mid-window: decay runs for `elapsed - grace`
        // seconds, no more. 3 s elapsed with 1 s grace remaining decays for 2 s.
        let got = decayed_with_grace(1.0, 3.0, 1.0);
        let want = 1.0 - 2.0 * FEVER_DECAY;
        assert!((got - want).abs() < 1e-9, "got {got}, want {want}");
    }

    #[test]
    fn decayed_with_grace_without_a_grace_matches_plain_decay() {
        // No grace active (`grace_remaining = 0`) must be identical to
        // `score_after_decay` — the two helpers compose, they don't disagree.
        for elapsed in [0.0, 0.5, 1.5, 10.0] {
            assert_eq!(
                decayed_with_grace(0.8, elapsed, 0.0),
                score_after_decay(0.8, elapsed),
                "elapsed {elapsed}"
            );
        }
    }

    #[test]
    fn next_stage_climbs_at_the_upper_hysteresis_edge() {
        // Promoting requires `score >= threshold + FEVER_HYSTERESIS`. The exact
        // boundary (0.25) holds; just over the edge (0.27 with H=0.02) crosses.
        assert_eq!(next_stage(FeverStage::One, 0.25), FeverStage::One);
        assert_eq!(next_stage(FeverStage::One, 0.26), FeverStage::One);
        assert_eq!(next_stage(FeverStage::One, 0.27), FeverStage::Two);
        assert_eq!(next_stage(FeverStage::Two, 0.52), FeverStage::Three);
        assert_eq!(next_stage(FeverStage::Three, 0.77), FeverStage::Four);
    }

    #[test]
    fn next_stage_falls_at_the_lower_hysteresis_edge() {
        // Demoting requires `score <= threshold - FEVER_HYSTERESIS`. Mirror of
        // the climb test — the deadband keeps the stage from flapping on either
        // side of a threshold.
        assert_eq!(next_stage(FeverStage::Two, 0.23), FeverStage::One);
        assert_eq!(next_stage(FeverStage::Two, 0.24), FeverStage::Two);
        assert_eq!(next_stage(FeverStage::Three, 0.48), FeverStage::Two);
        assert_eq!(next_stage(FeverStage::Four, 0.73), FeverStage::Three);
    }

    #[test]
    fn next_stage_holds_within_the_hysteresis_band() {
        // Right in the deadband around each threshold, every stage holds. This
        // is the whole purpose of the hysteresis: a score hovering around 0.25
        // (± 0.02) must not oscillate between One and Two every tick.
        for score in [0.24, 0.25, 0.26] {
            assert_eq!(next_stage(FeverStage::One, score), FeverStage::One);
            assert_eq!(next_stage(FeverStage::Two, score), FeverStage::Two);
        }
    }

    #[test]
    fn fever_startup_is_stage_one_with_zero_score() {
        // The user hasn't typed anything yet, so the ladder is at its bottom —
        // plain-plain, no color, no animation. This is the only startup state;
        // the renderer tests downstream can rely on it.
        let ui = UiState::new();
        assert_eq!(ui.stage(), FeverStage::One);
        assert_eq!(ui.fever_score(), 0.0);
        assert_eq!(ui.color_mode(), ColorMode::Mono);
    }

    #[test]
    fn register_press_fever_climbs_the_score() {
        // Each press adds `FEVER_CLIMB` (modulo an immeasurable sliver of
        // decay since construction). After `n` presses the score is within an
        // epsilon of `n * FEVER_CLIMB`, so the test tolerates the real-clock
        // decay without relying on it being zero.
        let mut ui = UiState::new();
        for n in 1..=5 {
            ui.register_press_fever();
            let expected = (n as f64 * FEVER_CLIMB).min(1.0);
            let got = ui.fever_score();
            assert!(
                (got - expected).abs() < 0.01,
                "press {n}: got {got}, want ~{expected}"
            );
        }
    }

    #[test]
    fn fever_climbs_through_the_full_ladder() {
        // Seven back-to-back presses climb from stage One to Four: 7 * 0.15 =
        // 1.05 → clamps at 1.0, which is firmly past the 0.77 upper-hysteresis
        // edge for Four. The whole ladder is reachable in a short burst,
        // deliberately — the mechanic is meant to feel generous, not punishing.
        let mut ui = UiState::new();
        assert_eq!(ui.stage(), FeverStage::One);
        for _ in 0..7 {
            ui.register_press_fever();
        }
        assert_eq!(ui.stage(), FeverStage::Four);
        assert_eq!(ui.color_mode(), ColorMode::Rainbow); // and rainbow unlocks
    }

    #[test]
    fn register_grace_suppresses_decay_in_the_grace_window() {
        // After `register_grace`, `apply_decay` must leave the score alone for
        // the first `FEVER_GRACE` of real time. Driven through the private
        // helper with a hand-picked `now` so the test doesn't need a sleep.
        //
        // The grace end is pinned to `fever_last_tick + FEVER_GRACE` directly
        // (rather than via `register_grace`, which would call `Instant::now()`
        // a few microseconds past the one `new()` captured) so the arithmetic
        // below lines up exactly on the grace boundary.
        let mut ui = UiState::new();
        ui.fever_score = 0.9;
        let start = ui.fever_last_tick;
        ui.reading_grace_until = Some(start + FEVER_GRACE);
        // Simulate one second passing *inside* the grace window.
        ui.apply_decay(start + Duration::from_secs(1));
        assert!(
            (ui.fever_score - 0.9).abs() < 1e-9,
            "grace must freeze the meter; got {}",
            ui.fever_score
        );
        // Now 3 s past the start — grace expired at 2.5 s, so decay runs for
        // the remaining 0.5 s only.
        ui.apply_decay(start + Duration::from_secs(3));
        let want = 0.9 - 0.5 * FEVER_DECAY;
        assert!(
            (ui.fever_score - want).abs() < 1e-9,
            "post-grace decay wrong; got {}, want {want}",
            ui.fever_score
        );
        assert!(
            ui.reading_grace_until.is_none(),
            "expired grace must be cleared"
        );
    }
}
