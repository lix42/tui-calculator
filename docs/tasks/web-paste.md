# web-paste: Paste an Expression in the Browser (optional)

> Sub-task of [web-ratzilla](web-ratzilla.md) (split 2026-10-06). **Optional** —
> the web build is complete without it.

## Goal

Bring native bracketed-paste parity to the web: pasting `78-65*5` into the page
enters it as one edit.

## Design

A DOM `paste` listener on the document (via `web-sys` / `wasm-bindgen` closure)
reads `clipboardData.getData("text")` and calls `ui.clear_status()` +
`App::apply_str` — the same backend-agnostic path native `Event::Paste` uses, so
it bypasses `activate` (no fever climb, no grace), matching native semantics.
Prefer the `paste` event over `navigator.clipboard.readText()`: no permission
prompt.

Watch for `web-spike` Q6: if Cmd-V/Ctrl-V also arrives as a `v` keydown, make sure
it doesn't double up with anything mapped to `v`.

## How to Verify

Paste `(1+2)×3` into the page → display shows it; `=` → `9`. Fever meter does not
climb from the paste.

## Dependencies

- [web-entry](web-entry.md)
