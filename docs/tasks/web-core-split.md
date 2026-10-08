# web-core-split: Extract `calculator-core` into a Cargo Workspace

> Sub-task of [web-ratzilla](web-ratzilla.md) (split 2026-10-06).

## Goal

Turn the single package into a Cargo workspace with a backend-free
`calculator-core` library and a thin native `calculator` binary. **Pure
relocation** — no behavior change, the full test suite passes unchanged.

## Design

Why a workspace and not target-gated deps in one package: Cargo features and
dependencies are per **package**. A web crate depending on today's package would
pull `crossterm` + `arboard` into the wasm build, and neither compiles for
`wasm32-unknown-unknown`. The core must be a package that never names them.

Verified 2026-10-06: `crossterm` is referenced **only** in `src/main.rs`; `ui.rs`,
`ui_state.rs`, `app.rs`, `action.rs`, `layout.rs`, `eval.rs` are already
backend-free (~5.3k of ~6k lines), so they move intact.

```
Cargo.toml                 # [workspace] members, shared [workspace.dependencies]
crates/core/               # calculator-core (lib): action, app, eval, layout, ui_state, ui
crates/native/ (or root)   # calculator (bin): main.rs — crossterm, arboard, event loop
```

- `calculator-core` deps: `ratatui` (+ `palette`), `web-time`. **No** crossterm,
  arboard, ratzilla.
- Native bin deps: `calculator-core`, `ratatui` (crossterm backend), `crossterm`,
  `arboard`.
- `web-time` stays a plain, un-gated core dep (the existing `Cargo.toml` comment
  explains why — move the comment with it).
- Keep the `Cargo.toml` dependency comments (palette, wayland-data-control).

## Implementation Suggestion

- Decide the ratatui feature set for core using `web-spike` Q4 if it's answered;
  otherwise keep default features in this task and trim them in `web-entry`.
- `pub` visibility: modules that were crate-private become the core's public API.
  Expose only what `main.rs` uses; don't widen everything to `pub`.
- Watch for `#[cfg(test)]` helpers in one module used by another module's tests —
  they stay intra-crate, fine; helpers used by `main.rs` tests need a home.
- Update `CLAUDE.md`'s Architecture section paths and the Commands section
  (`cargo test --workspace`, `cargo run -p calculator`).

## How to Verify

- `cargo test --workspace`: same test count as before the move, all green.
- `cargo clippy --workspace --all-targets` and `cargo fmt --check` clean.
- `cargo run` (or `-p calculator`) behaves identically.
- `cargo tree -p calculator-core` shows no `crossterm` / `arboard`.

## Dependencies

None hard. Soft: [web-spike](web-spike.md) Q4 informs the core's ratatui
features, but the split can land without it.
